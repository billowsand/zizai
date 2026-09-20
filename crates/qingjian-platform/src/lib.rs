//! 平台层共用的、与具体窗口系统无关的部分：配置文件，以及将来 Core 与壳之间的协议类型。
//!
//! 这里的类型必须可序列化：Windows 上 Core 在独立 Server 进程，协议类型两边都用。

mod config;
pub mod dirs;
mod error;
pub mod extra_dictionaries;
pub mod logs;
pub mod protocol;
pub mod resources;

pub use config::{
    ColorScheme, Config, DEFAULT_DOMAINS, DEFAULT_PAGE_KEYS, DictionariesConfig, GeneralConfig,
    LEARNING_LANGUAGE_OFF, LocalModelConfig, LogLevel, MAX_PAGE_SIZE, Modifiers, PAGE_KEY_OPTIONS,
    PreeditMode, ShortcutConfig, ThemeMode, VoiceConfig, VoiceTrigger,
};
pub use error::ConfigError;
