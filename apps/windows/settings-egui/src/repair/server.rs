//! 对本会话的 Server 动手：看它在不在、请它让位、结束残留进程、重新拉起。
//! 内核对象的名字与 Server、TSF DLL 共用 [`qingjian_platform::instance`]。

use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{
    E_ACCESSDENIED, ERROR_FILE_NOT_FOUND, GetLastError, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0,
};
use windows::Win32::System::Pipes::WaitNamedPipeW;
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::{
    CreateMutexW, EVENT_MODIFY_STATE, GetCurrentProcessId, GetExitCodeProcess, MUTEX_MODIFY_STATE,
    OpenEventW, OpenMutexW, ReleaseMutex, SYNCHRONIZATION_SYNCHRONIZE, SetEvent,
    WaitForSingleObject,
};
use windows::Win32::UI::Shell::{
    SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    ShellExecuteExW,
};
use windows::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWNORMAL};
use windows::core::{HSTRING, PCWSTR, w};

use qingjian_platform::instance::{
    LAUNCH_MUTEX, instance_mutex_name, session_pipe_name, step_down_event_name,
};

/// 与设置程序装在同一目录的 Server。
pub(crate) const SERVER_EXE: &str = "qingjian-server.exe";

/// 语音 Worker，随 Server 一起收拾。
pub(crate) const VOICE_WORKER_EXE: &str = "qingjian-voice-worker.exe";

/// `GetExitCodeProcess` 对还在跑的进程返回的值。
const STILL_ACTIVE: u32 = 259;

/// 无窗口跑子进程（taskkill / schtasks / regsvr32 / icacls）。
pub(crate) const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 本进程所在的登录会话号。
pub(crate) fn session() -> Option<u32> {
    let mut session = 0;
    unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) }
        .ok()
        .map(|()| session)
}

/// 安装目录（设置程序所在目录）里的 Server exe。
pub(crate) fn server_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join(SERVER_EXE))
}

/// 本会话的 Server 进程在不在：它从启动到退出一直持有单实例互斥体。开不了但存在也算在。
pub(crate) fn is_running(session: u32) -> bool {
    let name = HSTRING::from(instance_mutex_name(&session_pipe_name(session)));
    match unsafe { OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, &name) } {
        Ok(handle) => {
            drop(unsafe { OwnedHandle::from_raw_handle(handle.0) });
            true
        }
        Err(error) => error.code() == E_ACCESSDENIED,
    }
}

/// 本会话的管道建出来没有（建出来才算 Server 就绪、输入法能连上）。
pub(crate) fn pipe_ready(session: u32) -> bool {
    let name = HSTRING::from(session_pipe_name(session));
    if unsafe { WaitNamedPipeW(&name, 1) }.as_bool() {
        return true;
    }
    // 实例都忙（超时）也说明管道在；只有「找不到」才是没起。
    let error = unsafe { GetLastError() };
    error != ERROR_FILE_NOT_FOUND
}

/// 请现任 Server 让位（它先把学习数据落盘再退），最多等 `timeout`。返回它是不是已经退了。
pub(crate) fn request_step_down(session: u32, timeout: Duration) -> bool {
    let pipe = session_pipe_name(session);
    let mutex_name = HSTRING::from(instance_mutex_name(&pipe));
    let Ok(mutex) = (unsafe {
        OpenMutexW(
            SYNCHRONIZATION_SYNCHRONIZE | MUTEX_MODIFY_STATE,
            false,
            &mutex_name,
        )
    }) else {
        return !is_running(session);
    };
    let mutex = unsafe { OwnedHandle::from_raw_handle(mutex.0) };
    let event_name = HSTRING::from(step_down_event_name(&pipe));
    if let Ok(event) = unsafe { OpenEventW(EVENT_MODIFY_STATE, false, &event_name) } {
        let event = unsafe { OwnedHandle::from_raw_handle(event.0) };
        let _ = unsafe { SetEvent(raw(&event)) };
    }
    let millis = timeout.as_millis().try_into().unwrap_or(u32::MAX);
    let result = unsafe { WaitForSingleObject(raw(&mutex), millis) };
    if result == WAIT_OBJECT_0 || result == WAIT_ABANDONED {
        // 拿到就立刻放掉：本进程占着它，接下来拉起的 Server 会以为现任还在。
        let _ = unsafe { ReleaseMutex(raw(&mutex)) };
        return true;
    }
    false
}

