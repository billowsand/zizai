//! sherpa-onnx SenseVoice 包装。

use sherpa_onnx::{
    HomophoneReplacerConfig, OfflineModelConfig, OfflineRecognizer, OfflineRecognizerConfig,
    OfflineSenseVoiceModelConfig,
};

use crate::VoiceError;

use super::{AsrConfig, HrConfig};

pub(crate) struct AsrEngine {
    recognizer: OfflineRecognizer,
}

impl AsrEngine {
    pub(crate) fn new(config: &AsrConfig, hr: &HrConfig) -> Result<Self, VoiceError> {
        let model_config = OfflineModelConfig {
            sense_voice: OfflineSenseVoiceModelConfig {
                model: Some(config.model.clone()),
                language: Some(config.language.clone()),
                use_itn: true,
            },
            tokens: Some(config.tokens.clone()),
            num_threads: 4,
            debug: false,
            provider: Some("cpu".to_owned()),
            ..Default::default()
        };
        let hr = if hr.is_enabled() {
            HomophoneReplacerConfig {
                lexicon: hr.lexicon.clone(),
                rule_fsts: hr.rule_fsts.clone(),
            }
        } else {
            HomophoneReplacerConfig::default()
        };
        let recognizer = OfflineRecognizer::create(&OfflineRecognizerConfig {
            model_config,
            hr,
            ..Default::default()
        })
        .ok_or_else(|| VoiceError::Recognition("failed to create ASR recognizer".into()))?;
        Ok(Self { recognizer })
    }

    pub(crate) fn transcribe(&self, samples: &[f32]) -> Result<String, VoiceError> {
        let stream = self.recognizer.create_stream();
        stream.accept_waveform(16_000, samples);
        self.recognizer.decode(&stream);
        let result = stream
            .get_result()
            .ok_or_else(|| VoiceError::Recognition("ASR returned no result".into()))?;
        Ok(result.text.trim().to_owned())
    }
}
