//! 本进程的运行上下文：会话号、是不是服务账号、有没有被提权、有没有拿到 uiAccess。
//! 决定 Server 该不该跑（[`refusal`](ProcessContext::refusal)）与跑得理不理想（[`degraded`](ProcessContext::degraded)）。

use windows::Win32::Security::{
    IsWellKnownSid, TOKEN_ELEVATION_TYPE, TOKEN_USER, TokenElevationType, TokenElevationTypeFull,
    TokenUIAccess, TokenUser, WinLocalServiceSid, WinLocalSystemSid, WinNetworkServiceSid,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::GetCurrentProcessId;

use crate::security::token_information;

/// 这份 exe 编进了 uiAccess manifest（`QINGJIAN_UIACCESS=1`，见 build.rs）。
const UI_ACCESS_BUILD: bool = match option_env!("QINGJIAN_SERVER_UIACCESS") {
    Some(value) => matches!(value.as_bytes(), [b'1']),
    None => false,
};

/// 查不到的项按「正常」算：宁可照常跑，也别因为查询失败把 Server 拒掉。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessContext {
    /// 登录会话号；0 是服务会话。
    pub session: u32,

    /// 以 SYSTEM / LocalService / NetworkService 身份在跑。
    pub service_account: bool,

    /// 令牌被单独提权（UAC 开着时「以管理员身份运行」的那种）。
    pub elevated: bool,

    /// 令牌带 uiAccess（候选窗才能盖过开始菜单 / 任务栏搜索）。
    pub ui_access: bool,
}

impl ProcessContext {
    pub fn current() -> Self {
        let mut session = u32::MAX;
        if unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) }.is_err() {
            session = u32::MAX;
        }
        let service_account = token_information(TokenUser).is_some_and(|buffer| {
            // SAFETY: TokenUser 缓冲以 TOKEN_USER 开头，Sid 指向同一块缓冲。
            let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
            [WinLocalSystemSid, WinLocalServiceSid, WinNetworkServiceSid]
                .into_iter()
                .any(|kind| unsafe { IsWellKnownSid(sid, kind) }.as_bool())
        });
        // 只认「UAC 开着、本进程被单独提权」（Full）。UAC 关掉的机器上所有进程都是完整管理员令牌
        // （Default），那是常态，不算降级。
        let elevated = token_information(TokenElevationType).is_some_and(|buffer| {
            // SAFETY: TokenElevationType 缓冲就是一个 TOKEN_ELEVATION_TYPE（i32）。
            unsafe { *buffer.as_ptr().cast::<TOKEN_ELEVATION_TYPE>() == TokenElevationTypeFull }
        });
        let ui_access = token_information(TokenUIAccess)
            .is_some_and(|buffer| buffer.first().is_some_and(|word| *word as u32 != 0));
        Self {
            session,
            service_account,
            elevated,
            ui_access,
        }
    }

    /// 根本不该跑的上下文，返回原因。服务会话 / 服务账号里的 Server 画不到用户桌面上，
    /// 读写的还是系统配置目录；让它占着管道只会挡住用户自己那份。
    pub fn refusal(&self) -> Option<&'static str> {
        if self.session == 0 {
            Some("在服务会话（会话 0）里")
        } else if self.service_account {
            Some("以系统服务账号身份")
        } else {
            None
        }
    }

    /// 能跑但不理想：被提权（用的是提权那一刻的环境与配置），或者该有 uiAccess 却没拿到
    /// （候选窗会被开始菜单 / 任务栏搜索盖住）。降级的现任会被正常的后来者接管。
    pub fn degraded(&self) -> bool {
        self.elevated || (UI_ACCESS_BUILD && !self.ui_access)
    }
}
