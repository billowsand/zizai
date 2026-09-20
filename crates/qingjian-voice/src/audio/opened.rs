//! 已打开的麦克风流与它的采样格式。

pub(crate) struct OpenedInput {
    pub(crate) stream: cpal::Stream,

    pub(crate) sample_rate: u32,

    pub(crate) channels: u16,

    pub(crate) name: String,
}
