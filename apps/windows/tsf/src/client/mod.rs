//! DLL 的「引擎层」：不跑 Engine，而是连独立 Server 进程，把按键 / 上屏编排成协议消息。
//! [`EngineClient`] 泛型在任意双工字节流上，可脱离 Windows 端到端测；`cfg(windows)` 的 [`pipe`] 负责连管道，
//! [`launch`] 在 Server 没起时把它拉起来（宿主身份与会话号在 [`host`]），[`mismatch`] 管「升级后本进程还加载着旧 DLL」这种协议对不上的情况。

mod engine;
mod response;

#[cfg(windows)]
pub mod host;
#[cfg(windows)]
pub mod launch;
#[cfg(windows)]
pub mod mismatch;
#[cfg(windows)]
pub mod pipe;

pub use engine::EngineClient;
pub use response::KeyResponse;
