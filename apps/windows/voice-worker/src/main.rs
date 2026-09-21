//! 语音 Worker 进程入口：只在 stdin/stdout 上服务 Server，不碰剪贴板与前台窗口。
//! 日志写 `%LOCALAPPDATA%\Qingjian\logs\voice.*.log`（与 Server 同目录、不同前缀），
//! stdout 只留给帧协议，所以诊断信息只进文件。
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use qingjian_platform::protocol::{read_message, write_message};
use qingjian_voice::{Controller, WorkerRequest, WorkerResponse};

fn main() {
    init_logging();
    let mut controller: Option<Controller> = None;
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    loop {
        let request = match read_message::<_, WorkerRequest>(&mut input) {
            Ok(Some(request)) => request,
            Ok(None) => break,
            Err(error) => {
                let _ = write_message(&mut output, &WorkerResponse::Error(error.to_string()));
                break;
            }
        };
        let response = match request {
            WorkerRequest::Configure(config) => {
                controller = Some(Controller::new(*config));
                WorkerResponse::Ok
            }
            WorkerRequest::Start { request } => command(&controller, |value| value.start(request)),
            WorkerRequest::Stop { request } => command(&controller, |value| value.stop(request)),
            WorkerRequest::Cancel { request } => {
                command(&controller, |value| value.cancel(request))
            }
            WorkerRequest::Snapshot => controller.as_ref().map_or_else(
                || WorkerResponse::Error("worker is not configured".into()),
                |value| WorkerResponse::Snapshot(value.snapshot()),
            ),
            WorkerRequest::Shutdown => break,
        };
        if write_message(&mut output, &response).is_err() {
            break;
        }
    }
}

fn init_logging() {
    // Worker 不读 [general] log_level（配置由 Server 消费），只认 RUST_LOG 环境变量，缺省 info。
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let Some(dir) = qingjian_platform::dirs::log_dir() else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
        return;
    };
    std::fs::create_dir_all(&dir).ok();
    let appender = tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("voice")
        .filename_suffix("log")
        .max_log_files(7)
        .build(&dir)
        .expect("构建语音 Worker 滚动日志");
    let (writer, guard) = tracing_appender::non_blocking(appender);
    // guard 要活到进程结束，否则缓冲的日志不落盘。
    std::mem::forget(guard);
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_writer(writer)
        .init();
}

fn command(
    controller: &Option<Controller>,
    run: impl FnOnce(&Controller) -> Result<(), qingjian_voice::VoiceError>,
) -> WorkerResponse {
    match controller {
        Some(controller) => match run(controller) {
            Ok(()) => WorkerResponse::Ok,
            Err(error) => WorkerResponse::Error(error.to_string()),
        },
        None => WorkerResponse::Error("worker is not configured".into()),
    }
}
