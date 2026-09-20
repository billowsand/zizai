//! 「通用」页：输入方案、按键、标点、候选质量与语音输入。
//! 分节在左侧导航里，这里不再套「输入方案 / 按键 / 标点」的小节标题。

use eframe::egui;
use qingjian_platform::{Modifiers, VoiceTrigger};

use crate::app::Settings;
use crate::widgets::{CONTROL_WIDTH, LABEL_SIZE, list, page, toggle};

/// 双拼方案：界面名 + 配置写法（空串为全拼）。
const SHUANGPIN: [(&str, &str); 5] = [
    ("全拼", ""),
    ("小鹤双拼", "xiaohe"),
    ("自然码", "ziranma"),
    ("微软双拼", "microsoft"),
    ("搜狗双拼", "sogou"),
];

/// 辅码方案：界面名 + 配置写法（空串为关）。
const FUMA: [(&str, &str); 2] = [("关", ""), ("小鹤辅码", "xiaohe")];

/// 翻页键对：界面名 + 配置写法。
const PAGE_KEYS: [(&str, &str); 3] = [
    ("方括号 [ ]", "[]"),
    ("逗号句号 , .", ",."),
    ("减号等号 - =", "-="),
];

/// 删候选的修饰键预设：界面名 + 配置写法。
const MODIFIERS: [(&str, &str); 6] = [
    ("Ctrl", "ctrl"),
    ("Alt", "alt"),
    ("Shift", "shift"),
    ("Ctrl + Shift", "shift+ctrl"),
    ("Ctrl + Alt", "ctrl+alt"),
    ("Alt + Shift", "shift+alt"),
];

/// 按一下开始、再按一下结束的单键预设。
const VOICE_KEYS: [(&str, &str); 5] = [
    ("右 Alt", "right_alt"),
    ("右 Ctrl", "right_ctrl"),
    ("Caps Lock", "caps_lock"),
    ("Scroll Lock", "scroll_lock"),
    ("关闭快捷键", "off"),
];

pub(crate) fn view(settings: &mut Settings, ui: &mut egui::Ui) {
    page(ui, "通用", |ui| {
        list(ui, |list| {
            let shuangpin = settings.config.general.shuangpin.clone();
            list.row(
                "\u{E765}",
                "双拼方案",
                "开双拼后两键按方案表拆成声母 + 韵母；微软、搜狗方案的 ; 键是 ing 韵母键。",
                |ui| {
                    let (response, picked) = combo(ui, "shuangpin", &SHUANGPIN, &shuangpin);
                    if let Some(value) = picked {
                        settings.save("general", "shuangpin", value);
                    }
                    response
                },
            );
            let fuma = settings.config.general.fuma.clone();
            let enabled = !shuangpin.trim().is_empty();
            list.row(
                "\u{E8EC}",
                "辅码",
                "开双拼后可用：打完双拼再敲两个大写辅码键严格筛选候选（首字第 1 码 + 末字第 1 码，单字取两码），对不上就不出候选；第一码大写表示反转顺序。",
                |ui| {
                    ui.add_enabled_ui(enabled, |ui| {
                        let (response, picked) = combo(ui, "fuma", &FUMA, &fuma);
                        if let Some(value) = picked {
                            settings.save("general", "fuma", value);
                        }
                        response
                    })
                    .inner
                },
            );
            let page_keys = settings.config.general.page_keys.clone();
            list.row(
                "\u{E736}",
                "翻页键",
                "选「, .」或「- =」时组句中敲对应符号是翻页，不再是上屏加标点。",
                |ui| {
                    let (response, picked) = combo(ui, "page-keys", &PAGE_KEYS, &page_keys);
                    if let Some(value) = picked {
                        settings.save("general", "page_keys", value);
                    }
                    response
                },
            );
            let current = settings.config.shortcut.delete_candidate;
            list.row(
                "\u{E74D}",
                "删除候选",
                "按住修饰键再按候选序号：自己造的词整删；词库里的词清掉学习记录，回到原排序。",
                |ui| {
                    let (response, picked) = modifier_combo(ui, current);
                    if let Some(value) = picked {
                        settings.save("shortcut", "delete_candidate", value);
                    }
                    response
                },
            );
            let mut chinese = settings.config.general.full_width_punctuation;
            list.row(
                "\u{E90A}",
                "中文标点转全角",
                "没在打拼音时敲 , . ? ! 等出「，。？！」，数字后面的点保持半角；悬浮状态条的「，。」格也能切。",
                |ui| {
                    let response = toggle(ui, &mut chinese, "中文标点转全角");
                    if response.changed() {
                        settings.save("general", "full_width_punctuation", chinese);
                    }
                    response
                },
            );
            let mut english = settings.config.general.english_full_width_punctuation;
            list.row(
                "\u{E8D2}",
                "英文标点转全角",
                "中英各记一份，缺省英文半角。",
                |ui| {
                    let response = toggle(ui, &mut english, "英文标点转全角");
                    if response.changed() {
                        settings.save("general", "english_full_width_punctuation", english);
                    }
                    response
                },
            );
            let mut model = settings.config.model.enabled;
            list.row(
                "\u{E8CB}",
                "本地整句模型",
                "随包的小模型在本机给整句候选重新排序，全程离线；停键后几十毫秒生效。关掉只用词库统计。",
                |ui| {
                    let response = toggle(ui, &mut model, "本地整句模型");
                    if response.changed() {
                        settings.save("model", "enabled", model);
                    }
                    response
                },
            );
            let mut voice = settings.config.voice.enabled;
            list.row(
                "\u{E720}",
                "本地语音输入",
                "按一下快捷键开始说话，再按一下后离线识别并直接写入当前输入框。首次使用需按文档放置 SenseVoice 模型。",
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
            let mut chinese_first = settings.config.general.chinese_first;
            list.row(
                "\u{E71C}",
                "中文候选优先",
                "开着时整段输入是英文词时（hello、key）英文词排第二，空格上屏的仍是中文；关着（缺省）拼音不成立的输入英文词排第一。",
                |ui| {
                    let response = toggle(ui, &mut chinese_first, "中文候选优先");
                    if response.changed() {
                        settings.save("general", "chinese_first", chinese_first);
                    }
                    response
                },
            );
            let mut learning = settings.config.general.learning;
            list.row(
                "\u{E734}",
                "学习输入习惯",
                "按你的选择调整候选顺序、记新词与敲错纠正。关掉后不再学，已学的仍参与排序。",
                |ui| {
                    let response = toggle(ui, &mut learning, "学习输入习惯");
                    if response.changed() {
                        settings.save("general", "learning", learning);
                    }
                    response
                },
            );
        });
    });
}

/// 字符串下拉：返回控件的 `Response` 与「选了新项」时它的配置写法。
fn combo(
    ui: &mut egui::Ui,
    id: &str,
    options: &'static [(&'static str, &'static str)],
    current: &str,
) -> (egui::Response, Option<&'static str>) {
    let selected = options.iter().position(|(_, v)| *v == current).unwrap_or(0);
    show_combo(ui, id, options, selected)
}

/// 删候选修饰键的下拉：按解析后相等找当前项，不依赖字符串写法。
fn modifier_combo(ui: &mut egui::Ui, current: Modifiers) -> (egui::Response, Option<&'static str>) {
    let selected = MODIFIERS
        .iter()
        .position(|(_, value)| value.parse::<Modifiers>().ok() == Some(current))
        .unwrap_or(0);
    show_combo(ui, "delete-candidate", &MODIFIERS, selected)
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

/// 输入设备来自系统，不能用上面的静态预设下拉；空字符串表示跟随 Windows 默认设备。
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
