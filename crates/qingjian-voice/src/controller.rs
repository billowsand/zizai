//! 语音录制与识别线程；公开方法只投递短命令或读取快照。

use std::sync::{Arc, Mutex, mpsc};

use qingjian_platform::protocol::VoiceState;

use crate::command::Command;
use crate::{VoiceError, WorkerConfig, WorkerSnapshot};

/// 一个 Worker 进程内的语音控制器。
pub struct Controller {
    sender: mpsc::Sender<Command>,

    snapshot: Arc<Mutex<WorkerSnapshot>>,
}

impl Controller {
    /// 启动模型加载与录音线程；构造本身不等待模型。
    pub fn new(config: WorkerConfig) -> Self {
        let (sender, receiver) = mpsc::channel();
        let snapshot = Arc::new(Mutex::new(WorkerSnapshot {
            state: VoiceState::Loading,
            ..WorkerSnapshot::default()
        }));
        #[cfg(windows)]
        {
            let shared = snapshot.clone();
            std::thread::spawn(move || run_windows(config, receiver, &shared));
        }
        #[cfg(not(windows))]
        {
            let _ = config;
            let _ = receiver;
            replace_snapshot(
                &snapshot,
                WorkerSnapshot {
                    state: VoiceState::Failed,
                    message: Some("语音输入只支持 Windows".into()),
                    ..WorkerSnapshot::default()
                },
            );
        }
        Self { sender, snapshot }
    }

    pub fn start(&self, request: u64) -> Result<(), VoiceError> {
        self.send(Command::Start(request))
    }

    pub fn stop(&self, request: u64) -> Result<(), VoiceError> {
        self.send(Command::Stop(request))
    }

    pub fn cancel(&self, request: u64) -> Result<(), VoiceError> {
        self.send(Command::Cancel(request))
    }

    pub fn snapshot(&self) -> WorkerSnapshot {
        self.snapshot
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn send(&self, command: Command) -> Result<(), VoiceError> {
        self.sender.send(command).map_err(|_| VoiceError::Closed)
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        let _ = self.sender.send(Command::Shutdown);
    }
}

fn replace_snapshot(shared: &Arc<Mutex<WorkerSnapshot>>, value: WorkerSnapshot) {
    *shared
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = value;
}

#[cfg(windows)]
fn run_windows(
    config: WorkerConfig,
    receiver: mpsc::Receiver<Command>,
    snapshot: &Arc<Mutex<WorkerSnapshot>>,
) {
    use std::time::{Duration, Instant};

    use crate::asr::{AsrConfig, AsrEngine, HrConfig, Punctuator};
    use crate::audio::{
        LivePreview, OpenedInput, SilenceDetector, open_input, rms_energy, visual_level,
    };
    use crate::polish::TextPolisher;

    // 同音词资源是可选项：路径给到但文件不在（比如装机后文件被删）就丢掉这一项，
    // 不让 sherpa 建识别器失败拖垮整条语音输入。
    let existing =
        |path: Option<String>| path.filter(|value| std::fs::exists(value).unwrap_or(false));
    let asr = AsrConfig {
        model: config.model,
        tokens: config.tokens,
        language: config.language,
    };
    let hr = HrConfig {
        lexicon: existing(config.hr_lexicon),
        rule_fsts: existing(config.hr_rule_fsts),
    };
    let engine = match AsrEngine::new(&asr, &hr) {
        Ok(engine) => Arc::new(engine),
        Err(error) => {
            tracing::error!(%error, "语音模型加载失败");
            replace_snapshot(
                snapshot,
                WorkerSnapshot {
                    state: VoiceState::Failed,
                    message: Some("语音模型加载失败".into()),
                    ..WorkerSnapshot::default()
                },
            );
            wait_for_shutdown(receiver);
            return;
        }
    };
    tracing::info!("语音模型已加载");
    // 标点恢复是可选增强：路径错或模型坏只降级到原文，不打断语音输入。
    let punctuator = match Punctuator::new(config.punctuation_model.as_deref()) {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(%error, "标点模型加载失败，退回 SenseVoice 原文");
            None
        }
    };
    let polisher = config
        .polish
        .as_ref()
        .zip(config.polish_url.as_deref())
        .zip(config.polish_model.as_deref())
        .and_then(|((level, url), model)| {
            TextPolisher::new(*level, url, model, config.hotwords.clone())
        });
    let pipeline = PostPipeline {
        engine: engine.clone(),
        punctuator,
        polisher,
    };
    replace_snapshot(
        snapshot,
        WorkerSnapshot {
            state: VoiceState::Idle,
            ..WorkerSnapshot::default()
        },
    );

