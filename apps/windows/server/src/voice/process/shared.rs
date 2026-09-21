//! Worker 线程与 Router 线程之间交换快照的格子。

use std::sync::{Arc, Mutex};

use qingjian_platform::protocol::VoiceState;
use qingjian_voice::WorkerSnapshot;

/// 读写都只碰一次锁，锁里不做任何 I/O：Router 线程永远不会因为 Worker 慢而停住。
#[derive(Clone, Default)]
pub(super) struct SharedSnapshot {
    cell: Arc<Mutex<WorkerSnapshot>>,
}

impl SharedSnapshot {
    pub(super) fn read(&self) -> WorkerSnapshot {
        self.lock().clone()
    }

    pub(super) fn state(&self) -> VoiceState {
        self.lock().state
    }

    pub(super) fn publish(&self, snapshot: WorkerSnapshot) {
        *self.lock() = snapshot;
    }

    /// Worker 进程没了或起不来：换成带原因的 `Failed`，别让上一次的「录音中」留在格子里
    /// 把 Server 的状态机倒回去。
    pub(super) fn publish_failure(&self, message: &str) {
        self.publish(WorkerSnapshot {
            state: VoiceState::Failed,
            message: Some(message.to_owned()),
            ..WorkerSnapshot::default()
        });
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, WorkerSnapshot> {
        self.cell
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
