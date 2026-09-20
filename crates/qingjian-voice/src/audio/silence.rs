//! 基于已经归一化的界面电平做轻量静音检测，不在采音回调里执行阻塞工作。

use std::time::Duration;

/// 与候选窗“已经听到声音”的阈值保持一致，避免界面仍提示安静时 Worker 却自动结束。
const SPEECH_LEVEL: u16 = 24;

/// 过滤敲键、碰麦克风等瞬时噪声；需要至少这一段连续有效声音才算开始说话。
const MIN_SPEECH: Duration = Duration::from_millis(160);

pub(crate) struct SilenceDetector {
    stop_after: Option<Duration>,
    active_for: Duration,
    silent_for: Duration,
    heard_speech: bool,
}

impl SilenceDetector {
    pub(crate) fn new(stop_after_ms: u64) -> Self {
        Self {
            stop_after: (stop_after_ms > 0).then(|| Duration::from_millis(stop_after_ms)),
            active_for: Duration::ZERO,
            silent_for: Duration::ZERO,
            heard_speech: false,
        }
    }

    pub(crate) fn reset(&mut self) {
        self.active_for = Duration::ZERO;
        self.silent_for = Duration::ZERO;
        self.heard_speech = false;
    }

    /// 接收一块音频的视觉电平与实际时长；返回 `true` 时应结束当前录音。
    pub(crate) fn observe(&mut self, level: u16, chunk_duration: Duration) -> bool {
        let Some(stop_after) = self.stop_after else {
            return false;
        };
        if level >= SPEECH_LEVEL {
            self.active_for = self.active_for.saturating_add(chunk_duration);
            self.silent_for = Duration::ZERO;
            if self.active_for >= MIN_SPEECH {
                self.heard_speech = true;
            }
        } else if self.heard_speech {
            self.silent_for = self.silent_for.saturating_add(chunk_duration);
        } else {
            self.active_for = Duration::ZERO;
        }
        self.heard_speech && self.silent_for >= stop_after
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::SilenceDetector;

    #[test]
    fn disabled_detector_never_finishes() {
        let mut detector = SilenceDetector::new(0);
        assert!(!detector.observe(800, Duration::from_secs(1)));
        assert!(!detector.observe(0, Duration::from_secs(5)));
    }

    #[test]
    fn ignores_click_and_waits_for_silence_after_real_speech() {
        let mut detector = SilenceDetector::new(1_200);
        assert!(!detector.observe(800, Duration::from_millis(40)));
        assert!(!detector.observe(0, Duration::from_secs(2)));

        assert!(!detector.observe(500, Duration::from_millis(80)));
        assert!(!detector.observe(500, Duration::from_millis(80)));
        assert!(!detector.observe(0, Duration::from_millis(1_199)));
        assert!(detector.observe(0, Duration::from_millis(1)));
    }

    #[test]
    fn resumed_speech_restarts_the_silence_window() {
        let mut detector = SilenceDetector::new(1_200);
        assert!(!detector.observe(500, Duration::from_millis(200)));
        assert!(!detector.observe(0, Duration::from_millis(900)));
        assert!(!detector.observe(500, Duration::from_millis(100)));
        assert!(!detector.observe(0, Duration::from_millis(1_100)));
        assert!(detector.observe(0, Duration::from_millis(100)));
    }
}
