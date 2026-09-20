//! Server 可替换的语音 Worker 边界。

use qingjian_voice::WorkerSnapshot;

use super::VoiceBackendError;

/// 协调器只依赖这组同步小操作，进程和测试替身共用。
pub trait VoiceBackend: Send {
    fn start(&mut self, request: u64) -> Result<(), VoiceBackendError>;

    fn stop(&mut self, request: u64) -> Result<(), VoiceBackendError>;

    fn cancel(&mut self, request: u64) -> Result<(), VoiceBackendError>;

    fn snapshot(&mut self) -> Result<WorkerSnapshot, VoiceBackendError>;
}
