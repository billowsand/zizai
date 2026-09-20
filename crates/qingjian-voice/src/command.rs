//! Controller 线程内部命令。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Command {
    Start(u64),
    Stop(u64),
    Cancel(u64),
    Shutdown,
}
