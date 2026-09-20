//! 给语音 Worker 嵌图标和签名流程需要的 VERSIONINFO。

fn main() {
    embed_resources();
}

#[cfg(windows)]
fn embed_resources() {
    const ICON: &str = "../tsf/resources/qingjian.ico";
    println!("cargo:rerun-if-changed={ICON}");
    println!("cargo:rerun-if-env-changed=QINGJIAN_PRODUCT_VERSION");
    let version = std::env::var("QINGJIAN_PRODUCT_VERSION")
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_owned());
    let mut resources = winresource::WindowsResource::new();
    resources
        .set_icon(ICON)
        .set("ProductName", "Qingjian")
        .set("ProductVersion", &version)
        .set("FileVersion", &version)
        .set("FileDescription", "Qingjian voice worker");
    if let Err(error) = resources.compile() {
        println!("cargo:warning=嵌入语音 Worker 资源失败: {error}");
    }
}

#[cfg(not(windows))]
fn embed_resources() {}
