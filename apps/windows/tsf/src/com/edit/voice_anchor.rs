//! 没有拼音组句时，也为语音面板量一次当前插入点。

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use windows::Win32::Foundation::E_FAIL;
use windows::Win32::UI::TextServices::{
    ITfContext, ITfEditSession, ITfEditSession_Impl, TF_ES_READ,
};
use windows::core::{Error, Result, implement};

use super::update::request;
use super::{anchor_rect, selection_start};
use crate::com::composition::Shared;
use crate::com::service::SharedClient;

#[implement(ITfEditSession)]
struct VoiceAnchorSession {
    context: ITfContext,

    engine: SharedClient,

    shared: Rc<Shared>,
}

impl ITfEditSession_Impl for VoiceAnchorSession_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        match catch_unwind(AssertUnwindSafe(|| self.report(ec))) {
            Ok(result) => result,
            Err(_) => Err(Error::from(E_FAIL)),
        }
    }
}

impl VoiceAnchorSession {
    fn report(&self, ec: u32) -> Result<()> {
        let Some(range) = selection_start(&self.context, ec) else {
            return Ok(());
        };
        let rect = anchor_rect(&self.context, ec, &range, None);
        self.shared.set_last_anchor(rect);
        if let Ok(mut guard) = self.engine.try_borrow_mut()
            && let Some(client) = guard.as_mut()
            && let Err(error) = client.position_candidates(rect)
        {
            crate::com::log::log(&format!("上报语音面板位置失败: {error}"));
        }
        Ok(())
    }
}

pub(crate) fn request_voice_anchor(
    context: &ITfContext,
    client_id: u32,
    engine: SharedClient,
    shared: Rc<Shared>,
) -> Result<()> {
    let session = VoiceAnchorSession {
        context: context.clone(),
        engine,
        shared,
    };
    request(context, client_id, session.into(), TF_ES_READ)
}
