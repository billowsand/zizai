//! 深度修复的日志：追加写到日志目录的 `repair.log`，与 Server / 输入法的日志放在一起，「打包到桌面」会一并带上。

use std::io::Write;

/// 追加写日志目录里的 `repair.log`；写不了就算了。
pub(super) struct Log(Option<std::fs::File>);

impl Log {
    pub(super) fn open() -> Self {
        let file = qingjian_platform::dirs::log_dir().and_then(|dir| {
            std::fs::create_dir_all(&dir).ok()?;
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join("repair.log"))
                .ok()
        });
        Self(file)
    }

    pub(super) fn line(&mut self, text: &str) {
        if let Some(file) = &mut self.0 {
            let now = jiff::Zoned::now().strftime("%Y-%m-%d %H:%M:%S");
            let _ = writeln!(file, "{now} {text}");
        }
    }
}
