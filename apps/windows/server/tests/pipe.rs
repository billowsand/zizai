//! 真命名管道的端到端测试（仅 Windows）：监听线程 + 文件句柄客户端，走完「开会话 → 敲 nihao → 收候选」，
//! 外加「DLL 比 Server 新、发来一条读不懂的消息」时连接不该断。

#![cfg(windows)]

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use qingjian_platform::protocol::{
    ClientMessage, KeyEvent, PROTOCOL_VERSION, ServerMessage, SessionId,
};
use qingjian_windows_server::ipc::pipe::serve_pipe;
use qingjian_windows_server::ipc::{read_message, write_message};
use qingjian_windows_server::{AssemblySpec, Router, RouterConfig, assembly};

const SESSION: SessionId = SessionId(1);

fn letter(c: char) -> KeyEvent {
    KeyEvent::new(c.to_ascii_uppercase() as u32, Some(c), Default::default())
}

/// 客户端重试打开管道，直到监听线程建好实例。
fn connect(name: &str) -> std::fs::File {
    for _ in 0..50 {
        match OpenOptions::new().read(true).write(true).open(name) {
            Ok(file) => return file,
            Err(_) => thread::sleep(Duration::from_millis(50)),
        }
    }
    panic!("连不上管道 {name}");
}

/// 起一条监听线程，服务完一个客户端后阻塞等下一个，随进程退出即可。
fn serve_in_background(name: &str) {
    let name = name.to_string();
    thread::spawn(move || {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let engine = assembly::assemble(&AssemblySpec::new(root.join("assets/sample/dict.tsv")))
            .expect("assemble engine from sample data");
        let mut router = Router::new(engine, RouterConfig::default());
        let (work_tx, work_rx) = std::sync::mpsc::channel();
        let _ = serve_pipe(&name, &mut router, work_tx, work_rx);
    });
}

/// 直接写一帧原始 JSON（长度前缀 + body），拿来伪造 Server 还不认识的消息。
fn write_raw(stream: &mut std::fs::File, body: &str) {
    let bytes = body.as_bytes();
    stream
        .write_all(&(bytes.len() as u32).to_le_bytes())
        .unwrap();
    stream.write_all(bytes).unwrap();
}

#[test]
fn named_pipe_round_trips_the_open_type_loop() {
    let name = format!(r"\\.\pipe\qingjian-test-{}", std::process::id());
    serve_in_background(&name);

    let mut client = connect(&name);

    write_message(
        &mut client,
        &ClientMessage::OpenSession {
            session: SESSION,
            app: None,
            protocol: PROTOCOL_VERSION,
        },
    )
    .unwrap();
    for c in "nihao".chars() {
        write_message(
            &mut client,
            &ClientMessage::Key {
                session: SESSION,
                event: letter(c),
            },
        )
        .unwrap();
    }

    let mut last_frame = None;
    for _ in 0..5 {
        let message: ServerMessage = read_message(&mut client)
            .expect("read response")
            .expect("server closed early");
        if let ServerMessage::KeyResult { frame, .. } = message {
            last_frame = Some(frame);
        }
    }

    let frame = last_frame.expect("至少一条 KeyResult");
    let preedit: String = frame.preedit.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(preedit, "ni'hao");
    let texts: Vec<&str> = frame
        .candidates
        .items
        .iter()
        .map(|c| c.text.as_str())
        .collect();
    assert!(
        texts.contains(&"你好"),
        "候选里应有「你好」，实际：{texts:?}"
    );
}

/// 比 Server 新的 DLL 发来一条它不认识的消息：跳过这一条接着服务，别把连接断掉——
/// 断了对面每一键都要重连（升级安装后新旧混跑的那段时间就靠这个）。
#[test]
fn unknown_client_message_keeps_the_connection() {
    let name = format!(r"\\.\pipe\qingjian-test-unknown-{}", std::process::id());
    serve_in_background(&name);

    let mut client = connect(&name);
    write_message(
        &mut client,
        &ClientMessage::OpenSession {
            session: SESSION,
            app: None,
            protocol: PROTOCOL_VERSION + 1,
        },
    )
    .unwrap();
    write_raw(&mut client, r#"{"WhatIsThis":{"session":1,"extra":true}}"#);
    write_message(
        &mut client,
        &ClientMessage::Key {
            session: SESSION,
            event: letter('n'),
        },
    )
    .unwrap();

    let message: ServerMessage = read_message(&mut client)
        .expect("跳过读不懂的那条后，按键仍该有应答")
        .expect("server closed early");
    assert!(matches!(message, ServerMessage::KeyResult { .. }));
}

/// 接管：新起的 Server 往接管事件发一次信号，现任的工人循环应收到 `StepDown` 并干净退出（`Ok`）。
/// 这是「坏实例占着管道、新 Server 顶掉它」这条自愈路径的核心。
#[test]
fn step_down_signal_stops_the_running_server() {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{CreateEventW, SetEvent};
    use windows::core::HSTRING;

    let name = format!(r"\\.\pipe\qingjian-test-stepdown-{}", std::process::id());
    let event_name = qingjian_windows_server::ipc::pipe::step_down_event_name(&name);

    let serve_name = name.clone();
    let running = thread::spawn(move || {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let engine = assembly::assemble(&AssemblySpec::new(root.join("assets/sample/dict.tsv")))
            .expect("assemble engine from sample data");
        let mut router = Router::new(engine, RouterConfig::default());
        let (work_tx, work_rx) = std::sync::mpsc::channel();
        serve_pipe(&serve_name, &mut router, work_tx, work_rx)
    });
    // 连上就说明现任已开始监听（接管事件此时也已建好）。
    let _client = connect(&name);

    let event = unsafe { CreateEventW(None, false, false, &HSTRING::from(event_name)) }
        .expect("open step-down event");
    unsafe { SetEvent(event).expect("signal step-down") };
    unsafe { CloseHandle(event).expect("close event") };

    let result = running.join().expect("serve thread panicked");
    assert!(result.is_ok(), "收到接管信号后现任应干净退出");
}
