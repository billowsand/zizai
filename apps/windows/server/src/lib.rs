//! Server 的库部分：Engine 装配（[`assembly`]）、协议分派（[`Router`]）与传输（[`ipc`]），供 bin 与集成测试共用。

pub mod assembly;
pub mod dispatch;
pub mod error;
/// 单实例与接管；仅 Windows。
#[cfg(windows)]
pub mod instance;
pub mod ipc;
/// 进程环境缺用户目录变量时按已知文件夹补上；仅 Windows。
#[cfg(windows)]
pub mod known_folders;
/// 内核对象的安全描述符与令牌查询；仅 Windows。
#[cfg(windows)]
pub mod security;
/// 候选窗口 / 状态条的自绘线程；仅 Windows。
#[cfg(windows)]
pub mod ui;
pub mod voice;

pub use assembly::{AssemblySpec, LanguageModelFiles};
pub use dispatch::{Router, RouterConfig};
pub use error::ServerError;
pub use voice::ProcessVoiceBackend;
