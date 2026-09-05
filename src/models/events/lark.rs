//! 飞书事件订阅类型定义（AOP 事件统一目录）
//!
//! 覆盖 `im.message.receive_v1` 事件结构（P2P 私信场景）与 AOP 入站事件信封。
//! 事件类型为纯数据（serde DTO + Event impl），归属 models 层；
//! DAO 侧长连接 adapter 只负责收帧解析后 publish，消费在 `consumer/lark_inbound`。
//! 字段参考：https://open.feishu.cn/document/event/im.message.receive_v1

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `im.message.receive_v1` 事件顶层包装
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkMessageEvent {
    /// schema 版本
    #[serde(default)]
    pub schema: String,
    /// 事件头
    pub header: LarkEventHeader,
    /// 事件数据
    pub event: LarkMessageEventData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkMessageEventData {
    pub sender: LarkEventSender,
    pub message: LarkEventMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkEventSender {
    pub sender_id: LarkSenderId,
    /// 发送者 ID 类型，例如 "open_id"
    #[serde(default)]
    pub sender_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkTextContent {
    #[serde(default)]
    pub text: String,
}

// ==================== AOP 入站事件 ====================

/// 飞书 WS 入站消息事件（AOP 信封）
///
/// DAO 侧长连接 adapter 收到 `im.message.receive_v1` 后 publish 此事件，
/// 由业务 consumer（`ConsumeMode::Async`）异步消费——**读循环里不做业务**。
/// 信封只携带协议数据（app_id + 原始事件），身份与业务语义由消费侧补全。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkInboundEvent {
    /// 事件归属的飞书应用（多应用路由依据）
    pub app_id: String,
    /// 飞书原始事件
    pub event: LarkMessageEvent,
}

impl crate::pkg::aop::Event for LarkInboundEvent {
    fn kind(&self) -> crate::pkg::aop::EventKind {
        crate::pkg::aop::EventKind::new("lark.inbound.message")
    }

    fn id(&self) -> &str {
        &self.event.header.event_id
    }

    fn order_key(&self) -> &str {
        // 同应用内按到达顺序串行处理；不同应用并行
        &self.app_id
    }

    fn created_at(&self) -> i64 {
        self.event.header.create_time.parse().unwrap_or(0)
    }
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
            crate::pkg::aop::EventKind::new("lark.inbound.message")
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
}
