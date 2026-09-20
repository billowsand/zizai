//! 语音 Worker 错误。

/// 采音、重采样或识别错误；对外只传不含正文的短消息。
#[derive(Debug, thiserror::Error)]
pub enum VoiceError {
    #[error("audio input error: {0}")]
    Audio(String),

    #[error("audio resample error: {0}")]
    Resample(String),

    #[error("speech recognition error: {0}")]
    Recognition(String),

    #[error("worker command channel closed")]
    Closed,
}
