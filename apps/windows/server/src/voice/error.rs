//! 语音 Worker 进程边界错误。

/// Server 与语音 Worker 通信失败。
#[derive(Debug, thiserror::Error)]
pub enum VoiceBackendError {
    #[error("voice worker I/O: {0}")]
    Io(#[from] std::io::Error),

    #[error("voice worker protocol: {0}")]
    Protocol(#[from] qingjian_platform::protocol::CodecError),

    #[error("voice worker closed")]
    Closed,

    #[error("voice worker rejected request: {0}")]
    Rejected(String),
}
