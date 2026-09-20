//! 不经传输层，直接把协议消息喂给 Router 的闭环测试；用样例词库，跨平台可跑。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use qingjian_core::ShuangpinScheme;
use qingjian_core::sentence::SentenceScorer;
use qingjian_platform::PreeditMode;
use qingjian_platform::protocol::{
    ClientMessage, FUMA_PREEDIT_PROTOCOL, Frame, KeyEvent, KeyModifiers, KeyOutcome,
    PROTOCOL_VERSION, PreeditKind, ScreenRect, ServerMessage, SessionId, VoiceAction, VoiceState,
};
use qingjian_voice::WorkerSnapshot;
use qingjian_windows_server::dispatch::{
    CandidateSink, StatusEvent, StatusSink, StatusView, VoiceView,
};
use qingjian_windows_server::voice::{VoiceBackend, VoiceBackendError};
use qingjian_windows_server::{AssemblySpec, Router, RouterConfig, assembly};

const SESSION: SessionId = SessionId(1);

/// Caps Lock 亮着。
const CAPS: KeyModifiers = KeyModifiers {
    ctrl: false,
    shift: false,
    alt: false,
    win: false,
    caps: true,
    english_mode: false,
};

/// 持久英文模式（Caps 灭）。
const ENGLISH: KeyModifiers = KeyModifiers {
    ctrl: false,
    shift: false,
    alt: false,
    win: false,
    caps: false,
    english_mode: true,
};

/// 样例词库装一个 Router，开好一个会话。
fn router() -> Router {
    router_with(RouterConfig::default())
}

fn router_with(config: RouterConfig) -> Router {
    router_in(config, None)
}

/// 在某个应用（宿主 exe 名）里开会话。
fn router_in_app(app: &str) -> Router {
    router_in(RouterConfig::default(), Some(app.to_owned()))
}

fn router_in(config: RouterConfig, app: Option<String>) -> Router {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let dict = root.join("assets/sample/dict.tsv");
    let mut engine = assembly::assemble(&AssemblySpec {
        english: Some(root.join("assets/sample/english.tsv")),
        ..AssemblySpec::new(dict)
    })
    .expect("assemble engine from sample data");
    // 与 main.rs 一样，双拼方案是启动时直接设给 Engine 的。
    engine.set_shuangpin(config.shuangpin);
    let mut router = Router::new(engine, config);
    assert_eq!(
        router.handle(ClientMessage::OpenSession {
            session: SESSION,
            app,
            protocol: PROTOCOL_VERSION,
        }),
        None
    );
    router
}

fn letter(c: char) -> KeyEvent {
    letter_with(c, Default::default())
}

/// `c` 的大小写就是 DLL 按 Shift 解析出的字符。
fn letter_with(c: char, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(c.to_ascii_uppercase() as u32, Some(c), modifiers)
}

fn press(router: &mut Router, event: KeyEvent) -> (KeyOutcome, Option<String>, Frame) {
    key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event,
    }))
}

/// 英文模式下敲一串字母，返回最后一次的处理结果。
fn type_english(router: &mut Router, text: &str) -> (KeyOutcome, Option<String>, Frame) {
    let mut last = None;
    for c in text.chars() {
        last = Some(press(router, letter_with(c, ENGLISH)));
    }
    last.expect("typed at least one letter")
}

fn candidate_texts(frame: &Frame) -> Vec<&str> {
    frame
        .candidates
        .items
        .iter()
        .map(|c| c.text.as_str())
        .collect()
}

fn digit(n: u32) -> KeyEvent {
    digit_with(n, Default::default())
}

/// 数字键 1–9；`character` 按 DLL 的解析：按着 Shift 是上档字符。
fn digit_with(n: u32, modifiers: KeyModifiers) -> KeyEvent {
    let c = if modifiers.shift {
        b")!@#$%^&*("[n as usize] as char
    } else {
        char::from_digit(n, 10).unwrap()
    };
    KeyEvent::new(0x30 + n, Some(c), modifiers)
}

const SHIFT: KeyModifiers = KeyModifiers {
    shift: true,
    ..ALT_OFF
};

const CTRL: KeyModifiers = KeyModifiers {
    ctrl: true,
    ..ALT_OFF
};

const ALT_OFF: KeyModifiers = KeyModifiers {
    ctrl: false,
    shift: false,
    alt: false,
    win: false,
    caps: false,
    english_mode: false,
};

/// 两个平台的缺省快捷键都没用 Win，拿来测「没配到的修饰键归应用」。
const WIN: KeyModifiers = KeyModifiers {
    win: true,
    ..ALT_OFF
};

/// 旧版本的译词键（macOS 是 Alt，Windows 是 Ctrl）：本壳已没有这个功能，用来测它确实不再是快捷键。
#[cfg(not(windows))]
const TRANSLATE: KeyModifiers = KeyModifiers {
    alt: true,
    ..ALT_OFF
};
#[cfg(windows)]
const TRANSLATE: KeyModifiers = KeyModifiers {
    ctrl: true,
    ..ALT_OFF
};

/// 当前页里 `text` 排第几（1 起）。
fn slot_of(frame: &Frame, text: &str) -> u32 {
    let position = candidate_texts(frame)
        .iter()
        .position(|t| *t == text)
        .unwrap_or_else(|| panic!("{text} 应在当前页：{:?}", candidate_texts(frame)));
    position as u32 + 1
}

/// 带字符的按键（标点等），虚拟键码随便给一个 OEM 键。
fn punct(c: char) -> KeyEvent {
    KeyEvent::new(0xBE, Some(c), Default::default())
}

fn key_result(message: Option<ServerMessage>) -> (KeyOutcome, Option<String>, Frame) {
    match message {
        Some(ServerMessage::KeyResult {
            outcome,
            commit,
            frame,
            ..
        }) => (outcome, commit, frame),
        other => panic!("expected KeyResult, got {other:?}"),
    }
}

