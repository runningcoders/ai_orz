//! tests 单元测试（拆分自 impl.rs）
//!
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use crate::models::events::lark::{
    LarkEventHeader, LarkEventMessage, LarkEventSender, LarkMentionId, LarkMentionIds,
    LarkMessageEventData, LarkSenderId,
};

fn mention(key: &str, open_id: &str, name: &str) -> LarkMention {
    LarkMention {
        key: key.to_string(),
        id: LarkMentionId::Object(LarkMentionIds {
            open_id: open_id.to_string(),
            ..Default::default()
        }),
        name: name.to_string(),
        tenant_key: String::new(),
    }
}

/// 构造最小入站事件（只填 mentions 判定所需字段）
fn message_event(chat_type: &str, mentions: Vec<LarkMention>) -> LarkMessageEvent {
    LarkMessageEvent {
        schema: String::new(),
        header: LarkEventHeader {
            event_id: "evt_1".to_string(),
            event_type: "im.message.receive_v1".to_string(),
            create_time: "0".to_string(),
            token: String::new(),
            app_id: "cli_1".to_string(),
            tenant_key: "tenant_1".to_string(),
        },
        event: LarkMessageEventData {
            sender: LarkEventSender {
                sender_id: LarkSenderId {
                    open_id: "ou_sender".to_string(),
                    user_id: String::new(),
                    union_id: String::new(),
                },
                sender_type: "user".to_string(),
            },
            message: LarkEventMessage {
                message_id: "om_1".to_string(),
                root_id: None,
                parent_id: None,
                thread_id: None,
                create_time: "0".to_string(),
                chat_id: "oc_1".to_string(),
                chat_type: chat_type.to_string(),
                message_type: "text".to_string(),
                content: String::new(),
                mentions,
                extra: None,
            },
        },
    }
}

/// @ 本应用机器人 → 映射为「指向 Agent」
#[test]
fn matches_bot_mention_when_bot_is_mentioned() {
    let bot = Some("ou_bot".to_string());
    let event = message_event("group", vec![mention("@_user_1", "ou_bot", "小助手")]);
    assert!(matches_bot_mention(&event, &bot));
}

/// @ 其他人 → 不算指向 Agent
#[test]
fn does_not_match_when_other_user_is_mentioned() {
    let bot = Some("ou_bot".to_string());
    let event = message_event("group", vec![mention("@_user_1", "ou_user", "同事")]);
    assert!(!matches_bot_mention(&event, &bot));
}

/// 未配置 bot open_id 时无法判定，按放行处理
#[test]
fn passes_when_bot_open_id_missing() {
    let event = message_event("group", Vec::new());
    assert!(matches_bot_mention(&event, &None));
    assert!(matches_bot_mention(&event, &Some(String::new())));
}
