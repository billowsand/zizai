//! 「诊断与修复」的后台任务：开设置时查一遍静态诊断，按钮按下后在后台线程里重启 / 深度修复，
//! 进度写进共享的 [`Progress`]，每一步都请界面重画。

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use eframe::egui;

use super::progress::Progress;
use super::{Diagnosis, ELEVATED_FLAG, cleanup, launch_error_text, server};

/// 请现任 Server 让位后等它落盘退出的上限（与安装脚本一致）。
const STEP_DOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// 拉起 Server 后等管道出来的上限：装配要读词库与学习数据，慢盘上一两秒。
const LAUNCH_WAIT: Duration = Duration::from_secs(8);

/// 用户在 UAC 里点了「否」。
const ERROR_CANCELLED: u32 = 1223;

/// 设置窗口持有的一份修复任务。
pub(crate) struct RepairTask {
    progress: Arc<Mutex<Progress>>,

    /// 后台线程用它请界面重画。
    ctx: egui::Context,
}

impl RepairTask {
    /// 开窗时起一次静态诊断（要哈希整个 Server exe，放后台）。
    pub(crate) fn new(ctx: &egui::Context) -> Self {
        let task = Self {
            progress: Arc::default(),
            ctx: ctx.clone(),
        };
        let progress = task.progress.clone();
        let ctx = task.ctx.clone();
        std::thread::spawn(move || {
            let diagnosis = Diagnosis::check();
            lock(&progress).diagnosis = diagnosis;
            ctx.request_repaint();
        });
        task
    }

    /// 当前进度的快照。
    pub(crate) fn progress(&self) -> Progress {
        lock(&self.progress).clone()
    }

    /// 本会话的 Server 此刻在不在（开一下互斥体，便宜，每帧查也行）。
    pub(crate) fn server_running(&self) -> bool {
        server::session().is_some_and(server::is_running)
    }

    /// 重启 Server：请现任让位、结束残留进程、重新拉起。
    pub(crate) fn restart(&self) {
        self.spawn(|report| restart(report));
    }

    /// 深度修复：清掉当前用户的旧启动项，提权清理旧文件并重新注册，再重启 Server。
    pub(crate) fn deep_repair(&self) {
        self.spawn(|report| {
            cleanup::remove_user_leftovers();
            report("正在等待管理员授权…");
            let args = match server::session() {
                Some(session) => format!("{ELEVATED_FLAG} {session}"),
                None => ELEVATED_FLAG.to_owned(),
            };
            let cleaned = match server::run_elevated_self(&args) {
                Err(ERROR_CANCELLED) => return "已取消：深度修复需要管理员授权。".to_owned(),
                Err(code) => return format!("没能以管理员身份运行修复（系统错误 {code}）。"),
                Ok(0) => String::new(),
                Ok(failures) => {
                    format!("清理时有 {failures} 步没成功（详见日志里的 repair.log）；")
                }
            };
            format!("{cleaned}{}", restart(report))
        });
    }

    /// 在后台线程里跑一次修复；已有一次在跑就不起。`job` 拿一个汇报进度的回调，返回最终结果。
    fn spawn(&self, job: impl FnOnce(&dyn Fn(&str)) -> String + Send + 'static) {
        {
            let mut progress = lock(&self.progress);
            if progress.busy {
                return;
            }
            progress.busy = true;
            progress.message = None;
        }
        let progress = self.progress.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let report = |text: &str| {
                lock(&progress).message = Some(text.to_owned());
                ctx.request_repaint();
            };
            let result = job(&report);
            let diagnosis = Diagnosis::check();
            {
                let mut progress = lock(&progress);
                progress.busy = false;
                progress.message = Some(result);
                progress.diagnosis = diagnosis;
            }
            ctx.request_repaint();
        });
    }
}

/// 重启本会话的 Server，返回给用户看的结果。
fn restart(report: &dyn Fn(&str)) -> String {
    let Some(session) = server::session() else {
        return "查不到当前登录会话，没法重启服务。".to_owned();
    };
    // 修复期间别让各应用里的输入法抢着拉 Server。
    let _launch_guard = server::hold_launch_mutex();
    if server::is_running(session) {
        report("正在请旧的服务退出…");
        server::request_step_down(session, STEP_DOWN_TIMEOUT);
    }
    // 卡死不响应的、残留的语音进程：直接结束。
    server::kill_leftovers(session);
    for _ in 0..20 {
        if !server::is_running(session) {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if server::is_running(session) {
        return "旧的服务结束不了（可能是以管理员身份启动的），请点「深度修复」。".to_owned();
    }
    report("正在启动服务…");
    match server::launch(session, LAUNCH_WAIT) {
        Ok(server::Launch::Ready) => "服务已重新启动，回到输入框就能打字了。".to_owned(),
        Ok(server::Launch::Slow) => "服务已启动，但还没准备好，稍等几秒再试。".to_owned(),
        Ok(server::Launch::Exited(code)) => {
            format!("服务启动后马上退出了（退出码 {code}）。请在下方「日志」里打包日志发给作者。")
        }
        Err(code) => launch_error_text(code),
    }
}

/// 锁住进度；持锁的线程 panic 过也照用（进度只是几个字段，不会半写坏）。
fn lock(progress: &Mutex<Progress>) -> std::sync::MutexGuard<'_, Progress> {
    progress.lock().unwrap_or_else(PoisonError::into_inner)
}