/// 强行结束本会话里残留的 Server 与语音 Worker（卡死、不响应让位的）。没有在跑的就什么都不做。
pub(crate) fn kill_leftovers(session: u32) {
    for image in [SERVER_EXE, VOICE_WORKER_EXE] {
        let _ = std::process::Command::new("taskkill")
            .args(["/f", "/im", image, "/fi"])
            .arg(format!("SESSION eq {session}"))
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
}

/// 在本会话里持有「拉 Server」互斥体：持有期间各应用里的输入法不去自拉，免得和修复抢。
pub(crate) fn hold_launch_mutex() -> Option<OwnedHandle> {
    let handle = unsafe { CreateMutexW(None, true, &HSTRING::from(LAUNCH_MUTEX)) }.ok()?;
    Some(unsafe { OwnedHandle::from_raw_handle(handle.0) })
}

/// 拉起 Server 的结果。
pub(crate) enum Launch {
    /// 管道就绪，输入法能连上了。
    Ready,

    /// 进程起来后很快退出了，带退出码。
    Exited(u32),

    /// 起来了但等到超时管道也没出来。
    Slow,
}

/// 像双击一样拉起 Server（带 uiAccess 的 exe 只能经外壳拉），再等它的管道出来。
/// 拉不起来时返回系统错误码。
pub(crate) fn launch(session: u32, wait: Duration) -> Result<Launch, u32> {
    let exe = server_exe().ok_or(ERROR_FILE_NOT_FOUND.0)?;
    if !exe.is_file() {
        return Err(ERROR_FILE_NOT_FOUND.0);
    }
    let file = HSTRING::from(exe.as_os_str());
    let directory = exe
        .parent()
        .map(|dir| HSTRING::from(dir.as_os_str()))
        .unwrap_or_default();
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_FLAG_NO_UI | SEE_MASK_NOASYNC,
        lpVerb: w!("open"),
        lpFile: PCWSTR(file.as_ptr()),
        lpDirectory: PCWSTR(directory.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    if unsafe { ShellExecuteExW(&mut info) }.is_err() {
        return Err(unsafe { GetLastError() }.0);
    }
    let mut process = (!info.hProcess.is_invalid())
        .then(|| unsafe { OwnedHandle::from_raw_handle(info.hProcess.0) });
    let deadline = Instant::now() + wait;
    loop {
        if pipe_ready(session) {
            return Ok(Launch::Ready);
        }
        if let Some(code) = process.as_ref().and_then(exit_code) {
            // 同一份 Server 已经在跑（拉起的这份让给了它）时退出码是 0：不再看这个进程，接着等管道。
            if code != 0 || !is_running(session) {
                return Ok(Launch::Exited(code));
            }
            process = None;
        }
        if Instant::now() >= deadline {
            return Ok(Launch::Slow);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// 以管理员身份跑一份设置程序自己（带 `args`），等它结束，返回退出码。用户在 UAC 里点了否返回 `Err(1223)`。
pub(crate) fn run_elevated_self(args: &str) -> Result<u32, u32> {
    let exe = std::env::current_exe().map_err(|_| ERROR_FILE_NOT_FOUND.0)?;
    let file = HSTRING::from(exe.as_os_str());
    let parameters = HSTRING::from(args);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(parameters.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    if unsafe { ShellExecuteExW(&mut info) }.is_err() {
        return Err(unsafe { GetLastError() }.0);
    }
    if info.hProcess.is_invalid() {
        return Ok(0);
    }
    let process = unsafe { OwnedHandle::from_raw_handle(info.hProcess.0) };
    let _ = unsafe { WaitForSingleObject(raw(&process), u32::MAX) };
    Ok(exit_code(&process).unwrap_or(0))
}

/// 进程退了返回退出码，还在跑为 `None`。
fn exit_code(process: &OwnedHandle) -> Option<u32> {
    let mut code = 0;
    unsafe { GetExitCodeProcess(raw(process), &mut code) }.ok()?;
    (code != STILL_ACTIVE).then_some(code)
}

fn raw(handle: &OwnedHandle) -> HANDLE {
    HANDLE(handle.as_raw_handle())
}
