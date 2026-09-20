//! 语音 Worker 进程入口：只在 stdin/stdout 上服务 Server，不碰剪贴板与前台窗口。
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use qingjian_platform::protocol::{read_message, write_message};
use qingjian_voice::{Controller, WorkerRequest, WorkerResponse};

fn main() {
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
                controller = Some(Controller::new(config));
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
