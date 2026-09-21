//! `ITfKeyEventSink`：所有键先经 `OnTestKeyDown` 判吃不吃（[`TextService_Impl::would_eat`]，与 Router 的分派对齐），
//! 吃的键在 `OnKeyDown` 里转发给 Server 并按结果更新文档；单击 Shift 的判定也在这里。
//! 上下文禁了键盘（密码框，见 [`context`](crate::com::context)）时没在组句的键一律放行。

use std::time::Instant;

use windows::Win32::Foundation::{FALSE, LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, VK_CONTROL, VK_ESCAPE, VK_MENU, VK_RCONTROL, VK_RMENU,
};
use windows::Win32::UI::TextServices::{ITfContext, ITfKeyEventSink_Impl};
use windows::core::{BOOL, GUID, Ref, Result};

use qingjian_platform::VoiceTrigger;
use qingjian_platform::protocol::{KeyEvent, KeyOutcome, VoiceAction};

use super::TextService_Impl;
use super::next::Next;
use crate::client::mismatch;
use crate::com::composition::preedit_string;
use crate::com::edit::request_voice_anchor;
use crate::com::key::event::{digit_key, is_edit, is_letter, is_nav, to_key_event};
use crate::com::key_metrics;
use crate::com::log::log;

const VOICE_TOGGLE_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(350);

impl ITfKeyEventSink_Impl for TextService_Impl {
    /// 获焦：补一次连接（Server 起晚了 / 重启过），并刷指示器（系统会在切换焦点时重置它）。
    /// 失焦：把敲了一半的拼音原样落定（对应 macOS 的 `commitComposition`）。
    fn OnSetFocus(&self, fforeground: BOOL) -> Result<()> {
        self.shared.set_foreground(fforeground.as_bool());
        if fforeground.as_bool() {
            self.ensure_connected();
            self.refresh_mode_indicator();
        } else {
            self.cancel_voice();
            self.commit_pending();
        }
        Ok(())
    }

    /// 所有键（含之后被吃掉的）都先经过这里，Shift 单击的判定放在这一层。
    fn OnTestKeyDown(&self, pic: Ref<ITfContext>, wparam: WPARAM, lparam: LPARAM) -> Result<BOOL> {
        let vk = wparam.0 as u32;
        self.note_key_down(vk, lparam);
        if self.keyboard_disabled(&pic) {
            return Ok(FALSE);
        }
        if self.shared.voice_active() && vk == VK_ESCAPE.0 as u32 {
            return Ok(true.into());
        }
        if self.voice_key_down(vk, lparam) {
            return Ok(true.into());
        }
        Ok(self.would_eat(&self.key_event(vk)).into())
    }

    fn OnKeyDown(&self, pic: Ref<ITfContext>, wparam: WPARAM, lparam: LPARAM) -> Result<BOOL> {
        let vk = wparam.0 as u32;
        self.note_key_down(vk, lparam);
        if self.keyboard_disabled(&pic) {
            return Ok(FALSE);
        }
        if self.shared.voice_active() && vk == VK_ESCAPE.0 as u32 {
            log("Esc 取消语音输入");
            self.cancel_voice();
            return Ok(true.into());
        }
        if self.voice_key_down(vk, lparam) {
            self.shared.set_voice_key_held(true);
            self.shared.take_voice_key_combo();
            log(&format!(
                "语音快捷键按下（等待松开触发） trigger={:?} vk=0x{vk:02X} extended={}",
                self.shared.voice_trigger(),
                extended_key(lparam)
            ));
            return Ok(true.into());
        }
        let event = self.key_event(vk);
        Ok(self.handle_key(pic, event).into())
    }

    fn OnTestKeyUp(&self, pic: Ref<ITfContext>, wparam: WPARAM, lparam: LPARAM) -> Result<BOOL> {
        let vk = wparam.0 as u32;
        self.note_key_up(vk);
        if self.keyboard_disabled(&pic) {
            return Ok(FALSE);
        }
        if self.voice_key_up(vk, lparam) {
            return Ok(true.into());
        }
        Ok(FALSE)
    }

