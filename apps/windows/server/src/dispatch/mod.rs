//! 协议分派：把 DLL 发来的 [`ClientMessage`] 交给 Engine，产出回给 DLL 的 [`ServerMessage`]。
//! 消息分派在 [`message`]，会话在 [`session`]，组句展示状态在 [`composed`]，按键在 [`key`]，
//! 候选窗口输出在 [`candidates`]，状态条在 [`status`]，配置热加载在 [`reload`]，
//! 本地整句模型在 [`rescore`]。

mod candidates;
mod composed;
mod config;
mod key;
mod message;
mod reload;
mod rescore;
mod session;
mod status;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use qingjian_core::{Engine, FumaTable};
use qingjian_platform::LocalModelConfig;
use qingjian_platform::protocol::{ClientMessage, Frame, ScreenRect, ServerMessage, SessionId};

pub use self::candidates::{CandidateSink, NoopSink, VoiceView};
use self::composed::Composed;
pub use self::config::RouterConfig;
use self::reload::ConfigReload;
pub use self::reload::load_fuma;
pub use self::rescore::find_model;
use self::rescore::{ModelLoader, RescoreState};
use self::session::SessionInfo;
pub use self::status::{NoopStatusSink, StatusEvent, StatusSink, StatusView};
use crate::voice::{VoiceBackend, VoiceCoordinator};
use qingjian_platform::VoiceTrigger;

/// 学习数据落盘间隔（与 macOS 壳一致）；Server 没有定时器，借消息节拍看时间。
const LEARNING_FLUSH_INTERVAL: Duration = Duration::from_secs(60);

/// 同一时刻只有一个应用有键盘焦点，所以一个 Engine 持当前组句；焦点切到别的会话时先清掉上一个的残留。
pub struct Router {
    /// 输入内核，进程内唯一。
    engine: Engine,

    /// 随包辅码表；开 / 关由 `[general] fuma` 决定，热加载时按开关重新接上。
    /// 与 Engine 共用同一份（表有几千条，重接只克隆指针）。
    fuma_table: Option<Arc<FumaTable>>,

    /// 每页候选数 / 外观 / 翻页键等。
    config: RouterConfig,

    /// 活跃会话及各自的宿主应用。
    sessions: HashMap<SessionId, SessionInfo>,

    /// 当前持有组句的会话。
    focused: Option<SessionId>,

    /// 当前组句的展示状态；没在组句时为 `None`。
    composed: Option<Composed>,

    /// 删候选后的屏幕提示，随下一帧下发、下一次按键清。
    notice: Option<String>,

    /// 当前高亮候选在布局里的下标（跨页）。
    highlight: usize,

    /// 这轮查询里动过高亮：本地模型重排不动用户已经翻过 / 选过的那一页。
    navigated: bool,

    /// 上次把学习数据落盘的时间。
    last_flush: Instant,

    /// 配置热加载状态；`None` 表示不热加载。
    reload: Option<ConfigReload>,

    /// 候选窗口输出端；Windows 上由 [`crate::ui`] 注入。
    candidates: Box<dyn CandidateSink>,

    /// 悬浮状态条输出端；Windows 上由 [`crate::ui`] 注入。
    status: Box<dyn StatusSink>,

    /// 状态条要显示的中英模式；`None` 表示青简没在前台（还没有会话报过模式 / 切成了别的输入法），不显示。
    /// 应用退出不影响它：状态条是桌面常驻的，只跟「当前输入法是不是青简」走。
    status_mode: Option<bool>,

    /// 状态条上点出来、还没被 DLL 用 `SyncMode` 取走的目标模式。
    pending_mode: Option<bool>,

    /// 聚焦会话最近报来的光标矩形；重排结果异步到达时按它原地重摆候选窗口。
    last_rect: Option<ScreenRect>,

    /// 上次真正显示的帧与位置：没变就不重画（组字期间的空转 Poll 很多）。
    last_shown: Option<(Frame, ScreenRect)>,

    /// 同一个候选窗口当前是否处在语音态。
    voice_visible: bool,

    /// 错误 / 空录音提示只展示一次，约两秒后由低频 `SyncMode` 收起。
    voice_notice_until: Option<Instant>,

    voice_notice_seen: bool,

    /// 本地整句模型（`.qjm` 或三件套目录）；没有模型文件为 `None`。
    model_path: Option<PathBuf>,

    /// 进行中的模型加载；加载完接到 Engine 上就清掉。
    model_loader: Option<ModelLoader>,

    /// 上次套用的 `[model]`，变了才重载 / 卸载。
    applied_model: LocalModelConfig,

    /// 重排的防抖 / 轮询进行态。
    rescore: RescoreState,

    /// 独立语音 Worker 的会话绑定与可靠交付状态。
    voice: VoiceCoordinator,
}

impl Router {
    pub fn new(engine: Engine, config: RouterConfig) -> Self {
        Self {
            engine,
            fuma_table: None,
            config: RouterConfig {
                page_size: config.page_size.max(1),
                ..config
            },
            sessions: HashMap::new(),
            focused: None,
            composed: None,
            notice: None,
            highlight: 0,
            navigated: false,
            last_flush: Instant::now(),
            reload: None,
            candidates: Box::new(NoopSink),
            status: Box::new(NoopStatusSink),
            status_mode: None,
            pending_mode: None,
            last_rect: None,
            last_shown: None,
            voice_visible: false,
            voice_notice_until: None,
            voice_notice_seen: false,
            model_path: None,
            model_loader: None,
            applied_model: LocalModelConfig::default(),
            rescore: RescoreState::default(),
            voice: VoiceCoordinator::default(),
        }
    }

    /// 启用语音输入；Worker 后端由进程入口装配，测试可注入替身。
    pub fn configure_voice(&mut self, trigger: VoiceTrigger, backend: Box<dyn VoiceBackend>) {
        self.voice.configure(trigger, backend);
    }

    /// 语音已配置但 Worker 无法启动；仍下发失败状态，便于前端给出反馈。
    pub fn configure_voice_failure(&mut self, trigger: VoiceTrigger, message: String) {
        self.voice.configure_failure(trigger, message);
    }

    pub fn set_candidate_sink(&mut self, sink: Box<dyn CandidateSink>) {
        sink.set_font(self.config.font.clone());
        self.candidates = sink;
    }

    /// 直接碰 Engine：测试里改启动时才设的开关。
    pub fn engine_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }

    /// 随包辅码表交给 Router：热加载按 `[general] fuma` 开关重新接。
    pub fn set_fuma_table(&mut self, table: Option<Arc<FumaTable>>) {
        self.fuma_table = table.clone();
        self.engine.set_fuma(table);
    }

    pub fn set_status_sink(&mut self, sink: Box<dyn StatusSink>) {
        self.status = sink;
    }

    /// 处理一条消息；`None` 表示不用回话。到点顺带把学习数据落盘。
    pub fn handle(&mut self, message: ClientMessage) -> Option<ServerMessage> {
        let response = self.dispatch(message);
        if self.last_flush.elapsed() >= LEARNING_FLUSH_INTERVAL {
            self.flush_learning();
        }
        response
    }

    pub fn flush_learning(&mut self) {
        self.engine.flush_learning();
        self.last_flush = Instant::now();
    }
}
