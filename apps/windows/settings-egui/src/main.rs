//! 字在默认设置界面的入口：起无边框窗口、装中文字体、跑 eframe。
//!
//! 缺省是 GUI 子系统（装进安装包双击不弹黑窗）；量首帧耗时与 wgpu 适配器时用 `--features console` 编，
//! 那两行 `println!` 才有地方出。
#![cfg_attr(all(windows, not(feature = "console")), windows_subsystem = "windows")]

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod files;
#[cfg(windows)]
mod fonts;
#[cfg(windows)]
mod gpu;
#[cfg(windows)]
mod nav;
#[cfg(windows)]
mod pages;
#[cfg(windows)]
mod theme;
#[cfg(windows)]
mod title_bar;
#[cfg(windows)]
mod voice_devices;
#[cfg(windows)]
mod widgets;

/// 窗口初始大小：导航 168 + 正文一列，够放「标签 + 184 宽的控件」，不铺满半个屏幕。
#[cfg(windows)]
const WINDOW_SIZE: [f32; 2] = [700.0, 560.0];

/// 窗口最小大小：再小控件列就开始挤标签了。
#[cfg(windows)]
const MIN_WINDOW_SIZE: [f32; 2] = [620.0, 460.0];

#[cfg(windows)]
fn main() -> eframe::Result<()> {
    let started = std::time::Instant::now();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("字在设置")
            .with_inner_size(WINDOW_SIZE)
            .with_min_inner_size(MIN_WINDOW_SIZE)
            .with_decorations(false),
        wgpu_options: gpu::configuration(),
        ..Default::default()
    };
    eframe::run_native(
        "字在设置",
        options,
        Box::new(move |cc| Ok(Box::new(app::Settings::new(cc, started)))),
    )
}

#[cfg(not(windows))]
fn main() {
    eprintln!("qingjian-settings-egui 仅支持 Windows");
}
