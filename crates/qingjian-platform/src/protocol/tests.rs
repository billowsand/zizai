//! 协议的兼容性测试。两类：
//!
//! - **样例 JSON 盯着线上格式**：改了协议类型这些断言就红，提醒连带 +1
//!   [`PROTOCOL_VERSION`](super::PROTOCOL_VERSION)（升级安装后老 DLL 还留在没重启的应用里，两边得能对话）。
//! - **跨版本互操作**：缺字段、多字段、认不出的枚举名、认不出的消息变体，逐个验一遍退到哪。

use qingjian_core::{Candidate, CandidateKind, CandidateList};
use serde::Deserialize;

use super::{
    ClientMessage, Frame, Incoming, KeyEvent, KeyModifiers, KeyOutcome, PreeditKind,
    PreeditSegment, ScreenRect, ServerMessage, SessionId, read_incoming, read_message,
    write_message,
};

fn frame() -> Frame {
    Frame {
        preedit: vec![PreeditSegment {
            text: "ni'hao".into(),
            kind: PreeditKind::Typed,
        }],
        cursor: 6,
        candidates: CandidateList {
            items: vec![Candidate {
                text: "你好".into(),
                kind: CandidateKind::Chinese,
                syllables: vec!["ni".into(), "hao".into()],
                ..Candidate::default()
            }],
        },
        highlight: 0,
        page: 0,
        page_count: 1,
        ..Frame::default()
    }
}

/// 线上格式的样例：**改了协议类型这里就要跟着改，同时把 [`PROTOCOL_VERSION`](super::PROTOCOL_VERSION) +1**。
#[test]
fn server_message_wire_format() {
    let json = serde_json::to_string(&ServerMessage::KeyResult {
        session: SessionId(7),
        outcome: KeyOutcome::Consumed,
        commit: None,
        frame: frame(),
    })
    .expect("序列化");
    assert_eq!(
        json,
        r#"{"KeyResult":{"session":7,"outcome":"Consumed","commit":null,"frame":{"preedit":[{"text":"ni'hao","kind":"Typed"}],"cursor":6,"candidates":{"items":[{"text":"你好","kind":"Chinese","syllables":["ni","hao"],"reading":null,"translation":null,"fuma":null}]},"highlight":0,"page":0,"page_count":1,"layout":"horizontal","theme":"system","color_scheme":"zizai","notice":null,"inline_preedit":true}}}"#
    );
}

/// 同上，客户端方向。
#[test]
fn client_message_wire_format() {
    let json = serde_json::to_string(&ClientMessage::Key {
        session: SessionId(7),
        event: KeyEvent::new(
            65,
            Some('a'),
            KeyModifiers {
                shift: true,
                ..KeyModifiers::default()
            },
        ),
    })
    .expect("序列化");
    assert_eq!(
        json,
        r#"{"Key":{"session":7,"event":{"virtual_key":65,"character":"a","modifiers":{"ctrl":false,"shift":true,"alt":false,"win":false,"caps":false,"english_mode":false}}}}"#
    );
}

