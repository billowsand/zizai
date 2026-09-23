//! Server 进程内的 UI 线程：候选窗口与悬浮状态条都在这条线程上自绘（普通置顶窗会被商店 / 任务栏搜索
//! 这些高 z-band 宿主盖住，只有本进程配合 uiAccess 签名才能盖过）。
//!
//! HWND 线程亲和：Router 在工人线程上产出内容，经通道 + `PostThreadMessageW` 唤醒交给 UI 线程应用。
//! 线程句柄是 [`UiHandle`]，命令在 [`command`]，候选窗口在 [`candidates`]，状态条在 [`status`]，分层窗口合成在 [`layered`]。

mod candidates;
mod command;
mod layered;
mod monitor;
mod painter;
mod status;
mod window_class;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use windows::Win32::Foundation::{E_FAIL, HINSTANCE, HWND, LPARAM, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, HWND_TOPMOST, MSG, PostThreadMessageW, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SetWindowPos, TranslateMessage, WM_APP,
};
use windows::core::{Error, Result};

use qingjian_platform::protocol::{Frame, ScreenRect};

use self::candidates::CandidateWindow;
use self::command::UiCommand;
use self::painter::{Painter, SharedPainter};
use self::status::StatusBar;
use crate::dispatch::{CandidateSink, StatusEvent, StatusSink, StatusView, VoiceView};

/// 状态条上的操作（点格子 / 拖动结束）回给 Router 的回调，UI 线程上调。
pub type StatusEvents = Box<dyn Fn(StatusEvent) + Send>;

/// 唤醒 UI 线程去排空命令队列的线程消息。
const WM_WAKE: u32 = WM_APP;

/// UI 线程句柄：发命令 = 投进通道 + 一条 `WM_WAKE`。线程 detached，随进程存活。
#[derive(Clone)]
pub struct UiHandle {
    /// 命令通道的发送端。
    sender: Sender<UiCommand>,

    /// UI 线程 id，`PostThreadMessageW` 用。
    thread_id: u32,

    /// UI 线程是否还在跑（窗口建好置真，线程退出置假）。Server 靠它判断「还能不能画」：
    /// 不能画就退出，别占着独占管道变成「能打字、没窗口」且谁也接管不了的状态。
    alive: Arc<AtomicBool>,
}

impl UiHandle {
    /// 起 UI 线程并等它建好候选窗口。失败返回 `Err`，调用方退化为不画。
    pub fn spawn(on_status: StatusEvents) -> Result<Self> {
        // 用 Option<u32> 而非 Result 回报，免得 windows Error 跨线程。
        let (ready_tx, ready_rx) = mpsc::channel::<Option<u32>>();
        let (command_tx, command_rx) = mpsc::channel::<UiCommand>();
        let alive = Arc::new(AtomicBool::new(false));
        let thread_alive = alive.clone();
        thread::Builder::new()
            .name("qingjian-candidates".to_owned())
            .spawn(move || run(command_rx, &ready_tx, on_status, thread_alive))
            .map_err(|_| Error::from(E_FAIL))?;
        match ready_rx.recv() {
            Ok(Some(thread_id)) => Ok(Self {
                sender: command_tx,
                thread_id,
                alive,
            }),
            _ => Err(Error::from(E_FAIL)),
        }
    }

    /// UI 线程是否还在跑（窗口是否还能画）。线程没起或已退出为 `false`。
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    /// 线程已退出（通道断）时静默丢弃。
    fn post(&self, command: UiCommand) {
        if self.sender.send(command).is_ok() {
            let _ = unsafe { PostThreadMessageW(self.thread_id, WM_WAKE, WPARAM(0), LPARAM(0)) };
        }
    }
}

impl CandidateSink for UiHandle {
    fn show(&self, frame: Frame, rect: ScreenRect) {
        self.post(UiCommand::Show(Box::new((frame, rect))));
    }

    fn show_voice(&self, view: VoiceView, rect: ScreenRect) {
        self.post(UiCommand::ShowVoice(Box::new((view, rect))));
    }

    fn hide(&self) {
        self.post(UiCommand::Hide);
    }

