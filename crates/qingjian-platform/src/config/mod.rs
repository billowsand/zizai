mod color_scheme;
mod dictionaries;
mod general;
mod log_level;
mod model;
mod modifiers;
mod preedit_mode;
mod shortcut;
mod status_bar;
mod theme_mode;
mod voice;
mod voice_trigger;

use std::path::Path;

use serde::{Deserialize, Serialize};
use toml_edit::DocumentMut;

use crate::error::ConfigError;

pub use color_scheme::ColorScheme;
pub use dictionaries::{DEFAULT_DOMAINS, DictionariesConfig};
pub use general::{
    DEFAULT_PAGE_KEYS, GeneralConfig, LEARNING_LANGUAGE_OFF, MAX_PAGE_SIZE, PAGE_KEY_OPTIONS,
};
pub use log_level::LogLevel;
pub use model::LocalModelConfig;
pub use modifiers::Modifiers;
pub use preedit_mode::PreeditMode;
pub use shortcut::ShortcutConfig;
pub use status_bar::StatusBarConfig;
pub use theme_mode::ThemeMode;
pub use voice::VoiceConfig;
pub use voice_trigger::VoiceTrigger;

/// 用户配置文件（TOML）。所有平台同一份格式，缺省值全部在各分节的 `Default` 里。
///
/// 配置文件是唯一事实源：菜单、设置窗口、手改文件三个入口都只写这个文件，再由壳热加载。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// 常规：学习语言、每页候选数、翻页键、外观。
    pub general: GeneralConfig,

    /// 自定义短语；保存和读取均检查位置冲突。
    #[serde(deserialize_with = "deserialize_phrases")]
    pub custom_phrases: Vec<qingjian_core::CustomPhrase>,

    /// 快捷键：修饰键组合。
    pub shortcut: ShortcutConfig,

    /// 附加词库开关。
    pub dictionaries: DictionariesConfig,

    /// 悬浮状态条（桌面上常驻、可拖动的中 / 英浮窗）。
    pub status_bar: StatusBarConfig,

    /// 本地整句模型。
    pub model: LocalModelConfig,

    /// Windows 本地语音输入。
    pub voice: VoiceConfig,
}

fn deserialize_phrases<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<qingjian_core::CustomPhrase>, D::Error> {
    let phrases = Vec::<qingjian_core::CustomPhrase>::deserialize(deserializer)?;
    qingjian_core::custom_phrase::validate_phrases(&phrases).map_err(serde::de::Error::custom)?;
    Ok(phrases)
}

/// 模板 `[shortcut]` 一节里的修饰键组合（键名：alt / shift / ctrl / win）。
macro_rules! template_shortcut_keys {
    () => {
        r#"# 数字键配这些修饰键删掉候选：用户词（云端选过的、自动造的）整个删掉，词库里的词清掉对它的学习记录。组句中要打感叹号先把词上屏
# 任意修饰键组合（alt / shift / ctrl / win 用 + 连）。Alt+数字会被 Windows 当菜单快捷键截走，缺省用 Shift；组句时才拦，不打字时照常放行给应用
delete_candidate = "shift"
# 语音开关键（按一下开始、再按一下结束）：right_alt / right_ctrl / caps_lock / scroll_lock / off
voice = "right_alt"
"#
    };
}

