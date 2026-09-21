//! 按消息类型分派：会话开关、按键、轮询、失焦上屏、选区 / 光标矩形 / 中英模式的通知。

use qingjian_platform::protocol::{
    ClientMessage, Frame, KeyEvent, KeyOutcome, PROTOCOL_VERSION, ServerMessage, SessionId,
};

use super::Router;
use super::key::Effect;
use super::session::SessionInfo;

impl Router {
    pub(super) fn dispatch(&mut self, message: ClientMessage) -> Option<ServerMessage> {
        match message {
            ClientMessage::OpenSession {
                session,
                app,
                protocol,
            } => {
                tracing::debug!(?session, app, protocol, "会话打开");
                if protocol != PROTOCOL_VERSION {
                    tracing::warn!(
                        ?session,
                        app,
                        dll = protocol,
                        server = PROTOCOL_VERSION,
                        "DLL 与 Server 的协议版本不同，照常服务：\
                         升级安装后这个应用还没重启、用着旧 DLL，重启它就换成新的"
                    );
                }
                // 同一会话重开（DLL 断线重连）：从干净状态起。
                if self.focused == Some(session) {
                    self.reset_composition();
                    self.focused = None;
                }
                self.sessions.insert(
                    session,
                    SessionInfo {
                        app,
                        private: false,
                        protocol,
                    },
                );
                None
            }
            ClientMessage::Key { session, event } => {
                self.voice.cancel();
                Some(self.handle_key(session, event))
            }
            ClientMessage::Poll { session } => Some(self.handle_poll(session)),
            ClientMessage::Commit { session } => {
                let text = self.commit_raw_for(session);
                tracing::debug!(?session, ?text, "焦点离开，结束组句");
                Some(ServerMessage::Committed { session, text })
            }
            ClientMessage::Surrounding { session, text } => {
                tracing::trace!(?session, chars = text.chars().count(), "收到光标前文");
                self.set_surrounding(session, text);
                None
            }
            ClientMessage::Privacy { session, private } => {
                tracing::debug!(?session, private, "输入框私密状态");
                self.set_privacy(session, private);
                None
            }
            // 本壳不带「翻译选中文字」：从不发 `RequestSelection`，收到选区回复直接丢。
            ClientMessage::Selection { .. } => None,
            ClientMessage::PositionCandidates { session, rect } => {
                self.position_candidates(session, rect);
                None
            }
            ClientMessage::HideCandidates { session } => {
                // 组句在 DLL 侧结束（应用终止组句）：只收窗口；缓冲留给下一键的 Commit 清。
                if self.focused == Some(session) {
                    // 语音条复用候选窗。语音开始后，DLL 可能补发上一轮组句的收窗通知；
                    // 这条旧通知不能盖过正在进行的语音会话。
                    if self.voice.owns(session) {
                        tracing::debug!(?session, "语音进行中，忽略候选窗隐藏请求");
                        let voice = self.voice.sync(session);
                        self.reconcile_voice(session, &voice);
                    } else {
                        self.hide_candidate_window();
                    }
                }
                None
            }
            ClientMessage::ModeChanged { session, english } => {
                tracing::debug!(?session, english, "中英模式");
                self.handle_mode_changed(session, english);
                None
            }
            ClientMessage::SyncMode { session } => {
                let voice = self.voice.sync(session);
                self.reconcile_voice(session, &voice);
                Some(ServerMessage::ModeSync {
                    session,
                    english: self.take_pending_mode(session),
                    voice,
                })
            }
            ClientMessage::Voice { session, action } => {
                tracing::info!(?session, ?action, "收到语音快捷键动作");
                if action == qingjian_platform::protocol::VoiceAction::Start {
                    self.ensure_focus(session);
                    self.voice_notice_until = None;
                    self.voice_notice_seen = false;
                }
                self.voice.handle(session, action);
                None
            }
            ClientMessage::VoiceAck { session, request } => {
                self.voice.ack(session, request);
                if self.voice_visible {
                    self.hide_candidate_window();
                }
                self.voice_notice_until = None;
                self.voice_notice_seen = false;
                None
            }
            ClientMessage::ImeSwitched { session } => {
                tracing::debug!(?session, "切成了别的输入法");
                self.voice.cancel_for(session);
                self.handle_ime_switched();
                None
            }
            ClientMessage::CloseSession { session } => {
                self.voice.cancel_for(session);
                self.forget_status_session(session);
                self.sessions.remove(&session);
                if self.focused == Some(session) {
                    self.reset_composition();
                    self.focused = None;
                }
                self.flush_learning();
                tracing::debug!(?session, "会话关闭");
                None
            }
        }
    }

    fn handle_key(&mut self, session: SessionId, event: KeyEvent) -> ServerMessage {
        self.ensure_focus(session);
        self.claim_status_session(session);
        self.notice = None;
        let (commit, outcome) = match self.apply_key(&event) {
            Effect::Changed(commit) => {
                self.recompose();
                (commit, KeyOutcome::Consumed)
            }
            Effect::Navigated => (None, KeyOutcome::Consumed),
            Effect::Passthrough => (None, KeyOutcome::Passthrough),
        };
        let frame = self.current_frame();
        self.reconcile_candidates(&frame);
        ServerMessage::KeyResult {
            session,
            outcome,
            commit,
            frame,
        }
    }

    /// 轮询：聚焦会话回最新一帧，否则回空帧。本地整句模型的重排也借这个节拍收。
    fn handle_poll(&mut self, session: SessionId) -> ServerMessage {
        self.tick();
        let frame = if self.focused == Some(session) {
            let frame = self.current_frame();
            self.reconcile_candidates(&frame);
            frame
        } else {
            Frame::default()
        };
        ServerMessage::Update { session, frame }
    }
}
