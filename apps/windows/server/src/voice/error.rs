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

    /// 识别模型不在盘上（安装包没带语音模型）：不起 Worker。
    #[error("voice model missing: {}", .0.display())]
    ModelMissing(std::path::PathBuf),
}

impl VoiceBackendError {
    /// 按语音键时给用户看的一句话。
    pub fn notice(&self) -> &'static str {
        match self {
            Self::ModelMissing(_) => "未安装语音模型",
            _ => "语音工作进程启动失败",
        }
    }
}
