//! Worker 启动配置。

use serde::{Deserialize, Serialize};

/// Server 解析好绝对路径后交给 Worker 的配置快照。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkerConfig {
    pub model: String,

    pub tokens: String,

    pub language: String,

    pub input_device: Option<String>,

    pub hr_lexicon: Option<String>,

    pub hr_rule_fsts: Option<String>,
}
