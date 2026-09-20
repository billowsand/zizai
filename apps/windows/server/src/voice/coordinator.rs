//! 把按键会话、Worker 状态与可靠交付串成一个状态机。

use std::time::{Duration, Instant};

use qingjian_platform::VoiceTrigger;
use qingjian_platform::protocol::{SessionId, VoiceAction, VoiceDelivery, VoiceState, VoiceSync};

use super::VoiceBackend;

/// 本地短句通常数秒内完成；超过这个上限说明 Worker 或原生推理卡住，必须恢复可再次录音。
const RECOGNITION_TIMEOUT: Duration = Duration::from_secs(45);

/// Server 内唯一的语音输入协调器。
pub struct VoiceCoordinator {
    enabled: bool,

    trigger: VoiceTrigger,

    backend: Option<Box<dyn VoiceBackend>>,

    session: Option<SessionId>,

    request: Option<u64>,

    next_request: u64,

    state: VoiceState,

    state_since: Instant,

    delivery: Option<VoiceDelivery>,

    level: u16,

    partial: Option<String>,

    message: Option<String>,
}

impl Default for VoiceCoordinator {
    fn default() -> Self {
        Self {
            enabled: false,
            trigger: VoiceTrigger::Off,
            backend: None,
            session: None,
            request: None,
            next_request: 1,
            state: VoiceState::Disabled,
            state_since: Instant::now(),
            delivery: None,
            level: 0,
            partial: None,
            message: None,
        }
    }
}

impl VoiceCoordinator {
    pub fn owns(&self, session: SessionId) -> bool {
        self.session == Some(session)
    }

