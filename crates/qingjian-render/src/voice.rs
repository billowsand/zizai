//! 候选窗口内的语音输入展示模型。

/// 一帧语音面板；壳负责积累电平历史，渲染器只负责绘制。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VoiceFrame {
    /// 最近的录音电平，范围 `0..=1000`。
    pub levels: Vec<u16>,

    /// 波形右侧的阶段文字，例如“正在听”或“正在识别”。
    pub title: String,

    /// 下方临时/最终转写；还没有文字时显示 `hint`。
    pub transcript: Option<String>,

    /// 尚无转写时的次要提示。
    pub hint: String,
}
