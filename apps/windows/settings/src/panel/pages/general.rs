//! 「通用」页：输入方案（双拼 / 辅码）、按键、标点、候选质量与学习。
//! 原先各占一页的「快捷键」与「本地整句模型」项数太少，并到这里当分组。

use qingjian_platform::Modifiers;
use windows_reactor::*;

use crate::panel::controls::{field, group, index_of, page};
use crate::panel::{Message, Settings};

/// 双拼方案：界面名 + 配置写法（空串为全拼）。
pub(crate) const SHUANGPIN: [(&str, &str); 5] = [
    ("全拼（不启用双拼）", ""),
    ("小鹤双拼", "xiaohe"),
    ("自然码", "ziranma"),
    ("微软双拼", "microsoft"),
    ("搜狗双拼", "sogou"),
];

/// 辅码方案：界面名 + 配置写法（空串为关）。
pub(crate) const FUMA: [(&str, &str); 2] = [("关（不启用辅码）", ""), ("小鹤辅码", "xiaohe")];

/// 候选显示辅码的档位：界面名 + 配置写法（见 `FumaHint`）。
pub(crate) const FUMA_HINT: [(&str, &str); 3] = [
    ("敲了辅码时", "typed"),
    ("始终（学码）", "always"),
    ("不显示", "off"),
];

/// 翻页键对：界面名 + 配置写法。
pub(crate) const PAGE_KEYS: [(&str, &str); 3] = [
    ("方括号 [ ]", "[]"),
    ("逗号句号 , .", ",."),
    ("减号等号 - =", "-="),
];

/// 删候选的修饰键预设：界面名 + 配置写法。
pub(crate) const MODIFIERS: [(&str, &str); 6] = [
    ("Ctrl", "ctrl"),
    ("Alt", "alt"),
    ("Shift", "shift"),
    ("Ctrl + Shift", "shift+ctrl"),
    ("Ctrl + Alt", "ctrl+alt"),
    ("Alt + Shift", "shift+alt"),
];

fn string_combo(
    options: &'static [(&str, &str)],
    current: &str,
    callback: Callback<Option<usize>>,
) -> ComboBox {
    ComboBox::new()
        .items_source(options.iter().map(|(label, _)| *label))
        .selected_index(index_of(options, current))
        .on_selection_changed(callback)
}

/// 按解析后相等找当前项，不依赖字符串写法。
fn modifier_combo(current: Modifiers, callback: Callback<Option<usize>>) -> ComboBox {
    let selected = MODIFIERS
        .iter()
        .position(|(_, value)| value.parse::<Modifiers>().ok() == Some(current))
        .unwrap_or(0);
    ComboBox::new()
        .items_source(MODIFIERS.iter().map(|(label, _)| *label))
        .selected_index(selected)
        .on_selection_changed(callback)
}

fn scheme_group(settings: &Settings, context: &mut ViewContext<Settings>) -> View {
    let g = &settings.config.general;
    let fuma = string_combo(&FUMA, &g.fuma, context.callback(Message::Fuma))
        .is_enabled(!g.shuangpin.trim().is_empty());
    let fuma_hint = string_combo(
        &FUMA_HINT,
        g.fuma_hint.trim(),
        context.callback(Message::FumaHint),
    )
    .is_enabled(!g.shuangpin.trim().is_empty() && !g.fuma.trim().is_empty());
    group(
        Symbol::Character,
        "输入方案",
        [
            field(
                Symbol::Keyboard,
                "双拼",
                "开双拼后两键按方案表拆成声母 + 韵母；微软、搜狗方案的 ; 键是 ing 韵母键。",
                string_combo(
                    &SHUANGPIN,
                    &g.shuangpin,
                    context.callback(Message::Shuangpin),
                ),
            ),
            field(
                Symbol::Tag,
                "辅码",
                "开双拼后可用：打完双拼再敲两个大写辅码键严格筛选候选（首字第 1 码 + 末字第 1 码，单字取两码），对不上就不出候选；第一码大写表示反转顺序。辅码键不是要打的内容。",
                fuma,
            ),
            field(
                Symbol::View,
                "候选显示辅码",
                "在候选右上角用淡色小字标辅码。敲了第一码后标出还要敲的第二码；「始终」在没敲辅码时也标出完整两码，适合还在记码的时候。",
                fuma_hint,
            ),
        ],
    )
}

