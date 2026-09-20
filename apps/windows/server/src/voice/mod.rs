//! 语音 Worker 进程与会话协调。

mod backend;
mod coordinator;
mod error;
mod process;

pub use backend::VoiceBackend;
pub use coordinator::VoiceCoordinator;
pub use error::VoiceBackendError;
pub use process::ProcessVoiceBackend;