/// 新版本删掉一个字段：老 DLL 收到的帧缺字段，退到默认值而不是整帧失败
/// （0.1.6 删 `layout` 那次就是这里炸的；`layout` 本身另有坟墓字段照顾那批 DLL，见 `Frame::legacy_layout`）。
#[test]
fn frame_survives_removed_field() {
    let frame: Frame =
        serde_json::from_str(r#"{"preedit":[],"cursor":0}"#).expect("缺字段也要能读");
    assert_eq!(frame.page_count, 0);
    // 没有这个字段的老帧按「拼音放进应用」算，与加它之前的行为一致。
    assert!(frame.inline_preedit);
}

/// 新版本加了一个字段：老 DLL 不认识，忽略掉。
#[test]
fn frame_survives_added_field() {
    let frame: Frame = serde_json::from_str(r#"{"preedit":[],"cursor":0,"whats_this":"vertical"}"#)
        .expect("多字段也要能读");
    assert_eq!(frame, Frame::default());
    // `layout` 只写不读（给老 DLL 的坟墓字段），读到也照样忽略。
    let frame: Frame =
        serde_json::from_str(r#"{"preedit":[],"layout":"vertical"}"#).expect("坟墓字段也要能读");
    assert_eq!(frame.legacy_layout, "horizontal");
}

/// 新版本加了一种 preedit 片段 / 一种候选来源：老 DLL 退到它认识的那一档，照常画。
#[test]
fn unknown_enum_names_fall_back() {
    let segment: PreeditSegment =
        serde_json::from_str(r#"{"text":"x","kind":"Whatever"}"#).expect("认不出的种类要能读");
    assert_eq!(segment.kind, PreeditKind::Typed);

    let candidate: Candidate =
        serde_json::from_str(r#"{"text":"你好","kind":"Whatever"}"#).expect("认不出的来源要能读");
    assert_eq!(candidate.kind, CandidateKind::Chinese);

    let tagged: Candidate =
        serde_json::from_str(r#"{"text":"你好","kind":{"Whatever":3}}"#).expect("带字段的也要能读");
    assert_eq!(tagged.kind, CandidateKind::Chinese);

    // 认得的那些不受影响。
    let custom: Candidate =
        serde_json::from_str(r#"{"text":"你好","kind":{"Custom":3}}"#).expect("认得的要照旧");
    assert_eq!(custom.kind, CandidateKind::Custom(3));
}

/// 0.1.6 之前的 DLL 眼里的帧：那时字段都是必填的，`layout` 也还在。
/// 这批 DLL 已经装在用户机器上、升级后还留在没重启的应用里，所以新 Server 发的帧得照样能被它读出来。
#[derive(Deserialize)]
#[allow(dead_code)] // 字段齐全才是这条测试的意义：少一个就说明老 DLL 那边会解析失败
struct LegacyFrame {
    preedit: Vec<PreeditSegment>,

    cursor: usize,

    candidates: CandidateList,

    highlight: usize,

    page: usize,

    page_count: usize,

    layout: String,

    theme: String,

    #[serde(default)]
    notice: Option<String>,

    #[serde(default)]
    inline_preedit: bool,
}

/// 删掉 [`Frame::legacy_layout`] 这类坟墓字段时这条会红——那意味着又一次「升级要重启系统」。
#[test]
fn frame_still_parses_as_the_pre_0_1_6_dll_sees_it() {
    let json = serde_json::to_string(&frame()).expect("序列化");
    let legacy: LegacyFrame = serde_json::from_str(&json).expect("0.1.6 之前的 DLL 要能读新帧");
    assert_eq!(legacy.layout, "horizontal");
    assert_eq!(legacy.page_count, 1);
}

/// 认不出的按键处置退到放行：宁可把键还给应用，也不让整条应答失败。
#[test]
fn unknown_key_outcome_passes_through() {
    let outcome: KeyOutcome = serde_json::from_str(r#""Whatever""#).expect("认不出的处置要能读");
    assert_eq!(outcome, KeyOutcome::Passthrough);
}

/// 新版本加了一条消息：只管收的那端（Server）跳过这一条接着读下一条，不断连接。
#[test]
fn unknown_message_variant_is_skipped() {
    let mut wire = Vec::new();
    write_message(
        &mut wire,
        &serde_json::json!({ "Whatever": { "session": 1 } }),
    )
    .expect("写");
    write_message(
        &mut wire,
        &ClientMessage::CloseSession {
            session: SessionId(1),
        },
    )
    .expect("写");

    let mut reader = wire.as_slice();
    assert!(matches!(
        read_incoming::<_, ClientMessage>(&mut reader).expect("读"),
        Incoming::Unknown(_)
    ));
    assert!(matches!(
        read_incoming::<_, ClientMessage>(&mut reader).expect("读"),
        Incoming::Message(ClientMessage::CloseSession { .. })
    ));
    assert!(matches!(
        read_incoming::<_, ClientMessage>(&mut reader).expect("读"),
        Incoming::Eof
    ));
}

/// 一问一答那端（DLL）读不懂就得报错：等不到应答，没法接着往下走。
#[test]
fn read_message_reports_mismatch() {
    let mut wire = Vec::new();
    write_message(&mut wire, &serde_json::json!({ "Whatever": {} })).expect("写");
    let error = read_message::<_, ServerMessage>(&mut wire.as_slice()).expect_err("该报错");
    assert!(error.is_protocol_mismatch());
}

/// 缺字段的矩形退到零矩形，不让整条消息失败。
#[test]
fn screen_rect_survives_removed_field() {
    let rect: ScreenRect = serde_json::from_str(r#"{"left":3}"#).expect("缺字段也要能读");
    assert_eq!(rect.left, 3);
    assert_eq!(rect.bottom, 0);
}

/// 开会话带的协议版本：老 DLL 不带这个字段，读成 0（Server 据此记警告）。
#[test]
fn open_session_without_protocol_reads_zero() {
    let message: ClientMessage =
        serde_json::from_str(r#"{"OpenSession":{"session":1}}"#).expect("老 DLL 的开会话");
    let ClientMessage::OpenSession { protocol, app, .. } = message else {
        panic!("该是开会话");
    };
    assert_eq!(protocol, 0);
    assert_eq!(app, None);
}

/// 新 DLL 读旧 Server 的模式同步：没有语音字段时安全退到禁用。
#[test]
fn mode_sync_without_voice_is_disabled() {
    let message: ServerMessage =
        serde_json::from_str(r#"{"ModeSync":{"session":1,"english":null}}"#)
            .expect("旧 Server 的模式同步");
    let ServerMessage::ModeSync { voice, .. } = message else {
        panic!("该是模式同步");
    };
    assert!(!voice.enabled);
    assert_eq!(voice.state, super::VoiceState::Disabled);
}
