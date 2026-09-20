//! 用私有 stdio 帧协议管理独立语音 Worker 进程。

use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use qingjian_platform::VoiceConfig;
use qingjian_platform::protocol::{read_message, write_message};
use qingjian_voice::{WorkerConfig, WorkerRequest, WorkerResponse, WorkerSnapshot};

use super::{VoiceBackend, VoiceBackendError};

/// 独立 `qingjian-voice-worker.exe` 后端。
pub struct ProcessVoiceBackend {
    executable: PathBuf,

    config: WorkerConfig,

    child: Child,

    input: BufWriter<ChildStdin>,

    output: BufReader<ChildStdout>,
}

impl ProcessVoiceBackend {
    /// 把用户配置中的相对路径按随包根展开后启动 Worker。
    pub fn spawn_configured(
        executable: &Path,
        root: &Path,
        config: &VoiceConfig,
    ) -> Result<Self, VoiceBackendError> {
        let resolve = |value: &str| {
            let path = std::path::PathBuf::from(value);
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        };
        let optional =
            |value: &str| (!value.trim().is_empty()).then(|| resolve(value).display().to_string());
        Self::spawn(
            executable,
            WorkerConfig {
                model: resolve(&config.model).display().to_string(),
                tokens: resolve(&config.tokens).display().to_string(),
                language: config.language.clone(),
                input_device: (!config.input_device.trim().is_empty())
                    .then(|| config.input_device.clone()),
                auto_stop_ms: config.auto_stop_ms,
                polish_url: config.polish_enabled.then(|| config.polish_url.clone()),
                polish_model: config.polish_enabled.then(|| config.polish_model.clone()),
                hr_lexicon: optional(&config.hr_lexicon),
                hr_rule_fsts: optional(&config.hr_rule_fsts),
            },
        )
    }

    /// 启动 Worker 并交付一次不可变配置。
    pub fn spawn(executable: &Path, config: WorkerConfig) -> Result<Self, VoiceBackendError> {
        let mut command = Command::new(executable);
        command.stdin(Stdio::piped()).stdout(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command.spawn()?;
        let input = child.stdin.take().ok_or(VoiceBackendError::Closed)?;
        let output = child.stdout.take().ok_or(VoiceBackendError::Closed)?;
        let mut backend = Self {
            executable: executable.to_owned(),
            config: config.clone(),
            child,
            input: BufWriter::new(input),
            output: BufReader::new(output),
        };
        backend.exchange(WorkerRequest::Configure(config))?;
        Ok(backend)
    }

    /// Worker 崩溃后不重放旧请求；下一次新的 Start 才拉起干净进程。
    fn restart_if_exited(&mut self) -> Result<(), VoiceBackendError> {
        if self.child.try_wait()?.is_none() {
            return Ok(());
        }
        let replacement = Self::spawn(&self.executable, self.config.clone())?;
        *self = replacement;
        Ok(())
    }

    /// 新一轮 Start 发现管道已经坏掉时，结束残留进程并拉起干净 Worker。
    /// 只重放这次新的 Start，不重放崩溃前的录音请求。
    fn restart(&mut self) -> Result<(), VoiceBackendError> {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let replacement = Self::spawn(&self.executable, self.config.clone())?;
        *self = replacement;
        Ok(())
    }

    fn exchange(&mut self, request: WorkerRequest) -> Result<WorkerResponse, VoiceBackendError> {
        write_message(&mut self.input, &request)?;
        match read_message(&mut self.output)? {
            Some(WorkerResponse::Error(message)) => Err(VoiceBackendError::Rejected(message)),
            Some(response) => Ok(response),
            None => Err(VoiceBackendError::Closed),
        }
    }

    fn command(&mut self, request: WorkerRequest) -> Result<(), VoiceBackendError> {
        match self.exchange(request)? {
            WorkerResponse::Ok => Ok(()),
            WorkerResponse::Snapshot(_) => Err(VoiceBackendError::Rejected(
                "unexpected snapshot response".into(),
            )),
            WorkerResponse::Error(_) => unreachable!("exchange converts worker errors"),
        }
    }
}

impl VoiceBackend for ProcessVoiceBackend {
    fn start(&mut self, request: u64) -> Result<(), VoiceBackendError> {
        self.restart_if_exited()?;
        match self.command(WorkerRequest::Start { request }) {
            Ok(()) => Ok(()),
            Err(error) if recoverable(&error) => {
                tracing::warn!(%error, request, "语音 Worker 连接失效，自动重启");
                self.restart()?;
                self.command(WorkerRequest::Start { request })
            }
            Err(error) => Err(error),
        }
    }

    fn stop(&mut self, request: u64) -> Result<(), VoiceBackendError> {
        self.command(WorkerRequest::Stop { request })
    }

    fn cancel(&mut self, request: u64) -> Result<(), VoiceBackendError> {
        self.command(WorkerRequest::Cancel { request })
    }

    fn snapshot(&mut self) -> Result<WorkerSnapshot, VoiceBackendError> {
        match self.exchange(WorkerRequest::Snapshot)? {
            WorkerResponse::Snapshot(snapshot) => Ok(snapshot),
            WorkerResponse::Ok => Err(VoiceBackendError::Rejected(
                "unexpected command response".into(),
            )),
            WorkerResponse::Error(_) => unreachable!("exchange converts worker errors"),
        }
    }
}

fn recoverable(error: &VoiceBackendError) -> bool {
    matches!(
        error,
        VoiceBackendError::Io(_) | VoiceBackendError::Protocol(_) | VoiceBackendError::Closed
    )
}

impl Drop for ProcessVoiceBackend {
    fn drop(&mut self) {
        let _ = write_message(&mut self.input, &WorkerRequest::Shutdown);
        let _ = self.child.wait();
    }
}
