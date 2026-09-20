//! 一次等待 TSF 写进文档的语音识别结果。

use serde::{Deserialize, Serialize};

/// Server 会重复交付到 TSF 确认为止。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoiceDelivery {
    /// 听写请求编号，用于去重和确认。
    pub request: u64,

    /// 要写进文档的最终文本。
    pub text: String,
}