    fn OnKeyUp(&self, pic: Ref<ITfContext>, wparam: WPARAM, lparam: LPARAM) -> Result<BOOL> {
        let vk = wparam.0 as u32;
        self.note_key_up(vk);
        if self.keyboard_disabled(&pic) {
            return Ok(FALSE);
        }
        if self.voice_key_up(vk, lparam) {
            self.shared.set_voice_key_held(false);
            // 按住期间敲过别的键：这是「右 Alt + 某键」的组合用法，松手只收尾，不开录音。
            if self.shared.take_voice_key_combo() {
                log("语音快捷键被当作组合键使用，不触发录音");
                return Ok(true.into());
            }
            let now = Instant::now();
            if is_debounced(self.last_voice_toggle.get(), now) {
                log("语音快捷键重复触发已忽略");
                return Ok(true.into());
            }
            self.last_voice_toggle.set(Some(now));
            let action = toggle_voice_action(self.shared.voice_active());
            log(&format!(
                "语音快捷键触发 action={action:?} trigger={:?} vk=0x{vk:02X} extended={}",
                self.shared.voice_trigger(),
                extended_key(lparam)
            ));
            return Ok(self.handle_voice_key(pic, action).into());
        }
        Ok(FALSE)
    }

    /// 本壳没登记任何保留键（原先只有「翻译选中文字」一个），来了也不处理。
    fn OnPreservedKey(&self, _pic: Ref<ITfContext>, _rguid: *const GUID) -> Result<BOOL> {
        Ok(FALSE)
    }
}

impl TextService_Impl {
    fn voice_key_down(&self, vk: u32, lparam: LPARAM) -> bool {
        let trigger = self.shared.voice_trigger();
        is_voice_key_down(
            trigger,
            vk,
            extended_key(lparam),
            key_down(VK_RMENU),
            key_down(VK_RCONTROL),
        )
    }

    fn voice_key_up(&self, vk: u32, lparam: LPARAM) -> bool {
        let trigger = self.shared.voice_trigger();
        trigger.virtual_key() == Some(vk)
            || (self.shared.voice_key_held()
                && match trigger {
                    VoiceTrigger::RightAlt => vk == VK_MENU.0 as u32,
                    VoiceTrigger::RightCtrl => vk == VK_CONTROL.0 as u32,
                    _ => false,
                })
            || is_voice_key_down(
                trigger,
                vk,
                extended_key(lparam),
                key_down(VK_RMENU),
                key_down(VK_RCONTROL),
            )
    }

    fn handle_voice_key(&self, pic: Ref<ITfContext>, action: VoiceAction) -> bool {
        if action == VoiceAction::Start {
            self.commit_pending();
        }
        let Ok(context) = pic.ok() else {
            return false;
        };
        self.shared.set_last_context(Some(context.clone()));
        if !self.ensure_connected() {
            return false;
        }
        let result = self
            .engine
            .borrow_mut()
            .as_mut()
            .expect("ensure_connected returned true without a client")
            .voice(action);
        match result {
            Ok(()) => {
                if action == VoiceAction::Start {
                    self.shared.set_voice_active(true);
                    if let Err(error) = request_voice_anchor(
                        context,
                        self.client_id.get(),
                        self.engine.clone(),
                        self.shared.clone(),
                    ) {
                        log(&format!("语音面板定位会话没被受理: {error}"));
                    }
                }
                true
            }
            Err(error) => {
                log(&format!("发送语音动作失败: {error}"));
                self.disconnect();
                false
            }
        }
    }

    pub(super) fn cancel_voice(&self) {
        if !self.shared.voice_active() {
            return;
        }
        if let Ok(mut guard) = self.engine.try_borrow_mut()
            && let Some(client) = guard.as_mut()
        {
            let _ = client.voice(VoiceAction::Cancel);
        }
        self.shared.set_voice_active(false);
        self.shared.set_voice_key_held(false);
        self.shared.set_voice_queued(None);
    }

    /// 没在组句时看上下文有没有禁键盘（密码框）：禁了整键放行、不组句。组句中不看——那段组句是我们自己的，
    /// 应用要禁会先终止它。每键两次 compartment 读取，微秒级。
    fn keyboard_disabled(&self, pic: &Ref<ITfContext>) -> bool {
        if self.shared.composing() {
            return false;
        }
        let Ok(context) = pic.ok() else {
            return false;
        };
        let disabled = crate::com::context::keyboard_disabled(context);
        if disabled {
            log("上下文禁用键盘（密码框），放行");
        }
        disabled
    }

