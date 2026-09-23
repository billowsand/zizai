//! 嵌 uiAccess manifest：候选窗口要盖过商店 / 任务栏搜索这些高 z-band 宿主，`SetWindowPos(HWND_TOPMOST)`
//! 才能升进 UIAccess 高带。系统只对签名且装在 Program Files 的 exe 授予，光有 manifest 不够；
//! **没签名的 exe 带 uiAccess=true 会直接起不来**（os error 740），而没有证书才是常态（平时开发、CI、对外分发的包
//! 都不签名），所以**缺省不嵌**：只有 `QINGJIAN_UIACCESS=1` 才开，`installer\build.ps1 -Sign` 会替你设。
//! 代价是候选窗在 UWP 宿主里可能被盖住（用户文档已列为已知问题）。
//! manifest 缺省含 PerMonitorV2 DPI 感知，与运行时那次 `SetProcessDpiAwarenessContext` 一致。
//! 另把青简图标嵌进 exe（任务管理器 / 启动项里显示）。

use embed_manifest::manifest::ExecutionLevel;
use embed_manifest::{embed_manifest, new_manifest};

fn main() {
    // build.rs 跑在宿主机上，只有目标是 Windows 时才嵌。
    println!("cargo:rerun-if-env-changed=QINGJIAN_UIACCESS");
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        // 缺省关：起得来优先。开了就得签名，签错了连本机都起不来，所以要显式要。
        let ui_access = std::env::var("QINGJIAN_UIACCESS").is_ok_and(|v| v == "1");
        if ui_access {
            println!(
                "cargo:warning=QINGJIAN_UIACCESS=1：Server 带 uiAccess，必须签名且装进 Program Files 才起得来"
            );
        }
        // 运行时据此判断「该有 uiAccess 却没拿到」（降级，见 instance::ProcessContext）。
        println!(
            "cargo:rustc-env=QINGJIAN_SERVER_UIACCESS={}",
            if ui_access { "1" } else { "0" }
        );
        let manifest = new_manifest("Qingjian.Server")
            .requested_execution_level(ExecutionLevel::AsInvoker)
            .ui_access(ui_access);
        embed_manifest(manifest).expect("嵌入 Server manifest 失败");
    }
    embed_icon();
    println!("cargo:rerun-if-changed=build.rs");
}

/// 图标资源要 `rc.exe`（MSVC）编，只在 Windows 宿主上做；失败只警告，别让编译挂掉。
/// winresource 缺省不带 manifest，与上面链接器嵌的那份不冲突。
/// 同时嵌 VERSIONINFO 元数据：SignPath 签名按 product-name/product-version 校验
/// （docs/design/code-signing.md），版本统一用 QINGJIAN_PRODUCT_VERSION（CI 传安装包版本）。
#[cfg(windows)]
fn embed_icon() {
    const ICON: &str = "../tsf/resources/qingjian.ico";
    println!("cargo:rerun-if-changed={ICON}");
    println!("cargo:rerun-if-env-changed=QINGJIAN_PRODUCT_VERSION");
    let version = std::env::var("QINGJIAN_PRODUCT_VERSION")
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_owned());
    let mut res = winresource::WindowsResource::new();
    res.set_icon(ICON)
        .set("ProductName", "Qingjian")
        .set("ProductVersion", &version)
        .set("FileVersion", &version)
        .set("FileDescription", "Qingjian input server");
    if let Err(error) = res.compile() {
        println!("cargo:warning=嵌入 Server 图标失败: {error}");
    }
}

#[cfg(not(windows))]
fn embed_icon() {}
