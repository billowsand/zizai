//! 设置页只列设备名；打开流、权限与错误处理仍全部留在语音 Worker。

use cpal::traits::{DeviceTrait, HostTrait};

pub(crate) struct InputDevices {
    pub(crate) default: Option<String>,
    pub(crate) available: Vec<String>,
}

/// 列一次当前 WASAPI 输入设备，失败时保留“系统默认”选项，不让设置页打不开。
pub(crate) fn scan() -> InputDevices {
    let host = cpal::default_host();
    let default = host
        .default_input_device()
        .and_then(|device| device.name().ok());
    let mut available = host
        .input_devices()
        .map(|devices| {
            devices
                .filter_map(|device| device.name().ok())
                .filter(|name| !name.trim().is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    available.sort_by_key(|name| name.to_lowercase());
    available.dedup();
    InputDevices { default, available }
}
