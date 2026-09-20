//! 设置窗口状态：当前配置、`config.toml` 路径、当前分节、系统字族表，以及启动计时。
//! 每帧怎么画在 [`update`]。

mod update;

use std::path::{Path, PathBuf};
use std::time::Instant;

use qingjian_platform::{ColorScheme, Config};

use crate::{fonts, theme};

/// 左侧导航的分节：tag + 界面名 + 图标码点。
pub(crate) const PAGES: [(&str, &str, &str); 6] = [
    ("general", "通用", "\u{E713}"),
    ("voice", "语音输入", "\u{E720}"),
    ("appearance", "候选窗口", "\u{E890}"),
    ("dictionaries", "词库", "\u{E8F1}"),
    ("advanced", "高级", "\u{E90F}"),
    ("about", "关于", "\u{E897}"),
];

pub(crate) struct Settings {
    /// 当前配置，每次改动后从盘上重读。
    pub(crate) config: Config,

    /// `config.toml` 路径。
    path: PathBuf,

    /// 当前导航分节 tag。
    page: String,

    /// 系统里的字族名（DirectWrite 列举），「字体」下拉用；开窗时列一次。
    pub(crate) families: Vec<String>,

    /// 打开设置时列出的输入设备；实际采音仍由独立语音 Worker 完成。
    pub(crate) voice_devices: Vec<String>,

    /// 当前 Windows 默认输入设备名，只用于设置页说明。
    pub(crate) default_voice_device: Option<String>,

    /// 大模型服务地址的编辑缓冲；停键或失去焦点时落盘。
    pub(crate) voice_polish_url_edit: String,

    /// 大模型名称的编辑缓冲。
    pub(crate) voice_polish_model_edit: String,

    /// 服务地址或模型名称最后一次编辑时间，用来合并连续输入的落盘。
    voice_polish_dirty_since: Option<Instant>,

    /// 上次解析出的系统明暗，变了换一套 Visuals。
    pub(crate) dark: bool,

    /// 已装进 egui Visuals 的色系，配置热切换时据此判断是否重建。
    pub(crate) applied_scheme: ColorScheme,

    /// 进程启动的时刻，首帧画完时报一次耗时。
    started: Instant,

    /// 首帧是否已经报过耗时。
    reported: bool,
}

impl Settings {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>, started: Instant) -> Self {
        let path = Self::config_path();
        let config = Config::load(&path).unwrap_or_default();
        let applied_scheme = config.general.theme;
        fonts::install(&cc.egui_ctx, &config.general.font);
        theme::install(&cc.egui_ctx, applied_scheme);
        let voice_devices = crate::voice_devices::scan();
        let voice_polish_url_edit = config.voice.polish_url.clone();
        let voice_polish_model_edit = config.voice.polish_model.clone();
        Self {
            config,
            path,
            page: "general".to_owned(),
            families: qingjian_render::system_fonts::families(),
            voice_devices: voice_devices.available,
            default_voice_device: voice_devices.default,
            voice_polish_url_edit,
            voice_polish_model_edit,
            voice_polish_dirty_since: None,
            dark: theme::system_prefers_dark(),
            applied_scheme,
            started,
            reported: false,
        }
    }

    /// `%APPDATA%\Qingjian\config.toml`；取不到 `APPDATA` 退回工作目录。
    fn config_path() -> PathBuf {
        qingjian_platform::dirs::config_path().unwrap_or_else(|| PathBuf::from("config.toml"))
    }

    /// 配置文件本身，「高级」页要打开它。
    pub(crate) fn config_file(&self) -> &Path {
        &self.path
    }

    /// 数据目录 `%APPDATA%\Qingjian`。
    pub(crate) fn data_dir(&self) -> &Path {
        self.path.parent().unwrap_or_else(|| Path::new("."))
    }

    /// 落盘一个配置值再重读；失败写进设置程序日志输出。
    pub(crate) fn save(&mut self, section: &str, key: &str, value: impl Into<toml_edit::Value>) {
        if let Err(error) = Config::set_value(&self.path, section, key, value) {
            eprintln!("保存 [{section}] {key} 失败: {error}");
            return;
        }
        self.reload();
    }

    /// 落盘一个字符串数组再重读。
    pub(crate) fn save_array(&mut self, section: &str, key: &str, values: &[String]) {
        if let Err(error) = Config::set_array(&self.path, section, key, values) {
            eprintln!("保存 [{section}] {key} 失败: {error}");
            return;
        }
        self.reload();
    }

    pub(crate) fn reload(&mut self) {
        if let Ok(config) = Config::load(&self.path) {
            self.config = config;
        }
    }

    /// 记下大模型连接配置正在编辑；停键后合并写回，避免每个字符都重启语音 Worker。
    pub(crate) fn voice_polish_edited(&mut self) {
        self.voice_polish_dirty_since = Some(Instant::now());
    }

    /// 写回大模型连接配置；`force` 用于失焦或离开页面时立即保存。
    pub(crate) fn flush_voice_polish_edits(&mut self, force: bool) {
        let Some(since) = self.voice_polish_dirty_since else {
            return;
        };
        if !force && since.elapsed() < std::time::Duration::from_millis(500) {
            return;
        }
        let values = [
            ("polish_url", self.voice_polish_url_edit.trim().to_owned()),
            (
                "polish_model",
                self.voice_polish_model_edit.trim().to_owned(),
            ),
        ];
        for (key, value) in values {
            if let Err(error) = Config::set_value(&self.path, "voice", key, value) {
                eprintln!("保存 [voice] {key} 失败: {error}");
                return;
            }
        }
        self.voice_polish_dirty_since = None;
        self.reload();
    }
}
