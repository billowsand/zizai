//! 悬浮状态条：中英模式只在 DLL 侧，DLL 用 `ModeChanged` 推来（激活 / 获焦 / 切换时）；
//! 切成别的输入法时 DLL 发 `ImeSwitched` 收起。会话关闭（应用退出）不收——状态条常驻桌面。
//! 状态条上的点击经 [`StatusEvent`] 回到这里：切模式记成 `pending_mode` 等**状态条归属的那个会话**
//! 用 `SyncMode` 来取（所有激活了 TSF 的进程都在问，见 [`Router::take_pending_mode`]），
//! 切标点 / 拖动写回配置文件（热加载会再读回来）。

mod event;
mod sink;
mod view;

use qingjian_platform::Config;
use qingjian_platform::protocol::SessionId;

pub use self::event::StatusEvent;
pub use self::sink::{NoopStatusSink, StatusSink};
pub use self::view::StatusView;
use super::Router;

impl Router {
    /// DLL 报来自己的模式（激活、获焦、切换时各一次）：状态条显示它，归属也转到它名下。
    pub(super) fn handle_mode_changed(&mut self, session: SessionId, english: bool) {
        if self.status_session != Some(session) {
            // 换了归属：上一个应用没取走的那次点击作废，别隔着应用补切一次。
            self.pending_mode = None;
            self.status_session = Some(session);
        }
        self.status_mode = Some(english);
        self.reconcile_status();
    }

    pub(super) fn handle_ime_switched(&mut self) {
        self.status_mode = None;
        self.status_session = None;
        self.pending_mode = None;
        self.reconcile_status();
    }

    /// 有键落到这个会话 = 用户确实在这里打字，状态条的归属转过来。
    /// 后台应用启动时也会上报一次模式（`Activate` 里就报），不能让它一直霸着点击的去向。
    pub(super) fn claim_status_session(&mut self, session: SessionId) {
        if self.status_session == Some(session) {
            return;
        }
        self.status_session = Some(session);
        self.pending_mode = None;
    }

    /// 归属会话退出：没人来取的那次点击作废，状态条本身留着（它是桌面常驻的）。
    pub(super) fn forget_status_session(&mut self, session: SessionId) {
        if self.status_session == Some(session) {
            self.status_session = None;
            self.pending_mode = None;
        }
    }

    /// 状态条归属的会话来取点出的目标模式；取走即清。
    ///
    /// **只给归属会话。** 每个激活了 TSF 的进程都在按同一个节拍问，而跨进程的
    /// `OnSetFocus(FALSE)` 并不可靠，好几个进程会同时自认在前台：不认会话的话，
    /// 这一次点击就是谁先问到谁切，前台应用反而切不动（2026-09-21 真机日志里
    /// 连点四下落在四个不同 pid 上）。
    pub(super) fn take_pending_mode(&mut self, session: SessionId) -> Option<bool> {
        if self.status_session != Some(session) {
            return None;
        }
        self.pending_mode.take()
    }

    /// 状态条上的操作。
    pub fn handle_status_event(&mut self, event: StatusEvent) {
        match event {
            StatusEvent::ToggleMode => {
                let Some(english) = self.status_mode else {
                    return;
                };
                // 先把状态条翻过来，DLL 取走后回报 ModeChanged 再对一次账。
                self.pending_mode = Some(!english);
                self.status_mode = Some(!english);
                tracing::debug!(english = !english, "状态条：请求切换中英模式");
            }
            StatusEvent::TogglePunctuation => {
                // 中英各记一份，切的是当前模式那份；还没报过模式时按中文算。
                let english = self.status_mode == Some(true);
                let full_width = !self.full_width_for(english);
                let key = if english {
                    self.config.english_full_width = full_width;
                    "english_full_width_punctuation"
                } else {
                    self.config.full_width = full_width;
                    "full_width_punctuation"
                };
                tracing::debug!(english, full_width, "状态条：切换全角标点");
                self.persist("general", key, full_width);
            }
            StatusEvent::Moved(x, y) => {
                self.config.status_pos = Some((x, y));
                self.persist("status_bar", "x", i64::from(x));
                self.persist("status_bar", "y", i64::from(y));
            }
        }
        self.reconcile_status();
    }

    /// 写回配置文件一个键；没有配置路径（测试）就只改内存。
    fn persist(&self, section: &str, key: &str, value: impl Into<toml_edit::Value>) {
        let Some(path) = self.config_path() else {
            return;
        };
        if let Err(error) = Config::set_value(path, section, key, value) {
            tracing::warn!(%error, section, key, "写回配置失败");
        }
    }

    /// 当前模式下标点转不转全角：中英各一份配置。
    pub(super) fn full_width_for(&self, english: bool) -> bool {
        if english {
            self.config.english_full_width
        } else {
            self.config.full_width
        }
    }

    /// 开着且青简在前台就显示，否则收起。热加载后也调一次。
    pub(super) fn reconcile_status(&mut self) {
        match self.status_mode {
            Some(english) if self.config.status_enabled => {
                self.status.show_status(StatusView {
                    english,
                    scheme: self.config.shuangpin.map(|scheme| scheme.key().to_owned()),
                    full_width: self.full_width_for(english),
                    color_scheme: self.config.color_scheme,
                    anchor: self.config.status_pos,
                });
            }
            _ => self.status.hide_status(),
        }
    }
}
