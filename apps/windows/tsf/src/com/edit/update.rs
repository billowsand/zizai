//! 编辑会话：TSF 不允许直接改文档，要经 `RequestEditSession` 申请，在回调里拿着 edit cookie 写。
//!
//! 必须用异步会话（不带 `TF_ES_SYNC`）：沉浸式应用（Win11 新记事本等）的文本存区隔着进程边界，
//! 同步读写会让 `textinputframework.dll` 访问违例把宿主整个搞崩。回调可能在 `OnKeyDown` 返回之后才跑。

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use windows::Win32::Foundation::E_FAIL;
use windows::Win32::UI::TextServices::{
    ITfContext, ITfEditSession, ITfEditSession_Impl, TF_CONTEXT_EDIT_CONTEXT_FLAGS, TF_ES_READWRITE,
};
use windows::core::{Error, Result, implement};

use crate::com::composition::{Shared, apply};
use crate::com::log::log;
use crate::com::service::SharedClient;

/// 一次性的读写会话：把本次按键的组句更新写进 `context`。
#[implement(ITfEditSession)]
pub(crate) struct UpdateSession {
    /// 目标文档上下文。
    context: ITfContext,

    /// 引擎层：量到光标矩形后报给 Server 摆候选窗口。
    engine: SharedClient,

    /// 组句状态。
    shared: Rc<Shared>,

    /// 本次要落定上屏的文本。
    commit: Option<String>,

    /// 本次组句拼音行；空串表示收起组句。
    preedit: String,

    /// 语音听写请求；只有文档写入成功后才确认。
    voice_ack: Option<u64>,
}

impl ITfEditSession_Impl for UpdateSession_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        // 从框架的 C++ 调进来：panic 不能越过 FFI。
        let result = catch_unwind(AssertUnwindSafe(|| {
            apply(
                &self.shared,
                &self.engine,
                &self.context,
                ec,
                self.commit.as_deref(),
                &self.preedit,
            )
        }));
        match result {
            Ok(Ok(())) => {
                if let Some(request) = self.voice_ack {
                    self.shared.set_voice_queued(None);
                    self.shared.set_voice_committed(Some(request));
                    let acked = if let Ok(mut guard) = self.engine.try_borrow_mut() {
                        match guard.as_mut() {
                            Some(client) => match client.voice_ack(request) {
                                Ok(()) => true,
                                Err(error) => {
                                    log(&format!("语音结果已写入，但确认发送失败: {error}"));
                                    *guard = None;
                                    false
                                }
                            },
                            None => false,
                        }
                    } else {
                        false
                    };
                    if acked {
                        self.shared.set_voice_committed(None);
                        self.shared.set_voice_active(false);
                    }
                }
                Ok(())
            }
            Ok(Err(error)) => {
                if self.voice_ack.is_some() {
                    self.shared.set_voice_queued(None);
                }
                log(&format!("组句更新失败: {error}"));
                Err(error)
            }
            Err(_) => {
                if self.voice_ack.is_some() {
                    self.shared.set_voice_queued(None);
                }
                log("组句更新回调 panic（已兜住）");
                Err(Error::from(E_FAIL))
            }
        }
    }
}

/// 请求一个异步读写会话。`Ok` 只说明已受理，写入结果在回调里记日志。
pub(crate) fn request_update(
    context: &ITfContext,
    client_id: u32,
    engine: SharedClient,
    shared: Rc<Shared>,
    commit: Option<String>,
    preedit: String,
) -> Result<()> {
    let session = UpdateSession {
        context: context.clone(),
        engine,
        shared,
        commit,
        preedit,
        voice_ack: None,
    };
    request(context, client_id, session.into(), TF_ES_READWRITE)
}

/// 请求把一次最终听写文本直接写进文档；成功写入后在回调内向 Server ACK。
pub(crate) fn request_voice_update(
    context: &ITfContext,
    client_id: u32,
    engine: SharedClient,
    shared: Rc<Shared>,
    request_id: u64,
    text: String,
) -> Result<()> {
    let session = UpdateSession {
        context: context.clone(),
        engine,
        shared,
        commit: Some(text),
        preedit: String::new(),
        voice_ack: Some(request_id),
    };
    request(context, client_id, session.into(), TF_ES_READWRITE)
}

/// 异步提交一个编辑会话；会话对象由框架持有到回调跑完。
pub(super) fn request(
    context: &ITfContext,
    client_id: u32,
    session: ITfEditSession,
    flags: TF_CONTEXT_EDIT_CONTEXT_FLAGS,
) -> Result<()> {
    unsafe { context.RequestEditSession(client_id, &session, flags)? }.ok()
}
