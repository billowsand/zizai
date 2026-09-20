//! 本地离线语音识别。

mod config;
mod engine;
mod hr;

pub(crate) use config::AsrConfig;
pub(crate) use engine::AsrEngine;
pub(crate) use hr::HrConfig;