/// 首次运行写出的模板：默认值全部列出并注释，用户改一处即可。
/// 见 [`template_shortcut_keys!`]。
pub const TEMPLATE: &str = concat!(
    r#"# 字在输入法配置。保存后自动生效。

[general]
"#,
    r#"# 每页候选数（1–9）
page_size = 9
# 翻页键对：前一个上一页、后一个下一页。可选 "[]" 或 ",."；选 ",." 的话组句中敲逗号句号是翻页而不是上屏加标点
page_keys = "[]"
# 界面色系：cream 奶油 / zizai 字在蓝 / latte 紫藤拿铁 / forest 森林；明暗始终跟随 Windows
theme = "zizai"
# 候选窗口字体（字族名，如 "LXGW WenKai"）；空为系统字体，没装这个字体时自动回到系统字体
font = ""
# 组句中的拼音显示在哪：both 行内和候选窗口 / inline 只在行内 / window 只在候选窗口（应用里不放 marked text）
preedit = "both"
# 中文模式下整段输入是英文词时（hello / key）是否让中文候选排第一、英文词第二；缺省 false：拼音不像话的输入英文词排第一
chinese_first = false
# 中文模式下（没在组句时）敲的标点转全角：, . ? ! : ; ( ) 等，数字后面的 . 保持半角；悬浮状态条的「，。」格可以点着切
full_width_punctuation = true
# 英文模式下的同一件事，中英各记一份，状态条切的是当前模式那份
english_full_width_punctuation = false
# 双拼方案：留空为全拼；xiaohe 小鹤 / ziranma 自然码 / microsoft 微软 / sogou 搜狗
# 微软、搜狗方案的 ; 键是 ing
shuangpin = ""
# 辅码（辅助码）方案：留空为关；xiaohe 小鹤辅码。只在双拼下生效：组句中末尾敲的大写字母当辅码键，
# 敲完两码严格筛选候选（首字第 1 码 + 末字第 1 码，单字取两码）；对不上就不出候选。
# 第一码大写表示两码反转顺序匹配。辅码键不是要打的内容，回车上屏的字母不含它
fuma = ""
# 日志级别：info 缺省 / debug 详细（会记录敲的拼音与上屏的文字，配合作者排查问题时再开）。日志在 %LOCALAPPDATA%\Qingjian\logs\
log_level = "info"
# 输入日志：每次上屏记一行到数据目录的 input-log.jsonl（敲的键、看到的候选、选了什么），只写在这台电脑上，不上传；
# 用来离线评测排序和训练个人模型。false 不记；「高级」页可以清空
input_log = true
# 学习输入习惯：按你的选择调整候选顺序、记新词与敲错纠正。false 不再学，已学的仍参与排序；学习数据在数据目录，删掉文件即清空
learning = true

# 自定义短语示例：取消下面各行注释后启用；同码同位置不能重复。
# [[custom_phrases]]
# code = "ww"       # 输入码：1–32 个小写英文字母
# text = "；"       # 原样上屏的文本，可包含空格与换行
# position = 1      # 固定候选位置：1–9
# enabled = true    # 是否启用；停用仍保留位置

[shortcut]
"#,
    template_shortcut_keys!(),
    r#"

[dictionaries]
# 随包的领域词库（法律 / 医学 / 地名 / 成语 / 诗词 / IT / 财经 / 饮食 / 动物 / 汽车 / 历史人物），列在这里的才加载；
# 名字是文件名：animals automotive finance food historical_figures idioms it_computing law medicine places poetry_lines。
# 偏好设置「词库」页可以勾选
domains = ["idioms"]
# 自己导入的词库：放在配置同目录 dicts/ 下的 .qj 文件都会加载，这里列出要关掉的（文件名，不含扩展名）
disabled = []

[model]
# 本地整句模型：随包的小模型在本机给整句候选重新排序，全程离线；停顿后几十毫秒生效。关掉只用词库统计
enabled = true

[voice]
# 本地语音输入。模型文件不随源码仓库提供；准备好下面两项后再打开
enabled = false
model = "data/voice/sense-voice/model.int8.onnx"
tokens = "data/voice/sense-voice/tokens.txt"
language = "auto"
# 留空使用系统默认麦克风
input_device = ""
# 检测到说话后，连续静音多少毫秒自动开始识别；0 表示仍需再按一次快捷键
auto_stop_ms = 0
# 可选 sherpa-onnx 同音词替换资源；留空关闭
hr_lexicon = ""
hr_rule_fsts = ""

[status_bar]
# 桌面上常驻、可拖动的悬浮状态条（Windows）：「中 / 英」格点一下切换模式（开着双拼时还显示方案名）、「，。」格切全角 / 半角标点、齿轮打开设置。
# 只在当前输入法是字在时显示；与任务栏的中 / 英指示器并存
# 默认关；开着时可以拖到任意位置，拖到哪下次还在哪（拖动结束时把位置写进下面的 x / y，不用手填）
enabled = false
# 记住的屏幕位置（物理像素，拖动后自动写入）；留空则首次出现在屏幕右下角
# x = 0
# y = 0
"#
);

