//! 候选窗口：不抢焦点、置顶的分层窗口，跟随光标，画拼音行与候选列表，四周柔和阴影。
//! 字在渲染器出位图后由 Windows 壳贴上。绘制内容在 [`RenderData`]，一行的展示形态在 [`row`]，
//! 贴光标上方还是下方在 [`placement`]。设计语言对齐 macOS 端。

mod placement;
mod render_data;
pub(crate) mod row;

use std::cell::{Cell, RefCell};
use std::time::Instant;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, IDC_ARROW, LoadCursorW, SW_HIDE, SW_SHOWNA,
    ShowWindow, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_POPUP,
};
use windows::core::{PCWSTR, Result, w};

use qingjian_platform::ColorScheme;
use qingjian_platform::protocol::{Frame, VoiceState};
use qingjian_render::{VoiceFrame, VoiceTone};

use self::placement::{LastPlacement, place};
pub(crate) use self::render_data::RenderData;
use super::layered;
use super::painter::SharedPainter;
use super::window_class::WindowClass;
use crate::dispatch::VoiceView;

const CLASS_NAME: PCWSTR = w!("QingjianCandidateWindow");
static CLASS: WindowClass = WindowClass::new();

/// 视觉电平已扣过底噪；略高于零才算真正收到声音，避免环境噪声误判。
const HEARD_SOUND_LEVEL: u16 = 24;

const QUIET_HINT_AFTER_SECS: u64 = 2;

/// `HKCU\...\Themes\Personalize\AppsUseLightTheme` 为 0 是深色；读不到当浅色。
pub(super) fn system_prefers_dark() -> bool {
    windows_registry::CURRENT_USER
        .open(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
        .and_then(|key| key.get_u32("AppsUseLightTheme"))
        .is_ok_and(|value| value == 0)
}

/// 候选窗口。内容经 `UpdateLayeredWindow` 一次贴上，窗口过程只走默认处理。
pub(crate) struct CandidateWindow {
    hwnd: HWND,

    /// 绘制内容。
    data: RefCell<RenderData>,

    /// `Some` 时同一个 HWND 画语音态；普通候选帧到来即清掉。
    voice: RefCell<Option<VoiceRenderData>>,

    /// 上次用的 DPI。
    dpi: Cell<u32>,

    /// 上次解析出的深浅。
    dark: Cell<bool>,

    /// 上次贴在光标的哪一边；同一行里不因窗口高矮改边。
    placement: LastPlacement,

    /// 字在渲染器。
    painter: SharedPainter,
}

impl CandidateWindow {
    /// 建一个隐藏的候选窗口。
    pub(crate) fn new(painter: SharedPainter) -> Result<Self> {
        CLASS.ensure(|| WNDCLASSEXW {
            lpfnWndProc: Some(wndproc),
            hInstance: super::module_handle(),
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        })?;
        let dpi = unsafe { GetDpiForSystem() }.max(96);
        let dark = system_prefers_dark();
        let data = RefCell::new(RenderData::empty());
        // NOACTIVATE：显示时不抢应用焦点。
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                CLASS_NAME,
                w!("字在候选"),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(super::module_handle()),
                None,
            )?
        };
        Ok(Self {
            hwnd,
            data,
            voice: RefCell::new(None),
            dpi: Cell::new(dpi),
            dark: Cell::new(dark),
            placement: Cell::new(None),
            painter,
        })
    }

    /// 刷新内容（不定位、不显示）。
    pub(crate) fn set_content(&self, frame: &Frame) {
        *self.voice.borrow_mut() = None;
        self.data.borrow_mut().set(frame);
    }

    /// 保留最近一小段电平历史，形成从左向右滚动的波形。
    pub(crate) fn set_voice(&self, view: VoiceView) {
        let now = Instant::now();
        let mut voice = self.voice.borrow_mut();
        let reset = voice
            .as_ref()
            .is_none_or(|previous| previous.state != VoiceState::Recording)
            && view.state == VoiceState::Recording;
        let levels = if reset {
            Vec::new()
        } else {
            voice
                .as_ref()
                .map(|previous| previous.frame.levels.clone())
                .unwrap_or_default()
        };
        let mut levels = levels;
        levels.push(if view.state == VoiceState::Recording {
            view.level
        } else {
            0
        });
        if levels.len() > 24 {
            levels.drain(..levels.len() - 24);
        }
        let previous = voice.as_ref();
        let started_at = if view.state == VoiceState::Recording {
            if reset {
                Some(now)
            } else {
                previous.and_then(|value| value.started_at).or(Some(now))
            }
        } else {
            None
        };
        let elapsed_seconds = if let Some(started_at) = started_at {
            now.saturating_duration_since(started_at).as_secs()
        } else {
            previous.map_or(0, |value| value.elapsed_seconds)
        };
        let heard_sound = if reset {
            view.level >= HEARD_SOUND_LEVEL
        } else {
            previous.is_some_and(|value| value.heard_sound) || view.level >= HEARD_SOUND_LEVEL
        };
        let (title, hint, tone, show_elapsed) = match view.state {
            VoiceState::Recording => (
                "正在听",
                recording_hint(heard_sound, elapsed_seconds),
                VoiceTone::Listening,
                true,
            ),
            VoiceState::Recognizing => ("正在识别", "正在转写…", VoiceTone::Working, true),
            VoiceState::Polishing => ("正在转化", "大模型润色中…", VoiceTone::Working, true),
            VoiceState::Ready => ("即将上屏", "识别完成", VoiceTone::Success, true),
            VoiceState::Failed => (
                "语音暂不可用",
                "可再次按快捷键重试",
                VoiceTone::Warning,
                false,
            ),
            VoiceState::Idle => (
                "没有听清",
                "请靠近麦克风再试一次",
                VoiceTone::Warning,
                false,
            ),
            _ => ("语音输入", "请稍候", VoiceTone::Working, false),
        };
        *voice = Some(VoiceRenderData {
            state: view.state,
            color_scheme: view.color_scheme,
            started_at,
            elapsed_seconds,
            heard_sound,
            frame: VoiceFrame {
                levels,
                title: title.to_owned(),
                tone,
                elapsed: show_elapsed.then(|| format_elapsed(elapsed_seconds)),
                transcript: view.text,
                hint: hint.to_owned(),
            },
        });
    }

    /// 按光标矩形定位并显示：贴光标下方（放不下放上方，同一行里不改边），四周留出阴影。
    pub(crate) fn show(&self, anchor: RECT) {
        self.sync_environment();
        let rendered = if let Some(voice) = self.voice.borrow().as_ref() {
            self.painter.borrow_mut().render_voice(
                &voice.frame,
                voice.color_scheme,
                self.dark.get(),
                self.dpi.get(),
            )
        } else {
            let data = self.data.borrow();
            self.painter.borrow_mut().render_frame(
                &data.render_frame(),
                data.color_scheme,
                self.dark.get(),
                self.dpi.get(),
            )
        };
        let Some(rendered) = rendered else {
            self.hide();
            return;
        };
        let content = (
            rendered.content_width as i32,
            rendered.content_height as i32,
        );
        if content.0 <= 0 || content.1 <= 0 {
            self.hide();
            return;
        }
        let (content_x, content_y) = place(&self.placement, anchor, content);
        let updated = layered::present(
            self.hwnd,
            &rendered.pixmap,
            (
                content_x - rendered.content_x as i32,
                content_y - rendered.content_y as i32,
            ),
        );
        if updated.is_ok() {
            super::raise_topmost(self.hwnd);
            let _ = unsafe { ShowWindow(self.hwnd, SW_SHOWNA) };
        } else {
            self.hide();
        }
    }

    pub(crate) fn hide(&self) {
        let _ = unsafe { ShowWindow(self.hwnd, SW_HIDE) };
    }

    /// 每次 `show` 前同步 DPI 与深浅模式。
    fn sync_environment(&self) {
        let dpi = match unsafe { GetDpiForWindow(self.hwnd) } {
            0 => self.dpi.get(),
            dpi => dpi,
        };
        let dark = system_prefers_dark();
        self.dpi.set(dpi);
        self.dark.set(dark);
    }
}

