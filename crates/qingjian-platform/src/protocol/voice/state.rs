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
    Ready,
    Failed,
}