    fn set_font(&self, font: String) {
        self.post(UiCommand::SetFont(font));
    }
}

impl StatusSink for UiHandle {
    fn show_status(&self, view: StatusView) {
        self.post(UiCommand::StatusShow(Box::new(view)));
    }

    fn hide_status(&self) {
        self.post(UiCommand::StatusHide);
    }
}

/// 抬到置顶带的最前。建窗口时的 `WS_EX_TOPMOST` 只保证进普通置顶带，**升进 UIAccess 高带（盖过
/// 开始菜单 / 任务栏搜索这些 banded 宿主）要的是这次 `SetWindowPos(HWND_TOPMOST)`**，每次显示前调一次。
/// 本进程没拿到 uiAccess（未签名 / 没装 Program Files）时它只是普通置顶，不会失败。
pub(super) fn raise_topmost(hwnd: HWND) {
    let _ = unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
    };
}

/// 本进程 exe 的模块句柄（注册窗口类 / 建窗口用）。
pub(super) fn module_handle() -> HINSTANCE {
    let module = unsafe { GetModuleHandleW(None) }.unwrap_or_default();
    HINSTANCE(module.0)
}

/// 线程退出（含 panic 展开）时把存活标记清掉。
struct AliveGuard(Arc<AtomicBool>);

impl Drop for AliveGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

/// UI 线程主体：建窗口、报回线程 id、跑消息循环。
fn run(
    commands: Receiver<UiCommand>,
    ready: &Sender<Option<u32>>,
    on_status: StatusEvents,
    alive: Arc<AtomicBool>,
) {
    let _guard = AliveGuard(alive.clone());
    // 按物理像素定位，与应用报来的组句屏幕矩形对齐；已设过会失败，忽略。
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let thread_id = unsafe { GetCurrentThreadId() };
    let Some(painter) = Painter::new("") else {
        tracing::error!("初始化候选窗口渲染器失败，Server 将不显示候选框与状态条");
        let _ = ready.send(None);
        return;
    };
    let painter: SharedPainter = Rc::new(RefCell::new(painter));
    // 先建窗口再报 id：建窗口顺带建起本线程的消息队列，之后 PostThreadMessageW 才有处可投。
    let window = match CandidateWindow::new(painter.clone()) {
        Ok(window) => window,
        Err(error) => {
            tracing::error!(%error, "建候选窗口失败，Server 将不显示候选框");
            let _ = ready.send(None);
            return;
        }
    };
    let status = match StatusBar::new(on_status, painter.clone()) {
        Ok(status) => Some(status),
        Err(error) => {
            tracing::error!(%error, "建悬浮状态条失败，将不显示状态条");
            None
        }
    };
    alive.store(true, Ordering::Relaxed);
    if ready.send(Some(thread_id)).is_err() {
        return;
    }
    let mut msg = MSG::default();
    loop {
        let got = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if got.0 <= 0 {
            break;
        }
        if msg.message == WM_WAKE {
            // 一次唤醒排空整个队列，保住 Hide→Show 的先后。
            while let Ok(command) = commands.try_recv() {
                apply(&window, status.as_ref(), &painter, command);
            }
            continue;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn apply(
    window: &CandidateWindow,
    status: Option<&StatusBar>,
    painter: &SharedPainter,
    command: UiCommand,
) {
    match command {
        UiCommand::Show(payload) => {
            let (frame, rect) = *payload;
            window.set_content(&frame);
            window.show(to_win_rect(rect));
        }
        UiCommand::ShowVoice(payload) => {
            let (view, rect) = *payload;
            window.set_voice(view);
            window.show(to_win_rect(rect));
        }
        UiCommand::Hide => window.hide(),
        UiCommand::StatusShow(view) => {
            if let Some(status) = status {
                status.update(*view);
            }
        }
        UiCommand::StatusHide => {
            if let Some(status) = status {
                status.hide();
            }
        }
        UiCommand::SetFont(font) => Painter::set_font(painter, &font),
    }
}

fn to_win_rect(rect: ScreenRect) -> RECT {
    RECT {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}
