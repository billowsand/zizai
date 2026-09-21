//! Server 随状态同步下发给 TSF 的语音快照。

use serde::{Deserialize, Serialize};

use crate::VoiceTrigger;

use super::{VoiceDelivery, VoiceState};

/// 语音快捷键、运行状态与可选交付。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoiceSync {
    /// 功能是否可用；关闭时为 `false`。
    pub enabled: bool,

    /// TSF 该拦的物理键。
    pub trigger: VoiceTrigger,

    /// 当前运行阶段。
    pub state: VoiceState,

    /// 录音电平，`0..=1000`；非录音阶段为 0。
    pub level: u16,

    /// 录音过程中的临时转写，不参与最终上屏。
    pub partial: Option<String>,

    /// 只发给绑定会话的待上屏结果。
    pub delivery: Option<VoiceDelivery>,

    /// 面向用户的短错误；不含识别正文。
    pub message: Option<String>,
}

impl VoiceSync {
    /// 录音、识别、润色或等待确认期间需要 80 ms 同步。
    /// 漏掉任何一个进行中的阶段，TSF 那边就会当语音已经结束：降回 320 ms 同步、Esc 不再取消，
    /// 再按一次快捷键还会被当成新的开始。
    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            VoiceState::Recording
                | VoiceState::Recognizing
                | VoiceState::Polishing
                | VoiceState::Ready
        )
    }
}