impl Config {
    /// 保存自定义短语列表，冲突时不修改文件。
    pub fn set_custom_phrases(
        path: &Path,
        phrases: &[qingjian_core::CustomPhrase],
    ) -> Result<(), String> {
        qingjian_core::custom_phrase::validate_phrases(phrases)?;
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => TEMPLATE.to_owned(),
            Err(e) => return Err(e.to_string()),
        };
        let mut document: DocumentMut = source.parse::<DocumentMut>().map_err(|e| e.to_string())?;
        let old = document
            .get("custom_phrases")
            .and_then(toml_edit::Item::as_array_of_tables)
            .cloned()
            .unwrap_or_default();
        let mut used = std::collections::BTreeSet::new();
        // 先按输入码和位置匹配，重排或删除时注释跟随原规则。
        let mut matches: Vec<_> = phrases
            .iter()
            .map(|p| {
                let found = old
                    .iter()
                    .enumerate()
                    .find(|(_, t)| {
                        t.get("code").and_then(toml_edit::Item::as_str) == Some(p.code.as_str())
                            && t.get("position").and_then(toml_edit::Item::as_integer)
                                == Some(p.position as i64)
                    })
                    .map(|(i, _)| i);
                if let Some(i) = found {
                    used.insert(i);
                }
                found
            })
            .collect();
        // 等长列表中修改了输入码或位置的条目，沿用其未被占用的原表。
        if old.len() == phrases.len() {
            for (i, matched) in matches.iter_mut().enumerate() {
                if matched.is_none() && used.insert(i) {
                    *matched = Some(i);
                }
            }
        }
        let mut tables = toml_edit::ArrayOfTables::new();
        for (p, matched) in phrases.iter().zip(matches) {
            let mut t = matched
                .and_then(|i| old.get(i))
                .cloned()
                .unwrap_or_default();
            for (key, mut value) in [
                ("code", toml_edit::Value::from(p.code.as_str())),
                ("text", toml_edit::Value::from(p.text.as_str())),
                ("position", toml_edit::Value::from(p.position as i64)),
                ("enabled", toml_edit::Value::from(p.enabled)),
            ] {
                if let Some(previous) = t.get(key).and_then(toml_edit::Item::as_value) {
                    if previous.to_string() == value.to_string() {
                        continue;
                    }
                    *value.decor_mut() = previous.decor().clone();
                }
                t[key] = toml_edit::Item::Value(value);
            }
            // 序列化按文档位置排序；统一锚点后，同组条目使用本次列表顺序。
            t.set_position(old.iter().filter_map(toml_edit::Table::position).min());
            tables.push(t);
        }
        document["custom_phrases"] = toml_edit::Item::ArrayOfTables(tables);
        qingjian_core::storage::write_atomic_str(path, &document.to_string())
            .map_err(|e| e.to_string())
    }

    /// 读配置。文件不存在按默认值；存在但解析失败报错，不要静默吞掉用户的笔误。
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let source = match std::fs::read_to_string(path) {
            Ok(source) => source,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(source) => {
                return Err(ConfigError::Read {
                    path: path.to_owned(),
                    source,
                });
            }
        };
        toml::from_str(&source).map_err(|source| ConfigError::Parse {
            path: path.to_owned(),
            source: Box::new(source),
        })
    }

    /// 原地改一个布尔键，见 [`Self::set_value`]。
    pub fn set_bool(path: &Path, section: &str, key: &str, value: bool) -> Result<(), ConfigError> {
        Self::set_value(path, section, key, value)
    }

    /// 原地改一个键（`[section] key = value`），其余内容、注释与顺序原样保留：
    /// 菜单和设置窗口落盘都走这里。文件不存在时从模板起步；文件有语法错误就报错不写，
    /// 不能替用户「修复」成丢了注释的文件。
    pub fn set_value(
        path: &Path,
        section: &str,
        key: &str,
        value: impl Into<toml_edit::Value>,
    ) -> Result<(), ConfigError> {
        let source = match std::fs::read_to_string(path) {
            Ok(source) => source,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => TEMPLATE.to_owned(),
            Err(source) => {
                return Err(ConfigError::Read {
                    path: path.to_owned(),
                    source,
                });
            }
        };
        let mut document: DocumentMut = source.parse().map_err(|source| ConfigError::Edit {
            path: path.to_owned(),
            source: Box::new(source),
        })?;
        // 分节不存在时先建成标准表，否则 toml_edit 会写成顶层的行内表 `predict = { enabled = true }`
        if !document.get(section).is_some_and(|item| item.is_table()) {
            document[section] = toml_edit::table();
        }
        document[section][key] = toml_edit::value(value);
        // 写临时文件再改名：输入法进程随时可能被杀，不能留半个配置文件
        qingjian_core::storage::write_atomic_str(path, &document.to_string()).map_err(|source| {
            ConfigError::Write {
                path: path.to_owned(),
                source,
            }
        })
    }

    /// 原地把一个键改成字符串数组（`[section] key = ["a", "b"]`），其余内容、注释与顺序原样保留。
    /// 设置界面改词库列表（`[dictionaries] domains` / `disabled`）走这里，[`Self::set_value`] 只能写标量。
    pub fn set_array<S: AsRef<str>>(
        path: &Path,
        section: &str,
        key: &str,
        values: &[S],
    ) -> Result<(), ConfigError> {
        let source = match std::fs::read_to_string(path) {
            Ok(source) => source,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => TEMPLATE.to_owned(),
            Err(source) => {
                return Err(ConfigError::Read {
                    path: path.to_owned(),
                    source,
                });
            }
        };
        let mut document: DocumentMut = source.parse().map_err(|source| ConfigError::Edit {
            path: path.to_owned(),
            source: Box::new(source),
        })?;
        if !document.get(section).is_some_and(|item| item.is_table()) {
            document[section] = toml_edit::table();
        }
        let mut array = toml_edit::Array::new();
        for value in values {
            array.push(value.as_ref());
        }
        document[section][key] = toml_edit::value(array);
        qingjian_core::storage::write_atomic_str(path, &document.to_string()).map_err(|source| {
            ConfigError::Write {
                path: path.to_owned(),
                source,
            }
        })
    }

    /// 文件不存在时写出模板，返回是否写了。
    pub fn write_template_if_missing(path: &Path) -> Result<bool, ConfigError> {
        if path.exists() {
            return Ok(false);
        }
        qingjian_core::storage::write_atomic_str(path, TEMPLATE).map_err(|source| {
            ConfigError::Write {
                path: path.to_owned(),
                source,
            }
        })?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_parses_to_defaults() {
        let config: Config = toml::from_str(TEMPLATE).unwrap();
        assert_eq!(config, Config::default());
    }

    /// 0.1.3 及以前的配置文件里还写着英文候选那两项，读到当没有（英文模式已恒为直通）。
    #[test]
    fn retired_english_candidate_keys_are_ignored() {
        let config: Config = toml::from_str(
            "[general]\nenglish_candidates = true\n[apps]\nenglish_candidates_off = [\"Code.exe\"]\n",
        )
        .unwrap();
        assert_eq!(config, Config::default());
    }

    /// 0.1.6 及以前可切换 GDI 绘制；升级后旧键不影响其余配置加载。
    #[test]
    fn retired_renderer_key_is_ignored() {
        let config: Config =
            toml::from_str("[general]\nrenderer = \"system\"\nfont = \"LXGW WenKai\"\n").unwrap();
        assert_eq!(config.general.font, "LXGW WenKai");
    }

    /// 0.1.6 及以前可切换候选排布；升级后旧键不影响其余配置加载。
    #[test]
    fn retired_layout_key_is_ignored() {
        let config: Config =
            toml::from_str("[general]\nlayout = \"vertical\"\npage_size = 5\n").unwrap();
        assert_eq!(config.general.page_size(), 5);
    }

    #[test]
    fn partial_file_keeps_other_defaults() {
        let config: Config = toml::from_str("[model]\nenabled = false\n").unwrap();
        assert!(!config.model.enabled);
        // 没写的分节保持各自缺省：状态条缺省关、每页候选数缺省拉满。
        assert!(!config.status_bar.enabled);
        assert_eq!(config.general.page_size(), 9);
    }

    #[test]
    fn general_and_shortcut_sections_parse() {
        let config: Config = toml::from_str(
            "[general]\npage_size = 5\npage_keys = \"[]\"\ntheme = \"forest\"\npreedit = \"window\"\n[shortcut]\ndelete_candidate = \"ctrl\"\n",
        )
        .unwrap();
        assert_eq!(config.general.page_size(), 5);
        assert_eq!(config.general.page_keys(), ('[', ']'));
        assert_eq!(config.general.theme, ColorScheme::Forest);
        assert_eq!(config.general.preedit, PreeditMode::Window);
        assert_eq!(config.general.learning_language, "en");
        assert_eq!(config.general.shuangpin(), None);
        assert_eq!(config.general.log_level, LogLevel::Info);
        assert!(config.shortcut.delete_keys().ctrl);
    }

    #[test]
    fn set_value_writes_strings_and_integers() {
        let path = std::env::temp_dir().join("qingjian-config-set-value-test.toml");
        let _ = std::fs::remove_file(&path);
        Config::set_value(&path, "general", "page_size", 5i64).unwrap();
        Config::set_value(&path, "general", "theme", "latte").unwrap();
        let config = Config::load(&path).unwrap();
        assert_eq!(config.general.page_size, 5);
        assert_eq!(config.general.theme, ColorScheme::Latte);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn set_bool_keeps_comments_and_flips_only_that_key() {
        let path = std::env::temp_dir().join("qingjian-config-set-bool-test.toml");
        std::fs::write(&path, "# 头注释\n[model]\n# 说明\nenabled = false\n").unwrap();
        Config::set_bool(&path, "model", "enabled", true).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.starts_with("# 头注释\n[model]\n# 说明\nenabled = true\n"),
            "{text}"
        );
        let config = Config::load(&path).unwrap();
        assert!(config.model.enabled);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn set_bool_starts_from_template_when_missing() {
        let path = std::env::temp_dir().join("qingjian-config-set-bool-missing-test.toml");
        let _ = std::fs::remove_file(&path);
        Config::set_bool(&path, "model", "enabled", false).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("# 字在输入法配置"));
        assert!(!Config::load(&path).unwrap().model.enabled);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn set_bool_refuses_broken_file() {
        let path = std::env::temp_dir().join("qingjian-config-set-bool-broken-test.toml");
        std::fs::write(&path, "[model\nenabled = false\n").unwrap();
        assert!(matches!(
            Config::set_bool(&path, "model", "enabled", true),
            Err(ConfigError::Edit { .. })
        ));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[model\nenabled = false\n"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_is_default() {
        let path = std::env::temp_dir().join("qingjian-config-missing-test.toml");
        let _ = std::fs::remove_file(&path);
        assert_eq!(Config::load(&path).unwrap(), Config::default());
    }
}
