//! 候选窗口输出：Router 只产出 [`Frame`] 与光标矩形，交给 [`CandidateSink`] 去画；帧没变就不重画。

mod sink;

use std::borrow::Cow;

use qingjian_platform::protocol::{Frame, ScreenRect, SessionId, VoiceState, VoiceSync};

pub use self::sink::{CandidateSink, NoopSink, VoiceView};
use super::Router;

impl Router {
    /// 窗口那份帧：`[general] preedit` 说拼音只放行内时，把拼音行摘掉（候选还照画）。
    /// 摘完变空帧的（只有拼音、没有候选）由 [`reconcile_candidates`](Self::reconcile_candidates) 收窗口。
    fn window_frame<'a>(&self, frame: &'a Frame) -> Cow<'a, Frame> {
        if self.config.preedit.in_window() {
            return Cow::Borrowed(frame);
        }
        let mut frame = frame.clone();
        frame.preedit.clear();
        frame.cursor = 0;
        Cow::Owned(frame)
    }

    /// 空帧收窗口；非空且已知光标矩形就重绘；还没收到矩形（组句刚起）先不显示，免得在旧位置闪一下。
    pub(super) fn reconcile_candidates(&mut self, frame: &Frame) {
        // 语音条与普通候选共用一个窗口。语音开始前的组句可能还会留下最后一次空帧轮询；
        // 若继续按普通候选处理，会把刚显示的语音条误收起，并连光标锚点一起清掉。
        if let Some(session) = self.focused.filter(|session| self.voice.owns(*session)) {
            let sync = self.voice.sync(session);
            self.reconcile_voice(session, &sync);
            return;
        }
        let shown = self.window_frame(frame);
        let frame = shown.as_ref();
        if frame.is_empty() {
            self.engine.note_displayed(std::iter::empty());
            self.hide_candidate_window();
        } else if let Some(rect) = self.last_rect {
            let unchanged = matches!(&self.last_shown, Some((f, r)) if f == frame && *r == rect);
            if !unchanged {
                // 词汇记录的「看到轮次」按真正显示的页算，与 macOS 壳对齐。
                self.engine.note_displayed(frame.candidates.items.iter());
                self.candidates.show(frame.clone(), rect);
                self.last_shown = Some((frame.clone(), rect));
            }
        }
    }

    pub(super) fn hide_candidate_window(&mut self) {
        // 候选窗口与语音条共用一个 HWND。普通组句的 Commit / HideCandidates、会话重连，
        // 以及上一轮迟到的 ACK 都可能在新一轮语音已经显示后才到；活跃语音结束前，
        // 所有通用收窗入口都只能由语音状态机决定何时真正关闭。
        if self.voice.is_active() {
            tracing::debug!("语音处于活跃状态，忽略通用候选窗隐藏请求");
            return;
        }
        self.last_rect = None;
        self.last_shown = None;
        self.voice_visible = false;
        self.candidates.hide();
    }

    pub(super) fn position_candidates(&mut self, session: SessionId, rect: ScreenRect) {
        if self.focused != Some(session) {
            return;
        }
        self.last_rect = Some(rect);
        if self.voice.owns(session) {
            let sync = self.voice.sync(session);
            self.reconcile_voice(session, &sync);
            return;
        }
        let frame = self.current_frame();
        self.reconcile_candidates(&frame);
    }

    /// 把候选窗原地切成语音态；不新建第二个浮窗。
    pub(super) fn reconcile_voice(&mut self, session: SessionId, sync: &VoiceSync) {
        let active = self.voice.owns(session)
            && matches!(
                sync.state,
                VoiceState::Recording | VoiceState::Recognizing | VoiceState::Ready
            );
        if active {
            if let Some(rect) = self.last_rect {
                let text = sync
                    .delivery
                    .as_ref()
                    .map(|delivery| delivery.text.clone())
                    .or_else(|| sync.partial.clone());
                self.candidates.show_voice(
                    VoiceView {
                        state: sync.state,
                        level: sync.level,
                        text,
                        color_scheme: self.config.color_scheme,
                    },
                    rect,
                );
                self.voice_visible = true;
                self.last_shown = None;
            }
        } else if (self.voice.owns(session) || self.voice_visible) && sync.message.is_some() {
            if !self.voice_notice_seen {
                self.voice_notice_seen = true;
                self.voice_notice_until =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(2));
            }
            if self
                .voice_notice_until
                .is_some_and(|deadline| std::time::Instant::now() < deadline)
            {
                if let Some(rect) = self.last_rect {
                    self.candidates.show_voice(
                        VoiceView {
                            state: sync.state,
                            level: 0,
                            text: sync.message.clone(),
                            color_scheme: self.config.color_scheme,
                        },
                        rect,
                    );
                    self.voice_visible = true;
                }
            } else {
                self.hide_candidate_window();
            }
        } else if self.voice_visible {
            self.hide_candidate_window();
        }
    }
}