    fn key_event(&self, vk: u32) -> KeyEvent {
        to_key_event(vk, self.mode_state.english())
    }

    fn note_key_down(&self, vk: u32, lparam: LPARAM) {
        // 语音键自己的按下与系统自动重复不算组合；别的键按下才算。
        if self.shared.voice_key_held() && !self.voice_key_down(vk, lparam) {
            self.shared.note_voice_key_combo();
        }
        self.shift_tap.key_down(vk, lparam);
    }

    fn note_key_up(&self, vk: u32) {
        if self.shift_tap.key_up(vk) {
            self.set_english_mode(!self.mode_state.english());
        }
    }

    /// 这个键吃不吃，与 Router 的分派对齐；`OnTestKeyDown` 用，无副作用。
    /// 带 Ctrl/Alt/Win 只有组句中的「修饰键 + 数字」送 Server（删候选），其余归应用；
    /// 字母只有「中文模式、没在组句、按住 Shift 的大写」归应用；组句中功能键 / 方向键 / 可打印字符都吃；
    /// 没在组句时数字 / 标点也先「测吃」送去转全角（中英各有一份开关），Server 不转的回 Passthrough 再放行；`?` 是问字前缀。
    ///
    /// 协议对不上时一个键都不吃：本进程的旧 DLL 换不掉，吃了也变不出中文，还不如整键还给应用当英文打
    /// （见 [`mismatch`](crate::client::mismatch)）。
    fn would_eat(&self, event: &KeyEvent) -> bool {
        if mismatch::detected() {
            return false;
        }
        let modifiers = event.modifiers;
        if modifiers.has_command_key() {
            return self.shared.composing() && digit_key(event.virtual_key);
        }
        let vk = event.virtual_key;
        if is_letter(vk) {
            return modifiers.caps
                || modifiers.english_mode
                || !modifiers.shift
                || self.shared.composing();
        }
        if self.shared.composing() {
            return is_edit(vk) || is_nav(vk) || event.character.is_some_and(|c| !c.is_control());
        }
        event
            .character
            .is_some_and(|c| c.is_ascii_punctuation() || c.is_ascii_digit())
    }

    /// 不吃的键绝不碰组句（否则光标一移，组句会把拼音重插到别处）。
    fn handle_key(&self, pic: Ref<ITfContext>, event: KeyEvent) -> bool {
        if !self.would_eat(&event) {
            return false;
        }
        let started = Instant::now();
        let handled = self.forward_key(pic, event);
        key_metrics::record(started.elapsed());
        handled
    }

