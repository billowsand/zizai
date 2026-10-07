//! 深度修复：清掉旧安装留下的东西。提权那一半由设置程序以 `--repair <会话号>` 再跑一份自己来做（见 [`run_elevated`]），
//! 不开窗口，做完按失败步数退出；普通权限那一半（当前用户自己的启动项）在 [`remove_user_leftovers`]。
//! 与安装脚本 `qingjian.iss` 的 PrepareToInstall / DeleteStaleDlls / DeleteLegacyLogonTask 做的是同一套事。

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use windows::Win32::Storage::FileSystem::{MOVEFILE_DELAY_UNTIL_REBOOT, MoveFileExW};
use windows::core::{HSTRING, PCWSTR};

use super::log::Log;
use super::server::{self, CREATE_NO_WINDOW};

/// 输入法的 COM 类 id，与 `apps/windows/tsf/src/com/mod.rs` 的 `CLSID_QINGJIAN_STR` 对齐。
const CLSID: &str = "{4FDCA82D-E923-49BF-9E75-BB906B93B8BB}";

/// 旧版本建的「登录自启」计划任务名（现在改用「启动」文件夹快捷方式）。
const LEGACY_TASK: &str = "Qingjian Server";

/// 提权跑的那一份：结束残留进程、删旧计划任务、删旧版本 DLL 并重新注册当前的、补应用容器读权限。
/// 返回没做成的步数（作退出码）；每步结果记进日志目录的 `repair.log`。
pub(crate) fn run_elevated(session: Option<u32>) -> i32 {
    let mut log = Log::open();
    let mut failures = 0;
    match session {
        Some(session) => server::kill_leftovers(session),
        None => {
            for image in [server::SERVER_EXE, server::VOICE_WORKER_EXE] {
                run("taskkill", &["/f", "/im", image]);
            }
        }
    }
    log.line("已结束残留的 Server / 语音进程");
    // 没有旧任务时 schtasks 返回非 0，不算失败。
    run("schtasks", &["/delete", "/tn", LEGACY_TASK, "/f"]);
    log.line("已删除旧版本的登录自启计划任务（如有）");
    let Some(dir) = install_dir() else {
        log.line("找不到安装目录");
        return 1;
    };
    for x86 in [false, true] {
        let Some(current) = current_dll(&dir, x86) else {
            log.line(&format!(
                "没找到{}输入法 DLL",
                if x86 { " 32 位" } else { "" }
            ));
            failures += 1;
            continue;
        };
        for stale in dlls(&dir, x86)
            .into_iter()
            .filter(|path| !same_file(path, &current))
        {
            match std::fs::remove_file(&stale) {
                Ok(()) => log.line(&format!("已删除旧 DLL {}", stale.display())),
                // 还被某个应用加载着：登记成重启后删。
                Err(_) => {
                    let path = HSTRING::from(stale.as_os_str());
                    let _ =
                        unsafe { MoveFileExW(&path, PCWSTR::null(), MOVEFILE_DELAY_UNTIL_REBOOT) };
                    log.line(&format!("旧 DLL 正被占用，重启后删除 {}", stale.display()));
                }
            }
        }
        let system = if x86 { "SysWOW64" } else { "System32" };
        let regsvr32 = system_dir(system).join("regsvr32.exe");
        let current_arg = current.to_string_lossy().into_owned();
        if run(&regsvr32.to_string_lossy(), &["/s", &current_arg]) {
            log.line(&format!("已重新注册 {}", current.display()));
        } else {
            log.line(&format!("重新注册失败 {}", current.display()));
            failures += 1;
        }
    }
    // 任务栏搜索、开始菜单这类应用容器进程要能读安装目录，否则输入法在那些地方加载不了。
    let dir_arg = dir.to_string_lossy().into_owned();
    if run(
        "icacls",
        &[
            &dir_arg,
            "/grant",
            "*S-1-15-2-1:(OI)(CI)RX",
            "/T",
            "/C",
            "/Q",
        ],
    ) {
        log.line("已补上应用容器的读权限");
    } else {
        log.line("补应用容器读权限失败");
        failures += 1;
    }
    log.line(&format!("深度修复结束，{failures} 步失败"));
    failures
}

/// 当前用户「启动」文件夹里更早版本留下的自启快捷方式：与机器级那份并存会起两个 Server。
pub(crate) fn remove_user_leftovers() {
    let Some(appdata) = std::env::var_os("APPDATA") else {
        return;
    };
    let link = PathBuf::from(appdata)
        .join(r"Microsoft\Windows\Start Menu\Programs\Startup")
        .join("Qingjian Server.lnk");
    let _ = std::fs::remove_file(link);
}

/// 设置程序所在的安装目录。
fn install_dir() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.to_path_buf())
}

/// `%SystemRoot%\<name>`。
fn system_dir(name: &str) -> PathBuf {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    PathBuf::from(root).join(name)
}

/// 安装目录里的输入法 DLL（64 位或 32 位那一组，含改名待删的）。
fn dlls(dir: &Path, x86: bool) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            name.starts_with("qingjian_tsf")
                && name.ends_with(".dll")
                && name.ends_with("-x86.dll") == x86
        })
        .collect()
}

/// 要留下并注册的那份：系统登记的（在安装目录里、文件还在的话），否则不是改名待删的里面最新的一份。
fn current_dll(dir: &Path, x86: bool) -> Option<PathBuf> {
    let candidates = dlls(dir, x86);
    if let Some(registered) = registered_dll(x86)
        && let Some(found) = candidates.iter().find(|path| same_file(path, &registered))
    {
        return Some(found.clone());
    }
    candidates
        .into_iter()
        .filter(|path| !path.to_string_lossy().contains(".old-"))
        .max_by_key(|path| {
            std::fs::metadata(path)
                .and_then(|meta| meta.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH)
        })
}

/// Windows 路径不分大小写：注册表里登记的与目录里列出来的大小写可能不同。
fn same_file(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

/// 注册表里登记的输入法 DLL 路径（64 位 / 32 位视图）。
fn registered_dll(x86: bool) -> Option<PathBuf> {
    let classes = if x86 {
        r"SOFTWARE\WOW6432Node\Classes"
    } else {
        r"SOFTWARE\Classes"
    };
    let key = windows_registry::LOCAL_MACHINE
        .open(format!(r"{classes}\CLSID\{CLSID}\InprocServer32"))
        .ok()?;
    key.get_string("").ok().map(PathBuf::from)
}

/// 无窗口跑一条命令，返回是否成功退出。
fn run(program: &str, args: &[&str]) -> bool {
    std::process::Command::new(program)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .is_ok_and(|status| status.success())
}