struct VoiceRenderData {
    state: VoiceState,

    color_scheme: ColorScheme,

    started_at: Option<Instant>,

    elapsed_seconds: u64,

    heard_sound: bool,

    frame: VoiceFrame,
}

fn format_elapsed(seconds: u64) -> String {
    let minutes = seconds / 60;
    let seconds = seconds % 60;
    format!("{minutes:02}:{seconds:02}")
}

fn recording_hint(heard_sound: bool, elapsed_seconds: u64) -> &'static str {
    if !heard_sound && elapsed_seconds >= QUIET_HINT_AFTER_SECS {
        "声音较小 · 请靠近麦克风"
    } else if heard_sound {
        "再次按快捷键完成 · Esc 取消"
    } else {
        "请开始说话 · 再次按快捷键完成"
    }
}

impl Drop for CandidateWindow {
    fn drop(&mut self) {
        let _ = unsafe { DestroyWindow(self.hwnd) };
    }
}

/// 分层窗口无需 `WM_PAINT`，全交默认处理。
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::{format_elapsed, recording_hint};

    #[test]
    fn elapsed_time_is_stable_and_easy_to_scan() {
        assert_eq!(format_elapsed(0), "00:00");
        assert_eq!(format_elapsed(8), "00:08");
        assert_eq!(format_elapsed(754), "12:34");
    }

    #[test]
    fn quiet_hint_waits_before_warning_and_recovers_after_sound() {
        assert_eq!(recording_hint(false, 0), "请开始说话 · 再次按快捷键完成");
        assert_eq!(recording_hint(false, 2), "声音较小 · 请靠近麦克风");
        assert_eq!(recording_hint(true, 5), "再次按快捷键完成 · Esc 取消");
    }
}
