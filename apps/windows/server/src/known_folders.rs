//! `cfg(windows)`：进程环境里缺 `APPDATA` / `LOCALAPPDATA` 时（被某个剥了环境的宿主拉起），
//! 按系统的已知文件夹补上。配置、学习数据、日志都按这两个变量定位（`qingjian_platform::dirs`），
//! 语音 Worker 也继承这份环境；补不上就是既不读配置、又不写日志的 Server。

use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{
    FOLDERID_LocalAppData, FOLDERID_RoamingAppData, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
};
use windows::core::GUID;

/// 在 `main` 最前面、起任何线程之前调。
pub fn fill_missing() {
    for (variable, folder) in [
        ("APPDATA", &FOLDERID_RoamingAppData),
        ("LOCALAPPDATA", &FOLDERID_LocalAppData),
    ] {
        if std::env::var_os(variable).is_some() {
            continue;
        }
        if let Some(path) = known_folder(folder) {
            // SAFETY: 进程刚起、还是单线程，没有别的线程在读环境变量。
            unsafe { std::env::set_var(variable, path) };
        }
    }
}

fn known_folder(id: &GUID) -> Option<String> {
    let path = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) }.ok()?;
    let text = unsafe { path.to_string() }.ok();
    unsafe { CoTaskMemFree(Some(path.0.cast())) };
    text
}
