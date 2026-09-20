//! 编辑会话：TSF 不允许直接改文档，要经 `RequestEditSession` 申请、在回调里拿着 edit cookie 读写。
//! 写组句 / 上屏在 [`update`]，读光标前文与输入框私密判定（本地整句模型 / 密码框）在 [`surrounding`]，
//! 当前选区在 [`selection`]，候选窗口的定位锚点在 [`anchor`]。

mod anchor;
mod selection;
mod surrounding;
mod update;
mod voice_anchor;

pub(crate) use self::anchor::anchor_rect;
pub(crate) use self::selection::selection_start;
pub(crate) use self::surrounding::{InputContext, input_context};
pub(crate) use self::update::{request_update, request_voice_update};
pub(crate) use self::voice_anchor::request_voice_anchor;
