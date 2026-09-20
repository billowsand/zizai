//! 「语音输入」页：SenseVoice、快捷键、麦克风与可选的大模型整理。

use eframe::egui;
use qingjian_platform::VoiceTrigger;

use crate::app::Settings;
use crate::widgets::{CONTROL_WIDTH, LABEL_SIZE, caption, list, note, page, toggle};

/// 按一下开始、再按一下结束的单键预设。
const VOICE_KEYS: [(&str, &str); 5] = [
    ("右 Alt", "right_alt"),
    ("右 Ctrl", "right_ctrl"),
    ("Caps Lock", "caps_lock"),
    ("Scroll Lock", "scroll_lock"),
    ("关闭快捷键", "off"),
];

pub(crate) fn view(settings: &mut Settings, ui: &mut egui::Ui) {
    page(ui, "语音输入", |ui| {
        let mut voice = settings.config.voice.enabled;
        caption(ui, "识别");
        list(ui, |list| {
            list.row(
                "\u{E720}",
                "本地语音输入",
                "按一下快捷键开始说话，再按一下后离线识别并直接写入当前输入框。",
                |ui| {
                    let response = toggle(ui, &mut voice, "本地语音输入");
                    if response.changed() {
                        settings.save("voice", "enabled", voice);
                    }
                    response
                },
            );

            let current_voice_key = settings.config.shortcut.voice;
            list.row(
                "\u{E765}",
                "语音快捷键",
                "按一下开始录音，再按一下开始识别；密码框中不会拦截。",
                |ui| {
                    ui.add_enabled_ui(voice, |ui| {
                        let selected = match current_voice_key {
                            VoiceTrigger::RightAlt => 0,
                            VoiceTrigger::RightCtrl => 1,
                            VoiceTrigger::CapsLock => 2,
                            VoiceTrigger::ScrollLock => 3,
                            VoiceTrigger::Off => 4,
                        };
                        let (response, picked) = show_combo(ui, "voice-key", &VOICE_KEYS, selected);
                        if let Some(value) = picked {
                            settings.save("shortcut", "voice", value);
                        }
                        response
                    })
                    .inner
                },
            );

            let current_voice_device = settings.config.voice.input_device.clone();
            let voice_devices = settings.voice_devices.clone();
            let default_voice_device = settings.default_voice_device.clone();
            let device_tip = default_voice_device.as_deref().map_or_else(
                || "选择语音输入使用的麦克风；“系统默认”会跟随 Windows。".to_owned(),
                |name| format!("选择语音输入使用的麦克风；当前 Windows 默认设备：{name}。"),
            );
            list.row("\u{E720}", "语音麦克风", &device_tip, |ui| {
                ui.add_enabled_ui(voice, |ui| {
                    let (response, picked) =
                        voice_device_combo(ui, &current_voice_device, &voice_devices);
                    if let Some(value) = picked {
                        settings.save("voice", "input_device", value);
                    }
                    response
                })
                .inner
            });

            let mut auto_finish = settings.config.voice.auto_stop_ms > 0;
            list.row(
                "\u{E8FB}",
                "停顿后自动完成",
                "检测到你已经说话后，连续安静约 1.2 秒便自动开始识别；仍可再按一次快捷键立即完成。",
                |ui| {
                    ui.add_enabled_ui(voice, |ui| {
                        let response = toggle(ui, &mut auto_finish, "停顿后自动完成");
                        if response.changed() {
                            let milliseconds = if auto_finish { 1_200_i64 } else { 0_i64 };
                            settings.save("voice", "auto_stop_ms", milliseconds);
                        }
                        response
                    })
                    .inner
                },
            );

            list.row(
                "\u{E8D2}",
                "SenseVoice 原生标点",
                "SenseVoice 通过 ITN 直接输出标点和规范化数字，无需额外模型。",
                |ui| ui.add_enabled(false, egui::Button::new("已开启")),
            );
        });

        caption(ui, "大模型整理");
        list(ui, |list| {
            let mut polish = settings.config.voice.polish_enabled;
            list.row(
                "\u{E8D4}",
                "语句整理",
                "把 SenseVoice 最终转写发给已配置的 OpenAI 兼容服务，修正少量错字与断句。",
                |ui| {
                    ui.add_enabled_ui(voice, |ui| {
                        let response = toggle(ui, &mut polish, "语句整理");
                        if response.changed() {
                            settings.save("voice", "polish_enabled", polish);
                        }
                        response
                    })
                    .inner
                },
            );

            list.row(
                "\u{E774}",
                "服务地址",
                "例如 LM Studio 的 http://localhost:1234；程序会请求 /v1/chat/completions。",
                |ui| {
                    ui.add_enabled_ui(voice && polish, |ui| {
                        let response = ui.add_sized(
                            [CONTROL_WIDTH, 28.0],
                            egui::TextEdit::singleline(&mut settings.voice_polish_url_edit),
                        );
                        if response.changed() {
                            settings.voice_polish_edited();
                        }
                        if response.lost_focus() {
                            settings.flush_voice_polish_edits(true);
                        }
                        response
                    })
                    .inner
                },
            );

            list.row(
                "\u{E8B9}",
                "模型名称",
                "填服务在 /v1/models 中公布的模型 ID。",
                |ui| {
                    ui.add_enabled_ui(voice && polish, |ui| {
                        let response = ui.add_sized(
                            [CONTROL_WIDTH, 28.0],
                            egui::TextEdit::singleline(&mut settings.voice_polish_model_edit),
                        );
                        if response.changed() {
                            settings.voice_polish_edited();
                        }
                        if response.lost_focus() {
                            settings.flush_voice_polish_edits(true);
                        }
                        response
                    })
                    .inner
                },
            );
        });

        note(
            ui,
            "SenseVoice 已会输出基本标点。大模型整理是可选增强；服务未启动、超时或改动过大时，会直接使用 SenseVoice 原文。",
        );
    });
}

