//! Windows Server 单实例与接管用到的名字：命名管道、单实例互斥体、让位事件、构建标记。
//! Server、TSF DLL、安装脚本三方要对上同一串字面量（安装脚本那份手抄在 `qingjian.iss`，改这里要同步）。
//!
//! 只拼字符串，不碰系统 API。`Local\` 前缀的内核对象本来就是每登录会话一份；管道名却是整机共用的，
//! 所以管道名里要带会话号，否则快速切换用户 / 远程桌面时第二个用户的 DLL 会连到第一个用户的 Server。

use crate::protocol::DEFAULT_PIPE_NAME;

/// 旧版 DLL 连的管道名（不带会话号）。升级后还留在没重启的应用里的旧 DLL 只认它，
/// Server 尽力在它上面也听一份；新 DLL 不碰它。
pub const LEGACY_PIPE_NAME: &str = DEFAULT_PIPE_NAME;

/// DLL 拉 Server 的会话内互斥体：同一时刻只让一个进程去拉。安装器在装的过程中一直持有它，
/// 让各应用里的（新旧）DLL 都别在文件替换到一半时拉起 Server。
pub const LAUNCH_MUTEX: &str = r"Local\Qingjian.ServerLaunch";

/// 本会话的管道名 `\\.\pipe\qingjian.<会话号>`。
pub fn session_pipe_name(session: u32) -> String {
    format!(r"{DEFAULT_PIPE_NAME}.{session}")
}

/// 单实例互斥体：Server 进程从启动（装配 Engine 之前）到退出一直持有；进程没了系统自动放掉（遗弃）。
/// DLL 看到它在就不拉 Server——Server 正在起来或正在接管的路上，管道一会儿就有。
pub fn instance_mutex_name(pipe: &str) -> String {
    format!(r"Local\Qingjian.Server.{}", object_suffix(pipe))
}

/// 让位事件：现任 Server 建、等；后来的 Server（或安装器）只打开、`SetEvent`，请现任收尾退出。
pub fn step_down_event_name(pipe: &str) -> String {
    format!(r"Local\Qingjian.ServerStepDown.{}", object_suffix(pipe))
}

/// 构建标记：现任把自己的构建号挂成一个命名事件，后来者打开得到就说明「同一份程序已经在跑」，不必接管。
pub fn build_marker_name(pipe: &str, build: &str) -> String {
    format!(
        r"Local\Qingjian.ServerBuild.{}.{}",
        object_suffix(pipe),
        sanitize(build)
    )
}

/// 降级标记：现任跑在不理想的上下文里（被提权拉起、该有 uiAccess 却没拿到）时挂出，
/// 后来的正常实例据此接管。
pub fn degraded_marker_name(pipe: &str) -> String {
    format!(r"Local\Qingjian.ServerDegraded.{}", object_suffix(pipe))
}

/// 管道名去掉 `\\.\pipe\` 前缀、非字母数字换 `_`：`\\.\pipe\qingjian.1` → `qingjian_1`。
fn object_suffix(pipe: &str) -> String {
    let bare = pipe.strip_prefix(r"\\.\pipe\").unwrap_or(pipe);
    sanitize(bare)
}

fn sanitize(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_pipe_carries_the_session_number() {
        assert_eq!(session_pipe_name(3), r"\\.\pipe\qingjian.3");
    }

    /// 安装脚本手抄了这两串（`qingjian.iss` 的 StopServerGracefully），变了要一起改。
    #[test]
    fn object_names_match_the_installer() {
        let pipe = session_pipe_name(1);
        assert_eq!(
            instance_mutex_name(&pipe),
            r"Local\Qingjian.Server.qingjian_1"
        );
        assert_eq!(
            step_down_event_name(&pipe),
            r"Local\Qingjian.ServerStepDown.qingjian_1"
        );
    }

    #[test]
    fn build_marker_is_a_valid_object_name() {
        let name = build_marker_name(r"\\.\pipe\qingjian-test", "0.1.10-dev/ab");
        assert_eq!(
            name,
            r"Local\Qingjian.ServerBuild.qingjian_test.0_1_10_dev_ab"
        );
    }
}
