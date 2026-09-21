//! Worker 启动配置。

use serde::{Deserialize, Serialize};

use qingjian_platform::PolishLevel;

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

    pub punctuation_model: Option<String>,

    /// 整理档位；None 为关闭。开了才读 polish_url / polish_model。
    pub polish: Option<PolishLevel>,

    /// OpenAI 兼容服务根地址（已解析，绝对）。
    pub polish_url: Option<String>,

    /// 服务中的模型名称。
    pub polish_model: Option<String>,

    /// 专名保护表：换行分隔，注入 system prompt，让模型按原样保留用户自己建立的写法。
    pub hotwords: Option<String>,

    pub hr_lexicon: Option<String>,

    pub hr_rule_fsts: Option<String>,
}
