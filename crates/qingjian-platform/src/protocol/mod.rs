//! Server ↔ DLL 的 IPC 协议类型。
//!
//! Windows 的 TSF DLL 会被加载进每一个应用进程，所以 [`qingjian_core::Engine`] 不能待在 DLL 里，
//! 得跑在独立的 Server 进程；DLL 只做 IPC，把系统按键翻译成 [`ClientMessage`] 发给 Server，
//! 把 Server 回的 [`ServerMessage`] 画到候选窗口。结构与 Weasel（WeaselServer）/ 水杉（Server 进程）一致。
//!
//! 这里的类型两端共用，必须可序列化（serde）。协议只描述「按键进、要画什么出」这一层，
//! 不复制 Core 的排序 / 词库逻辑：候选直接用 [`qingjian_core::CandidateList`]，preedit 分段用
//! [`PreeditSegment`]（Core 内部的 `MarkedSegment` 的可序列化镜像，避免协议耦合 Core 的内部枚举）。

mod client;
mod codec;
mod screen_rect;
mod server;
mod session;

#[cfg(test)]
mod tests;

/// 协议版本，DLL 开会话时带上。**改了这个目录里的任何类型就 +1**（`tests` 里的样例 JSON 会盯着，
/// 忘了改测试就红）；Server 对不上只记警告——升级安装后老 DLL 还留在没重启的应用里，得继续服务。
///
/// 两端不同版本还能对话，靠的是：
/// - **结构体缺字段退默认值**：[`Frame`] / [`PreeditSegment`] / [`KeyEvent`] / [`ScreenRect`] /
///   [`KeyModifiers`] 都是整个结构 `#[serde(default)]`，所以加字段、删字段、改名都不炸对面。
/// - **未知枚举名退到安全的一档**：[`PreeditKind`] 退 `Typed`、[`KeyOutcome`] 退 `Passthrough`。
/// - **未知消息变体整条跳过**：收的一方用 [`read_incoming`] 读，得到 [`Incoming::Unknown`] 就跳过
///   （老 DLL 是一问一答，等不到应答仍会失败，所以新消息还是要么等老 DLL 淘汰、要么由 Server
///   按会话版本降级发送，见 `dispatch::composed` 里对 [`PreeditKind::Fuma`] 的处理）。
///
/// 0.1.6 之前 [`Frame`] 的字段是必填的，删掉 `layout` 那次让所有没重启的应用每键都失败
/// （只能重启系统），上面第一条就是为这个加的。
///
/// 7 → 8 只有一处：`Frame` 加了有默认值的 `color_scheme`（旧 DLL 忽略它；给旧 DLL 的 `theme` 照旧写 `system`）。
/// 8 → 9 增加语音控制消息与 `ModeSync.voice`；新字段有默认值，旧 DLL 会忽略。
/// 9 → 10 增加语音电平与临时转写，供同一个候选窗口画录音态。
pub const PROTOCOL_VERSION: u32 = 10;

/// 从这版起 preedit 里可能出现 [`PreeditKind::Fuma`]；更早的 DLL 要降级成它认识的种类。
pub const FUMA_PREEDIT_PROTOCOL: u32 = 5;

pub mod frame;
pub mod key;
pub mod voice;

pub use client::ClientMessage;
pub use codec::{
    CodecError, DEFAULT_PIPE_NAME, Incoming, read_incoming, read_message, write_message,
};
pub use frame::{Frame, PreeditKind, PreeditSegment};
pub use key::{KeyEvent, KeyModifiers, KeyOutcome};
pub use screen_rect::ScreenRect;
pub use server::ServerMessage;
pub use session::SessionId;
pub use voice::{VoiceAction, VoiceDelivery, VoiceState, VoiceSync};