/// 中文模式下敲一串字母，返回最后一次的处理结果。
fn type_letters(router: &mut Router, text: &str) -> (KeyOutcome, Option<String>, Frame) {
    let mut last = None;
    for c in text.chars() {
        last = Some(key_result(router.handle(ClientMessage::Key {
            session: SESSION,
            event: letter(c),
        })));
    }
    last.expect("typed at least one letter")
}

fn preedit(frame: &Frame) -> String {
    frame.preedit.iter().map(|s| s.text.as_str()).collect()
}

#[test]
fn typing_pinyin_shows_candidates() {
    let mut router = router();
    let (outcome, commit, frame) = type_letters(&mut router, "nihao");

    assert_eq!(outcome, KeyOutcome::Consumed);
    assert_eq!(commit, None);
    assert_eq!(preedit(&frame), "ni'hao");
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

#[test]
fn selecting_by_digit_commits_and_clears() {
    let mut router = router();
    let (_, _, frame) = type_letters(&mut router, "nihao");
    let position = frame
        .candidates
        .items
        .iter()
        .position(|c| c.text == "你好")
        .expect("「你好」在候选页内");
    let (outcome, commit, after) = key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: digit(position as u32 + 1),
    }));

    assert_eq!(outcome, KeyOutcome::Consumed);
    assert_eq!(commit.as_deref(), Some("你好"));
    assert!(
        after.is_empty(),
        "上屏后应收起候选，实际 preedit={:?}",
        preedit(&after)
    );
}

#[test]
fn space_commits_first_candidate() {
    let mut router = router();
    type_letters(&mut router, "ni");
    let space = KeyEvent::new(0x20, Some(' '), Default::default());
    let (outcome, commit, after) = key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: space,
    }));

    assert_eq!(outcome, KeyOutcome::Consumed);
    assert_eq!(commit.as_deref(), Some("你"), "「ni」首选应是「你」");
    assert!(after.is_empty());
}

#[test]
fn backspace_shrinks_preedit() {
    let mut router = router();
    let (_, _, frame) = type_letters(&mut router, "nihao");
    assert_eq!(preedit(&frame), "ni'hao");
    let back = KeyEvent::new(0x08, None, Default::default());
    let (outcome, _, after) = key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: back,
    }));

    assert_eq!(outcome, KeyOutcome::Consumed);
    assert_eq!(preedit(&after), "ni'ha");
}

#[test]
fn non_letter_without_composing_passes_through() {
    let mut router = router();
    let space = KeyEvent::new(0x20, Some(' '), Default::default());
    let (outcome, commit, frame) = key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: space,
    }));

    assert_eq!(outcome, KeyOutcome::Passthrough);
    assert_eq!(commit, None);
    assert!(frame.is_empty());
}

#[test]
fn focus_leave_commits_raw_pinyin() {
    let mut router = router();
    type_letters(&mut router, "nihao");
    let committed = router.handle(ClientMessage::Commit { session: SESSION });
    assert_eq!(
        committed,
        Some(ServerMessage::Committed {
            session: SESSION,
            text: Some("nihao".to_owned()),
        })
    );
    let (_, _, frame) = type_letters(&mut router, "ni");
    assert_eq!(preedit(&frame), "ni");
    // 没在组句时 Commit 不交东西。
    router.handle(ClientMessage::Key {
        session: SESSION,
        event: KeyEvent::new(0x1B, None, Default::default()),
    });
    assert_eq!(
        router.handle(ClientMessage::Commit { session: SESSION }),
        Some(ServerMessage::Committed {
            session: SESSION,
            text: None,
        })
    );
}

#[test]
fn commit_from_other_session_does_not_take_buffer() {
    let mut router = router();
    type_letters(&mut router, "ni");
    let other = SessionId(2);
    router.handle(ClientMessage::OpenSession {
        session: other,
        app: None,
        protocol: PROTOCOL_VERSION,
    });
    // 别的会话拿不到这个会话的拼音，但残留组句一并清掉。
    assert_eq!(
        router.handle(ClientMessage::Commit { session: other }),
        Some(ServerMessage::Committed {
            session: other,
            text: None,
        })
    );
    let space = KeyEvent::new(0x20, Some(' '), Default::default());
    let (outcome, _, _) = key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: space,
    }));
    assert_eq!(outcome, KeyOutcome::Passthrough);
}

#[test]
fn page_keys_follow_config() {
    // 每页 1 条保证多页；翻页键改成 `,` `.`。
    let mut router = router_with(RouterConfig {
        page_size: 1,
        page_keys: (',', '.'),
        ..RouterConfig::default()
    });
    let (_, _, frame) = type_letters(&mut router, "ni");
    assert!(frame.page_count > 1, "样例词库里 ni 应不止一个候选");
    assert_eq!(frame.page, 0);

    let key = |router: &mut Router, c| {
        key_result(router.handle(ClientMessage::Key {
            session: SESSION,
            event: punct(c),
        }))
    };
    let (outcome, commit, frame) = key(&mut router, '.');
    assert_eq!((outcome, commit), (KeyOutcome::Consumed, None));
    assert_eq!(frame.page, 1, "`.` 应翻到下一页");
    let (_, _, frame) = key(&mut router, ',');
    assert_eq!(frame.page, 0, "`,` 应翻回上一页");
    // 缺省的 `]` 此时不再翻页，进直输段。
    let (_, _, frame) = key(&mut router, ']');
    assert_eq!(frame.page, 0);
    assert!(
        preedit(&frame).contains(']'),
        "`]` 应进直输段：{}",
        preedit(&frame)
    );
}

