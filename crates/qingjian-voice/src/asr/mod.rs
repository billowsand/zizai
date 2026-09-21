//! 本地离线语音识别。

mod config;
mod engine;
mod hr;
mod punct;

pub(crate) use config::AsrConfig;
pub(crate) use engine::AsrEngine;
pub(crate) use hr::HrConfig;
pub(crate) use punct::Punctuator;
