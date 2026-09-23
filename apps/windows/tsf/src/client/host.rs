//! `cfg(windows)`：DLL 所在宿主进程的身份——会话号、是否提权、是否服务账号。
//! 连哪条管道按会话号定；宿主身份不对（提权、服务账号、服务会话）时不拉 Server：
//! 拉起的 Server 会继承宿主的令牌与环境，变成一个提权的、读错配置目录的、甚至画不到用户桌面上的 Server。

use std::sync::OnceLock;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{
    GetTokenInformation, IsWellKnownSid, TOKEN_ELEVATION_TYPE, TOKEN_INFORMATION_CLASS,
    TOKEN_QUERY, TOKEN_USER, TokenElevationType, TokenElevationTypeFull, TokenUser,
    WinLocalServiceSid, WinLocalSystemSid, WinNetworkServiceSid,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::{GetCurrentProcess, GetCurrentProcessId, OpenProcessToken};

/// 本进程所在的登录会话号；查不到为 `None`。
pub fn session() -> Option<u32> {
    static SESSION: OnceLock<Option<u32>> = OnceLock::new();
    *SESSION.get_or_init(|| {
        let mut session = 0;
        unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) }
            .ok()
            .map(|()| session)
    })
}

/// 宿主不适合拉 Server 的原因；适合为 `None`。结果按进程缓存（身份在进程生命期内不变）。
pub fn launch_refusal() -> Option<&'static str> {
    static REFUSAL: OnceLock<Option<&'static str>> = OnceLock::new();
    *REFUSAL.get_or_init(|| {
        if session().is_none_or(|session| session == 0) {
            return Some("宿主在服务会话里");
        }
        let service = token_information(TokenUser).is_some_and(|buffer| {
            // SAFETY: TokenUser 缓冲以 TOKEN_USER 开头，Sid 指向同一块缓冲。
            let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
            [WinLocalSystemSid, WinLocalServiceSid, WinNetworkServiceSid]
                .into_iter()
                .any(|kind| unsafe { IsWellKnownSid(sid, kind) }.as_bool())
        });
        if service {
            return Some("宿主以系统服务账号运行");
        }
        // 只认「UAC 开着、这个进程被单独提权」（Full）；UAC 关掉的机器上人人都是完整管理员令牌（Default），
        // 那不算提权，照常拉。
        let elevated = token_information(TokenElevationType).is_some_and(|buffer| {
            // SAFETY: TokenElevationType 缓冲就是一个 TOKEN_ELEVATION_TYPE（i32）。
            unsafe { *buffer.as_ptr().cast::<TOKEN_ELEVATION_TYPE>() == TokenElevationTypeFull }
        });
        elevated.then_some("宿主是提权进程")
    })
}

/// 读一项本进程令牌信息，原样返回缓冲（按 8 字节对齐）。
fn token_information(class: TOKEN_INFORMATION_CLASS) -> Option<Vec<u64>> {
    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.ok()?;
    let mut needed = 0u32;
    let _ = unsafe { GetTokenInformation(token, class, None, 0, &mut needed) };
    let mut buffer = vec![0u64; (needed as usize).div_ceil(8).max(1)];
    let read = unsafe {
        GetTokenInformation(
            token,
            class,
            Some(buffer.as_mut_ptr().cast()),
            (buffer.len() * 8) as u32,
            &mut needed,
        )
    };
    unsafe {
        let _ = CloseHandle(token);
    }
    read.ok()?;
    Some(buffer)
}
