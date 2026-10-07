//! 「高级 → 诊断与修复」：查输入法服务（Server）起没起、为什么起不来，并一键修复。
//!
//! Server 没起时各应用里的输入法整键放行，用户看到的就是「候选窗不出来、只能打英文」。修复分两档：
//! [`RepairTask::restart`] 普通权限就能做——请现任让位、结束残留进程、像双击一样重新拉起；
//! [`RepairTask::deep_repair`] 要管理员——设置程序以 [`ELEVATED_FLAG`] 提权再跑一份自己（[`run_elevated`]），
//! 清掉旧版本留下的 DLL 与计划任务、重新注册输入法，然后照常重启。
//! 提权那份**不拉 Server**：拉起的 Server 会继承管理员令牌，变成降级实例。

mod cleanup;
mod diagnosis;
mod log;
mod progress;
mod server;
mod signature;
mod task;

pub(crate) use diagnosis::Diagnosis;
pub(crate) use task::RepairTask;

/// 提权修复的命令行开关，后面可跟会话号：`qingjian-settings.exe --repair 1`。
pub(crate) const ELEVATED_FLAG: &str = "--repair";

/// 命令行带 [`ELEVATED_FLAG`] 时做完提权那一半，返回退出码（失败步数）；不带返回 `None`，照常开窗口。
pub(crate) fn run_elevated() -> Option<i32> {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some(ELEVATED_FLAG) {
        return None;
    }
    let session = args.next().and_then(|value| value.parse().ok());
    Some(cleanup::run_elevated(session))
}

/// 拉 Server 失败的系统错误码翻成给用户看的一句话。
fn launch_error_text(code: u32) -> String {
    match code {
        // ERROR_ELEVATION_REQUIRED / ERROR_DS_REFERRAL：带 uiAccess 的 exe 签名不受信任或不在 Program Files。
        740 | 8235 => format!(
            "系统拒绝启动输入法服务（系统错误 {code}）：这个安装包的签名在这台电脑上不受信任，\
             或没装在 Program Files。请导入随包的内测证书，或改装不签名的安装包。"
        ),
        2 | 3 => "找不到 qingjian-server.exe，请重新安装字在。".to_owned(),
        5 => "启动输入法服务被拒绝（系统错误 5），可能被安全软件拦截了。".to_owned(),
        _ => format!("启动输入法服务失败（系统错误 {code}）。"),
    }
}
