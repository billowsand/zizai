//! SenseVoice 模型配置。

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AsrConfig {
    pub(crate) model: String,

    pub(crate) tokens: String,

    pub(crate) language: String,
}
