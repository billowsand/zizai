//! 用私有 stdio 帧协议管理独立语音 Worker 进程。
//!
//! 管道读写全在自己的线程上：Router 是单线程工人循环，所有应用的按键都排在它后面，
//! 在那上面同步等 Worker 回话，等于把「音频 / ONNX 原生代码卡住」直接变成「全系统打不出字」。
//! 这里只留两个不会阻塞的接口：命令投进有界队列，状态读共享快照。

mod shared;
mod worker;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::thread::JoinHandle;

use qingjian_platform::{PolishLevel, VoiceConfig};
use qingjian_voice::{WorkerConfig, WorkerRequest, WorkerSnapshot};

use self::shared::SharedSnapshot;
use self::worker::Worker;
use super::{VoiceBackend, VoiceBackendError};

/// 独立 `qingjian-voice-worker.exe` 后端。
pub struct ProcessVoiceBackend {
    commands: Sender<WorkerRequest>,

    snapshot: SharedSnapshot,

    /// `Drop` 里 join，保证换配置时旧进程先收摊再算数。
    thread: Option<JoinHandle<()>>,
}

impl ProcessVoiceBackend {
    /// 把用户配置中的相对路径按随包根展开后启动 Worker；热词是润色的专名保护表。
    pub fn spawn_configured(
        executable: &Path,
        root: &Path,
        config: &VoiceConfig,
        hotwords: Vec<String>,
    ) -> Result<Self, VoiceBackendError> {
        let hotwords = (!hotwords.is_empty()).then(|| hotwords.join("\n"));
        let resolve = |value: &str| {
            let path = PathBuf::from(value);
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        };
        // 随包的标点模型与同音词替换资源按固定相对路径自动发现；设置里两个布尔开关选择是否加载，
        // 显式路径字段只留作高级覆盖。
        let bundled = |value: &str| {
            let path = root.join(value);
            path.is_file().then(|| path.display().to_string())
        };
        let punctuation_model = if !config.punctuation_model.trim().is_empty() {
            Some(resolve(&config.punctuation_model).display().to_string())
        } else if config.punctuation {
            bundled("data\\voice\\punctuation\\model.int8.onnx")
        } else {
            None
        };
        let hr_lexicon = if !config.hr_lexicon.trim().is_empty() {
            Some(resolve(&config.hr_lexicon).display().to_string())
        } else if config.hr {
            bundled("data\\voice\\hr\\lexicon.txt")
        } else {
            None
        };
        let hr_rule_fsts = if !config.hr_rule_fsts.trim().is_empty() {
            Some(resolve(&config.hr_rule_fsts).display().to_string())
        } else if config.hr {
            bundled("data\\voice\\hr\\replace.fst")
        } else {
            None
        };
        let level = config.polish_level();
        // polish_url 是 URL 不是随包文件路径：不走 resolve（它会把 "http://…" 拼进安装根目录）。
        let polish_url = (!config.polish_url.trim().is_empty())
            .then(|| config.polish_url.trim().trim_end_matches('/').to_owned());
        Self::spawn(
            executable,
            WorkerConfig {
                model: resolve(&config.model).display().to_string(),
                tokens: resolve(&config.tokens).display().to_string(),
                language: config.language.clone(),
                input_device: (!config.input_device.trim().is_empty())
                    .then(|| config.input_device.clone()),
                auto_stop_ms: config.auto_stop_ms,
                punctuation_model,
                polish: (level != PolishLevel::Off
                    && !config.polish_url.trim().is_empty()
                    && !config.polish_model.trim().is_empty())
                .then_some(level),
                polish_url,
                polish_model: (!config.polish_model.trim().is_empty())
                    .then(|| config.polish_model.clone()),
                hotwords,
                hr_lexicon,
                hr_rule_fsts,
            },
        )
    }

    /// 起进程并交付一次不可变配置，再把管道交给自己的线程。
    /// 启动这一步仍是同步的：起不来要当场报给用户，而它只等 Worker 收下配置，不等模型加载。
    pub fn spawn(executable: &Path, config: WorkerConfig) -> Result<Self, VoiceBackendError> {
        let worker = Worker::spawn(executable, config)?;
        let snapshot = SharedSnapshot::default();
        let (commands, receiver) = mpsc::channel();
        let shared = snapshot.clone();
        let thread = std::thread::Builder::new()
            .name("qingjian-voice-ipc".to_owned())
            .spawn(move || worker::run(worker, receiver, shared))?;
        Ok(Self {
            commands,
            snapshot,
            thread: Some(thread),
        })
    }

    fn send(&mut self, request: WorkerRequest) -> Result<(), VoiceBackendError> {
        self.commands
            .send(request)
            .map_err(|_| VoiceBackendError::Closed)
    }
}

impl VoiceBackend for ProcessVoiceBackend {
    fn start(&mut self, request: u64) -> Result<(), VoiceBackendError> {
        self.send(WorkerRequest::Start { request })
    }

    fn stop(&mut self, request: u64) -> Result<(), VoiceBackendError> {
        self.send(WorkerRequest::Stop { request })
    }

    fn cancel(&mut self, request: u64) -> Result<(), VoiceBackendError> {
        self.send(WorkerRequest::Cancel { request })
    }

    /// 读格子，不走管道。Worker 那侧的失败已经写成带原因的 `Failed` 快照。
    fn snapshot(&mut self) -> Result<WorkerSnapshot, VoiceBackendError> {
        Ok(self.snapshot.read())
    }
}

impl Drop for ProcessVoiceBackend {
    fn drop(&mut self) {
        let _ = self.commands.send(WorkerRequest::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
