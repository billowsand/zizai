//! 语音输入状态。

use serde::{Deserialize, Serialize};

/// Worker 当前所处阶段。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoiceState {
    #[default]
    Disabled,
    Loading,
    Idle,
    Recording,
    Recognizing,

    /// 识别完成后调大模型润色的阶段（档位开时才有）。
    Polishing,
    Ready,
    Failed,
}
