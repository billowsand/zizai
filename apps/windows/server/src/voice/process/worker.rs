//! 持有 Worker 子进程的那一侧：所有管道读写都在这个线程上，Router 线程只投命令、读快照。

use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use qingjian_platform::protocol::{VoiceState, read_message, write_message};
use qingjian_voice::{WorkerConfig, WorkerRequest, WorkerResponse, WorkerSnapshot};

use super::shared::SharedSnapshot;
use crate::voice::VoiceBackendError;

/// 有活儿在身时的快照节拍：比 DLL 的 80 ms 同步快一档，电平与状态不会慢一拍才到。
const ACTIVE_INTERVAL: Duration = Duration::from_millis(40);

/// 空闲时只要能看住「进程还活着」就够，节拍放慢省掉无谓的往返。
const IDLE_INTERVAL: Duration = Duration::from_millis(500);

/// 退出时等 Worker 自己收摊的上限；到点直接杀，避免热加载语音配置时卡住调用方。
const SHUTDOWN_GRACE: Duration = Duration::from_millis(300);

/// 一个 Worker 子进程及其管道。
pub(super) struct Worker {
    executable: PathBuf,

    config: WorkerConfig,

    child: Child,

    input: BufWriter<ChildStdin>,

    output: BufReader<ChildStdout>,

    /// 管道已经坏掉：只有下一次 Start 才重新拉进程，不在这中间反复重试。
    broken: bool,
}

impl Worker {
    /// 启动 Worker 并交付一次不可变配置。构造在调用方线程上做，失败能当场报给用户。
    pub(super) fn spawn(
        executable: &Path,
        config: WorkerConfig,
    ) -> Result<Self, VoiceBackendError> {
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
        let mut worker = Self {
            executable: executable.to_owned(),
            config: config.clone(),
            child,
            input: BufWriter::new(input),
            output: BufReader::new(output),
            broken: false,
        };
        worker.exchange(WorkerRequest::Configure(Box::new(config)))?;
        Ok(worker)
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

    /// 结束残留进程并拉起干净 Worker。只重放触发重启的这一条 Start，不重放崩溃前的录音请求。
    fn restart(&mut self) -> Result<(), VoiceBackendError> {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let replacement = Self::spawn(&self.executable, self.config.clone())?;
        // 旧的 child 已经收尸，直接换掉整体状态即可。
        *self = replacement;
        Ok(())
    }

    /// Worker 崩溃或管道坏掉后，只在新一轮 Start 时重来一遍。
    fn ensure_alive(&mut self, snapshot: &SharedSnapshot) -> Result<(), VoiceBackendError> {
        if !self.broken && self.child.try_wait()?.is_none() {
            return Ok(());
        }
        self.restart()?;
        // 新进程还在加载模型：先摆回 Loading，清掉上一轮留下的失败提示。
        snapshot.publish(WorkerSnapshot {
            state: VoiceState::Loading,
            ..WorkerSnapshot::default()
        });
        Ok(())
    }

    fn fail(&mut self, snapshot: &SharedSnapshot, error: &VoiceBackendError, what: &str) {
        self.broken = true;
        tracing::error!(%error, what, "语音 Worker 通信失败");
        snapshot.publish_failure("语音工作进程已退出，请重试");
    }

    fn handle(&mut self, request: WorkerRequest, snapshot: &SharedSnapshot) {
        if matches!(request, WorkerRequest::Start { .. })
            && let Err(error) = self.ensure_alive(snapshot)
        {
            self.fail(snapshot, &error, "restart");
            return;
        }
        // 管道已经坏了：Stop / Cancel 没有对象可发，等下一次 Start 重新拉进程。
        if self.broken {
            return;
        }
        if let Err(error) = self.command(request) {
            self.fail(snapshot, &error, "command");
            return;
        }
        self.refresh(snapshot);
    }

    fn refresh(&mut self, snapshot: &SharedSnapshot) {
        if self.broken {
            return;
        }
        match self.exchange(WorkerRequest::Snapshot) {
            Ok(WorkerResponse::Snapshot(value)) => snapshot.publish(value),
            Ok(_) => {
                let error = VoiceBackendError::Rejected("unexpected command response".into());
                self.fail(snapshot, &error, "snapshot");
            }
            Err(error) => self.fail(snapshot, &error, "snapshot"),
        }
    }

    /// 请 Worker 自己退出；宽限期内没走就杀掉。无论如何都要收尸，不留句柄。
    fn shutdown(&mut self) {
        if !self.broken {
            let _ = write_message(&mut self.input, &WorkerRequest::Shutdown);
        }
        let deadline = Instant::now() + SHUTDOWN_GRACE;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
        tracing::warn!("语音 Worker 没在宽限期内退出，强制结束");
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// 线程主循环：有命令就转给 Worker，没命令就按当前阶段的节拍刷一次快照。
pub(super) fn run(mut worker: Worker, commands: Receiver<WorkerRequest>, snapshot: SharedSnapshot) {
    loop {
        match commands.recv_timeout(interval(&snapshot)) {
            Ok(WorkerRequest::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(request) => worker.handle(request, &snapshot),
            Err(RecvTimeoutError::Timeout) => worker.refresh(&snapshot),
        }
    }
    worker.shutdown();
}

fn interval(snapshot: &SharedSnapshot) -> Duration {
    match snapshot.state() {
        VoiceState::Recording
        | VoiceState::Recognizing
        | VoiceState::Polishing
        | VoiceState::Ready => ACTIVE_INTERVAL,
        _ => IDLE_INTERVAL,
    }
}