#[test]
fn english_mode_types_straight_into_the_app() {
    let mut router = router();
    let (outcome, commit, frame) = type_english(&mut router, "hel");
    assert_eq!(
        (outcome, commit.as_deref()),
        (KeyOutcome::Consumed, Some("l")),
        "字母逐个直接插进输入框"
    );
    assert!(frame.is_empty(), "英文模式不组句也不出候选：{frame:?}");
    // 其他键交给应用。
    let (outcome, commit, _) = press(&mut router, KeyEvent::new(0x20, Some(' '), ENGLISH));
    assert_eq!((outcome, commit), (KeyOutcome::Passthrough, None));
}

#[test]
fn caps_lock_types_direct_uppercase_english_regardless_of_mode() {
    let mut router = router();
    let (outcome, commit, frame) = press(&mut router, letter_with('H', CAPS));
    assert_eq!(
        (outcome, commit.as_deref()),
        (KeyOutcome::Consumed, Some("H"))
    );
    assert!(frame.is_empty(), "Caps 直接上屏不出候选：{frame:?}");
    // 组句中 Caps 亮着敲字母：拼音先原样上屏，再接大写字母。
    type_letters(&mut router, "ni");
    let (_, commit, after) = press(&mut router, letter_with('A', CAPS));
    assert_eq!(commit.as_deref(), Some("niA"));
    assert!(after.is_empty());
}

#[test]
fn english_mode_letters_follow_shift_case() {
    let mut router = router();
    let shifted = KeyModifiers {
        shift: true,
        ..ENGLISH
    };
    let (_, commit, _) = press(&mut router, letter_with('H', shifted));
    assert_eq!(commit.as_deref(), Some("H"));
    let (_, commit, _) = press(&mut router, letter_with('i', ENGLISH));
    assert_eq!(commit.as_deref(), Some("i"));
}

#[test]
fn focus_switch_drops_the_other_session_composition() {
    // 两个应用同时在线：焦点回到前一个会话时，那边的残留组句先清掉。
    let mut router = router_in_app("Code.exe");
    let notepad = SessionId(2);
    router.handle(ClientMessage::OpenSession {
        session: notepad,
        app: Some("notepad.exe".to_owned()),
        protocol: PROTOCOL_VERSION,
    });
    let (_, _, frame) = key_result(router.handle(ClientMessage::Key {
        session: notepad,
        event: letter('n'),
    }));
    assert_eq!(preedit(&frame), "n", "记事本会话组句");
    let (_, _, frame) = press(&mut router, letter('h'));
    assert_eq!(preedit(&frame), "h", "切回编辑器会话从头组句");
}

#[test]
fn switching_to_english_mid_word_flushes_the_pinyin() {
    let mut router = router();
    type_letters(&mut router, "ni");
    // 敲了一半切到英文模式：拼音原样上屏，字母跟在后面一起插。
    let (outcome, commit, frame) = press(&mut router, letter_with('h', ENGLISH));
    assert_eq!(
        (outcome, commit.as_deref()),
        (KeyOutcome::Consumed, Some("nih"))
    );
    assert!(frame.is_empty());
}

#[test]
fn shift_uppercase_while_composing_commits_raw_first() {
    let mut router = router();
    type_letters(&mut router, "ni");
    // 中文模式按住 Shift 打大写字母：拼音原样上屏，字母跟在后面一起插。
    let shifted = KeyModifiers {
        shift: true,
        ..KeyModifiers::default()
    };
    let (outcome, commit, frame) = press(&mut router, letter_with('A', shifted));
    assert_eq!(
        (outcome, commit.as_deref()),
        (KeyOutcome::Consumed, Some("niA"))
    );
    assert!(frame.is_empty());
    // 没在组句时大写字母交给应用。
    let (outcome, commit, _) = press(&mut router, letter_with('A', shifted));
    assert_eq!((outcome, commit), (KeyOutcome::Passthrough, None));
}

#[test]
fn translate_modifier_digit_is_passed_through() {
    let mut router = router();
    type_letters(&mut router, "nihao");
    // 本壳不带译文功能：以前的译词键（mac ⌥ / Windows Ctrl）+ 数字不再配到任何快捷键，
    // 组句中也整键归应用（Ctrl+数字是很多应用的切标签页热键），组句不受影响。
    let (outcome, commit, after) = press(&mut router, digit_with(1, TRANSLATE));
    assert_eq!((outcome, commit), (KeyOutcome::Passthrough, None));
    assert_eq!(preedit(&after), "ni'hao");
}

#[test]
fn shift_digit_forgets_candidate_and_requeries() {
    let mut router = router();
    let (_, _, frame) = type_letters(&mut router, "nihao");
    let slot = slot_of(&frame, "你好");
    // 缺省 Shift + 数字：删候选（词库词只清学习记录），重新查一遍，组句不变。
    let (outcome, commit, after) = press(&mut router, digit_with(slot, SHIFT));
    assert_eq!((outcome, commit), (KeyOutcome::Consumed, None));
    assert_eq!(preedit(&after), "ni'hao");
    assert!(!after.candidates.items.is_empty());
}

#[test]
fn unconfigured_modifier_digit_is_not_a_selection() {
    // 删候选改成 Ctrl+Shift：Shift+4 就是普通的 `$`；Win+1 没配到快捷键，归应用。
    let mut router = router_with(RouterConfig {
        delete_keys: KeyModifiers {
            shift: true,
            ..CTRL
        },
        ..RouterConfig::default()
    });
    type_letters(&mut router, "nihao");
    let (outcome, commit, frame) = press(&mut router, digit_with(4, SHIFT));
    assert_eq!((outcome, commit), (KeyOutcome::Consumed, None));
    assert!(preedit(&frame).contains('$'), "{}", preedit(&frame));
    let (outcome, commit, _) = press(&mut router, digit_with(1, WIN));
    assert_eq!((outcome, commit), (KeyOutcome::Passthrough, None));
}

