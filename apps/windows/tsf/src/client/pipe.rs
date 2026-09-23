//! `cfg(windows)`：打开 Server 的命名管道，得到一条双工流交给 [`EngineClient`](super::EngineClient)。

use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::fs::OpenOptionsExt;

use std::os::windows::io::AsRawHandle;

use windows::Win32::Foundation::{ERROR_PIPE_BUSY, HANDLE};
use windows::Win32::System::Pipes::{GetNamedPipeServerSessionId, WaitNamedPipeW};
use windows::core::HSTRING;

use qingjian_platform::instance::session_pipe_name;

use super::host;

/// 连好的命名管道。对端关闭时读到 EOF；`flush` 是空操作（管道上 `FlushFileBuffers` 会阻塞到对端读完）。
pub type PipeStream = File;

/// 实例都被占着时等一个可用实例的超时（毫秒）与重试次数。要短：这里在应用 UI 线程上，等久了 TSF 看门狗会切走输入法。
const BUSY_WAIT_MS: u32 = 300;
const BUSY_RETRIES: u32 = 1;

/// 连本会话的 Server（`\\.\pipe\qingjian.<会话号>`）。管道名是整机共用的，所以连上后再核对一次
/// 对端 Server 确实在本会话：别的会话里的进程抢先建了同名管道时，别把本会话的按键送过去。
pub fn connect_default() -> io::Result<PipeStream> {
    let session = host::session().ok_or_else(|| io::Error::other("查不到本进程的会话号"))?;
    let stream = connect(&session_pipe_name(session))?;
    let mut server_session = 0;
    if unsafe { GetNamedPipeServerSessionId(HANDLE(stream.as_raw_handle()), &mut server_session) }
        .is_ok()
        && server_session != session
    {
        return Err(io::Error::other(format!(
            "管道对端在会话 {server_session}，不是本会话 {session}，不连"
        )));
    }
    Ok(stream)
}

pub fn connect(name: &str) -> io::Result<PipeStream> {
    let mut attempts = 0;
    loop {
        match OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(name)
        {
            Err(error)
                if error.raw_os_error() == Some(ERROR_PIPE_BUSY.0 as i32)
                    && attempts < BUSY_RETRIES =>
            {
                attempts += 1;
                let _ = unsafe { WaitNamedPipeW(&HSTRING::from(name), BUSY_WAIT_MS) };
            }
            result => return result,
        }
    }
}
