//! 往文档写字：按键结果经异步编辑会话写上屏文本 + 组句拼音行；失焦 / 停用 / 切模式时让 Server 交出缓冲区原样落定。

use windows::Win32::UI::TextServices::ITfContext;
use windows::core::Ref;

use super::TextService_Impl;
use crate::com::edit::request_update;
use crate::com::log::log;

impl TextService_Impl {
    /// 失焦 / 停用 / 切模式：让 Server 交出缓冲区，原样落进最近收键的文档并收掉组句。
    /// 组句已被应用终止的（拼音已是普通文本）只清 Server 不再插。
    pub(super) fn commit_pending(&self) {
        let stale = self.shared.take_server_stale();
        if !self.shared.composing() && !stale {
            return;
        }
        let text = {
            let mut guard = self.engine.borrow_mut();
            let Some(client) = guard.as_mut() else {
                self.shared.reset();
                return;
            };
            match client.commit() {
                Ok(text) => text,
                Err(error) => {
                    log(&format!("失焦上屏失败，断开，下一键重连: {error}"));
                    drop(guard);
                    self.disconnect();
                    return;
                }
            }
        };
        if stale {
            return;
        }
        self.shared.end_composing();
        let Some(context) = self.shared.last_context() else {
            let chars = text.as_deref().map_or(0, |value| value.chars().count());
            log(&format!("失焦上屏没有上下文，丢弃 {chars} 字"));
            self.shared.reset();
            return;
        };
        let chars = text.as_deref().map_or(0, |value| value.chars().count());
        log(&format!("失焦上屏 {chars} 字"));
        let requested = request_update(
            &context,
            self.client_id.get(),
            self.engine.clone(),
            self.shared.clone(),
            text.filter(|t| !t.is_empty()),
            String::new(),
        );
        if let Err(error) = requested {
            log(&format!("失焦上屏的编辑会话没被受理: {error}"));
            self.shared.reset();
        }
    }

    /// 经异步编辑会话把上屏文本 + 组句拼音行写进文档。
    pub(super) fn update_document(
        &self,
        pic: Ref<ITfContext>,
        commit: Option<String>,
        preedit: String,
    ) {
        // 退到最后一个字母时帧已空但组句句柄还在，得跑一次把它收掉。
        if commit.is_none()
            && preedit.is_empty()
            && !self.shared.composing()
            && !self.shared.has_composition()
        {
            return;
        }
        let Ok(context) = pic.ok() else {
            let commit_chars = commit.as_deref().map_or(0, |value| value.chars().count());
            let preedit_chars = preedit.chars().count();
            log(&format!(
                "无上下文，丢弃更新: commit={commit_chars} 字 preedit={preedit_chars} 字"
            ));
            return;
        };
        if let Err(error) = request_update(
            context,
            self.client_id.get(),
            self.engine.clone(),
            self.shared.clone(),
            commit,
            preedit,
        ) {
            log(&format!("请求组句更新失败: {error}"));
        }
    }
}
