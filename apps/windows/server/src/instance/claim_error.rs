//! 抢不到单实例时的原因。

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClaimError {
    /// 同一份程序已经在跑、而且跑得好好的：本进程该安静退出，不打扰它。
    #[error("a healthy server of the same build is already running")]
    AlreadyRunning,

    /// 请现任让位了，但它在等待上限内没退（老版本不认让位事件，或卡住了）。
    #[error("the running server did not step down in time")]
    Timeout,

    /// 建 / 开单实例互斥体失败（例如会话里另一个用户的 Server 建的，本用户开不了）。
    #[error("cannot open the single-instance mutex: {0}")]
    Mutex(windows::core::Error),
}
