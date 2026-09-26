//! 根组件的 Reactor 生命周期：建状态、按消息落盘、画品牌头 + 左侧导航 + 当前页。

use qingjian_platform::{ColorScheme, Config, LogLevel, PreeditMode};
use windows_reactor::*;

use super::controls::{export_logs, log_dir, open_in_editor, open_with_explorer};
use super::pages::{about, dictionaries, general};
use super::{Message, Settings, brand};

/// 左侧导航栏宽度。
const PANE_WIDTH: f64 = 220.0;

impl Component for Settings {
    type Input = ();
    type Message = Message;

    fn create(_input: &(), _context: &ComponentContext<Self>) -> Self {
        let path = Self::config_path();
        let config = Config::load(&path).unwrap_or_default();
        Self {
            config,
            path,
            page: "general".to_string(),
            families: qingjian_render::system_fonts::families(),
        }
    }

    fn update(&mut self, message: Message, _context: &ComponentContext<Self>) {
        match message {
            Message::Navigate(Some(tag)) => self.page = tag,
            Message::Navigate(None) => {}

            // 通用页 · 输入方案
            Message::Shuangpin(Some(i)) if i < general::SHUANGPIN.len() => {
                self.save("general", "shuangpin", general::SHUANGPIN[i].1);
            }
            Message::Fuma(Some(i)) if i < general::FUMA.len() => {
                self.save("general", "fuma", general::FUMA[i].1);
            }
            Message::FumaHint(Some(i)) if i < general::FUMA_HINT.len() => {
                self.save("general", "fuma_hint", general::FUMA_HINT[i].1);
            }

            // 通用页 · 按键
            Message::PageKeys(Some(i)) if i < general::PAGE_KEYS.len() => {
                self.save("general", "page_keys", general::PAGE_KEYS[i].1);
            }
            Message::DeleteCandidate(Some(i)) if i < general::MODIFIERS.len() => {
                self.save("shortcut", "delete_candidate", general::MODIFIERS[i].1);
            }

            // 通用页 · 标点
            Message::FullWidthPunctuation(on) => {
                self.save("general", "full_width_punctuation", on);
            }
            Message::EnglishFullWidthPunctuation(on) => {
                self.save("general", "english_full_width_punctuation", on);
            }

            // 通用页 · 候选质量
            Message::LocalModel(on) => self.save("model", "enabled", on),
            Message::ChineseFirst(on) => self.save("general", "chinese_first", on),
            Message::Learning(on) => self.save("general", "learning", on),

            // 候选窗口页
            Message::Theme(Some(i)) if i < ColorScheme::ALL.len() => {
                self.save("general", "theme", ColorScheme::ALL[i].key());
            }
            Message::PageSize(Some(value)) => {
                let size = (value.round() as i64).clamp(1, 9);
                self.save("general", "page_size", size);
            }
            Message::Preedit(Some(i)) if i < PreeditMode::ALL.len() => {
                self.save("general", "preedit", PreeditMode::ALL[i].key());
            }
            Message::Font(Some(0)) => self.save("general", "font", ""),
            Message::Font(Some(index)) if index <= self.families.len() => {
                let family = self.families[index - 1].clone();
                self.save("general", "font", family);
            }
            // 最后那一项是配置里写了、但系统里没装的字体：保持原值，不动配置
            Message::Font(_) => {}
            Message::StatusBar(on) => self.save("status_bar", "enabled", on),

            // 词库页
            Message::ToggleDomain(name, on) => {
                let mut domains = self.config.dictionaries.domains.clone();
                if on {
                    if !domains.contains(&name) {
                        domains.push(name);
                    }
                } else {
                    domains.retain(|d| d != &name);
                }
                self.save_array("dictionaries", "domains", &domains);
            }
            Message::ToggleUserDict(name, on) => {
                // 用户词库缺省启用，`disabled` 列的是关掉的。
                let mut disabled = self.config.dictionaries.disabled.clone();
                if on {
                    disabled.retain(|d| d != &name);
                } else if !disabled.contains(&name) {
                    disabled.push(name);
                }
                self.save_array("dictionaries", "disabled", &disabled);
            }
            Message::RemoveUserDict(name) => {
                dictionaries::remove_user_dict(self, &name);
                self.reload();
            }
            Message::ImportDictionary => {
                dictionaries::import(self);
                self.reload();
            }

            // 高级页
            Message::VerboseLog(on) => {
                let level = if on { LogLevel::Debug } else { LogLevel::Info };
                self.save("general", "log_level", level.key());
            }
            Message::InputLog(on) => self.save("general", "input_log", on),
            Message::OpenConfigFile => open_in_editor(&self.path),
            Message::OpenDataDir => {
                open_with_explorer(&self.data_dir().to_string_lossy());
            }
            Message::OpenLogDir => {
                if let Some(logs) = log_dir() {
                    open_with_explorer(&logs.to_string_lossy());
                }
            }
            Message::ExportLogs => export_logs(),
            Message::ClearInputLog => {
                let log = self.data_dir().join("input-log.jsonl");
                if let Err(error) = std::fs::remove_file(&log)
                    && error.kind() != std::io::ErrorKind::NotFound
                {
                    crate::log::warn(format!("清空输入日志失败: {error}"));
                }
            }

            // 关于页
            Message::OpenWebsite => open_with_explorer(about::WEBSITE_URL),
            Message::OpenRepository => open_with_explorer(about::REPOSITORY_URL),

            // 下拉被清空 / 越界：不改
            _ => {}
        }
    }

    fn view(&self, _input: &(), context: &mut ViewContext<Self>) -> View {
        context.window_title("字在设置");
        let item = |tag: &str, label: &str, symbol| {
            KeyedView::new(
                tag,
                NavigationViewItem::new()
                    .tag(tag)
                    .is_selected(self.page == tag)
                    .slots([
                        SlotView::new(
                            NavigationViewItemSlot::Icon,
                            SymbolIcon::new().symbol(symbol),
                        ),
                        SlotView::new(NavigationViewItemSlot::Content, label),
                    ]),
            )
        };
        let items = [
            item("general", "通用", Symbol::Setting),
            item("appearance", "候选窗口", Symbol::View),
            item("dictionaries", "词库", Symbol::Library),
            item("advanced", "高级", Symbol::Repair),
            item("about", "关于", Symbol::Help),
        ];
        NavigationView::new()
            .pane_display_mode(NavigationViewPaneDisplayMode::Left)
            .open_pane_length(PANE_WIDTH)
            .is_pane_open(true)
            .is_pane_toggle_button_visible(false)
            .is_back_button_visible(NavigationViewBackButtonVisible::Collapsed)
            .is_settings_visible(false)
            .always_show_header(false)
            .on_selected_tag_changed(context.callback(Message::Navigate))
            .slots([
                SlotView::new(NavigationViewSlot::PaneCustomContent, brand::header()),
                SlotView::collection(NavigationViewSlot::MenuItems, items),
                SlotView::new(NavigationViewSlot::Content, self.page_content(context)),
            ])
    }
}
