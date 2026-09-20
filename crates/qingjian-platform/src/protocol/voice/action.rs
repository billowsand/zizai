//! TSF 发给 Server 的语音按键动作。

use serde::{Deserialize, Serialize};

/// 一次语音输入控制动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoiceAction {
    /// 第一次触发语音开关键。
    Start,

    /// 再次触发语音开关键。
    Stop,

    /// 焦点或输入顺序变化，取消未交付的结果。
    Cancel,
}
