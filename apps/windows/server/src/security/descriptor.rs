//! 由 SDDL 转出来的安全描述符，建内核对象（管道 / 互斥体 / 事件）时用；随值释放。

use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::core::HSTRING;

/// 自相关格式的安全描述符（`LocalAlloc` 出来的一块）。转换失败时为空，建对象退回默认 DACL。
pub struct SecurityDescriptor(PSECURITY_DESCRIPTOR);

// SAFETY: 描述符建好后只读，指针指向进程堆上一块不变的内存，跨线程共享只读访问没有数据竞争。
unsafe impl Send for SecurityDescriptor {}
unsafe impl Sync for SecurityDescriptor {}

impl SecurityDescriptor {
    /// 空描述符：建对象时不传安全属性，系统按进程令牌的默认 DACL 给（只放行本用户与 SYSTEM）。
    pub fn system_default() -> Self {
        Self(PSECURITY_DESCRIPTOR::default())
    }

    /// 转换失败记警告、返回空描述符（`attributes` 给 `None`，系统用默认 DACL）。
    pub fn from_sddl(sddl: &str) -> Self {
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        let converted = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                &HSTRING::from(sddl),
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )
        };
        if let Err(error) = converted {
            tracing::warn!(%error, sddl, "构建安全描述符失败，退回默认 DACL");
            return Self(PSECURITY_DESCRIPTOR::default());
        }
        Self(descriptor)
    }

    /// 以本描述符的 `SECURITY_ATTRIBUTES` 调 `f`（描述符为空时给 `None`）。结构只在调用期间有效。
    pub fn with_attributes<R>(&self, f: impl FnOnce(Option<*const SECURITY_ATTRIBUTES>) -> R) -> R {
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0.0,
            bInheritHandle: false.into(),
        };
        f((!self.0.0.is_null()).then_some(&raw const attributes))
    }
}

impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            unsafe {
                let _ = LocalFree(Some(HLOCAL(self.0.0)));
            }
        }
    }
}
