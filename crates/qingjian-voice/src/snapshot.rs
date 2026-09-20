//! Worker 当前状态快照。

use qingjian_platform::protocol::VoiceState;
use serde::{Deserialize, Serialize};

/// Server 轮询的只读状态；正文只在 `Ready` 时出现。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkerSnapshot {
    pub state: VoiceState,

    pub request: Option<u64>,

    /// 当前录音电平，`0..=1000`；整数便于跨进程稳定序列化。
    pub level: u16,

    /// 录音期间的临时转写；最终上屏仍只认 `text`。
    pub partial: Option<String>,

    pub text: Option<String>,

    pub message: Option<String>,
}
