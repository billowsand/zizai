//! 修复任务与界面之间共享的进度：后台线程写，界面每帧读。

use super::Diagnosis;

/// 修复的当前进度。
#[derive(Debug, Clone, Default)]
pub(crate) struct Progress {
    /// 有修复正在跑：按钮置灰，别再起一份。
    pub(crate) busy: bool,

    /// 正在做哪一步，或上一次修复的结果，给用户看的一句话。
    pub(crate) message: Option<String>,

    /// 静态诊断（后台查完之前为默认值，即「没查出问题」）。
    pub(crate) diagnosis: Diagnosis,
}