#[test]
fn learning_data_persists_to_user_dir() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let user_dir =
        std::env::temp_dir().join(format!("qingjian-windows-learning-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&user_dir);
    std::fs::create_dir_all(&user_dir).unwrap();
    let engine = assembly::assemble(&AssemblySpec {
        user_dir: Some(user_dir.clone()),
        ..AssemblySpec::new(root.join("assets/sample/dict.tsv"))
    })
    .unwrap();
    let mut router = Router::new(engine, RouterConfig::default());
    router.handle(ClientMessage::OpenSession {
        session: SESSION,
        app: None,
        protocol: PROTOCOL_VERSION,
    });
    let (_, _, frame) = type_letters(&mut router, "nihao");
    let position = frame
        .candidates
        .items
        .iter()
        .position(|c| c.text == "你好")
        .unwrap();
    router.handle(ClientMessage::Key {
        session: SESSION,
        event: digit(position as u32 + 1),
    });
    // 关会话时落盘。
    router.handle(ClientMessage::CloseSession { session: SESSION });

    let user = std::fs::read_to_string(user_dir.join("user.tsv")).expect("user.tsv 应已写出");
    assert!(user.contains("你好"), "user.tsv 里应记了「你好」：{user}");
    assert!(user_dir.join("usage.tsv").is_file(), "usage.tsv 应已写出");
    let _ = std::fs::remove_dir_all(&user_dir);
}

/// 记录状态条调用：`Some(模式格文字)` 是显示、`None` 是收起。
#[derive(Clone, Default)]
struct RecordingStatus(Arc<Mutex<Vec<Option<String>>>>);

impl RecordingStatus {
    fn calls(&self) -> Vec<Option<String>> {
        self.0.lock().unwrap().clone()
    }
}

impl StatusSink for RecordingStatus {
    fn show_status(&self, view: StatusView) {
        let label = match (view.english, view.scheme) {
            (true, _) => "英".to_owned(),
            (false, Some(scheme)) => format!("中 · {scheme}"),
            (false, None) => "中".to_owned(),
        };
        self.0.lock().unwrap().push(Some(label));
    }

    fn hide_status(&self) {
        self.0.lock().unwrap().push(None);
    }
}

#[test]
fn status_bar_mode_click_is_handed_to_dll_via_sync_mode() {
    let config = RouterConfig {
        status_enabled: true,
        ..RouterConfig::default()
    };
    let mut router = router_with(config);
    let recorder = RecordingStatus::default();
    router.set_status_sink(Box::new(recorder.clone()));
    router.handle(ClientMessage::ModeChanged {
        session: SESSION,
        english: false,
    });

    // 点「中」：状态条先翻成「英」，DLL 来取时拿到目标模式，取一次就清。
    router.handle_status_event(StatusEvent::ToggleMode);
    assert_eq!(recorder.calls().last(), Some(&Some("英".to_owned())));
    assert_eq!(
        router.handle(ClientMessage::SyncMode { session: SESSION }),
        Some(ServerMessage::ModeSync {
            session: SESSION,
            english: Some(true),
            voice: Default::default(),
        })
    );
    assert_eq!(
        router.handle(ClientMessage::SyncMode { session: SESSION }),
        Some(ServerMessage::ModeSync {
            session: SESSION,
            english: None,
            voice: Default::default(),
        })
    );
}

#[test]
fn chinese_punctuation_is_full_width_only_when_not_composing() {
    let mut router = router();
    // 没在组句：逗号转全角；数字后的点保持半角。
    let comma = KeyEvent::new(0xBC, Some(','), Default::default());
    assert_eq!(
        press(&mut router, comma),
        (
            KeyOutcome::Consumed,
            Some("，".to_owned()),
            Frame::default()
        )
    );
    press(&mut router, digit(3));
    let period = KeyEvent::new(0xBE, Some('.'), Default::default());
    assert_eq!(press(&mut router, period).0, KeyOutcome::Passthrough);
    assert_eq!(press(&mut router, period).1, Some("。".to_owned()));

    // 组句中：标点进英文直输段，不转。
    type_letters(&mut router, "ni");
    let (outcome, commit, frame) = press(&mut router, comma);
    assert_eq!((outcome, commit), (KeyOutcome::Consumed, None));
    assert!(!frame.is_empty(), "组句应还在");

    // 状态条上关掉全角：原样交给应用。
    router.handle(ClientMessage::Commit { session: SESSION });
    router.handle_status_event(StatusEvent::TogglePunctuation);
    assert_eq!(press(&mut router, comma).0, KeyOutcome::Passthrough);
}

#[test]
fn status_bar_follows_mode_when_enabled() {
    let config = RouterConfig {
        status_enabled: true,
        ..RouterConfig::default()
    };
    let mut router = router_with(config);
    let recorder = RecordingStatus::default();
    router.set_status_sink(Box::new(recorder.clone()));

    // 中文 → 英文：各刷一次；会话关掉（应用退出）不收；切成别的输入法才收起。
    router.handle(ClientMessage::ModeChanged {
        session: SESSION,
        english: false,
    });
    router.handle(ClientMessage::ModeChanged {
        session: SESSION,
        english: true,
    });
    router.handle(ClientMessage::CloseSession { session: SESSION });
    assert_eq!(
        recorder.calls(),
        vec![Some("中".to_owned()), Some("英".to_owned())]
    );

    router.handle(ClientMessage::ImeSwitched { session: SESSION });
    assert_eq!(recorder.calls().last(), Some(&None));
}

#[test]
fn status_bar_passes_shuangpin_scheme_key_in_chinese() {
    let config = RouterConfig {
        status_enabled: true,
        shuangpin: Some(ShuangpinScheme::Xiaohe),
        ..RouterConfig::default()
    };
    let mut router = router_with(config);
    let recorder = RecordingStatus::default();
    router.set_status_sink(Box::new(recorder.clone()));

    router.handle(ClientMessage::ModeChanged {
        session: SESSION,
        english: false,
    });

    assert_eq!(recorder.calls(), vec![Some("中 · xiaohe".to_owned())]);
}

#[test]
fn status_bar_stays_hidden_when_disabled() {
    let mut router = router();
    let recorder = RecordingStatus::default();
    router.set_status_sink(Box::new(recorder.clone()));

    router.handle(ClientMessage::ModeChanged {
        session: SESSION,
        english: false,
    });

    assert_eq!(recorder.calls(), vec![None]);
}

#[test]
fn deleting_a_candidate_shows_a_notice_until_next_key() {
    let mut router = router();
    let (_, _, frame) = type_letters(&mut router, "nihao");
    let slot = slot_of(&frame, "你好");

    // 「你好」是词库词且没学习记录，删不掉，但提示照样给出。
    let (outcome, _, after) = press(&mut router, digit_with(slot, SHIFT));
    assert_eq!(outcome, KeyOutcome::Consumed);
    assert!(!after.is_empty(), "删候选后仍在组句");
    let notice = after.notice.as_deref().expect("删候选后应带屏幕提示");
    assert!(notice.contains("你好"), "提示应提到候选词，实际：{notice}");

    let (_, _, next) = press(&mut router, KeyEvent::new(0x28, None, Default::default())); // VK_DOWN
    assert_eq!(next.notice, None, "提示应只活到下一次按键");
}

#[test]
fn bare_question_mark_is_plain_punctuation_by_default() {
    let mut router = router();
    // 缺省 `?` 不进问字：中文模式直接出全角问号，英文模式半角。
    let (outcome, commit, frame) = press(&mut router, punct('?'));
    assert_eq!(outcome, KeyOutcome::Consumed);
    assert_eq!(commit.as_deref(), Some("？"));
    assert!(preedit(&frame).is_empty());
    let (outcome, commit, _) = press(&mut router, KeyEvent::new(0xBF, Some('?'), ENGLISH));
    assert_eq!((outcome, commit), (KeyOutcome::Passthrough, None));
}

#[test]
fn punctuation_toggle_is_remembered_per_mode() {
    let mut router = router_with(RouterConfig {
        status_enabled: true,
        ..RouterConfig::default()
    });
    let comma = KeyEvent::new(0xBC, Some(','), Default::default());
    let english_comma = KeyEvent::new(0xBC, Some(','), ENGLISH);
    // 中文模式下切成半角。
    router.handle(ClientMessage::ModeChanged {
        session: SESSION,
        english: false,
    });
    router.handle_status_event(StatusEvent::TogglePunctuation);
    assert_eq!(press(&mut router, comma).0, KeyOutcome::Passthrough);
    // 英文模式缺省半角；点那一格切成全角，英文模式下真转。
    router.handle(ClientMessage::ModeChanged {
        session: SESSION,
        english: true,
    });
    assert_eq!(press(&mut router, english_comma).0, KeyOutcome::Passthrough);
    router.handle_status_event(StatusEvent::TogglePunctuation);
    assert_eq!(press(&mut router, english_comma).1, Some("，".to_owned()));
    // 切回中文：还是中文自己记住的半角；再切回英文：还是英文记住的全角。
    router.handle(ClientMessage::ModeChanged {
        session: SESSION,
        english: false,
    });
    assert_eq!(press(&mut router, comma).0, KeyOutcome::Passthrough);
    router.handle(ClientMessage::ModeChanged {
        session: SESSION,
        english: true,
    });
    assert_eq!(press(&mut router, english_comma).1, Some("，".to_owned()));
    // 英文模式敲的字母直插，跟在后面的标点仍按英文那份转。
    type_english(&mut router, "hello");
    let (_, commit, _) = press(&mut router, english_comma);
    assert_eq!(commit.as_deref(), Some("，"));
}

/// 假打分器：偏爱某个文本，其余都给低分（与 Core 的重打分测试同款）。
struct Prefers(&'static str);

impl SentenceScorer for Prefers {
    fn score(&self, _context: &str, texts: &[&str]) -> Vec<f64> {
        texts
            .iter()
            .map(|t| if *t == self.0 { -1.0 } else { -20.0 })
            .collect()
    }
}

/// 接了假模型的 Router：本地整句模型在壳里是异步接法，按键先按词级出候选，停顿后 tick 才换。
fn router_with_scorer(preferred: &'static str) -> Router {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let mut engine = assembly::assemble(&AssemblySpec::new(root.join("assets/sample/dict.tsv")))
        .expect("assemble engine from sample data");
    engine.set_async_sentence_scorer(Some(Box::new(Prefers(preferred))));
    let mut router = Router::new(engine, RouterConfig::default());
    router.handle(ClientMessage::OpenSession {
        session: SESSION,
        app: None,
        protocol: PROTOCOL_VERSION,
    });
    router
}

/// 一直 tick 到首选变成 `text` 或等满 `timeout`；返回最后一帧。
fn tick_until_first(router: &mut Router, text: &str, timeout: std::time::Duration) -> Frame {
    let started = std::time::Instant::now();
    loop {
        std::thread::sleep(router.next_tick().min(std::time::Duration::from_millis(20)));
        router.tick();
        let frame = match router.handle(ClientMessage::Poll { session: SESSION }) {
            Some(ServerMessage::Update { frame, .. }) => frame,
            other => panic!("expected Update, got {other:?}"),
        };
        if candidate_texts(&frame).first() == Some(&text) || started.elapsed() > timeout {
            return frame;
        }
    }
}

#[test]
fn local_model_rescoring_reorders_sentence_after_pause() {
    // k 优路径按末词分状态，几条路径要在末词上不同才都留下来：ni + ta → 你他 / 你她 / 你它
    let mut router = router_with_scorer("你它");
    let (_, _, frame) = type_letters(&mut router, "nita");
    // 按键时只按词级模型：他 的词频高，首选是「你他」
    assert_eq!(candidate_texts(&frame).first(), Some(&"你他"));
    // 在等防抖，工人循环该在 80 ms 内醒来
    assert!(router.next_tick() <= std::time::Duration::from_millis(80));

    let frame = tick_until_first(&mut router, "你它", std::time::Duration::from_secs(3));
    assert_eq!(
        candidate_texts(&frame).first(),
        Some(&"你它"),
        "停顿后模型偏爱的整句应换到首位，实际：{:?}",
        candidate_texts(&frame)
    );
    // 换完不再等；空闲节拍回到看配置文件的一秒
    assert_eq!(router.next_tick(), std::time::Duration::from_secs(1));
}

#[test]
fn local_model_does_not_touch_a_navigated_page() {
    let mut router = router_with_scorer("你它");
    type_letters(&mut router, "nita");
    // 用户动过高亮：模型的结果只留在缓存里，不换正在看的这页
    let (_, _, frame) = press(&mut router, KeyEvent::new(0x28, None, Default::default())); // VK_DOWN
    assert_eq!(candidate_texts(&frame).first(), Some(&"你他"));
    // 防抖 80 ms + 假模型立即回分，300 ms 足够等到结果；首选仍是原来的
    let frame = tick_until_first(&mut router, "你它", std::time::Duration::from_millis(300));
    assert_eq!(candidate_texts(&frame).first(), Some(&"你他"));
}

#[test]
fn surrounding_text_arriving_after_the_first_key_still_rescoring() {
    let mut router = router_with_scorer("你它");
    type_letters(&mut router, "nita");
    // DLL 在起组句的编辑会话里读到前文、按键之后才送来：前文换了，缓存按旧前文记的作废，要能重新排期
    assert_eq!(
        router.handle(ClientMessage::Surrounding {
            session: SESSION,
            text: "今天".to_owned(),
        }),
        None
    );
    assert!(router.next_tick() <= std::time::Duration::from_millis(80));
    let frame = tick_until_first(&mut router, "你它", std::time::Duration::from_secs(3));
    assert_eq!(candidate_texts(&frame).first(), Some(&"你它"));
    // 别的会话送来的前文不影响聚焦会话
    assert_eq!(
        router.handle(ClientMessage::Surrounding {
            session: SessionId(9),
            text: "无关".to_owned(),
        }),
        None
    );
}

/// DLL 报来「私密输入框」：Engine 进私密（不学不记），焦点换到别的会话按那个会话的状态重设，切回来再进。
#[test]
fn privacy_follows_the_focused_session() {
    let mut router = router();
    // 真实顺序：第一键起组句，DLL 在那次编辑会话里判出私密再报来
    let (outcome, commit, _) = type_letters(&mut router, "kaifa");
    assert_eq!(outcome, KeyOutcome::Consumed);
    assert_eq!(commit, None);
    assert!(!router.is_private());
    assert_eq!(
        router.handle(ClientMessage::Privacy {
            session: SESSION,
            private: true,
        }),
        None
    );
    assert!(router.is_private());
    // 私密中照常上屏
    let (_, commit, _) = press(&mut router, digit(1));
    assert!(commit.is_some());
    // 另一个会话开进来拿焦点：它不私密
    assert_eq!(
        router.handle(ClientMessage::OpenSession {
            session: SessionId(2),
            app: None,
            protocol: PROTOCOL_VERSION,
        }),
        None
    );
    press_in(&mut router, SessionId(2), letter('k'));
    assert!(!router.is_private());
    // 焦点回到第一个会话：仍是私密
    press_in(&mut router, SESSION, letter('k'));
    assert!(router.is_private());
    // 报不私密了
    assert_eq!(
        router.handle(ClientMessage::Privacy {
            session: SESSION,
            private: false,
        }),
        None
    );
    assert!(!router.is_private());
    // 别的会话的私密状态不影响聚焦会话
    assert_eq!(
        router.handle(ClientMessage::Privacy {
            session: SessionId(9),
            private: true,
        }),
        None
    );
    assert!(!router.is_private());
}

fn press_in(router: &mut Router, session: SessionId, event: KeyEvent) {
    let _ = router.handle(ClientMessage::Key { session, event });
}

/// 辅码开着：组句中 Shift + 字母是辅码键（进缓冲区），两码敲完候选严格过滤。
#[test]
fn fuma_keys_filter_candidates_strictly() {
    let mut router = fuma_router();
    // 小鹤 `kdfa` = kai'fa；辅码按文档打法第一码小写、第二码 Shift 大写
    type_letters(&mut router, "kdfaf");
    // 只敲了第一码时末 2 键全小写，还没激活：它当普通拼音，候选照常有 开发
    let (_, _, frame) = press(&mut router, letter_with('X', SHIFT));
    // `fX` → (f, x) = （开第 1 码, 发第 1 码）：只剩 开发
    assert_eq!(candidate_texts(&frame), vec!["开发"]);

    // 第二码对不上：一条候选都不剩
    let mut router = fuma_router();
    type_letters(&mut router, "kdfaf");
    let (_, _, frame) = press(&mut router, letter_with('Y', SHIFT));
    assert!(candidate_texts(&frame).is_empty());

    // 第一码大写是反转顺序：`Xf` 同样匹配实际辅码 (f, x)
    let mut router = fuma_router();
    type_letters(&mut router, "kdfa");
    press(&mut router, letter_with('X', SHIFT));
    let (_, _, frame) = press(&mut router, letter('f'));
    assert_eq!(candidate_texts(&frame), vec!["开发"]);
}

/// 辅码段要画在拼音行里：不然敲进去的辅码一点痕迹都没有，候选被筛空了也看不出原因。
#[test]
fn fuma_keys_show_in_the_preedit() {
    let mut router = fuma_router();
    type_letters(&mut router, "kdfaf");
    let (_, _, frame) = press(&mut router, letter_with('X', SHIFT));
    assert_eq!(preedit(&frame), "kai'fa fX");
    // 辅码段单独一段，DLL 与渲染器据此画淡
    let kinds: Vec<PreeditKind> = frame.preedit.iter().map(|s| s.kind).collect();
    assert_eq!(kinds, vec![PreeditKind::Typed, PreeditKind::Fuma]);
}

/// 老 DLL（升级安装后没重启的应用）不认识 `PreeditKind::Fuma`：整条消息会反序列化失败，
/// 所以发帧前按会话报来的协议版本降级成它认识的 `Rest`。
#[test]
fn fuma_preedit_downgrades_for_old_dlls() {
    let mut router = fuma_router();
    // 同一个会话按认识 Fuma 之前的协议重开（DLL 断线重连就是这条路）
    router.handle(ClientMessage::OpenSession {
        session: SESSION,
        app: None,
        protocol: FUMA_PREEDIT_PROTOCOL - 1,
    });
    type_letters(&mut router, "kdfaf");
    let (_, _, frame) = press(&mut router, letter_with('X', SHIFT));
    // 文本一字不差，只是种类降了级
    assert_eq!(preedit(&frame), "kai'fa fX");
    let kinds: Vec<PreeditKind> = frame.preedit.iter().map(|s| s.kind).collect();
    assert_eq!(kinds, vec![PreeditKind::Typed, PreeditKind::Rest]);
}

/// 随包辅码表（样例数据上跑）：开=fk、发=xa，「开发」期望（f, x）、单字「开」期望 (f, k)。
fn fuma_router() -> Router {
    let mut router = router_with(RouterConfig {
        shuangpin: Some(ShuangpinScheme::Xiaohe),
        ..RouterConfig::default()
    });
    router.set_fuma_table(Some(std::sync::Arc::new(
        qingjian_core::FumaTable::parse("开=fk\n发=xa\n").unwrap(),
    )));
    router
}

/// 没开辅码时 Shift + 字母照旧临时打英文：先把拼音原样上屏，字符直通。
#[test]
fn fuma_off_keeps_uppercase_as_temporary_english() {
    let mut router = router_with(RouterConfig {
        shuangpin: Some(ShuangpinScheme::Xiaohe),
        ..RouterConfig::default()
    });
    type_letters(&mut router, "kdfa");
    let (outcome, commit, _) = press(&mut router, letter_with('X', SHIFT));
    // 拼音原样上屏 + 大写直通在 Windows 上合起来一次插入
    assert_eq!(commit.as_deref(), Some("kdfaX"));
    assert_eq!(outcome, KeyOutcome::Consumed);
}

/// 记录候选窗口调用：`Some(帧)` 是显示、`None` 是收起。
#[derive(Clone, Default)]
struct RecordingCandidates(Arc<Mutex<Vec<Option<Frame>>>>);

impl RecordingCandidates {
    /// 最后一次真正显示出来的那一帧。
    fn shown(&self) -> Option<Frame> {
        self.0.lock().unwrap().iter().rev().find_map(Clone::clone)
    }
}

impl CandidateSink for RecordingCandidates {
    fn show(&self, frame: Frame, _rect: ScreenRect) {
        self.0.lock().unwrap().push(Some(frame));
    }

    fn hide(&self) {
        self.0.lock().unwrap().push(None);
    }

    fn set_font(&self, _font: String) {}
}

#[derive(Clone, Default)]
struct RecordingVoiceCandidates {
    views: Arc<Mutex<Vec<VoiceView>>>,

    hides: Arc<Mutex<usize>>,
}

impl CandidateSink for RecordingVoiceCandidates {
    fn show(&self, _frame: Frame, _rect: ScreenRect) {}

    fn show_voice(&self, view: VoiceView, _rect: ScreenRect) {
        self.views.lock().unwrap().push(view);
    }

    fn hide(&self) {
        *self.hides.lock().unwrap() += 1;
    }

    fn set_font(&self, _font: String) {}
}

struct SharedVoiceBackend(Arc<Mutex<WorkerSnapshot>>);

impl VoiceBackend for SharedVoiceBackend {
    fn start(&mut self, request: u64) -> Result<(), VoiceBackendError> {
        *self.0.lock().unwrap() = WorkerSnapshot {
            state: VoiceState::Recording,
            request: Some(request),
            level: 420,
            ..WorkerSnapshot::default()
        };
        Ok(())
    }

    fn stop(&mut self, request: u64) -> Result<(), VoiceBackendError> {
        let partial = self.0.lock().unwrap().partial.clone();
        *self.0.lock().unwrap() = WorkerSnapshot {
            state: VoiceState::Recognizing,
            request: Some(request),
            partial,
            ..WorkerSnapshot::default()
        };
        Ok(())
    }

    fn cancel(&mut self, _request: u64) -> Result<(), VoiceBackendError> {
        *self.0.lock().unwrap() = WorkerSnapshot {
            state: VoiceState::Idle,
            ..WorkerSnapshot::default()
        };
        Ok(())
    }

    fn snapshot(&mut self) -> Result<WorkerSnapshot, VoiceBackendError> {
        Ok(self.0.lock().unwrap().clone())
    }
}

#[test]
fn voice_reuses_candidate_window_at_the_current_caret() {
    let mut router = router();
    let worker = Arc::new(Mutex::new(WorkerSnapshot::default()));
    router.configure_voice(
        qingjian_platform::VoiceTrigger::RightAlt,
        Box::new(SharedVoiceBackend(worker.clone())),
    );
    let sink = RecordingVoiceCandidates::default();
    router.set_candidate_sink(Box::new(sink.clone()));

    router.handle(ClientMessage::Voice {
        session: SESSION,
        action: VoiceAction::Start,
    });
    router.handle(ClientMessage::PositionCandidates {
        session: SESSION,
        rect: ScreenRect {
            left: 300,
            top: 200,
            right: 302,
            bottom: 220,
        },
    });
    let first = sink.views.lock().unwrap().last().cloned().unwrap();
    assert_eq!(first.state, VoiceState::Recording);
    assert_eq!(first.level, 420);

    // 语音开始前的旧组句可能迟到一个收窗通知和空帧轮询；二者都不能把语音条关掉。
    let hides_before = *sink.hides.lock().unwrap();
    router.handle(ClientMessage::HideCandidates { session: SESSION });
    assert_eq!(*sink.hides.lock().unwrap(), hides_before);
    router.handle(ClientMessage::Poll { session: SESSION });
    assert_eq!(*sink.hides.lock().unwrap(), hides_before);
    assert_eq!(
        sink.views.lock().unwrap().last().unwrap().state,
        VoiceState::Recording
    );

    *worker.lock().unwrap() = WorkerSnapshot {
        state: VoiceState::Recording,
        request: Some(1),
        level: 760,
        partial: Some("正在实时转写".into()),
        ..WorkerSnapshot::default()
    };
    router.handle(ClientMessage::SyncMode { session: SESSION });
    let partial = sink.views.lock().unwrap().last().cloned().unwrap();
    assert_eq!(partial.text.as_deref(), Some("正在实时转写"));

    router.handle(ClientMessage::Voice {
        session: SESSION,
        action: VoiceAction::Stop,
    });
    router.handle(ClientMessage::SyncMode { session: SESSION });
    assert_eq!(
        sink.views.lock().unwrap().last().unwrap().state,
        VoiceState::Recognizing
    );

    *worker.lock().unwrap() = WorkerSnapshot {
        state: VoiceState::Ready,
        request: Some(1),
        partial: Some("最终文字".into()),
        text: Some("最终文字".into()),
        ..WorkerSnapshot::default()
    };
    let response = router.handle(ClientMessage::SyncMode { session: SESSION });
    let request = match response {
        Some(ServerMessage::ModeSync { voice, .. }) => voice.delivery.unwrap().request,
        other => panic!("expected voice delivery, got {other:?}"),
    };
    assert_eq!(
        sink.views.lock().unwrap().last().unwrap().state,
        VoiceState::Ready
    );

    router.handle(ClientMessage::VoiceAck {
        session: SESSION,
        request,
    });
    assert!(*sink.hides.lock().unwrap() > 0);

    // 上一轮 ACK 可能由异步编辑会话迟到；下一轮已开始时，它不能关闭新的语音条。
    router.handle(ClientMessage::Voice {
        session: SESSION,
        action: VoiceAction::Start,
    });
    router.handle(ClientMessage::PositionCandidates {
        session: SESSION,
        rect: ScreenRect {
            left: 320,
            top: 210,
            right: 322,
            bottom: 230,
        },
    });
    let hides_before = *sink.hides.lock().unwrap();
    router.handle(ClientMessage::VoiceAck {
        session: SESSION,
        request,
    });
    assert_eq!(*sink.hides.lock().unwrap(), hides_before);
    assert_eq!(
        sink.views.lock().unwrap().last().unwrap().state,
        VoiceState::Recording
    );

    // 同理，普通组句的迟到 Commit 只能清拼音，不能收掉活跃语音条。
    router.handle(ClientMessage::Commit { session: SESSION });
    assert_eq!(*sink.hides.lock().unwrap(), hides_before);
}

/// 按 `[general] preedit` 敲一串拼音，返回（发给 DLL 的帧，候选窗口画的那帧）。
/// 按真实顺序走：先收键，DLL 写完文档再报光标矩形——没有矩形 Server 不显示窗口。
fn typed_with_preedit(mode: PreeditMode) -> (Frame, Option<Frame>) {
    let mut router = router_with(RouterConfig {
        preedit: mode,
        ..RouterConfig::default()
    });
    let sink = RecordingCandidates::default();
    router.set_candidate_sink(Box::new(sink.clone()));
    let (_, _, frame) = type_letters(&mut router, "nihao");
    router.handle(ClientMessage::PositionCandidates {
        session: SESSION,
        rect: ScreenRect {
            left: 100,
            top: 100,
            right: 102,
            bottom: 120,
        },
    });
    (frame, sink.shown())
}

/// 缺省「行内 + 候选窗口」：两处都有拼音。
#[test]
fn preedit_both_shows_the_pinyin_inline_and_in_the_window() {
    let (frame, shown) = typed_with_preedit(PreeditMode::Both);
    assert!(frame.inline_preedit, "DLL 该把拼音写进应用");
    let shown = shown.expect("候选窗口该显示");
    assert_eq!(preedit(&shown), "ni'hao");
}

/// 「只在行内」：应用里照写，候选窗口那份帧里没有拼音行，候选照画。
#[test]
fn preedit_inline_keeps_the_pinyin_out_of_the_window() {
    let (frame, shown) = typed_with_preedit(PreeditMode::Inline);
    assert!(frame.inline_preedit, "DLL 该把拼音写进应用");
    let shown = shown.expect("候选窗口该显示");
    assert_eq!(preedit(&shown), "");
    assert_eq!(shown.cursor, 0);
    assert!(
        candidate_texts(&shown).contains(&"你好"),
        "候选还得照画: {:?}",
        candidate_texts(&shown)
    );
}

/// 「只在候选窗口」：`inline_preedit` 关掉让 DLL 不留 marked text，窗口那份仍带拼音行。
#[test]
fn preedit_window_keeps_the_pinyin_out_of_the_application() {
    let (frame, shown) = typed_with_preedit(PreeditMode::Window);
    assert!(!frame.inline_preedit, "DLL 不该把拼音写进应用");
    // 拼音分段照发：候选窗口是 Server 自绘的，画它要靠这份帧
    assert_eq!(preedit(&frame), "ni'hao");
    let shown = shown.expect("候选窗口该显示");
    assert_eq!(preedit(&shown), "ni'hao");
}
