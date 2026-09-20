//! 配置热加载：空闲时看 `config.toml` 的 mtime，改了就重读并应用（与 macOS 壳对齐）。
//! 便宜的设置无条件重设；附加词库只在对应项变了才重建。热加载状态在 [`ConfigReload`]。

mod state;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use qingjian_core::{FumaScheme, FumaTable};
use qingjian_platform::{Config, extra_dictionaries};

pub(super) use self::state::ConfigReload;

/// 看配置文件 mtime 的最短间隔；工人循环空闲时按它等，重排的短节拍来得更勤时按这个节流。
pub(super) const CONFIG_POLL_INTERVAL: Duration = Duration::from_secs(1);
use super::{Router, RouterConfig};
use crate::assembly::user_dicts_dir;
use crate::voice::ProcessVoiceBackend;

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
}

/// 随包辅码表（`assets/fuma/<方案>.txt`）；读不到就当辅码关着。启动与热加载共用。
pub fn load_fuma(root: &Path, scheme: FumaScheme) -> Option<Arc<FumaTable>> {
    let path = root.join("assets").join(scheme.asset());
    match FumaTable::from_path(&path) {
        Ok(table) => {
            tracing::info!(scheme = scheme.key(), words = table.len(), "辅码表已加载");
            Some(Arc::new(table))
        }
        Err(error) => {
            tracing::error!(scheme = scheme.key(), %error, path = %path.display(), "辅码表加载失败，辅码关");
            None
        }
    }
}

impl Router {
    /// `config.toml` 路径；没开热加载（测试）时为 `None`。
    pub(super) fn config_path(&self) -> Option<&Path> {
        self.reload
            .as_ref()
            .map(|reload| reload.config_path.as_path())
    }

    /// 开启热加载：记下路径与当前已应用的 dictionaries。
    pub fn watch_config(
        &mut self,
        config: &Config,
        config_path: PathBuf,
        root: PathBuf,
        user_dir: Option<PathBuf>,
    ) {
        let last_mtime = mtime(&config_path);
        let bundled_dicts_dir = Some(root.join("data/generated/dicts")).filter(|dir| dir.is_dir());
        let voice_worker = std::env::current_exe().ok().and_then(|path| {
            path.parent()
                .map(|parent| parent.join("qingjian-voice-worker.exe"))
        });
        self.reload = Some(ConfigReload {
            config_path,
            last_check: Instant::now(),
            root,
            bundled_dicts_dir,
            user_dir,
            last_mtime,
            applied_dictionaries: config.dictionaries.clone(),
            applied_voice: config.voice.clone(),
            applied_voice_trigger: config.shortcut.voice,
            voice_worker,
        });
    }

    /// 空闲时调；一秒内只真正看一次文件。解析失败保持原配置，mtime 照记（不每秒重试同一个坏文件）。
    pub fn poll_config_reload(&mut self) {
        let Some(reload) = &mut self.reload else {
            return;
        };
        if reload.last_check.elapsed() < CONFIG_POLL_INTERVAL {
            return;
        }
        reload.last_check = Instant::now();
        let current = mtime(&reload.config_path);
        if current == reload.last_mtime {
            return;
        }
        reload.last_mtime = current;
        let path = reload.config_path.clone();
        match Config::load(&path) {
            Ok(config) => {
                self.apply_config(&config);
                tracing::info!("配置已热加载");
            }
            Err(error) => tracing::error!(%error, "配置热加载解析失败，保持原配置"),
        }
    }

    /// 辅码按新配置重接。启动时辅码是关的就没读过表，用户在设置里刚打开时现读一次，
    /// 否则开关只在重启后才生效。
    fn apply_fuma(&mut self, config: &Config) {
        let Some(scheme) = config.general.fuma() else {
            self.engine.set_fuma(None);
            return;
        };
        if self.fuma_table.is_none()
            && let Some(root) = self.reload.as_ref().map(|reload| reload.root.clone())
        {
            self.fuma_table = load_fuma(&root, scheme);
        }
        self.engine.set_fuma(self.fuma_table.clone());
    }

    /// 应用新配置。
    fn apply_config(&mut self, config: &Config) {
        self.engine.set_shuangpin(config.general.shuangpin());
        self.apply_fuma(config);
        self.engine.set_learning(config.general.learning);
        self.engine.set_chinese_first(config.general.chinese_first);
        let previous_font = self.config.font.clone();
        let previous_scheme = self.config.color_scheme;
        self.config = RouterConfig::from(config);
        if self.config.font != previous_font {
            self.candidates.set_font(self.config.font.clone());
        }
        if self.config.font != previous_font || self.config.color_scheme != previous_scheme {
            self.last_shown = None;
            let frame = self.current_frame();
            self.reconcile_candidates(&frame);
        }
        self.reconcile_status();
        self.apply_model_config(&config.model);
        self.apply_voice_config(config);

        let Some(reload) = &mut self.reload else {
            return;
        };
        if config.dictionaries != reload.applied_dictionaries {
            // 别传用户目录本身：那里的学习数据 .tsv 会被当词库装。
            let dicts = extra_dictionaries::load(
                reload.bundled_dicts_dir.as_deref(),
                user_dicts_dir(reload.user_dir.as_deref()).as_deref(),
                &config.dictionaries,
            );
            tracing::info!(count = dicts.len(), "附加词库已热重装");
            self.engine.set_extra_dictionaries(dicts);
            reload.applied_dictionaries = config.dictionaries.clone();
        }
    }

    fn apply_voice_config(&mut self, config: &Config) {
        let Some(reload) = self.reload.as_ref() else {
            return;
        };
        if config.voice == reload.applied_voice
            && config.shortcut.voice == reload.applied_voice_trigger
        {
            return;
        }
        let root = reload.root.clone();
        let worker = reload.voice_worker.clone();
        if let Some(reload) = self.reload.as_mut() {
            reload.applied_voice = config.voice.clone();
            reload.applied_voice_trigger = config.shortcut.voice;
        }
        self.voice.disable();
        if !config.voice.enabled || config.shortcut.voice.virtual_key().is_none() {
            tracing::info!("语音输入已关闭");
            return;
        }
        let Some(worker) = worker else {
            self.voice
                .configure_failure(config.shortcut.voice, "找不到语音工作进程".into());
            return;
        };
        match ProcessVoiceBackend::spawn_configured(&worker, &root, &config.voice) {
            Ok(backend) => {
                self.voice
                    .configure(config.shortcut.voice, Box::new(backend));
                tracing::info!("语音配置已热加载");
            }
            Err(error) => {
                tracing::error!(%error, "热加载语音 Worker 失败");
                self.voice
                    .configure_failure(config.shortcut.voice, "语音工作进程启动失败".into());
            }
        }
    }
}
