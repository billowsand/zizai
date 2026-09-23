//! `cfg(windows)`：Server 单实例与接管。
//!
//! 每个登录会话只该有一个 Server。进程一起来（**装配 Engine、读学习数据之前**）就抢会话内的单实例互斥体，
//! 持有到进程退出；进程没了系统遗弃互斥体，等着的后来者随即拿到。互斥体已被占时：
//!
//! - 现任挂着与本进程相同的构建标记、且没挂降级标记（或本进程同样降级）→ **不接管**，本进程安静退出。
//!   开机时 DLL 先拉起一个、一分钟后「启动」文件夹又拉一个，就是这种情况；顶掉现任只会丢组句状态和学习数据。
//! - 否则（升级后的新构建、现任被提权 / 没拿到 uiAccess、或带 `--replace` 显式要求）→ 往现任的让位事件
//!   `SetEvent`，等互斥体被遗弃再继续。现任收到后先把学习数据落盘再退出。
//!
//! 接管在装配之前做完，所以后来者读到的是现任落盘之后的学习数据，不会拿旧快照把它盖掉。
//! 名字见 [`qingjian_platform::instance`]。

mod claim_error;
mod context;

pub use claim_error::ClaimError;
pub use context::ProcessContext;

use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::{HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, ResetEvent,
    SYNCHRONIZATION_SYNCHRONIZE, SetEvent, WaitForSingleObject,
};
use windows::core::HSTRING;

use qingjian_platform::instance::{
    build_marker_name, degraded_marker_name, instance_mutex_name, step_down_event_name,
};

use crate::security::{SecurityDescriptor, object_descriptor};

/// 本进程持有的单实例。**必须在一直活到进程退出的线程（主线程）上 [`claim`](Self::claim)**：
/// 互斥体归线程所有，那个线程一退就算遗弃，后来者会以为现任没了。
pub struct Instance {
    /// 单实例互斥体。不主动释放，进程退出时由系统遗弃。
    _mutex: OwnedHandle,

    /// 让位事件；建不出来为 `None`（那就只能被 taskkill）。
    step_down: Option<OwnedHandle>,

    /// 构建 / 降级标记，只靠「存在」表意，持有到进程退出。
    _markers: Vec<OwnedHandle>,
}

impl Instance {
    /// 抢本管道的单实例，规则见模块文档。`force` 为真时无条件请现任让位（`--replace`）。
    /// 请求让位后最多等 `timeout`。
    pub fn claim(
        pipe: &str,
        context: &ProcessContext,
        force: bool,
        timeout: Duration,
    ) -> Result<Self, ClaimError> {
        let descriptor = object_descriptor();
        let build = build_id();
        let mutex = create_mutex(&instance_mutex_name(pipe), &descriptor)?;
        if !acquire(&mutex, 0) {
            if !force && incumbent_is_fine(pipe, &build, context) {
                return Err(ClaimError::AlreadyRunning);
            }
            request_step_down(pipe);
            if !acquire(&mutex, timeout.as_millis().try_into().unwrap_or(u32::MAX)) {
                return Err(ClaimError::Timeout);
            }
            tracing::info!("现任 Server 已让位，接管");
        }
        // 事件可能被别的句柄（安装器、没退干净的后来者）撑着、还留着上一轮的信号：清掉再等，
        // 不然本进程一开始等就「收到」让位请求，刚接管就退出。
        let step_down = create_event(&step_down_event_name(pipe), &descriptor);
        if let Some(event) = &step_down {
            let _ = unsafe { ResetEvent(raw(event)) };
        }
        let mut markers: Vec<OwnedHandle> =
            create_event(&build_marker_name(pipe, &build), &descriptor)
                .into_iter()
                .collect();
        if context.degraded() {
            tracing::warn!(
                ?context,
                "本 Server 跑在降级上下文里（被提权 / 没拿到 uiAccess），正常实例起来会接管"
            );
            markers.extend(create_event(&degraded_marker_name(pipe), &descriptor));
        }
        Ok(Self {
            _mutex: mutex,
            step_down,
            _markers: markers,
        })
    }

