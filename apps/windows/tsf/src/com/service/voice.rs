//! 语音状态同步与最终文本的可靠写入。

use qingjian_platform::VoiceTrigger;
use qingjian_platform::protocol::VoiceSync;

use super::TextService_Impl;
use crate::com::edit::request_voice_update;
use crate::com::log::log;

impl TextService_Impl {
    pub(super) fn apply_voice_sync(&self, sync: VoiceSync) {
        let trigger = if sync.enabled {
            sync.trigger
        } else {
            VoiceTrigger::Off
        };
        if self.shared.voice_trigger() != trigger {
            log(&format!("语音快捷键已同步: {trigger:?}"));
            self.shared.set_voice_trigger(trigger);
        }
        self.shared.set_voice_active(sync.is_active());

        let Some(delivery) = sync.delivery else {
            return;
        };
        if self.shared.voice_committed() == Some(delivery.request) {
            let acked = {
                let mut guard = self.engine.borrow_mut();
                match guard.as_mut() {
                    Some(client) => match client.voice_ack(delivery.request) {
                        Ok(()) => true,
                        Err(error) => {
                            log(&format!("重发语音确认失败: {error}"));
                            *guard = None;
                            false
                        }
                    },
                    None => false,
                }
            };
            if acked {
                self.shared.set_voice_committed(None);
                self.shared.set_voice_queued(None);
                self.shared.set_voice_active(false);
            }
            return;
        }
        if self.shared.voice_queued() == Some(delivery.request) {
            return;
        }
        let Some(context) = self.shared.last_context() else {
            return;
        };
        self.shared.set_voice_queued(Some(delivery.request));
        if let Err(error) = request_voice_update(
            &context,
            self.client_id.get(),
            self.engine.clone(),
            self.shared.clone(),
            delivery.request,
            delivery.text,
        ) {
            self.shared.set_voice_queued(None);
            log(&format!("语音上屏的编辑会话没被受理: {error}"));
        }
    }
}
