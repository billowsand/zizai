//! `cfg(windows)`：Server 建内核对象用的安全描述符与本进程令牌查询。

mod descriptor;

pub use descriptor::SecurityDescriptor;

use windows::Win32::Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows::Win32::Security::{
    GetTokenInformation, TOKEN_INFORMATION_CLASS, TOKEN_QUERY, TOKEN_USER,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::PWSTR;

/// 单实例互斥体、让位事件、构建标记的 SDDL：只放行本用户与 SYSTEM，完整性标 Medium
/// （提权起的 Server 建的对象，普通权限的后来者也写得进）。`{user}` 换成本用户 SID。
/// 不像管道那样放行 Everyone / AppContainer：否则任何沙箱应用都能随时让输入法退出。
const OBJECT_SDDL: &str = "D:(A;;GA;;;{user})(A;;GA;;;SY)S:(ML;;NW;;;ME)";

/// 只有本用户（与 SYSTEM）能开的对象描述符。拿不到本用户 SID 时为空（默认 DACL，同样只放行本用户）。
pub fn object_descriptor() -> SecurityDescriptor {
    match current_user_sid() {
        Some(sid) => SecurityDescriptor::from_sddl(&OBJECT_SDDL.replace("{user}", &sid)),
        // 别给空 SDDL：那会转出一个「无 DACL」的描述符，等于谁都能开。
        None => SecurityDescriptor::system_default(),
    }
}

/// 本进程令牌的用户 SID（`S-1-5-21-…`）。
pub fn current_user_sid() -> Option<String> {
    let buffer = token_information(windows::Win32::Security::TokenUser)?;
    // SAFETY: TokenUser 的缓冲以 TOKEN_USER 开头，Sid 指向同一块缓冲，buffer 存活期间有效。
    let user = unsafe { &*(buffer.as_ptr().cast::<TOKEN_USER>()) };
    let mut text = PWSTR::null();
    unsafe { ConvertSidToStringSidW(user.User.Sid, &mut text) }.ok()?;
    let sid = unsafe { text.to_string() }.ok();
    unsafe {
        let _ = LocalFree(Some(HLOCAL(text.0.cast())));
    }
    sid
}

/// 读一项本进程令牌信息，原样返回缓冲（按 8 字节对齐）。
pub fn token_information(class: TOKEN_INFORMATION_CLASS) -> Option<Vec<u64>> {
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
