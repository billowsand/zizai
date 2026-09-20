//! 配置热加载记的状态。

use std::path::PathBuf;
use std::time::{Instant, SystemTime};

use qingjian_platform::{DictionariesConfig, VoiceConfig, VoiceTrigger};

/// 热加载状态。
pub(crate) struct ConfigReload {
    /// `config.toml` 路径。
    pub(super) config_path: PathBuf,

    /// 上次看文件的时间（节流用）。
    pub(super) last_check: Instant,

    /// 安装根目录，随包资源（辅码表这类启动时可能没装上的）热加载时按它现读。
    pub(super) root: PathBuf,

    /// 随包领域词库目录。
    pub(super) bundled_dicts_dir: Option<PathBuf>,

    /// 用户数据目录（导入词库在其 `dicts/` 下）。
    pub(super) user_dir: Option<PathBuf>,

    /// 上次看到的 mtime。
    pub(super) last_mtime: Option<SystemTime>,

    /// 已应用的 `[dictionaries]`。
    pub(super) applied_dictionaries: DictionariesConfig,

    /// 上次应用的语音模型配置与快捷键；任一变化时重启 Worker。
    pub(super) applied_voice: VoiceConfig,

    pub(super) applied_voice_trigger: VoiceTrigger,

    /// 与 Server 同目录的语音 Worker。
    pub(super) voice_worker: Option<PathBuf>,
}
