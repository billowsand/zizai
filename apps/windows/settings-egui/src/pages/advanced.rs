//! 「高级」页：诊断与修复、文件与目录、日志与诊断开关。

use std::time::Duration;

use eframe::egui;
use qingjian_platform::LogLevel;

use crate::app::Settings;
use crate::files;
use crate::widgets::{caption, list, note, page, toggle};

/// 停在这一页时多久重查一次服务在不在（开一下互斥体，很便宜）。
const SERVER_POLL: Duration = Duration::from_secs(1);

pub(crate) fn view(settings: &mut Settings, ui: &mut egui::Ui) {
    page(ui, "高级", |ui| {
        repair(settings, ui);
        caption(ui, "文件与日志");
        list(ui, |list| {
            list.row(
                "\u{E8A5}",
                "配置文件",
                "设置改完会自动生效（Server 每秒看一次配置文件）。改坏了删掉它就回到默认设置。",
                |ui| {
                    let response = ui.button("打开");
                    if response.clicked() {
                        files::open_in_editor(settings.config_file());
                    }
                    response
                },
            );
            list.row(
                "\u{E838}",
                "数据目录",
                "词频、用户词、学习数据与统计都在这里。",
                |ui| {
                    let response = ui.button("打开");
                    if response.clicked() {
                        files::open_with_explorer(&settings.data_dir().to_string_lossy());
                    }
                    response
                },
            );
            list.row(
                "\u{E7C3}",
                "日志",
                "输入法、引擎与设置程序的日志都在一个目录，按天分文件、保留 7 天；打包的 zip 放到桌面，发给作者即可。",
                |ui| {
                    let export = ui.button("打包到桌面");
                    if export.clicked() {
                        files::export_logs();
                    }
                    let open = ui.button("打开");
                    if open.clicked()
                        && let Some(logs) = files::log_dir()
                    {
                        files::open_with_explorer(&logs.to_string_lossy());
                    }
                    open | export
                },
            );
            let mut input_log = settings.config.general.input_log;
            list.row(
                "\u{E70F}",
                "记录输入日志",
                "每次上屏记一行，只写本机、不上传，用于离线评测与个人模型。",
                |ui| {
                    let response = toggle(ui, &mut input_log, "记录输入日志");
                    if response.changed() {
                        settings.save("general", "input_log", input_log);
                    }
                    response
                },
            );
            let mut verbose = settings.config.general.log_level == LogLevel::Debug;
            list.row(
                "\u{E8FD}",
                "详细日志",
                "排查问题时临时打开，会记下敲的拼音与上屏文字。",
                |ui| {
                    let response = toggle(ui, &mut verbose, "详细日志");
                    if response.changed() {
                        let level = if verbose {
                            LogLevel::Debug
                        } else {
                            LogLevel::Info
                        };
                        settings.save("general", "log_level", level.key());
                    }
                    response
                },
            );
            list.row("\u{E894}", "清空输入日志", "", |ui| {
                let response = ui.button("清空");
                if response.clicked() {
                    let log = settings.data_dir().join("input-log.jsonl");
                    if let Err(error) = std::fs::remove_file(&log)
                        && error.kind() != std::io::ErrorKind::NotFound
                    {
                        eprintln!("清空输入日志失败: {error}");
                    }
                }
                response
            });
        });
        note(ui, "数据只留在本机，不上传。");
    });
}

/// 「诊断与修复」卡：服务状态 + 重启、深度修复，下面一行小字写诊断出的问题或修复进度。
fn repair(settings: &mut Settings, ui: &mut egui::Ui) {
    let progress = settings.repair.progress();
    let running = settings.repair.server_running();
    ui.ctx().request_repaint_after(SERVER_POLL);
    caption(ui, "诊断与修复");
    list(ui, |list| {
        list.row(
            "\u{E9D9}",
            "输入法服务",
            "候选窗口不出来、只能打英文，多半是输入法服务没在运行。「重启」会结束旧的服务并重新启动它。",
            |ui| {
                let response = ui.add_enabled(!progress.busy, egui::Button::new("重启"));
                if response.clicked() {
                    settings.repair.restart();
                }
                ui.label(if running { "运行中" } else { "未运行" });
                response
            },
        );
        list.row(
            "\u{E90F}",
            "深度修复",
            "结束残留进程、清理旧版本留下的文件与自启任务、重新注册输入法，然后重启服务。需要管理员授权。",
            |ui| {
                let response = ui.add_enabled(!progress.busy, egui::Button::new("修复"));
                if response.clicked() {
                    settings.repair.deep_repair();
                }
                response
            },
        );
    });
    let status = progress
        .message
        .clone()
        .or_else(|| progress.diagnosis.blocker.clone())
        .or_else(|| {
            (!running).then(|| "输入法服务没在运行，现在只能打英文。点「重启」试试。".to_owned())
        });
    if let Some(status) = status {
        note(ui, &status);
        ui.add_space(10.0);
    }
}
