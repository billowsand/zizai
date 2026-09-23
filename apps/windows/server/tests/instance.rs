//! Server 单实例与接管的规则（仅 Windows）：同一份程序不互相顶、`--replace` 与降级现任会被接管、
//! 现任不理会时按超时失败、残留的让位信号不会让新现任刚接管就退出。
//!
//! 互斥体归线程所有，所以「现任」放在单独的线程里抢，那个线程活到收到让位请求或测试叫停为止。

#![cfg(windows)]

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{CreateEventW, SetEvent};
use windows::core::HSTRING;

use qingjian_platform::instance::step_down_event_name;
use qingjian_windows_server::instance::{ClaimError, Instance, ProcessContext};

const TIMEOUT: Duration = Duration::from_secs(5);

/// 每个测试一个管道名，互不干扰。
fn pipe(tag: &str) -> String {
    format!(
        r"\\.\pipe\qingjian-test-instance-{tag}-{}",
        std::process::id()
    )
}

/// 不降级的上下文（测试可能跑在提权终端里）。
fn normal() -> ProcessContext {
    ProcessContext {
        elevated: false,
        ui_access: true,
        ..ProcessContext::current()
    }
}

fn degraded() -> ProcessContext {
    ProcessContext {
        elevated: true,
        ..normal()
    }
}

/// 在自己线程上持有实例的现任。
struct Incumbent {
    /// 收到过让位请求就有一条。
    stepped_down: mpsc::Receiver<()>,

    /// 叫现任线程退出（遗弃互斥体）。
    exit: mpsc::Sender<()>,

    thread: thread::JoinHandle<()>,
}

impl Incumbent {
    /// `watch` 为假时现任不理会让位请求（模拟老版本 / 卡住的现任）。
    fn start(pipe: &str, context: ProcessContext, watch: bool) -> Self {
        let pipe = pipe.to_owned();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (stepped_tx, stepped_down) = mpsc::channel();
        let (exit, exit_rx) = mpsc::channel::<()>();
        let on_step_down = exit.clone();
        let thread = thread::spawn(move || {
            let instance =
                Instance::claim(&pipe, &context, false, TIMEOUT).expect("incumbent claims");
            if watch {
                instance.watch(move || {
                    let _ = stepped_tx.send(());
                    let _ = on_step_down.send(());
                });
            }
            ready_tx.send(()).unwrap();
            let _ = exit_rx.recv();
            drop(instance);
        });
        ready_rx.recv().expect("incumbent ready");
        Self {
            stepped_down,
            exit,
            thread,
        }
    }

    fn stop(self) {
        let _ = self.exit.send(());
        self.thread.join().unwrap();
    }
}

/// 在新线程上抢（主测试线程可能已经拿过别的互斥体，互斥体可重入，会误判）。
fn claim_elsewhere(
    pipe: &str,
    context: ProcessContext,
    force: bool,
    timeout: Duration,
) -> Result<(), ClaimError> {
    let pipe = pipe.to_owned();
    thread::spawn(move || Instance::claim(&pipe, &context, force, timeout).map(drop))
        .join()
        .unwrap()
}

#[test]
fn same_build_does_not_replace_a_healthy_incumbent() {
    let pipe = pipe("same");
    let incumbent = Incumbent::start(&pipe, normal(), true);

    let claim = claim_elsewhere(&pipe, normal(), false, TIMEOUT);
    assert!(
        matches!(claim, Err(ClaimError::AlreadyRunning)),
        "{claim:?}"
    );
    assert!(
        incumbent.stepped_down.try_recv().is_err(),
        "现任不该收到让位请求"
    );
    incumbent.stop();
}

#[test]
fn replace_asks_the_incumbent_to_step_down_and_takes_over() {
    let pipe = pipe("replace");
    let incumbent = Incumbent::start(&pipe, normal(), true);

    let claim = claim_elsewhere(&pipe, normal(), true, TIMEOUT);
    assert!(claim.is_ok(), "{claim:?}");
    assert!(
        incumbent.stepped_down.try_recv().is_ok(),
        "现任应收到让位请求"
    );
    incumbent.thread.join().unwrap();
}

#[test]
fn degraded_incumbent_is_replaced_by_a_normal_one() {
    let pipe = pipe("degraded");
    let incumbent = Incumbent::start(&pipe, degraded(), true);

    let claim = claim_elsewhere(&pipe, normal(), false, TIMEOUT);
    assert!(claim.is_ok(), "{claim:?}");
    assert!(incumbent.stepped_down.try_recv().is_ok());
    incumbent.thread.join().unwrap();
}

#[test]
fn degraded_newcomer_does_not_replace_a_degraded_incumbent() {
    let pipe = pipe("both-degraded");
    let incumbent = Incumbent::start(&pipe, degraded(), true);

    let claim = claim_elsewhere(&pipe, degraded(), false, TIMEOUT);
    assert!(
        matches!(claim, Err(ClaimError::AlreadyRunning)),
        "{claim:?}"
    );
    incumbent.stop();
}

#[test]
fn unresponsive_incumbent_times_out() {
    let pipe = pipe("stuck");
    let incumbent = Incumbent::start(&pipe, normal(), false);

    let claim = claim_elsewhere(&pipe, normal(), true, Duration::from_millis(300));
    assert!(matches!(claim, Err(ClaimError::Timeout)), "{claim:?}");
    incumbent.stop();
}

/// 让位事件被别的句柄撑着、还留着信号时，新现任不能一开始等就「收到」让位请求。
#[test]
fn stale_step_down_signal_does_not_evict_a_new_incumbent() {
    let pipe = pipe("stale");
    let name = HSTRING::from(step_down_event_name(&pipe));
    let stale = unsafe { CreateEventW(None, false, false, &name) }.expect("create event");
    unsafe { SetEvent(stale) }.expect("leave a signal behind");

    let incumbent = Incumbent::start(&pipe, normal(), true);
    thread::sleep(Duration::from_millis(300));
    assert!(
        incumbent.stepped_down.try_recv().is_err(),
        "残留信号不该让新现任退出"
    );
    incumbent.stop();
    unsafe { CloseHandle(stale) }.unwrap();
}

/// 现任退出（进程 / 线程没了）后，下一个直接拿到，不必请谁让位。
#[test]
fn abandoned_instance_is_claimed_without_waiting() {
    let pipe = pipe("abandoned");
    Incumbent::start(&pipe, normal(), true).stop();

    let claim = claim_elsewhere(&pipe, normal(), false, Duration::ZERO);
    assert!(claim.is_ok(), "{claim:?}");
}