    /// 把按键送给 Server 并按结果更新文档；返回吃不吃。
    fn forward_key(&self, pic: Ref<ITfContext>, event: KeyEvent) -> bool {
        // 没连上 Server（开机时它还没起来、刚升级完、被结束了进程）：整键放行。
        // 吃掉的话这段时间里用户什么都打不出来，还不如让字母直接进应用当英文打——反正没有 Server
        // 也变不出中文。`OnTestKeyDown` 那边说了吃、这里放行，是 TSF 允许的（应用照收不误）。
        if !self.ensure_connected() {
            return false;
        }
        // OnTestKeyDown 已声明吃的可打印字符，Server 放行时由输入法自己插入：退回应用的话，企业微信 /
        // 微信 / notepad++ 这类自绘输入框会把它丢掉。功能键（无字符）仍交给应用。
        let passthrough_char = event.character.filter(|c| !c.is_control());
        if let Ok(context) = pic.ok() {
            self.shared.set_last_context(Some(context.clone()));
        }
        // Server 交互在这段借用里做完，放掉借用再走编辑会话。
        let next = {
            let mut guard = self.engine.borrow_mut();
            let Some(client) = guard.as_mut() else {
                return false;
            };
            // 组句被应用终止过：先让 Server 清掉残留的拼音（文本已在文档里，交出的丢弃）。
            let response = if self.shared.take_server_stale() {
                client.commit().and_then(|_| client.key(event))
            } else {
                client.key(event)
            };
            match response {
                Ok(response) => {
                    let preedit = preedit_string(&response.frame);
                    self.shared.set_composing(!response.frame.is_empty());
                    let consumed = matches!(response.outcome, KeyOutcome::Consumed);
                    Next::Document {
                        commit: response.commit,
                        preedit,
                        consumed,
                    }
                }
                Err(error) => {
                    // 读不懂 Server 的话 = 本进程加载的是升级前的旧 DLL：合上闸，之后整键放行，
                    // 别再每键「失败 → 重连 → 再失败」地抖下去（那样这个应用里既打不出字又吞键）。
                    if error.is_protocol_mismatch() {
                        mismatch::mark(&error.to_string());
                    } else {
                        log(&format!("转发按键失败，放行并断开，下一键重连: {error}"));
                    }
                    *guard = None;
                    self.last_connect_failure.set(None);
                    self.shared.end_composing();
                    Next::Abort
                }
            }
        };
        // 带 Ctrl / Alt / Win 的组合放行时仍交还应用，别把热键的字母插进文档。
        let insertable = !event.modifiers.has_command_key();
        match (next, passthrough_char) {
            // 放行 + 没在组句 + 可打印字符：输入法插入，吃掉；Server 顺带交出的英文直输段字母拼在前面。
            // 「没在组句」看 Server 的帧空不空，不看 `preedit`：拼音只在候选窗口时行内本来就是空串。
            (
                Next::Document {
                    consumed: false,
                    commit,
                    ..
                },
                Some(c),
            ) if insertable && !self.shared.composing() => {
                let mut text = commit.unwrap_or_default();
                text.push(c);
                self.update_document(pic, Some(text), String::new());
                true
            }
            // 放行的功能键：Server 没动缓冲区，交还应用（应用处理这个键时光标可能会移）。
            (
                Next::Document {
                    consumed: false, ..
                },
                _,
            ) => false,
            (
                Next::Document {
                    commit, preedit, ..
                },
                _,
            ) => {
                self.update_document(pic, commit, preedit);
                true
            }
            (Next::Abort, _) => false,
        }
    }
}

fn extended_key(lparam: LPARAM) -> bool {
    ((lparam.0 as usize >> 24) & 1) != 0
}

fn key_down(vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) -> bool {
    unsafe { GetKeyState(vk.0 as i32) < 0 }
}

fn is_voice_key_down(
    trigger: VoiceTrigger,
    vk: u32,
    extended: bool,
    right_alt_down: bool,
    right_ctrl_down: bool,
) -> bool {
    if trigger.virtual_key() == Some(vk) {
        return true;
    }
    match (trigger, vk) {
        (VoiceTrigger::RightAlt, key) if key == VK_MENU.0 as u32 => extended || right_alt_down,
        (VoiceTrigger::RightCtrl, key) if key == VK_CONTROL.0 as u32 => extended || right_ctrl_down,
        _ => false,
    }
}

fn toggle_voice_action(active: bool) -> VoiceAction {
    if active {
        VoiceAction::Stop
    } else {
        VoiceAction::Start
    }
}

fn is_debounced(previous: Option<Instant>, now: Instant) -> bool {
    previous.is_some_and(|value| now.saturating_duration_since(value) < VOICE_TOGGLE_DEBOUNCE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_right_alt_uses_live_side_state_when_tsf_drops_extended_bit() {
        assert!(is_voice_key_down(
            VoiceTrigger::RightAlt,
            VK_MENU.0 as u32,
            false,
            true,
            false
        ));
    }

    #[test]
    fn generic_left_alt_does_not_trigger_voice() {
        assert!(!is_voice_key_down(
            VoiceTrigger::RightAlt,
            VK_MENU.0 as u32,
            false,
            false,
            false
        ));
    }

    #[test]
    fn release_toggles_recording_without_requiring_a_key_down_callback() {
        assert_eq!(toggle_voice_action(false), VoiceAction::Start);
        assert_eq!(toggle_voice_action(true), VoiceAction::Stop);
    }

    #[test]
    fn voice_toggle_ignores_only_the_short_debounce_window() {
        let now = Instant::now();
        assert!(is_debounced(
            Some(now - VOICE_TOGGLE_DEBOUNCE + std::time::Duration::from_millis(1)),
            now
        ));
        assert!(!is_debounced(Some(now - VOICE_TOGGLE_DEBOUNCE), now));
        assert!(!is_debounced(None, now));
    }
}
