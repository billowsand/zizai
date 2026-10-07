//! 不用动手就能查出来的「Server 起不来」原因：exe 不在、uiAccess 构建的签名在本机不受信任、没装在安全位置。

use super::{server, signature};

/// 一次静态诊断的结果。
#[derive(Debug, Clone, Default)]
pub(crate) struct Diagnosis {
    /// 会让 Server 起不来的已知原因，给用户看的一句话；查不出为 `None`。
    pub(crate) blocker: Option<String>,
}

impl Diagnosis {
    /// 查一遍。校验签名要把 exe 整个哈希一遍（几十毫秒），别放在 UI 线程上。
    pub(crate) fn check() -> Self {
        let Some(exe) = server::server_exe() else {
            return Self::default();
        };
        let ui_access = exe.is_file() && signature::wants_ui_access(&exe);
        let blocker = if !exe.is_file() {
            Some(format!(
                "安装目录里没有 {}，请重新安装字在。",
                server::SERVER_EXE
            ))
        } else if ui_access && !signature::signature_trusted(&exe) {
            Some(
                "这是自签名的内测安装包，它的证书在这台电脑上不受信任，系统会拒绝启动输入法服务，\
                 所以只能打英文。请以管理员身份导入随安装包附带的 Qingjian-Dev-CodeSign.cer，\
                 或改装不签名的安装包。"
                    .to_owned(),
            )
        } else if ui_access && !signature::in_secure_location(&exe) {
            Some("输入法服务必须装在 Program Files 里才能启动，请重新安装到默认位置。".to_owned())
        } else {
            None
        };
        Self { blocker }
    }
}
