//! Server 发给语音 Worker 的请求。

use serde::{Deserialize, Serialize};

use crate::WorkerConfig;

/// stdio 私有协议请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkerRequest {
    Configure(Box<WorkerConfig>),
    Start { request: u64 },
    Stop { request: u64 },
    Cancel { request: u64 },
    Snapshot,
    Shutdown,
}