fn keys_group(settings: &Settings, context: &mut ViewContext<Settings>) -> View {
    let g = &settings.config.general;
    group(
        Symbol::Switch,
        "按键",
        [
            field(
                Symbol::TwoPage,
                "翻页键",
                "选「, .」或「- =」时组句中敲对应符号是翻页，不再是上屏加标点。",
                ComboBox::new()
                    .items_source(PAGE_KEYS.iter().map(|(label, _)| *label))
                    .selected_index(index_of(&PAGE_KEYS, &g.page_keys))
                    .on_selection_changed(context.callback(Message::PageKeys)),
            ),
            field(
                Symbol::Delete,
                "删除候选",
                "按住修饰键再按候选序号：自己造的词整删；词库里的词清掉学习记录，回到原排序。",
                modifier_combo(
                    settings.config.shortcut.delete_candidate,
                    context.callback(Message::DeleteCandidate),
                ),
            ),
        ],
    )
}

fn punctuation_group(settings: &Settings, context: &mut ViewContext<Settings>) -> View {
    let g = &settings.config.general;
    group(
        Symbol::Emoji2,
        "标点",
        [
            field(
                Symbol::Comment,
                "中文模式标点转全角",
                "没在打拼音时敲 , . ? ! 等出「，。？！」，数字后面的点保持半角；悬浮状态条的「，。」格也能切，切的是当前模式那份。",
                ToggleSwitch::new()
                    .is_on(g.full_width_punctuation)
                    .on_toggled(context.callback(Message::FullWidthPunctuation)),
            ),
            field(
                Symbol::Font,
                "英文模式标点转全角",
                "中英各记一份，缺省英文半角。",
                ToggleSwitch::new()
                    .is_on(g.english_full_width_punctuation)
                    .on_toggled(context.callback(Message::EnglishFullWidthPunctuation)),
            ),
        ],
    )
}

fn quality_group(settings: &Settings, context: &mut ViewContext<Settings>) -> View {
    group(
        Symbol::SolidStar,
        "候选质量",
        [
            field(
                Symbol::Sort,
                "本地整句模型",
                "随包的小模型在本机给整句候选重新排序，全程离线；停键后几十毫秒生效。关掉只用词库统计。模型文件优先读用户目录 model\\ 下的 .qjm，其次安装目录 data\\model\\。",
                ToggleSwitch::new()
                    .is_on(settings.config.model.enabled)
                    .on_toggled(context.callback(Message::LocalModel)),
            ),
            field(
                Symbol::Filter,
                "输入拼音时中文候选排在英文词前面",
                "开着时整段输入是英文词时（hello、key）英文词排第二，空格上屏的仍是中文；关着（缺省）拼音不成立的输入英文词排第一。",
                ToggleSwitch::new()
                    .is_on(settings.config.general.chinese_first)
                    .on_toggled(context.callback(Message::ChineseFirst)),
            ),
            field(
                Symbol::Favorite,
                "学习输入习惯",
                "按你的选择调整候选顺序、记新词与敲错纠正。关掉后不再学，已学的仍参与排序；学习数据在数据目录里。",
                ToggleSwitch::new()
                    .is_on(settings.config.general.learning)
                    .on_toggled(context.callback(Message::Learning)),
            ),
        ],
    )
}

pub(crate) fn view(settings: &Settings, context: &mut ViewContext<Settings>) -> View {
    page(
        "通用",
        "输入方案、按键与候选行为",
        [
            scheme_group(settings, context),
            keys_group(settings, context),
            punctuation_group(settings, context),
            quality_group(settings, context),
        ],
    )
}
