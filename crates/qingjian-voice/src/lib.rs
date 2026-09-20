//! Windows 本地语音识别 Worker 的可复用实现。
//!
//! ASR、麦克风与重采样实现源自 auto-voice（MIT，Copyright (c) 2026 billowsand），
//! 已移除全局键盘钩子、剪贴板、模拟粘贴、托盘与 OSD。

mod command;
mod config;
mod controller;
mod error;
mod request;
mod response;
mod snapshot;

#[cfg(windows)]
mod asr;
#[cfg(windows)]
mod audio;

pub use config::WorkerConfig;
pub use controller::Controller;
pub use error::VoiceError;
pub use request::WorkerRequest;
pub use response::WorkerResponse;
pub use snapshot::WorkerSnapshot;
