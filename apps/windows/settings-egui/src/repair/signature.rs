//! Server exe 的启动前提：带 uiAccess 的构建必须「签名在本机受信任 + 装在 Program Files」才起得来，
//! 否则外壳拉它直接失败（系统错误 740 / 8235），输入法在用户眼里就是「只能打英文」。

use std::path::Path;

use windows::Win32::Foundation::HWND;
use windows::Win32::Security::WinTrust::{
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO,
    WTD_CHOICE_FILE, WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
    WinVerifyTrust,
};
use windows::core::{HSTRING, PCWSTR};

/// exe 的清单里要了 uiAccess（`installer\build.ps1 -Sign` 打的包）。清单是 exe 资源里的一段 UTF-8 文本，直接找字面量。
pub(crate) fn wants_ui_access(exe: &Path) -> bool {
    let Ok(bytes) = std::fs::read(exe) else {
        return false;
    };
    let needle = br#"uiAccess="true""#;
    bytes.windows(needle.len()).any(|window| window == needle)
}

/// exe 的 Authenticode 签名在本机验得过（签了、没被改过、证书链到本机信任的根）。
/// 不查吊销：离线机器上查吊销会白等，而 uiAccess 的放行也不看它。
pub(crate) fn signature_trusted(exe: &Path) -> bool {
    let path = HSTRING::from(exe.as_os_str());
    let mut file = WINTRUST_FILE_INFO {
        cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(path.as_ptr()),
        ..Default::default()
    };
    let mut data = WINTRUST_DATA {
        cbStruct: size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 { pFile: &mut file },
        dwStateAction: WTD_STATEACTION_VERIFY,
        ..Default::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let status = unsafe {
        WinVerifyTrust(
            HWND::default(),
            &mut action,
            (&raw mut data).cast::<core::ffi::c_void>(),
        )
    };
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    let _ = unsafe {
        WinVerifyTrust(
            HWND::default(),
            &mut action,
            (&raw mut data).cast::<core::ffi::c_void>(),
        )
    };
    status == 0
}

/// exe 装在 uiAccess 认可的安全位置（`Program Files` / `Program Files (x86)` / `Windows`）。
pub(crate) fn in_secure_location(exe: &Path) -> bool {
    let exe = exe.to_string_lossy().to_lowercase();
    ["ProgramFiles", "ProgramFiles(x86)", "SystemRoot"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(|dir| {
            let mut dir = dir.to_string_lossy().to_lowercase();
            if !dir.ends_with('\\') {
                dir.push('\\');
            }
            dir
        })
        .any(|dir| exe.starts_with(&dir))
}
