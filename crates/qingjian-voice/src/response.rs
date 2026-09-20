//! 语音 Worker 回给 Server 的响应。

use serde::{Deserialize, Serialize};

use crate::WorkerSnapshot;

/// stdio 私有协议响应。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkerResponse {
    Ok,
    Snapshot(WorkerSnapshot),
    Error(String),
}
