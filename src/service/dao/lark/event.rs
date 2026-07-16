//! 飞书事件订阅类型定义
//!
//! 主要覆盖 `im.message.receive_v1` 事件结构（P2P 私信场景）。
//! 字段参考：https://open.feishu.cn/document/event/im.message.receive_v1

use serde::Deserialize;
use serde_json::Value;

/// `im.message.receive_v1` 事件顶层包装
#[derive(Debug, Clone, Deserialize)]
pub struct LarkMessageEvent {
    /// schema 版本
    #[serde(default)]
    pub schema: String,
    /// 事件头
    pub header: LarkEventHeader,
    /// 事件数据
    pub event: LarkMessageEventData,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LarkEventHeader {
    /// 事件唯一 ID（用于幂等去重）
    pub event_id: String,
    /// 事件类型，例如 "im.message.receive_v1"
    pub event_type: String,
    /// 事件创建时间（毫秒字符串）
    pub create_time: String,
    /// Verification Token（用于事件校验）
    #[serde(default)]
    pub token: String,
    /// 应用 ID
    #[serde(default)]
    pub app_id: String,
    /// 租户 key
    #[serde(default)]
    pub tenant_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LarkMessageEventData {
    pub sender: LarkEventSender,
    pub message: LarkEventMessage,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LarkEventSender {
    pub sender_id: LarkSenderId,
    /// 发送者 ID 类型，例如 "open_id"
    #[serde(default)]
    pub sender_type: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LarkSenderId {
    /// 用户的 open_id（主要标识）
    #[serde(default)]
    pub open_id: String,
    /// 用户的 user_id（企业内）
    #[serde(default)]
    pub user_id: String,
    /// 用户的 union_id
    #[serde(default)]
    pub union_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LarkEventMessage {
    /// 消息 ID
    pub message_id: String,
    /// 根消息 ID（话题消息时存在）
    #[serde(default)]
    pub root_id: Option<String>,
    /// 父消息 ID（回复消息时存在）
    #[serde(default)]
    pub parent_id: Option<String>,
    /// 消息创建时间（毫秒字符串）
    pub create_time: String,
    /// 会话 ID
    #[serde(default)]
    pub chat_id: String,
    /// 会话类型：`p2p`（私信）或 `group`（群聊）
    #[serde(default)]
    pub chat_type: String,
    /// 消息类型：`text`/`image`/`file` 等，本期只处理 `text`
    #[serde(default)]
    pub message_type: String,
    /// 消息内容（JSON 字符串）
    ///
    /// 文本消息：`{"text":"消息内容"}`
    /// 富文本等其他类型保留为原始字符串，由上层判断后丢弃。
    #[serde(default)]
    pub content: String,
    /// 消息原始扩展字段（保留未识别字段）
    #[serde(default)]
    pub extra: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LarkTextContent {
    #[serde(default)]
    pub text: String,
}

impl LarkMessageEvent {
    /// 是否为 P2P 私信
    pub fn is_p2p(&self) -> bool {
        self.event.message.chat_type == "p2p"
    }

    /// 是否为文本消息
    pub fn is_text(&self) -> bool {
        self.event.message.message_type == "text"
    }

    /// 解析文本消息内容
    pub fn parse_text(&self) -> Option<String> {
        if !self.is_text() {
            return None;
        }
        let parsed: LarkTextContent = serde_json::from_str(&self.event.message.content).ok()?;
        Some(parsed.text)
    }

    /// 发送者 open_id
    pub fn sender_open_id(&self) -> &str {
        &self.event.sender.sender_id.open_id
    }
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn test_group_message_is_not_p2p() {
        let raw = P2P_TEXT_EVENT.replace("\"p2p\"", "\"group\"");
        let event: LarkMessageEvent = serde_json::from_str(&raw).unwrap();
        assert!(!event.is_p2p());
        assert!(event.is_text());
    }

    #[test]
    fn test_non_text_event_parse_text_returns_none() {
        let raw = P2P_TEXT_EVENT.replace("\"text\"", "\"image\"")
            .replace("{\"text\":\"你好\"}", "{}");
        let event: LarkMessageEvent = serde_json::from_str(&raw).unwrap();
        assert!(!event.is_text());
        assert_eq!(event.parse_text(), None);
    }
}
