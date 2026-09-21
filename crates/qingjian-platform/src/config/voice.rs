//! Windows 语音输入配置；路径由壳相对安装根目录解析。

use serde::{Deserialize, Serialize};

/// 大模型整理档位：体现允许的改动强度。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolishLevel {
    /// 关闭，直接用识别结果。
    #[default]
    Off,

    /// 只去口语填充词，不换任何用词；改动红线最严。
    Spoken,

    /// 去填充词后转成书面表达，允许改写措辞；整段差异红线较宽。
    Written,
}

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

    /// 本地标点恢复模型自动加载（随包 `data\voice\punctuation\model.int8.onnx` 存在时生效）；文件不在则静默降级。
    pub punctuation: bool,

    /// 本地标点恢复模型的显式路径覆盖；空则用随包路径。
    pub punctuation_model: String,

    /// 同音词替换资源自动加载（随包 `data\voice\hr\` 存在时生效）。
    pub hr: bool,

    /// 大模型整理档位；配置缺省没有 `polish` 键时回退旧 `polish_enabled`（开 = `Spoken`）。
    pub polish: Option<PolishLevel>,

    /// 旧配置的语句整理总开关；只用于迁移，新配置写 `polish`。
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

impl VoiceConfig {
    /// 生效的整理档位：显式 `polish` 键优先，老配置回退 `polish_enabled`。
    pub fn polish_level(&self) -> PolishLevel {
        match self.polish {
            Some(level) => level,
            None if self.polish_enabled => PolishLevel::Spoken,
            None => PolishLevel::Off,
        }
    }
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
            punctuation: true,
            punctuation_model: String::new(),
            hr: true,
            polish: None,
            polish_enabled: false,
            polish_url: "http://localhost:1234".into(),
            polish_model: "local-model".into(),
            hr_lexicon: String::new(),
            hr_rule_fsts: String::new(),
        }
    }
}
