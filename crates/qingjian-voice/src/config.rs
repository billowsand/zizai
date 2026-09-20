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

    /// 检测到有效语音后，连续静音多久自动结束；0 表示关闭。
    pub auto_stop_ms: u64,

    pub hr_lexicon: Option<String>,

    pub hr_rule_fsts: Option<String>,
}
