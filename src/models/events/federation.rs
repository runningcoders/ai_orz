//! 联邦 WS 长连接帧信封与 AOP 事件
//!
//! **帧信封红线**：`FederationFrame` 只携带指令 / 追踪 / 幂等键，
//! 绝不携带 `user_id` / `caller_organization_id` 等身份字段——身份一律由
//! 会话握手（`resolve_federation_identity`）认定后经 ctx 注入。
//!
//! 事件链路（与 lark 样板同构）：
//! - 入站：DAO adapter 收帧 → publish `FederationInboundEvent` → 业务 consumer
//!   异步消费（调 handler 核心函数）
//! - 出站：业务 publish `FederationOutboundEvent` → WS 出站 consumer 查连接
//!   注册表 → push 到对端
//! - 响应帧（kind=response）不进事件总线，由 session 直接唤醒 pending 表

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::pkg::aop::{Event, EventKind};

/// 命令名：A2A 委派（P8 最小闭环唯一命令）
pub const FEDERATION_CMD_SEND_TASK: &str = "send_task";
/// 命令名：命令执行结果回推
pub const FEDERATION_CMD_RESPONSE: &str = "response";

/// 联邦 WS 帧信封（双向通用）
///
/// 一个 WS 会话上双向跑同一种信封；`kind` 区分命令，`correlation_id`
/// 配对请求-响应，`payload` 为命令参数 / 执行结果的 JSON。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationFrame {
    /// 命令名（如 `send_task` / `response`）
    pub kind: String,
    /// 请求-响应配对键（发起侧生成，UUID）
    pub correlation_id: String,
    /// 命令参数 / 执行结果
    pub payload: Value,
}

impl FederationFrame {
    /// 构造命令帧
    pub fn command(kind: &str, correlation_id: String, payload: Value) -> Self {
        Self {
            kind: kind.to_string(),
            correlation_id,
            payload,
        }
    }

    /// 构造响应帧
    pub fn response(correlation_id: String, payload: Value) -> Self {
        Self {
            kind: FEDERATION_CMD_RESPONSE.to_string(),
            correlation_id,
            payload,
        }
    }

    /// 是否为响应帧
    pub fn is_response(&self) -> bool {
        self.kind == FEDERATION_CMD_RESPONSE
    }
}

// ==================== 入站事件 ====================

/// 联邦入站事件（对端命令帧 → publish → 业务 consumer 消费）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationInboundEvent {
    /// 本端组织 ID（会话握手认定，事件级注入——帧内无身份字段）
    pub local_org: String,
    /// 对端组织 ID（会话握手认定，事件级注入——帧内无身份字段）
    pub peer_org: String,
    /// 命令帧
    pub frame: FederationFrame,
}

impl FederationInboundEvent {
    /// send_task 命令的 EventKind（一种命令一个 kind + 一个 consumer）
    pub const KIND_SEND_TASK: EventKind = EventKind::new("federation.inbound.send_task");
    /// 未识别命令的兜底 kind（无 consumer 订阅，仅可观测）
    pub const KIND_OTHER: EventKind = EventKind::new("federation.inbound.other");

    /// 按帧内命令名映射 EventKind
    fn kind_of(cmd: &str) -> EventKind {
        match cmd {
            FEDERATION_CMD_SEND_TASK => Self::KIND_SEND_TASK,
            _ => Self::KIND_OTHER,
        }
    }
}

impl Event for FederationInboundEvent {
    fn kind(&self) -> EventKind {
        Self::kind_of(&self.frame.kind)
    }

    fn id(&self) -> &str {
        &self.frame.correlation_id
    }

    fn order_key(&self) -> &str {
        // 同对端串行（对端会话内顺序保证）；不同对端并行
        &self.peer_org
    }
}

// ==================== 出站事件 ====================

/// 联邦出站事件（业务 publish → WS 出站 consumer 按对端路由 push）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationOutboundEvent {
    /// 目标对端组织 ID（路由键：连接注册表按此查活连接）
    pub peer_org: String,
    /// 命令帧（命令或响应）
    pub frame: FederationFrame,
}

impl Event for FederationOutboundEvent {
    fn kind(&self) -> EventKind {
        EventKind::new("federation.outbound")
    }

    fn id(&self) -> &str {
        &self.frame.correlation_id
    }

    fn order_key(&self) -> &str {
        &self.peer_org
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 信封红线：序列化输出不含身份字段
    #[test]
    fn frame_has_no_identity_fields() {
        let frame = FederationFrame::command(
            FEDERATION_CMD_SEND_TASK,
            "corr-1".to_string(),
            serde_json::json!({"params": {}}),
        );
        let json = serde_json::to_string(&frame).unwrap();
        assert!(!json.contains("user_id"));
        assert!(!json.contains("organization_id"));
        assert!(!json.contains("caller"));
        assert!(json.contains("correlation_id"));
    }

    /// kind 映射：send_task → 专用 kind；未知 → 兜底 kind；response → 兜底
    /// （response 帧在 session 内被 pending 表截获，不应 publish 成事件）
    #[test]
    fn inbound_event_kind_mapping() {
        use crate::pkg::aop::Event as _;
        let evt = FederationInboundEvent {
            local_org: "org_b".to_string(),
            peer_org: "org_a".to_string(),
            frame: FederationFrame::command(
                FEDERATION_CMD_SEND_TASK,
                "c1".to_string(),
                Value::Null,
            ),
        };
        assert_eq!(evt.kind(), FederationInboundEvent::KIND_SEND_TASK);

        let other = FederationInboundEvent {
            local_org: "org_b".to_string(),
            peer_org: "org_a".to_string(),
            frame: FederationFrame::command("future_cmd", "c2".to_string(), Value::Null),
        };
        assert_eq!(other.kind(), FederationInboundEvent::KIND_OTHER);
        assert_eq!(other.id(), "c2");
        assert_eq!(other.order_key(), "org_a");
    }

    /// 响应帧判定
    #[test]
    fn response_frame_detection() {
        let resp = FederationFrame::response("c1".to_string(), serde_json::json!({"ok": true}));
        assert!(resp.is_response());
        let cmd = FederationFrame::command(FEDERATION_CMD_SEND_TASK, "c2".to_string(), Value::Null);
        assert!(!cmd.is_response());
    }

    /// Value 往返（AOP 队列以 serde_json::Value 传输）
    #[test]
    fn events_value_roundtrip() {
        let out = FederationOutboundEvent {
            peer_org: "org_b".to_string(),
            frame: FederationFrame::response("c1".to_string(), serde_json::json!({"id": "t1"})),
        };
        assert_eq!(out.kind(), EventKind::new("federation.outbound"));
        let back: FederationOutboundEvent =
            serde_json::from_value(serde_json::to_value(&out).unwrap()).unwrap();
        assert_eq!(back.peer_org, "org_b");
        assert_eq!(back.frame.correlation_id, "c1");
    }
}
