//! tests 单元测试（拆分自 wechat.rs）
//!
//! 文件瘦身：原 501 行 → 327 行，测试体 175 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;

/// 官方形态样本：`message_type` / `message_state` 为**数字**，文本字段为 `text`，
/// 顶层权威 ID 为 `message_id`
fn sample_message_json() -> String {
    r#"{
        "from_user_id": "peer_wx_1",
        "to_user_id": "bot_1",
        "message_id": "7400000000000000123",
        "client_id": "cid_1001",
        "message_type": 1,
        "message_state": 2,
        "context_token": "ctx_tok_a",
        "item_list": [
            {"type": 1, "text_item": {"text": "你好，agent"}},
            {"type": 2}
        ]
    }"#
    .to_string()
}

/// 官方形态解析：数字枚举 + `text` 字段 + 幂等键取服务端 `message_id`
#[test]
fn test_parse_ilink_message() {
    let msg: IlinkMessage = serde_json::from_str(&sample_message_json()).unwrap();
    assert_eq!(msg.from_user_id, "peer_wx_1");
    assert_eq!(msg.message_type, IlinkMessageType::USER);
    assert_eq!(msg.message_state, IlinkMessageState::FINISH);
    assert!(msg.is_user());
    assert!(msg.is_finished());
    assert_eq!(msg.context_token.as_deref(), Some("ctx_tok_a"));
    assert_eq!(msg.text(), Some("你好，agent".to_string()));
    // 幂等键：/message_id（服务端权威）优先于 client_id
    assert_eq!(msg.message_key(), "7400000000000000123");

    // Value 往返（AOP 队列以 serde_json::Value 传输）
    let value = serde_json::to_value(&msg).unwrap();
    let back: IlinkMessage = serde_json::from_value(value).unwrap();
    assert_eq!(back, msg);
}

/// 早期形态兼容：`message_type` 为字符串、文本字段为 `content` —— 双形态都收
///
/// 这一条是**回归护栏**：此前声明成 `String` 时，线上数字形态会让整条消息
/// 反序列化失败并被 `.ok()` 吞掉，症状是「一帧都没收到」。
#[test]
fn test_parse_ilink_message_string_form_and_content_alias() {
    let legacy = r#"{
        "from_user_id": "peer_wx_2",
        "client_id": "cid_2002",
        "message_type": "USER",
        "message_state": "FINISH",
        "item_list": [{"type": 1, "text_item": {"content": "旧形态文本"}}]
    }"#;
    let msg: IlinkMessage = serde_json::from_str(legacy).unwrap();
    assert!(msg.is_user());
    assert!(msg.is_finished());
    assert_eq!(msg.text(), Some("旧形态文本".to_string()));
    // 无 message_id / msg_id → 回落 client_id
    assert_eq!(msg.message_key(), "cid_2002");

    // 数字字符串同样被接受（两种线上形态都不挂）
    let numeric_string = r#"{"message_type":"1","message_state":"2"}"#;
    let msg: IlinkMessage = serde_json::from_str(numeric_string).unwrap();
    assert!(msg.is_user());
    assert!(msg.is_finished());

    // 未知字符串：宽容归零而非报错（协议较新）
    let unknown = r#"{"message_type":"SOME_NEW_KIND"}"#;
    let msg: IlinkMessage = serde_json::from_str(unknown).unwrap();
    assert_eq!(msg.message_type, IlinkMessageType::NONE);
    assert!(!msg.is_user());
}

/// 枚举双形态：数字/字符串 → 同一判定；序列化恒为数字（出站 spec 形态）
#[test]
fn test_wire_enum_roundtrip_is_numeric() {
    let parsed: IlinkMessageType = serde_json::from_str("\"BOT\"").unwrap();
    assert_eq!(parsed, IlinkMessageType::BOT);
    assert_eq!(serde_json::to_string(&parsed).unwrap(), "2");

    let parsed: IlinkMessageState = serde_json::from_str("2").unwrap();
    assert_eq!(parsed, IlinkMessageState::FINISH);
    assert_eq!(serde_json::to_string(&parsed).unwrap(), "2");

    // 浮点形态（JSON 数字被解析成 1.0）同样归一
    let parsed: IlinkMessageType = serde_json::from_str("1.0").unwrap();
    assert_eq!(parsed, IlinkMessageType::USER);
}

/// 幂等键优先级：`message_id` → `client_id` → item `msg_id` → 空
#[test]
fn test_message_key_priority() {
    // 全空
    let mut msg = IlinkMessage::default();
    assert_eq!(msg.message_key(), "");

    // item 级 msg_id 兜底
    msg.item_list.push(IlinkMessageItem {
        msg_id: Some(Value::Number(serde_json::Number::from(42_i64))),
        ..Default::default()
    });
    assert_eq!(msg.message_key(), "42");

    // client_id 优先于 item msg_id
    msg.client_id = "cid_x".into();
    assert_eq!(msg.message_key(), "cid_x");

    // 顶层 message_id 最优先（字符串形态）
    msg.message_id = Some(Value::String("srv_id".into()));
    assert_eq!(msg.message_key(), "srv_id");

    // 数字形态归一为十进制字符串（无损，不做浮点转换）
    msg.message_id = Some(Value::Number(serde_json::Number::from(
        7_400_000_000_000_000_123_i64,
    )));
    assert_eq!(msg.message_key(), "7400000000000000123");
}

/// 非文本 / BOT 回声 / 生成中消息的过滤辅助
#[test]
fn test_message_filters() {
    let mut msg: IlinkMessage = serde_json::from_str(&sample_message_json()).unwrap();
    msg.message_type = IlinkMessageType::BOT;
    assert!(!msg.is_user());

    msg.message_type = IlinkMessageType::USER;
    msg.message_state = IlinkMessageState::GENERATING;
    assert!(!msg.is_finished());

    // NEW(0) 与字段缺失一并放行（宽容）
    msg.message_state = IlinkMessageState::NEW;
    assert!(msg.is_finished());
    msg.message_state = IlinkMessageState::default();
    assert!(msg.is_finished());

    msg.item_list.clear();
    assert_eq!(msg.text(), None);
}

/// AOP 信封：Event 语义（kind/id/order_key）
#[test]
fn test_wechat_inbound_event_semantics() {
    use crate::pkg::aop::Event;
    let msg: IlinkMessage = serde_json::from_str(&sample_message_json()).unwrap();
    let event = WechatInboundEvent {
        channel_id: "ch_1".to_string(),
        bot_id: "bot_1".to_string(),
        message_key: msg.message_key(),
        cursor: Some("opaque_cursor_1".to_string()),
        message: msg,
    };
    assert_eq!(
        event.kind(),
        common::enums::EventTopic::WechatInboundMessage
    );
    assert_eq!(event.id(), "7400000000000000123");
    assert_eq!(event.order_key(), "bot_1");

    // Value 往返
    let value = serde_json::to_value(&event).unwrap();
    let back: WechatInboundEvent = serde_json::from_value(value).unwrap();
    assert_eq!(back.message.text(), Some("你好，agent".to_string()));
    assert_eq!(back.cursor.as_deref(), Some("opaque_cursor_1"));

    // 旧封套（无 cursor 字段）仍可解析 —— P2 上线前的在途事件不被拦
    let legacy = serde_json::json!({
        "channel_id": "ch_1",
        "bot_id": "bot_1",
        "message_key": "cid_1001",
        "message": {"from_user_id": "peer_wx_1"},
    });
    let legacy: WechatInboundEvent = serde_json::from_value(legacy).unwrap();
    assert_eq!(legacy.cursor, None);
}
