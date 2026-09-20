//! 候选窗口内的语音输入展示模型。

/// 语音面板的阶段色；只表达界面语义，不承载业务状态机。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VoiceTone {
    #[default]
    Listening,

    Working,

    Success,

    Warning,
}

/// 一帧语音面板；壳负责积累电平历史，渲染器只负责绘制。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VoiceFrame {
    /// 最近的录音电平，范围 `0..=1000`。
    pub levels: Vec<u16>,

    /// 波形右侧的阶段文字，例如“正在听”或“正在识别”。
    pub title: String,

    /// 阶段点与波形使用的语义色。
    pub tone: VoiceTone,

    /// 本轮录音时长；录音结束后冻结，例如 `00:08`。
    pub elapsed: Option<String>,

    /// 下方临时/最终转写；还没有文字时显示 `hint`。
    pub transcript: Option<String>,

    /// 尚无转写时的次要提示。
    pub hint: String,
}
