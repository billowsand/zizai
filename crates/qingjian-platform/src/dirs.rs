//! Windows 上用户目录的约定，Server / TSF DLL / 设置程序三处共用：
//!
//! - **数据** `%APPDATA%\Qingjian`：配置、密钥、学习数据、统计、输入日志（漫游目录，跟着账户走）。
//! - **日志** `%LOCALAPPDATA%\Qingjian\logs`：三个进程的运行日志都在这一个目录里，
//!   `server.<日期>.log` / `tsf.<日期>.log` / `settings.<日期>.log`，用户反馈问题时整个目录打包即可。
//!   放本机目录有两个原因：日志本来就是这台机器的东西，不该跟账户漫游；DLL 被加载进
//!   AppContainer 应用（任务栏搜索 / 设置）时写不了漫游目录，Server 启动时会给这个目录授权。
//!
//! 只查 Windows 的环境变量，不碰系统 API。

use std::path::PathBuf;

/// 用户数据目录 `%APPDATA%\Qingjian`。
pub fn user_dir() -> Option<PathBuf> {
    roaming_base().map(|base| base.join("Qingjian"))
}

/// 配置文件 `%APPDATA%\Qingjian\config.toml`。
pub fn config_path() -> Option<PathBuf> {
    user_dir().map(|dir| dir.join("config.toml"))
}

/// 运行日志目录 `%LOCALAPPDATA%\Qingjian\logs`，不负责创建。
pub fn log_dir() -> Option<PathBuf> {
    local_base().map(|base| base.join("Qingjian").join("logs"))
}

/// 漫游基目录：`%APPDATA%`，缺失时（进程环境被剥离，例如提权 / 安装器 ShellExecute 拉起）回落
/// `%USERPROFILE%\AppData\Roaming`。日志与配置据此定位，环境不全也不至于既不写日志又不读配置。
fn roaming_base() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(PathBuf::from).or_else(|| {
        std::env::var_os("USERPROFILE")
            .map(|profile| PathBuf::from(profile).join("AppData").join("Roaming"))
    })
}

/// 本机基目录：`%LOCALAPPDATA%`，缺失时回落 `%USERPROFILE%\AppData\Local`。
fn local_base() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(|profile| PathBuf::from(profile).join("AppData").join("Local"))
        })
}
