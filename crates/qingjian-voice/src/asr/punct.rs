//! sherpa-onnx CT-Transformer 离线标点恢复包装。

use sherpa_onnx::{OfflinePunctuation, OfflinePunctuationConfig};

use crate::VoiceError;

/// 给 SenseVoice 原文补标点；识别只跑一次，模型加载成本摊在每轮会话上。
pub(crate) struct Punctuator {
    punctuator: OfflinePunctuation,
}

impl Punctuator {
    pub(crate) fn new(model: Option<&str>) -> Result<Option<Self>, VoiceError> {
        let Some(model) = model.filter(|value| !value.trim().is_empty()) else {
            return Ok(None);
        };
        let punctuator = OfflinePunctuation::create(&OfflinePunctuationConfig {
            model: sherpa_onnx::OfflinePunctuationModelConfig {
                ct_transformer: Some(model.to_owned()),
                num_threads: 2,
                debug: false,
                provider: Some("cpu".to_owned()),
            },
        })
        .ok_or_else(|| VoiceError::Recognition("failed to create punctuation model".into()))?;
        Ok(Some(Self { punctuator }))
    }

    pub(crate) fn add(&self, text: &str) -> String {
        self.punctuator
            .add_punctuation(text)
            .unwrap_or_else(|| text.to_owned())
    }
}
