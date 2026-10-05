//! tests 单元测试（拆分自 lark.rs）
//!
//! 文件瘦身：原 365 行 → 193 行，测试体 173 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;

const P2P_TEXT_EVENT: &str = r#"{
    "schema": "2.0",
    "header": {
        "event_id": "evt_xxx",
        "event_type": "im.message.receive_v1",
        "create_time": "1700000000000",
        "token": "verify_token_xxx",
        "app_id": "cli_xxx",
        "tenant_key": "tenant_xxx"
    },
    "event": {
        "sender": {
            "sender_id": {
                "open_id": "ou_xxx",
                "user_id": "uid_xxx",
                "union_id": "un_xxx"
            },
            "sender_type": "open_id"
        },
        "message": {
            "message_id": "msg_xxx",
            "create_time": "1700000000000",
            "chat_id": "oc_xxx",
            "chat_type": "p2p",
            "message_type": "text",
            "content": "{\"text\":\"你好\"}"
        }
    }
}"#;

#[test]
fn test_parse_p2p_text_event() {
    let event: LarkMessageEvent = serde_json::from_str(P2P_TEXT_EVENT).unwrap();
    assert_eq!(event.header.event_id, "evt_xxx");
    assert_eq!(event.header.event_type, "im.message.receive_v1");
    assert!(event.is_p2p());
    assert!(event.is_text());
    assert_eq!(event.sender_open_id(), "ou_xxx");
    assert_eq!(event.parse_text(), Some("你好".to_string()));
}

/// 线上实测 payload：应用无 user_id/union_id 权限，飞书下发 `null`（非缺失字段）。
/// 修复前整帧反序列化失败 → 消息静默丢失且不 ACK（服务端持续重推）。
#[test]
fn test_parse_event_with_null_sender_ids() {
    const REAL_NULL_IDS_EVENT: &str = r#"{
        "schema": "2.0",
        "header": {
            "event_id": "962e93e8705d3b0925bc45a62ddcbac2",
            "token": "",
            "create_time": "1791015529231",
            "event_type": "im.message.receive_v1",
            "tenant_key": "2d5565227d0f175d",
            "app_id": "cli_a9402a7268781cb0"
        },
        "event": {
            "message": {
                "chat_id": "oc_eac2088b249e4578c6cd4c4d274a5a87",
                "chat_type": "p2p",
                "content": "{\"text\":\"测试连通性\"}",
                "create_time": "1791015528564",
                "message_id": "om_x100b633fe84b58a0b1faeb76e3bddf7",
                "message_type": "text",
                "update_time": "1791015528564"
            },
            "sender": {
                "sender_id": {
                    "open_id": "ou_a19c91734de4b79c38b618ae5d45c5fc",
                    "union_id": null,
                    "user_id": null
                },
                "sender_type": "user",
                "tenant_key": "2d5565227d0f175d"
            }
        }
    }"#;
    let event: LarkMessageEvent = serde_json::from_str(REAL_NULL_IDS_EVENT).unwrap();
    assert!(event.is_p2p());
    assert!(event.is_text());
    assert_eq!(event.parse_text(), Some("测试连通性".to_string()));
    assert_eq!(
        event.sender_open_id(),
        "ou_a19c91734de4b79c38b618ae5d45c5fc"
    );
    // null → 空串，不 panic 不丢帧
    assert_eq!(event.event.sender.sender_id.user_id, "");
    assert_eq!(event.event.sender.sender_id.union_id, "");
}

#[test]
fn test_group_message_is_not_p2p() {
    let raw = P2P_TEXT_EVENT.replace("\"p2p\"", "\"group\"");
    let event: LarkMessageEvent = serde_json::from_str(&raw).unwrap();
    assert!(!event.is_p2p());
    assert!(event.is_group());
    assert!(event.is_text());
}

/// 话题消息：携带 thread_id（omt_ 前缀），群聊形态，共享话题标识
#[test]
fn test_parse_group_thread_message() {
    const THREAD_EVENT: &str = r#"{
        "schema": "2.0",
        "header": {
            "event_id": "evt_thread",
            "event_type": "im.message.receive_v1",
            "create_time": "1700000000000",
            "token": "verify_token_xxx",
            "app_id": "cli_xxx",
            "tenant_key": "tenant_xxx"
        },
        "event": {
            "sender": {
                "sender_id": { "open_id": "ou_xxx" },
                "sender_type": "open_id"
            },
            "message": {
                "message_id": "om_xxx",
                "root_id": "om_root",
                "parent_id": "om_parent",
                "thread_id": "omt_xxx",
                "create_time": "1700000000000",
                "chat_id": "oc_xxx",
                "chat_type": "group",
                "message_type": "text",
                "content": "{\"text\":\"话题内回复\"}"
            }
        }
    }"#;
    let event: LarkMessageEvent = serde_json::from_str(THREAD_EVENT).unwrap();
    assert!(event.is_group());
    assert!(event.is_text());
    assert_eq!(event.event.message.thread_id.as_deref(), Some("omt_xxx"));
    assert_eq!(event.event.message.parent_id.as_deref(), Some("om_parent"));
    assert_eq!(event.parse_text(), Some("话题内回复".to_string()));
}

#[test]
fn test_non_text_event_parse_text_returns_none() {
    let raw = P2P_TEXT_EVENT
        .replace("\"text\"", "\"image\"")
        .replace("{\"text\":\"你好\"}", "{}");
    let event: LarkMessageEvent = serde_json::from_str(&raw).unwrap();
    assert!(!event.is_text());
    assert_eq!(event.parse_text(), None);
}

/// AOP 信封：序列化回环 + Event 语义（kind/id/order_key/created_at）
#[test]
fn test_lark_inbound_event_roundtrip() {
    use crate::pkg::aop::Event;
    let inner: LarkMessageEvent = serde_json::from_str(P2P_TEXT_EVENT).unwrap();
    let aop_event = LarkInboundEvent {
        app_id: "cli_app".to_string(),
        event: inner,
    };
    assert_eq!(
        aop_event.kind(),
        common::enums::EventTopic::LarkInboundMessage
    );
    assert_eq!(aop_event.id(), "evt_xxx");
    assert_eq!(aop_event.order_key(), "cli_app");
    assert_eq!(aop_event.created_at(), 1700000000000);

    // Value 往返（AOP 队列以 serde_json::Value 传输）
    let value = serde_json::to_value(&aop_event).unwrap();
    let back: LarkInboundEvent = serde_json::from_value(value).unwrap();
    assert_eq!(back.app_id, "cli_app");
    assert_eq!(back.event.header.event_id, "evt_xxx");
    assert_eq!(back.event.parse_text(), Some("你好".to_string()));
}
