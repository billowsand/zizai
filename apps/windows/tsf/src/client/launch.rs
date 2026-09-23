//! `cfg(windows)`：Server 没起来时按需把它拉起来。
//!
//! Server 平时由「启动」文件夹里的快捷方式在登录时拉起，但 Explorer 对启动项有延迟（实测开机到
//! Server 就绪隔了一分钟），升级安装、用户手动结束进程之后也各有一段空窗。这几段里 DLL 连不上管道，
//! 输入法在用户眼里就是「切过去打不出字」。所以连不上时自己拉一把：Server 装配只要一百毫秒，
//! 拉起后下一次按键就能用。
//!
//! 拉起走 `ShellExecuteEx`（等同双击）而不是 `CreateProcess`：签名包的 Server 带 uiAccess，
//! 那种 exe 只能由 AppInfo 经外壳拉起，`CreateProcess` 会报 740（见安装脚本的 `[Run]`）。

use std::path::PathBuf;

use windows::Win32::Foundation::{CloseHandle, E_ACCESSDENIED, ERROR_ALREADY_EXISTS, GetLastError};
use windows::Win32::System::Threading::{CreateMutexW, OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, PCWSTR, w};

use qingjian_platform::instance::{LAUNCH_MUTEX, instance_mutex_name, session_pipe_name};

use super::host;
use crate::com::log::log;

/// Server 的 exe，与 DLL 装在同一个目录（见安装脚本的 `[Files]`）。
const SERVER_EXE: &str = "qingjian-server.exe";

/// 拉起 Server。**只负责把进程起出来**，管道要等一会儿才有（实测 ShellExecute 返回后约 120 ms），
/// 由调用方轮询重连——不在这里等：`WaitNamedPipeW` 只能等「管道在、实例都忙」，
/// 管道还没建出来时它立刻就失败（2026-09-18 真机踩到，表现为拉起了却要三秒才连上）。
/// 尽力而为：拉不起来（AppContainer 应用没权限起进程、exe 不在）只记日志，调用方照常按退避重试。
pub fn launch_server() {
    // debug 构建不拉：开发时 DLL 与 Server 都在 target\debug，拉起来会占住 exe 文件，
    // 下一次 cargo build 就链接不上了。开发时自己 `cargo run -p qingjian-windows-server`。
    if cfg!(debug_assertions) {
        return;
    }
    // 提权 / 服务账号 / 服务会话里的宿主拉起的 Server 会继承它的令牌与环境（见 [`host`]），宁可不拉。
    if let Some(reason) = host::launch_refusal() {
        log(&format!("{reason}，不拉 Server"));
        return;
    }
    // Server 进程已经在了（正在装配、正在接管，管道一会儿就有）：别再拉一个。
    if server_process_exists() {
        return;
    }
    // 同一个登录会话里只让一个进程去拉（每个应用里都有一份 DLL，切焦点时会接连发现连不上）；
    // 安装器在装的过程中也一直持有它，免得文件替换到一半时拉起旧 Server。
    // 拿不到互斥体（AppContainer 里没有这个命名空间 / 安装器提权建的开不了）就别拉了。
    let Ok(mutex) = (unsafe { CreateMutexW(None, true, &HSTRING::from(LAUNCH_MUTEX)) }) else {
        return;
    };
    if unsafe { GetLastError() } != ERROR_ALREADY_EXISTS {
        match spawn() {
            Ok(path) => log(&format!("Server 没起，已拉起: {path}")),
            Err(error) => log(&format!("拉起 Server 失败: {error}")),
        }
    }
    let _ = unsafe { CloseHandle(mutex) };
}

/// 本会话的 Server 单实例互斥体在不在（Server 从启动到退出一直持有它）。开不了但它存在
/// （例如另一个用户的）也算在，别去拉。
fn server_process_exists() -> bool {
    let Some(session) = host::session() else {
        return false;
    };
    let name = HSTRING::from(instance_mutex_name(&session_pipe_name(session)));
    match unsafe { OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, &name) } {
        Ok(handle) => {
            let _ = unsafe { CloseHandle(handle) };
            true
        }
        Err(error) => error.code() == E_ACCESSDENIED,
    }
}

/// 起一次 Server，返回拉起的路径。
fn spawn() -> Result<String, String> {
    let exe = server_path()?;
    let directory = exe
        .parent()
        .map(|dir| HSTRING::from(dir.as_os_str()))
        .unwrap_or_default();
    let file = HSTRING::from(exe.as_os_str());
    // 工作目录给 exe 所在目录，与「启动」文件夹那个快捷方式一致（Server 按它找随包数据）。
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR(directory.as_ptr()),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW 的返回值是个假句柄：大于 32 才算成功，否则它就是错误码。
    let code = result.0 as usize;
    if code <= 32 {
        return Err(format!("ShellExecute 返回 {code}"));
    }
    Ok(file.to_string())
}

/// 与本 DLL 同目录的 Server exe；不在（开发时单跑 DLL）就别拉。
fn server_path() -> Result<PathBuf, String> {
    let module = crate::com::module_path().map_err(|error| error.to_string())?;
    let exe = PathBuf::from(module.to_string())
        .parent()
        .ok_or_else(|| "DLL 路径没有上级目录".to_string())?
        .join(SERVER_EXE);
    if !exe.is_file() {
        return Err(format!("{} 不在", exe.display()));
    }
    Ok(exe)
}
