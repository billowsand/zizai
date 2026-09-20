//! 录音中的临时转写：后台周期性识别当前音频，最终文本仍由松键后的完整识别决定。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::asr::AsrEngine;
use crate::audio::to_mono_16k;

const PASS_INTERVAL: Duration = Duration::from_millis(650);
const MAX_INTERVAL: Duration = Duration::from_secs(3);
const MIN_AUDIO_SECS: f32 = 0.9;
const COMMIT_AFTER_SECS: f32 = 8.0;
const FRAME: usize = 320;

struct Shared {
    raw: Mutex<Vec<f32>>,
    grew: Condvar,
    stopped: AtomicBool,
}

/// 一次录音对应一个预览线程；丢弃句柄即停止继续发布结果。
pub(crate) struct LivePreview {
    shared: Arc<Shared>,
}

impl LivePreview {
    pub(crate) fn start(
        engine: Arc<AsrEngine>,
        on_partial: impl Fn(String) + Send + 'static,
        sample_rate: u32,
        channels: u16,
    ) -> Option<Self> {
        let shared = Arc::new(Shared {
            raw: Mutex::new(Vec::new()),
            grew: Condvar::new(),
            stopped: AtomicBool::new(false),
        });
        let worker = shared.clone();
        std::thread::Builder::new()
            .name("qingjian-asr-preview".to_owned())
            .spawn(move || run(worker, engine, on_partial, sample_rate, channels))
            .map_err(|error| tracing::warn!(%error, "实时转写预览不可用"))
            .ok()?;
        Some(Self { shared })
    }

    pub(crate) fn push(&self, chunk: &[f32]) {
        let mut raw = lock(&self.shared.raw);
        raw.extend_from_slice(chunk);
        drop(raw);
        self.shared.grew.notify_one();
    }
}

impl Drop for LivePreview {
    fn drop(&mut self) {
        self.shared.stopped.store(true, Ordering::Release);
        self.shared.grew.notify_all();
    }
}

fn run(
    shared: Arc<Shared>,
    engine: Arc<AsrEngine>,
    on_partial: impl Fn(String) + Send + 'static,
    sample_rate: u32,
    channels: u16,
) {
    let stride = usize::from(channels.max(1));
    let mut committed_text = String::new();
    let mut interval = PASS_INTERVAL;
    let mut last_pass = Instant::now() - interval;
    while let Some(raw) = wait_for_audio(&shared, sample_rate, stride, interval, &mut last_pass) {
        let started = Instant::now();
        let Ok(mono) = to_mono_16k(&raw, sample_rate, channels) else {
            continue;
        };
        let Ok(text) = engine.transcribe(&mono) else {
            continue;
        };
        if shared.stopped.load(Ordering::Acquire) {
            return;
        }
        on_partial(joined(&committed_text, &text));
        interval = PASS_INTERVAL
            .max(started.elapsed().mul_f32(1.5))
            .min(MAX_INTERVAL);

        // 长录音只反复识别最近一段：在后半段最安静的 20 ms 处分块，避免预览耗时随
        // 录音长度平方增长。最终上屏仍会完整识别原始 `samples`，这里不改变结果。
        if mono.len() as f32 / 16_000.0 >= COMMIT_AFTER_SECS
            && let Some(split) = quiet_split(&mono)
            && let Ok(head) = engine.transcribe(&mono[..split])
        {
            committed_text = joined(&committed_text, &head);
            let mut buffer = lock(&shared.raw);
            let frozen = raw_offset(split, sample_rate, stride).min(buffer.len());
            buffer.drain(..frozen);
        }
    }
}

fn wait_for_audio(
    shared: &Shared,
    sample_rate: u32,
    stride: usize,
    interval: Duration,
    last_pass: &mut Instant,
) -> Option<Vec<f32>> {
    let mut raw = lock(&shared.raw);
    loop {
        if shared.stopped.load(Ordering::Acquire) {
            return None;
        }
        let waited = last_pass.elapsed();
        let seconds = raw.len() as f32 / stride as f32 / sample_rate.max(1) as f32;
        if waited >= interval && seconds >= MIN_AUDIO_SECS {
            *last_pass = Instant::now();
            return Some(raw.clone());
        }
        let timeout = interval
            .saturating_sub(waited)
            .max(Duration::from_millis(30));
        raw = shared
            .grew
            .wait_timeout(raw, timeout)
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .0;
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn raw_offset(mono_index: usize, sample_rate: u32, stride: usize) -> usize {
    let frames = (mono_index as f64 * f64::from(sample_rate) / 16_000.0) as usize;
    frames * stride
}

fn joined(head: &str, tail: &str) -> String {
    let (head, tail) = (head.trim_end(), tail.trim());
    if head.is_empty() {
        return tail.to_owned();
    }
    if tail.is_empty() {
        return head.to_owned();
    }
    let needs_space = head.ends_with(|character: char| character.is_ascii_alphanumeric())
        && tail.starts_with(|character: char| character.is_ascii_alphanumeric());
    if needs_space {
        format!("{head} {tail}")
    } else {
        format!("{head}{tail}")
    }
}

fn quiet_split(mono: &[f32]) -> Option<usize> {
    let frames = mono.len() / FRAME;
    if frames < 25 {
        return None;
    }
    let energy = |frame: usize| {
        let window = &mono[frame * FRAME..(frame + 1) * FRAME];
        (window.iter().map(|sample| sample * sample).sum::<f32>() / FRAME as f32).sqrt()
    };
    let average = (0..frames).map(energy).sum::<f32>() / frames as f32;
    let (quietest, level) = (frames * 45 / 100..frames * 90 / 100)
        .map(|frame| (frame, energy(frame)))
        .min_by(|left, right| left.1.total_cmp(&right.1))?;
    (level <= average * 0.35).then_some(quietest * FRAME)
}

#[cfg(test)]
mod tests {
    use super::{FRAME, joined, quiet_split, raw_offset};

    #[test]
    fn preview_segments_join_chinese_and_latin_naturally() {
        assert_eq!(joined("今天天气", "不错"), "今天天气不错");
        assert_eq!(joined("hello", "world"), "hello world");
    }

    #[test]
    fn quiet_pause_is_a_safe_split_point() {
        let mut mono = vec![0.4f32; FRAME * 60];
        mono[FRAME * 26..FRAME * 32].fill(0.0);
        let split = quiet_split(&mono).expect("pause should be found");
        assert!((FRAME * 26..FRAME * 32).contains(&split));
    }

    #[test]
    fn resampled_offset_maps_back_to_interleaved_capture() {
        assert_eq!(raw_offset(16_000, 48_000, 2), 96_000);
    }
}
