//! 语音输入触发键；首版只支持不带修饰键的单个物理键。

use serde::{Deserialize, Serialize};

/// `[shortcut] voice` 的可选值，也是 Server 下发给 TSF 的按键约定。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceTrigger {
    /// 关闭语音快捷键。
    #[default]
    Off,

    /// 右 Alt。
    RightAlt,

    /// 右 Ctrl。
    RightCtrl,

    /// Caps Lock。
    CapsLock,

    /// Scroll Lock。
    ScrollLock,
}

impl VoiceTrigger {
    /// Windows 虚拟键码；`Off` 没有键码。
    pub const fn virtual_key(self) -> Option<u32> {
        match self {
            Self::Off => None,
            Self::RightAlt => Some(0xA5),
            Self::RightCtrl => Some(0xA3),
            Self::CapsLock => Some(0x14),
            Self::ScrollLock => Some(0x91),
        }
    }
}