    let (audio_sender, audio_receiver) = mpsc::sync_channel::<Vec<f32>>(128);
    let mut opened: Option<OpenedInput> = None;
    let mut preview: Option<LivePreview> = None;
    let mut samples = Vec::new();
    let mut started = None::<Instant>;
    let mut silence = SilenceDetector::new(config.auto_stop_ms);

    loop {
        if opened.is_some() {
            let mut should_auto_finish = false;
            while let Ok(chunk) = audio_receiver.try_recv() {
                if let Some(preview) = preview.as_ref() {
                    preview.push(&chunk);
                }
                if let Some(input) = opened.as_ref() {
                    let level = visual_level(rms_energy(&chunk, input.channels));
                    update_level(snapshot, level);
                    let frames = chunk.len() / usize::from(input.channels.max(1));
                    let chunk_duration = Duration::from_secs_f64(
                        frames as f64 / f64::from(input.sample_rate.max(1)),
                    );
                    should_auto_finish |= silence.observe(level, chunk_duration);
                }
                samples.extend_from_slice(&chunk);
                if should_auto_finish {
                    break;
                }
            }
            let reached_limit =
                started.is_some_and(|value| value.elapsed() >= Duration::from_secs(90));
            if should_auto_finish || reached_limit {
                let request = current_request(snapshot).unwrap_or_default();
                if should_auto_finish {
                    tracing::info!(request, "检测到停顿，自动结束语音录音");
                }
                preview = None;
                finish(
                    request,
                    &mut opened,
                    &audio_receiver,
                    &mut samples,
                    &pipeline,
                    snapshot,
                );
                started = None;
                silence.reset();
            }
        }

        let command = match receiver.recv_timeout(Duration::from_millis(10)) {
            Ok(command) => command,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        match command {
            Command::Start(request) => {
                if opened.is_some() {
                    continue;
                }
                while audio_receiver.try_recv().is_ok() {}
                samples.clear();
                silence.reset();
                match open_input(config.input_device.as_deref(), audio_sender.clone()) {
                    Ok(input) => {
                        tracing::info!(device = %input.name, request, "麦克风已打开");
                        let sample_rate = input.sample_rate;
                        let channels = input.channels;
                        opened = Some(input);
                        started = Some(Instant::now());
                        replace_snapshot(
                            snapshot,
                            WorkerSnapshot {
                                state: VoiceState::Recording,
                                request: Some(request),
                                ..WorkerSnapshot::default()
                            },
                        );
                        let shared = snapshot.clone();
                        preview = LivePreview::start(
                            engine.clone(),
                            move |text| update_partial(&shared, request, text),
                            sample_rate,
                            channels,
                        );
                    }
                    Err(error) => {
                        tracing::error!(%error, "麦克风打开失败");
                        replace_snapshot(
                            snapshot,
                            WorkerSnapshot {
                                state: VoiceState::Failed,
                                request: Some(request),
                                message: Some("麦克风打开失败".into()),
                                ..WorkerSnapshot::default()
                            },
                        );
                    }
                }
            }
            Command::Stop(request) if current_request(snapshot) == Some(request) => {
                preview = None;
                finish(
                    request,
                    &mut opened,
                    &audio_receiver,
                    &mut samples,
                    &pipeline,
                    snapshot,
                );
                started = None;
                silence.reset();
            }
            Command::Cancel(request) if current_request(snapshot) == Some(request) => {
                preview = None;
                opened = None;
                started = None;
                silence.reset();
                samples.clear();
                while audio_receiver.try_recv().is_ok() {}
                replace_snapshot(
                    snapshot,
                    WorkerSnapshot {
                        state: VoiceState::Idle,
                        ..WorkerSnapshot::default()
                    },
                );
            }
            Command::Shutdown => break,
            Command::Stop(_) | Command::Cancel(_) => {}
        }
    }
}

/// 一次会话共用，模型已加载完的推理链路：识别 → 标点 → 大模型整理。
struct PostPipeline {
    engine: Arc<crate::asr::AsrEngine>,

    punctuator: Option<crate::asr::Punctuator>,

