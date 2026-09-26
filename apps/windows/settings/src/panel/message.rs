//! 设置窗口的消息类型：导航切换与各页的「改动」，根组件的 `update` 据此落盘。

/// 设置窗口的消息；「改动」消息带控件新值，`update` 据此落盘。
#[derive(Clone)]
pub(crate) enum Message {
    /// 导航切换分节（`None` 是取消选中，忽略）。
    Navigate(Option<String>),

    // 通用页 · 输入方案
    Shuangpin(Option<usize>),
    /// 辅码方案下拉（关 / 小鹤辅码）。
    Fuma(Option<usize>),
    /// 候选显示辅码下拉（敲了辅码时 / 始终 / 不显示）。
    FumaHint(Option<usize>),

    // 通用页 · 按键
    PageKeys(Option<usize>),
    DeleteCandidate(Option<usize>),

    // 通用页 · 标点
    FullWidthPunctuation(bool),
    EnglishFullWidthPunctuation(bool),

    // 通用页 · 候选质量
    /// 本地整句模型开关（`[model] enabled`）。
    LocalModel(bool),
    ChineseFirst(bool),
    /// 学习输入习惯开关。
    Learning(bool),

    // 候选窗口页
    Theme(Option<usize>),
    PageSize(Option<f64>),
    Preedit(Option<usize>),
    /// 从系统字体列表里选了一个：0 是「系统字体」，其后按 `Settings::families` 的下标。
    Font(Option<usize>),
    StatusBar(bool),

    // 词库页
    ToggleDomain(String, bool),
    ToggleUserDict(String, bool),
    /// 挪进 dicts\removed，不真删。
    RemoveUserDict(String),
    ImportDictionary,

    // 高级页
    VerboseLog(bool),
    InputLog(bool),
    OpenConfigFile,
    OpenDataDir,
    OpenLogDir,
    /// 日志目录 + config.toml 打成 zip 放桌面。
    ExportLogs,
    ClearInputLog,

    // 关于页
    OpenWebsite,
    OpenRepository,
}