    /// 语音窗口必须持续可见的阶段。Router 用它挡住来自普通候选生命周期的迟到收窗动作。
    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            VoiceState::Recording | VoiceState::Recognizing | VoiceState::Ready
        )
    }

    pub fn disable(&mut self) {
        self.cancel();
        self.enabled = false;
        self.trigger = VoiceTrigger::Off;
        self.backend = None;
        self.set_state(VoiceState::Disabled);
        self.level = 0;
        self.partial = None;
        self.message = None;
    }

    pub fn configure(&mut self, trigger: VoiceTrigger, backend: Box<dyn VoiceBackend>) {
        self.enabled = trigger != VoiceTrigger::Off;
        self.trigger = trigger;
        self.backend = Some(backend);
        self.set_state(if self.enabled {
            VoiceState::Loading
        } else {
            VoiceState::Disabled
        });
        self.level = 0;
        self.partial = None;
        self.message = None;
    }

    pub fn configure_failure(&mut self, trigger: VoiceTrigger, message: String) {
        self.enabled = trigger != VoiceTrigger::Off;
        self.trigger = trigger;
        self.backend = None;
        self.set_state(if self.enabled {
            VoiceState::Failed
        } else {
            VoiceState::Disabled
        });
        self.level = 0;
        self.partial = None;
        self.message = self.enabled.then_some(message);
    }

    pub fn handle(&mut self, session: SessionId, action: VoiceAction) {
        if !self.enabled {
            return;
        }
        match action {
            VoiceAction::Start => self.start(session),
            VoiceAction::Stop => self.stop(session),
            VoiceAction::Cancel => self.cancel_for(session),
        }
    }

    pub fn cancel_for(&mut self, session: SessionId) {
        if self.session == Some(session) {
            self.cancel();
        }
    }

    pub fn cancel(&mut self) {
        if let (Some(backend), Some(request)) = (self.backend.as_mut(), self.request)
            && let Err(error) = backend.cancel(request)
        {
            tracing::warn!(%error, request, "取消语音输入失败");
        }
        self.session = None;
        self.request = None;
        self.delivery = None;
        self.set_state(if self.enabled {
            VoiceState::Idle
        } else {
            VoiceState::Disabled
        });
        self.level = 0;
        self.partial = None;
        self.message = None;
    }

    pub fn ack(&mut self, session: SessionId, request: u64) {
        if self.session == Some(session) && self.request == Some(request) {
            self.cancel();
        }
    }

    pub fn sync(&mut self, session: SessionId) -> VoiceSync {
        self.poll_backend();
        VoiceSync {
            enabled: self.enabled,
            trigger: self.trigger,
            state: self.state,
            level: self.level,
            partial: (self.session == Some(session))
                .then(|| self.partial.clone())
                .flatten(),
            delivery: (self.session == Some(session))
                .then(|| self.delivery.clone())
                .flatten(),
            message: self.message.clone(),
        }
    }

    fn start(&mut self, session: SessionId) {
        if self.backend.is_none() {
            self.session = Some(session);
            self.state = VoiceState::Failed;
            return;
        }
        if self.session == Some(session)
            && matches!(self.state, VoiceState::Recording | VoiceState::Recognizing)
        {
            return;
        }
        self.cancel();
        let request = self.next_request;
        self.next_request = self.next_request.wrapping_add(1).max(1);
        self.session = Some(session);
        self.request = Some(request);
        self.delivery = None;
        self.level = 0;
        self.partial = None;
        self.message = None;
        let Some(backend) = self.backend.as_mut() else {
            return;
        };
        let result = backend.start(request);
        match result {
            Ok(()) => self.set_state(VoiceState::Recording),
            Err(error) => self.fail(error.to_string()),
        }
    }

    fn stop(&mut self, session: SessionId) {
        if self.session != Some(session) || self.state != VoiceState::Recording {
            return;
        }
        let Some(request) = self.request else {
            return;
        };
        let Some(backend) = self.backend.as_mut() else {
            return;
        };
        let result = backend.stop(request);
        match result {
            Ok(()) => self.set_state(VoiceState::Recognizing),
            Err(error) => self.fail(error.to_string()),
        }
    }

    fn poll_backend(&mut self) {
        if !self.enabled {
            return;
        }
        if self.state == VoiceState::Recognizing
            && self.state_since.elapsed() >= RECOGNITION_TIMEOUT
        {
            self.timeout_recognition();
            return;
        }
        let Some(backend) = self.backend.as_mut() else {
            return;
        };
        let snapshot = match backend.snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.fail(error.to_string());
                return;
            }
        };
        // Start / Stop 已投进 Worker，但它可能还在加载模型或尚未取到下一条命令；
        // 这段间隙不能把 Server 的前进状态倒回去，否则快速按下再松开会丢掉 Stop。
        if self.request.is_some()
            && (snapshot.request.is_none()
                || (self.state == VoiceState::Recognizing
                    && snapshot.state == VoiceState::Recording))
        {
            return;
        }
        if snapshot.request.is_some() && snapshot.request != self.request {
            return;
        }
        self.set_state(snapshot.state);
        self.level = snapshot.level;
        self.partial = snapshot.partial;
        self.message = snapshot.message;
        self.delivery = match (snapshot.request, snapshot.text) {
            (Some(request), Some(text)) if self.request == Some(request) => {
                Some(VoiceDelivery { request, text })
            }
            _ => None,
        };
        if self.state == VoiceState::Idle && self.delivery.is_none() {
            self.session = None;
            self.request = None;
        }
    }

    fn fail(&mut self, message: String) {
        tracing::error!(%message, "语音输入不可用");
        self.set_state(VoiceState::Failed);
        self.level = 0;
        self.partial = None;
        self.message = Some(message);
        self.delivery = None;
    }

    fn timeout_recognition(&mut self) {
        let request = self.request.take();
        if let (Some(backend), Some(request)) = (self.backend.as_mut(), request)
            && let Err(error) = backend.cancel(request)
        {
            tracing::warn!(%error, request, "识别超时后取消 Worker 请求失败");
        }
        self.set_state(VoiceState::Failed);
        self.level = 0;
        self.partial = None;
        self.delivery = None;
        self.message = Some("识别超时，请重试".into());
        tracing::warn!(?request, "语音识别超时，已恢复快捷键");
    }

    fn set_state(&mut self, state: VoiceState) {
        if self.state == state {
            return;
        }
        let previous = self.state;
        self.state = state;
        self.state_since = Instant::now();
        tracing::info!(from = ?previous, to = ?state, request = ?self.request, "语音状态推进");
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use qingjian_platform::VoiceTrigger;
    use qingjian_platform::protocol::{SessionId, VoiceAction, VoiceState};
    use qingjian_voice::WorkerSnapshot;

    use super::{RECOGNITION_TIMEOUT, VoiceCoordinator};
    use crate::voice::{VoiceBackend, VoiceBackendError};

    #[derive(Default)]
    struct FakeBackend {
        snapshot: Arc<Mutex<WorkerSnapshot>>,
    }

    impl VoiceBackend for FakeBackend {
        fn start(&mut self, request: u64) -> Result<(), VoiceBackendError> {
            *self.snapshot.lock().unwrap() = WorkerSnapshot {
                state: VoiceState::Recording,
                request: Some(request),
                ..WorkerSnapshot::default()
            };
            Ok(())
        }

        fn stop(&mut self, request: u64) -> Result<(), VoiceBackendError> {
            *self.snapshot.lock().unwrap() = WorkerSnapshot {
                state: VoiceState::Recognizing,
                request: Some(request),
                ..WorkerSnapshot::default()
            };
            Ok(())
        }

        fn cancel(&mut self, _request: u64) -> Result<(), VoiceBackendError> {
            *self.snapshot.lock().unwrap() = WorkerSnapshot {
                state: VoiceState::Idle,
                ..WorkerSnapshot::default()
            };
            Ok(())
        }

        fn snapshot(&mut self) -> Result<WorkerSnapshot, VoiceBackendError> {
            Ok(self.snapshot.lock().unwrap().clone())
        }
    }

    #[test]
    fn result_stays_bound_and_repeats_until_matching_ack() {
        let shared = Arc::new(Mutex::new(WorkerSnapshot::default()));
        let backend = FakeBackend {
            snapshot: shared.clone(),
        };
        let first = SessionId(1);
        let other = SessionId(2);
        let mut voice = VoiceCoordinator::default();
        voice.configure(VoiceTrigger::RightAlt, Box::new(backend));

        voice.handle(first, VoiceAction::Start);
        voice.handle(first, VoiceAction::Stop);
        *shared.lock().unwrap() = WorkerSnapshot {
            state: VoiceState::Ready,
            request: Some(1),
            text: Some("测试文本".into()),
            ..WorkerSnapshot::default()
        };

        assert!(voice.sync(other).delivery.is_none());
        let delivery = voice.sync(first).delivery.expect("绑定会话收到结果");
        assert_eq!(delivery.text, "测试文本");
        assert_eq!(voice.sync(first).delivery, Some(delivery.clone()));

        voice.ack(other, delivery.request);
        assert!(voice.sync(first).delivery.is_some());
        voice.ack(first, delivery.request);
        assert!(voice.sync(first).delivery.is_none());
        assert_eq!(voice.sync(first).state, VoiceState::Idle);
    }

    #[test]
    fn cancel_drops_a_late_result() {
        let shared = Arc::new(Mutex::new(WorkerSnapshot::default()));
        let backend = FakeBackend {
            snapshot: shared.clone(),
        };
        let session = SessionId(7);
        let mut voice = VoiceCoordinator::default();
        voice.configure(VoiceTrigger::RightAlt, Box::new(backend));
        voice.handle(session, VoiceAction::Start);
        voice.cancel_for(session);
        *shared.lock().unwrap() = WorkerSnapshot {
            state: VoiceState::Ready,
            request: Some(1),
            text: Some("不应交付".into()),
            ..WorkerSnapshot::default()
        };
        assert!(voice.sync(session).delivery.is_none());
    }

    struct LoadingBackend {
        calls: Arc<Mutex<Vec<&'static str>>>,
    }

    impl VoiceBackend for LoadingBackend {
        fn start(&mut self, _request: u64) -> Result<(), VoiceBackendError> {
            self.calls.lock().unwrap().push("start");
            Ok(())
        }

        fn stop(&mut self, _request: u64) -> Result<(), VoiceBackendError> {
            self.calls.lock().unwrap().push("stop");
            Ok(())
        }

        fn cancel(&mut self, _request: u64) -> Result<(), VoiceBackendError> {
            Ok(())
        }

        fn snapshot(&mut self) -> Result<WorkerSnapshot, VoiceBackendError> {
            Ok(WorkerSnapshot {
                state: VoiceState::Loading,
                ..WorkerSnapshot::default()
            })
        }
    }

    #[test]
    fn quick_release_is_not_lost_while_worker_is_loading() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut voice = VoiceCoordinator::default();
        voice.configure(
            VoiceTrigger::RightAlt,
            Box::new(LoadingBackend {
                calls: calls.clone(),
            }),
        );
        let session = SessionId(11);
        voice.handle(session, VoiceAction::Start);
        assert_eq!(voice.sync(session).state, VoiceState::Recording);
        voice.handle(session, VoiceAction::Stop);
        assert_eq!(voice.sync(session).state, VoiceState::Recognizing);
        assert_eq!(*calls.lock().unwrap(), vec!["start", "stop"]);
    }

    #[test]
    fn empty_result_with_matching_request_returns_to_idle() {
        let shared = Arc::new(Mutex::new(WorkerSnapshot::default()));
        let backend = FakeBackend {
            snapshot: shared.clone(),
        };
        let session = SessionId(12);
        let mut voice = VoiceCoordinator::default();
        voice.configure(VoiceTrigger::RightAlt, Box::new(backend));

        voice.handle(session, VoiceAction::Start);
        voice.handle(session, VoiceAction::Stop);
        *shared.lock().unwrap() = WorkerSnapshot {
            state: VoiceState::Idle,
            request: Some(1),
            message: Some("没有听清".into()),
            ..WorkerSnapshot::default()
        };

        let sync = voice.sync(session);
        assert_eq!(sync.state, VoiceState::Idle);
        assert_eq!(sync.message.as_deref(), Some("没有听清"));
        assert!(!voice.owns(session));

        voice.handle(session, VoiceAction::Start);
        assert_eq!(voice.sync(session).state, VoiceState::Recording);
    }

    #[test]
    fn recognition_timeout_recovers_for_the_next_recording() {
        let shared = Arc::new(Mutex::new(WorkerSnapshot::default()));
        let backend = FakeBackend {
            snapshot: shared.clone(),
        };
        let session = SessionId(13);
        let mut voice = VoiceCoordinator::default();
        voice.configure(VoiceTrigger::RightAlt, Box::new(backend));
        voice.handle(session, VoiceAction::Start);
        voice.handle(session, VoiceAction::Stop);
        voice.state_since = Instant::now() - RECOGNITION_TIMEOUT - Duration::from_millis(1);

        let timed_out = voice.sync(session);
        assert_eq!(timed_out.state, VoiceState::Failed);
        assert_eq!(timed_out.message.as_deref(), Some("识别超时，请重试"));

        voice.handle(session, VoiceAction::Start);
        assert_eq!(voice.sync(session).state, VoiceState::Recording);
    }
}