    polisher: Option<crate::polish::TextPolisher>,
}

#[cfg(windows)]
fn finish(
    request: u64,
    opened: &mut Option<crate::audio::OpenedInput>,
    receiver: &mpsc::Receiver<Vec<f32>>,
    samples: &mut Vec<f32>,
    pipeline: &PostPipeline,
    snapshot: &Arc<Mutex<WorkerSnapshot>>,
) {
    use qingjian_platform::protocol::VoiceState;

    let Some(input) = opened.take() else {
        return;
    };
    let sample_rate = input.sample_rate;
    let channels = input.channels;
    drop(input.stream);
    while let Ok(chunk) = receiver.try_recv() {
        samples.extend_from_slice(&chunk);
    }
    let partial = current_partial(snapshot);
    replace_snapshot(
        snapshot,
        WorkerSnapshot {
            state: VoiceState::Recognizing,
            request: Some(request),
            partial,
            ..WorkerSnapshot::default()
        },
    );
    let result = crate::audio::to_mono_16k(samples, sample_rate, channels)
        .and_then(|mono| pipeline.engine.transcribe(&mono));
    samples.clear();
    match result {
        Ok(text) if crate::audio::is_meaningful(&text) => {
            let recognized = pipeline
                .punctuator
                .as_ref()
                .map_or_else(|| text.clone(), |value| value.add(&text));
            // 润色开时先公示“正在转化”这一态（候选窗显示草稿与大模型提示），结束后才一次性 Ready。
            if pipeline.polisher.is_some() {
                replace_snapshot(
                    snapshot,
                    WorkerSnapshot {
                        state: VoiceState::Polishing,
                        request: Some(request),
                        partial: Some(recognized.clone()),
                        ..WorkerSnapshot::default()
                    },
                );
            }
            let text = pipeline
                .polisher
                .as_ref()
                .and_then(|value| value.polish(&recognized))
                .unwrap_or(recognized);
            tracing::info!(request, chars = text.chars().count(), "语音识别完成");
            replace_snapshot(
                snapshot,
                WorkerSnapshot {
                    state: VoiceState::Ready,
                    request: Some(request),
                    partial: Some(text.clone()),
                    text: Some(text),
                    ..WorkerSnapshot::default()
                },
            );
        }
        Ok(_) => replace_snapshot(
            snapshot,
            WorkerSnapshot {
                state: VoiceState::Idle,
                // 让 Server 知道这不是模型仍在加载时的“尚未接单”空闲态，而是本次请求
                // 已经处理完、只是没有听清。Server 看见匹配的 request 后才能结束会话并允许下一次录音。
                request: Some(request),
                message: Some("没有听清".into()),
                ..WorkerSnapshot::default()
            },
        ),
        Err(error) => {
            tracing::error!(%error, request, "语音识别失败");
            replace_snapshot(
                snapshot,
                WorkerSnapshot {
                    state: VoiceState::Failed,
                    request: Some(request),
                    message: Some("语音识别失败".into()),
                    ..WorkerSnapshot::default()
                },
            );
        }
    }
}

#[cfg(windows)]
fn current_request(snapshot: &Arc<Mutex<WorkerSnapshot>>) -> Option<u64> {
    snapshot
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .request
}

#[cfg(windows)]
fn current_partial(snapshot: &Arc<Mutex<WorkerSnapshot>>) -> Option<String> {
    snapshot
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .partial
        .clone()
}

#[cfg(windows)]
fn update_level(snapshot: &Arc<Mutex<WorkerSnapshot>>, level: u16) {
    let mut value = snapshot
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if value.state == VoiceState::Recording {
        value.level = level;
    }
}

#[cfg(windows)]
fn update_partial(snapshot: &Arc<Mutex<WorkerSnapshot>>, request: u64, text: String) {
    if !crate::audio::is_meaningful(&text) {
        return;
    }
    let mut value = snapshot
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if value.state == VoiceState::Recording && value.request == Some(request) {
        value.partial = Some(text);
    }
}

#[cfg(windows)]
fn wait_for_shutdown(receiver: mpsc::Receiver<Command>) {
    while let Ok(command) = receiver.recv() {
        if command == Command::Shutdown {
            break;
        }
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::Controller;
    use crate::WorkerConfig;

    #[test]
    fn unsupported_platform_fails_without_panicking() {
        let controller = Controller::new(WorkerConfig::default());
        assert_eq!(
            controller.snapshot().state,
            qingjian_platform::protocol::VoiceState::Failed
        );
    }
}