fn show_combo(
    ui: &mut egui::Ui,
    id: &str,
    options: &'static [(&'static str, &'static str)],
    selected: usize,
) -> (egui::Response, Option<&'static str>) {
    let mut picked = None;
    let response = egui::ComboBox::from_id_salt(id)
        .width(CONTROL_WIDTH)
        .selected_text(egui::RichText::new(options[selected].0).size(LABEL_SIZE))
        .show_ui(ui, |ui| {
            for (index, (label, value)) in options.iter().enumerate() {
                if ui.selectable_label(index == selected, *label).clicked() && index != selected {
                    picked = Some(*value);
                }
            }
        })
        .response;
    (response, picked)
}

/// 输入设备来自系统，空字符串表示跟随 Windows 默认设备。
fn voice_device_combo(
    ui: &mut egui::Ui,
    current: &str,
    devices: &[String],
) -> (egui::Response, Option<String>) {
    let mut picked = None;
    let selected = if current.trim().is_empty() {
        "系统默认"
    } else {
        current
    };
    let response = egui::ComboBox::from_id_salt("voice-device")
        .width(CONTROL_WIDTH)
        .selected_text(egui::RichText::new(selected).size(LABEL_SIZE))
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(current.trim().is_empty(), "系统默认")
                .clicked()
                && !current.trim().is_empty()
            {
                picked = Some(String::new());
            }
            if !devices.is_empty() {
                ui.separator();
            }
            for device in devices {
                if ui.selectable_label(current == device, device).clicked() && current != device {
                    picked = Some(device.clone());
                }
            }
            if devices.is_empty() {
                ui.add_enabled(false, egui::Label::new("未发现输入设备"));
            } else if !current.trim().is_empty() && !devices.iter().any(|device| device == current)
            {
                ui.separator();
                ui.add_enabled(false, egui::Label::new(format!("当前不可用：{current}")));
            }
        })
        .response;
    (response, picked)
}
