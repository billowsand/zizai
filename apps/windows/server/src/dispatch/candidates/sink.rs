//! 候选窗口的输出端。

use qingjian_platform::ColorScheme;
use qingjian_platform::protocol::{Frame, ScreenRect, VoiceState};

/// 候选窗口切到语音态时需要的一帧数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceView {
    pub state: VoiceState,

    pub level: u16,

    pub text: Option<String>,

    pub color_scheme: ColorScheme,
}

/// Router 只产出帧，画交给它；Windows 上由 UI 线程实现。
pub trait CandidateSink: Send {
    /// 把候选窗口摆到 `rect`（组句范围的屏幕矩形）下方并按 `frame` 重绘。
    fn show(&self, frame: Frame, rect: ScreenRect);

    /// 复用同一个候选窗口，在光标旁切成语音面板。
    fn show_voice(&self, _view: VoiceView, _rect: ScreenRect) {}

    fn hide(&self);

    /// 换候选窗口字体：装上时与配置热加载后调，只在设置变了时调。
    fn set_font(&self, font: String);
}

/// 不画候选窗口的空实现。
pub struct NoopSink;

impl CandidateSink for NoopSink {
    fn show(&self, _frame: Frame, _rect: ScreenRect) {}

    fn show_voice(&self, _view: VoiceView, _rect: ScreenRect) {}

    fn hide(&self) {}

    fn set_font(&self, _font: String) {}
}
