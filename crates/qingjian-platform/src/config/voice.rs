//! Windows 语音输入配置；路径由壳相对安装根目录解析。

use serde::{Deserialize, Serialize};

/// 配置文件 `[voice]` 分节。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoiceConfig {
    /// 是否启用语音输入。模型不随仓库提供，缺省关闭。
    pub enabled: bool,

    /// SenseVoice ONNX 模型路径。
    pub model: String,

    /// SenseVoice token 表路径。
    pub tokens: String,

    /// 识别语言提示。
    pub language: String,

    /// 麦克风设备名；空为系统默认。
    pub input_device: String,

    /// 检测到说话后，连续静音多久自动结束录音；0 表示关闭。
    pub auto_stop_ms: u64,

    /// 是否用 OpenAI 兼容的大模型服务整理最终转写。
    pub polish_enabled: bool,

    /// OpenAI 兼容服务根地址，例如 LM Studio。
    pub polish_url: String,

    /// 服务中的模型名称。
    pub polish_model: String,

    /// sherpa-onnx 同音词词典路径；空为关闭。
    pub hr_lexicon: String,

    /// sherpa-onnx 同音词 FST 路径；空为关闭。
    pub hr_rule_fsts: String,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model: "data/voice/sense-voice/model.int8.onnx".into(),
            tokens: "data/voice/sense-voice/tokens.txt".into(),
            language: "auto".into(),
            input_device: String::new(),
            auto_stop_ms: 0,
            polish_enabled: false,
            polish_url: "http://localhost:1234".into(),
            polish_model: "local-model".into(),
            hr_lexicon: String::new(),
            hr_rule_fsts: String::new(),
        }
    }
}
