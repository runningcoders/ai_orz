//! 微信 iLink 事件类型定义（AOP 事件统一目录）
//!
//! 覆盖 iLink `getupdates` 返回的消息条目结构与 AOP 入站事件信封。
//! 事件类型为纯数据（serde DTO + Event impl），归属 models 层；
//! DAO 侧长轮询 adapter 收帧解析后 publish，消费在 `consumer/wechat_inbound`。
//! 协议参考：docs/design/wechat_channel_integration_design.md §5.1。
//!
//! iLink 为 2026 年新协议，字段解析全部宽容（`serde(default)` + 未知字段忽略），
//! 协议漂移只需调整本文件与 `dao/wechat/ilink.rs`。

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ==================== iLink 消息 DTO ====================

/// iLink 消息条目（`getupdates` 响应 `msg_list` / `msgs` 的元素）
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct IlinkMessage {
    /// 发送者（对端微信用户标识，稳定，等同 openid 语义）
    #[serde(default)]
    pub from_user_id: String,
    /// 接收者（bot 侧标识）
    #[serde(default)]
    pub to_user_id: String,
    /// 客户端消息 ID（幂等键首选）
    #[serde(default)]
    pub client_id: String,
    /// 服务端消息 ID（协议可能为数字或字符串；`client_id` 缺失时作幂等键兜底）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub msg_id: Option<Value>,
    /// 消息方向：`USER`（对端发来）| `BOT`（本 bot 发出，回声需过滤）
    #[serde(default)]
    pub message_type: String,
    /// 消息状态：`FINISH` 表示完整消息
    #[serde(default)]
    pub message_state: String,
    /// 会话上下文令牌（滚动刷新；回消息时必须回传最新值）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_token: Option<String>,
    /// 消息条目（文本 / 图片 / 视频 / 文件 / 语音；阶段一只处理文本）
    #[serde(default)]
    pub item_list: Vec<IlinkMessageItem>,
}

/// iLink 消息内容条目
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct IlinkMessageItem {
    /// 条目类型（协议为数字，1=文本；保留原始值防协议漂移）
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<Value>,
    /// 文本条目（非文本消息为 None，由上层按需扩展）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_item: Option<IlinkTextItem>,
}

/// iLink 文本条目
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct IlinkTextItem {
    #[serde(default)]
    pub content: String,
}

impl IlinkMessage {
    /// 是否为对端用户发来的消息（过滤 BOT 自身回声）
    pub fn is_user(&self) -> bool {
        self.message_type == "USER"
    }

    /// 是否为完整消息（状态缺失时宽容为完整，协议较新）
    pub fn is_finished(&self) -> bool {
        self.message_state.is_empty() || self.message_state == "FINISH"
    }

    /// 提取文本内容（首个非空文本条目；非文本消息返回 None）
    pub fn text(&self) -> Option<String> {
        self.item_list
            .iter()
            .filter_map(|item| item.text_item.as_ref())
            .map(|t| t.content.trim().to_string())
            .find(|t| !t.is_empty())
    }

    /// 幂等键：`client_id` 优先，`msg_id` 兜底（数字转字符串），均缺失返回空串
    /// （调用方构造事件时应保证非空，否则自行生成占位 ID）
    pub fn message_key(&self) -> String {
        if !self.client_id.is_empty() {
            return self.client_id.clone();
        }
        match &self.msg_id {
            Some(Value::String(s)) => s.clone(),
            Some(v @ Value::Number(_)) => v.to_string(),
            _ => String::new(),
        }
    }
}

// ==================== AOP 入站事件 ====================

/// 微信 iLink 入站消息事件（AOP 信封）
///
/// DAO 侧长轮询 adapter 收到消息后 publish 此事件，由业务 consumer
/// （`ConsumeMode::Async`）异步消费——**读循环里不做业务**。
/// 信封只携带协议数据（channel_id + bot_id + 原始消息），身份与业务语义由消费侧补全。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WechatInboundEvent {
    /// 消息归属的渠道 ID（长轮询按 channel 隔离，一个 bot 微信号 = 一个 channel）
    pub channel_id: String,
    /// bot 标识（order_key：同 bot 内串行、不同 bot 并行）
    pub bot_id: String,
    /// 幂等键（DAO 构造时从 message 解析，缺省时生成占位 ID）
    pub message_key: String,
    /// iLink 原始消息
    pub message: IlinkMessage,
}

impl crate::pkg::aop::Event for WechatInboundEvent {
    fn kind(&self) -> crate::pkg::aop::EventKind {
        crate::pkg::aop::EventKind::new("wechat.inbound.message")
    }

    fn id(&self) -> &str {
        &self.message_key
    }

    fn order_key(&self) -> &str {
        &self.bot_id
    }
}

// ==================== 单测 ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_message_json() -> String {
        r#"{
            "from_user_id": "peer_wx_1",
            "to_user_id": "bot_1",
            "client_id": "cid_1001",
            "message_type": "USER",
            "message_state": "FINISH",
            "context_token": "ctx_tok_a",
            "item_list": [
                {"type": 1, "text_item": {"content": "你好，agent"}},
                {"type": 2}
            ]
        }"#
        .to_string()
    }

    /// 消息解析：字段 + 宽容性（未知字段 / 多条目混合）
    #[test]
    fn test_parse_ilink_message() {
        let msg: IlinkMessage = serde_json::from_str(&sample_message_json()).unwrap();
        assert_eq!(msg.from_user_id, "peer_wx_1");
        assert!(msg.is_user());
        assert!(msg.is_finished());
        assert_eq!(msg.context_token.as_deref(), Some("ctx_tok_a"));
        assert_eq!(msg.text(), Some("你好，agent".to_string()));
        assert_eq!(msg.message_key(), "cid_1001");

        // Value 往返（AOP 队列以 serde_json::Value 传输）
        let value = serde_json::to_value(&msg).unwrap();
        let back: IlinkMessage = serde_json::from_value(value).unwrap();
        assert_eq!(back, msg);
    }

    /// 幂等键：client_id 优先，msg_id 兜底（数字/字符串两种形态）
    #[test]
    fn test_message_key_fallback() {
        let mut msg = IlinkMessage::default();
        assert_eq!(msg.message_key(), "");

        msg.msg_id = Some(Value::Number(serde_json::Number::from(42_i64)));
        assert_eq!(msg.message_key(), "42");

        msg.msg_id = Some(Value::String("srv_id".into()));
        assert_eq!(msg.message_key(), "srv_id");

        msg.client_id = "cid_x".into();
        assert_eq!(msg.message_key(), "cid_x");
    }

    /// 非文本 / BOT 回声 / 未完成消息的过滤辅助
    #[test]
    fn test_message_filters() {
        let mut msg: IlinkMessage = serde_json::from_str(&sample_message_json()).unwrap();
        msg.message_type = "BOT".into();
        assert!(!msg.is_user());

        msg.message_type = "USER".into();
        msg.message_state = "SENDING".into();
        assert!(!msg.is_finished());

        msg.message_state = "".into();
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
            message: msg,
        };
        assert_eq!(
            event.kind(),
            crate::pkg::aop::EventKind::new("wechat.inbound.message")
        );
        assert_eq!(event.id(), "cid_1001");
        assert_eq!(event.order_key(), "bot_1");

        // Value 往返
        let value = serde_json::to_value(&event).unwrap();
        let back: WechatInboundEvent = serde_json::from_value(value).unwrap();
        assert_eq!(back.message.text(), Some("你好，agent".to_string()));
    }
}