    /// 起一条线程等让位请求，来了调一次 `on_step_down`。没有让位事件时什么都不做。
    pub fn watch(&self, on_step_down: impl FnOnce() + Send + 'static) {
        let Some(event) = self
            .step_down
            .as_ref()
            .and_then(|event| event.try_clone().ok())
        else {
            return;
        };
        thread::spawn(move || {
            if unsafe { WaitForSingleObject(raw(&event), INFINITE) } == WAIT_OBJECT_0 {
                on_step_down();
            }
        });
    }
}

/// 本构建的标识：版本号 + exe 路径 + 大小 + 修改时间，FNV-1a 成 16 位十六进制。
/// 同一份文件在同一个位置跑两次得到同一个值；升级替换了 exe、或换了安装位置就不同。
pub fn build_id() -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    };
    feed(env!("CARGO_PKG_VERSION").as_bytes());
    if let Ok(exe) = std::env::current_exe() {
        feed(exe.as_os_str().as_encoded_bytes());
        if let Ok(meta) = std::fs::metadata(&exe) {
            feed(&meta.len().to_le_bytes());
            if let Ok(modified) = meta.modified()
                && let Ok(since) = modified.duration_since(std::time::UNIX_EPOCH)
            {
                feed(&since.as_nanos().to_le_bytes());
            }
        }
    }
    format!("{hash:016x}")
}

/// 现任是同一份程序、而且不比本进程差：不必接管。
fn incumbent_is_fine(pipe: &str, build: &str, context: &ProcessContext) -> bool {
    let same_build = exists(&build_marker_name(pipe, build));
    let incumbent_degraded = exists(&degraded_marker_name(pipe));
    same_build && (!incumbent_degraded || context.degraded())
}

/// 往现任的让位事件发一次信号。只开不建：现任没建（老版本 / 建失败）就算了，由超时兜底。
fn request_step_down(pipe: &str) {
    let name = HSTRING::from(step_down_event_name(pipe));
    match unsafe { OpenEventW(EVENT_MODIFY_STATE, false, &name) } {
        Ok(handle) => {
            let event = unsafe { OwnedHandle::from_raw_handle(handle.0) };
            let _ = unsafe { SetEvent(raw(&event)) };
            tracing::info!("已有一个 qingjian-server，已请求它让位");
        }
        Err(error) => tracing::warn!(%error, "现任没有让位事件（老版本？），等它自己退出"),
    }
}

/// 等互斥体，拿到（含现任遗弃）为真。
fn acquire(mutex: &OwnedHandle, millis: u32) -> bool {
    let result = unsafe { WaitForSingleObject(raw(mutex), millis) };
    result == WAIT_OBJECT_0 || result == WAIT_ABANDONED
}

fn create_mutex(name: &str, descriptor: &SecurityDescriptor) -> Result<OwnedHandle, ClaimError> {
    let name = HSTRING::from(name);
    let handle = descriptor
        .with_attributes(|attributes| unsafe { CreateMutexW(attributes, false, &name) })
        .map_err(ClaimError::Mutex)?;
    Ok(unsafe { OwnedHandle::from_raw_handle(handle.0) })
}

/// 建（或打开已有的）自动复位事件，初始无信号。
fn create_event(name: &str, descriptor: &SecurityDescriptor) -> Option<OwnedHandle> {
    let name = HSTRING::from(name);
    let handle = descriptor
        .with_attributes(|attributes| unsafe { CreateEventW(attributes, false, false, &name) })
        .inspect_err(|error| tracing::warn!(%error, %name, "建命名事件失败"))
        .ok()?;
    Some(unsafe { OwnedHandle::from_raw_handle(handle.0) })
}

/// 这个名字的命名事件存不存在（打得开就算在，开不了——包括没权限——按不在算）。
fn exists(name: &str) -> bool {
    let name = HSTRING::from(name);
    unsafe { OpenEventW(SYNCHRONIZATION_SYNCHRONIZE, false, &name) }
        .map(|handle| drop(unsafe { OwnedHandle::from_raw_handle(handle.0) }))
        .is_ok()
}

fn raw(handle: &OwnedHandle) -> HANDLE {
    HANDLE(handle.as_raw_handle())
}
