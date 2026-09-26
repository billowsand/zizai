//! 候选窗口的一行：序号、候选词、annotation 片段。

use super::tone::Tone;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// 显示用序号文本，如 `1`。
    pub index: String,

    /// 候选词。
    pub text: String,

    /// 右侧 annotation，按顺序绘制；没有译文时为空。
    pub annotation: Vec<(String, Tone)>,

    /// 来自云联想：词前画一个小云朵，与本地候选区分。
    pub cloud: bool,

    /// 本地整句模型拼出的整句：词后右上角画一个小星标，与词库里现成的词区分。
    pub sentence: bool,

    /// 词后右上角的淡色小字（辅码：还要敲的码 `s`，学码时完整两码 `ms`）。跟在词的同一行，
    /// 不像 annotation 那样另起一行，所以有没有它候选框高度都不变。
    pub corner: Option<String>,
}

impl Row {
    /// 只有序号和候选词的一行。
    pub fn plain(index: usize, text: impl Into<String>) -> Self {
        Self {
            index: (index + 1).to_string(),
            text: text.into(),
            annotation: Vec::new(),
            cloud: false,
            sentence: false,
            corner: None,
        }
    }
}
