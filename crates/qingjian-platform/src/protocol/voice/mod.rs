//! Windows 语音输入协议类型。

mod action;
mod delivery;
mod state;
mod sync;

pub use action::VoiceAction;
pub use delivery::VoiceDelivery;
pub use state::VoiceState;
pub use sync::VoiceSync;
