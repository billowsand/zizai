//! 命名管道传输：在本会话的 `\\.\pipe\qingjian.<会话号>` 上服务 DLL 客户端，字节模式，帧由协议 codec 切。
//!
//! 每个应用进程各开一条连接且失焦后连接仍在，所以不能串行服务（新聚焦的应用连不上会阻塞 UI 线程、
//! 触发 TSF 看门狗）：后台接受循环每来一个客户端就新建实例、起一条线程；[`Router`] 不跨线程，
//! 留在调用线程跑工人循环，各连接经通道把消息转给它串行处理。

use std::fs::File;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_PIPE_CONNECTED, HANDLE};
use windows::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows::core::{HRESULT, HSTRING};

use qingjian_platform::protocol::{
    ClientMessage, Incoming, ServerMessage, read_incoming, write_message,
};

use super::Work;
use crate::dispatch::Router;
use crate::security::SecurityDescriptor;

const BUFFER_SIZE: u32 = 64 * 1024;

/// 管道的 SDDL：放行 Everyone / ALL APPLICATION PACKAGES / ALL RESTRICTED APPLICATION PACKAGES，
/// 完整性标 Low。任务栏搜索、设置这类 AppContainer 进程在默认 DACL 下连不上。
const PIPE_SDDL: &str = "D:(A;;GA;;;WD)(A;;GA;;;AC)(A;;GA;;;S-1-15-2-2)S:(ML;;NW;;;LW)";

/// 抢第一个实例时管道还被占着（上一个 Server 刚退、句柄还没关完）最多等多久。单实例互斥体已经保证
/// 只有本进程在抢；这里等不到说明占着管道的是不认单实例互斥体的老版本 Server。
const PIPE_RELEASE_TIMEOUT: Duration = Duration::from_secs(3);

/// 在命名管道上服务多个客户端。当前线程独占 [`Router`] 跑工人循环，收到 [`Work::StepDown`] 时
/// 把学习数据落盘后返回。`work` 通道由调用方建（UI 线程也往里投状态条事件），这里拿一份发送端给各连接。
///
/// 谁该当 Server 由单实例互斥体（`crate::instance`）在装配之前就定了；这里第一个实例仍带
/// `FILE_FLAG_FIRST_PIPE_INSTANCE`，挡住不认互斥体的老版本。管道被占时短等一会儿（上一个 Server
/// 退出到句柄关完之间），等不到就报错退出。
pub fn serve_pipe(
    name: &str,
    router: &mut Router,
    sender: Sender<Work>,
    receiver: Receiver<Work>,
) -> io::Result<()> {
    let pipe = HSTRING::from(name);
    let descriptor = SecurityDescriptor::from_sddl(PIPE_SDDL);
    let first = acquire_first(&pipe, &descriptor).map_err(|error| {
        if error.raw_os_error() == Some(ERROR_ACCESS_DENIED.0 as i32) {
            io::Error::other("命名管道被另一个（老版本的）qingjian-server 占着，本进程退出")
        } else {
            error
        }
    })?;
    thread::spawn(move || accept_loop(&pipe, &descriptor, first, sender));
    tracing::info!(pipe = name, "命名管道监听中");
    // 按 Router 的节拍来 tick：在等本地整句模型就几十毫秒一次，否则一秒看一次配置文件。
    // 到点时间是绝对的，不随消息重新计时——前台进程里的 DLL 隔几百毫秒就问一次切模式（SyncMode），
    // 若每收一条消息就重等一秒，tick 永远到不了，热加载与模型接入都会停摆。
    let mut due = Instant::now() + router.next_tick();
    loop {
        let now = Instant::now();
        if now >= due {
            router.tick();
            due = Instant::now() + router.next_tick();
            continue;
        }
        match receiver.recv_timeout(due - now) {
            Ok(Work::Client(message, reply)) => {
                let _ = reply.send(router.handle(message));
            }
            Ok(Work::Status(event)) => router.handle_status_event(event),
            Ok(Work::StepDown) => {
                tracing::info!(
                    "收到收尾信号（另一个 Server 请求接管 / 安装器要升级 / UI 线程退出），退出让位"
                );
                break;
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        }
        // 处理完消息节拍可能变短了（按键起了防抖）：到点时间只提前不推后
        due = due.min(Instant::now() + router.next_tick());
    }
    // 让位前把学习数据落盘，别把这一段的输入丢掉。
    router.flush_learning();
    Ok(())
}

/// 在旧版 DLL 认的管道名上也听一份（尽力而为）：升级后没重启的应用里还加载着旧 DLL，它们只认
/// 不带会话号的 `\\.\pipe\qingjian`。别的会话的 Server 已经占了它（多用户同时登录）就算了——
/// 那也是旧 DLL 原来的行为。等旧 DLL 淘汰干净就删掉。
pub fn listen_legacy(name: &str, sender: Sender<Work>) {
    let pipe = HSTRING::from(name);
    let descriptor = SecurityDescriptor::from_sddl(PIPE_SDDL);
    match create_instance(&pipe, &descriptor, true) {
        Ok(first) => {
            thread::spawn(move || accept_loop(&pipe, &descriptor, first, sender));
            tracing::info!(
                pipe = name,
                "旧版管道名也在监听（给没重启的应用里的旧 DLL）"
            );
        }
        Err(error) => {
            tracing::info!(%error, pipe = name, "旧版管道名没抢到，旧 DLL 这回连不上本 Server")
        }
    }
}

/// 抢第一个实例；被占着就每 100 ms 再试，直到 [`PIPE_RELEASE_TIMEOUT`]。
fn acquire_first(name: &HSTRING, descriptor: &SecurityDescriptor) -> io::Result<File> {
    let deadline = Instant::now() + PIPE_RELEASE_TIMEOUT;
    loop {
        match create_instance(name, descriptor, true) {
            Ok(file) => return Ok(file),
            Err(error)
                if error.raw_os_error() == Some(ERROR_ACCESS_DENIED.0 as i32)
                    && Instant::now() < deadline =>
            {
                thread::sleep(Duration::from_millis(100));
            }
            Err(error) => return Err(error),
        }
    }
}

/// 先在建好的第一个实例上等客户端，之后每建一个实例、等一个客户端连上，就起一条线程服务它。建实例出错才停。
fn accept_loop(name: &HSTRING, descriptor: &SecurityDescriptor, first: File, sender: Sender<Work>) {
    let mut instance = Ok(first);
    loop {
        let stream = match instance.and_then(wait_client) {
            Ok(stream) => stream,
            Err(error) => {
                tracing::error!(%error, "建管道实例失败，停止接受");
                break;
            }
        };
        let sender = sender.clone();
        thread::spawn(move || serve_connection(stream, sender));
        instance = create_instance(name, descriptor, false);
    }
}

/// 建一个实例。句柄交给 `File` 管：对端关闭时 std 把 `ERROR_BROKEN_PIPE` 当 EOF。
fn create_instance(
    name: &HSTRING,
    descriptor: &SecurityDescriptor,
    first: bool,
) -> io::Result<File> {
    let mut open_mode = PIPE_ACCESS_DUPLEX;
    if first {
        open_mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
    }
    let handle = descriptor.with_attributes(|attributes| unsafe {
        CreateNamedPipeW(
            name,
            open_mode,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            PIPE_UNLIMITED_INSTANCES,
            BUFFER_SIZE,
            BUFFER_SIZE,
            0,
            attributes,
        )
    });
    if handle.is_invalid() {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_handle(handle.0) })
}

/// 阻塞等一个客户端连上这个实例。
fn wait_client(stream: File) -> io::Result<File> {
    let handle = HANDLE(stream.as_raw_handle());
    // 客户端在建实例与 ConnectNamedPipe 之间就连上了也算成功。
    if let Err(error) = unsafe { ConnectNamedPipe(handle, None) }
        && error.code() != HRESULT::from_win32(ERROR_PIPE_CONNECTED.0)
    {
        return Err(io::Error::other(error));
    }
    Ok(stream)
}

/// 服务一条连接：读消息 → 转给工人线程 → 写回，直到对端在帧边界关闭或出错。
/// 读不懂的消息（比自己新的 DLL 发来的新变体）跳过接着读，别把连接断掉——断了对面每键都要重连。
fn serve_connection(mut stream: File, sender: Sender<Work>) {
    let (reply_sender, reply_receiver) = mpsc::channel::<Option<ServerMessage>>();
    loop {
        let message = match read_incoming::<_, ClientMessage>(&mut stream) {
            Ok(Incoming::Message(message)) => message,
            Ok(Incoming::Unknown(reason)) => {
                tracing::warn!(reason, "跳过一条读不懂的客户端消息（DLL 比 Server 新？）");
                continue;
            }
            Ok(Incoming::Eof) => break,
            Err(error) => {
                tracing::warn!(%error, "客户端会话读出错");
                break;
            }
        };
        if sender
            .send(Work::Client(message, reply_sender.clone()))
            .is_err()
        {
            break;
        }
        match reply_receiver.recv() {
            Ok(Some(response)) => {
                if write_message(&mut stream, &response).is_err() {
                    break;
                }
            }
            Ok(None) => {}
            Err(_) => break,
        }
    }
    let _ = unsafe { DisconnectNamedPipe(HANDLE(stream.as_raw_handle())) };
    tracing::debug!("客户端断开");
}
