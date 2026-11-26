use crate::pkg::aop::Event;
use common::enums::EventTopic;
use serde::{Deserialize, Serialize};

/// 定时触发事件。
///
/// 携带完整的 When/Who/What/How 快照，让消费侧无需回查表。
/// 用 i32 而非枚举：AOP 封套是 `serde_json::Value`，事件要能被无依赖解析
/// （`MessageCreatedEvent.from_role` 等同款先例）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronTriggerEvent {
    pub event_id: String,
    pub trigger_id: String,
    pub trigger_name: String,
    /// 原始 payload（兼容存量 action 解析）
    pub payload: String,
    pub created_at: i64,

    // ===== When / What（消费侧行为分派所需）=====
    #[serde(default)]
    pub trigger_type: i32,
    #[serde(default)]
    pub behavior: i32,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub description: String,

    // ===== Who（归属/创建者/负责 Agent，P2 派发消息所需）=====
    #[serde(default)]
    pub org_id: Option<String>,
    #[serde(default)]
    pub owner_user_id: Option<String>,
    #[serde(default)]
    pub creator_type: Option<i32>,
    #[serde(default)]
    pub creator_id: Option<String>,
    /// 负责 Agent：唤醒目标；通知类消息的发送者
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub lease_owner: Option<String>,
}

impl Event for CronTriggerEvent {
    fn kind(&self) -> EventTopic {
        EventTopic::CronTrigger
    }

    fn id(&self) -> &str {
        &self.event_id
    }

    fn order_key(&self) -> &str {
        &self.trigger_id
    }

    fn created_at(&self) -> i64 {
        self.created_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 旧封套（只有 5 个字段）必须能反序列化：升级期间内存中可能有旧事件，
    /// 且 producer 的 `fired_at_of` 拿到的封套是 AOP 注入过元字段的超集。
    #[test]
    fn test_legacy_envelope_deserializes() {
        let json = serde_json::json!({
            "event_id": "t-1",
            "trigger_id": "t",
            "trigger_name": "legacy",
            "payload": "{\"action\":\"agent_rest\"}",
            "created_at": 100_i64,
            "attempt": 1,
        });
        let event: CronTriggerEvent = serde_json::from_value(json).expect("旧封套应可解析");
        assert_eq!(event.behavior, 0);
        assert_eq!(event.action, None);
        assert!(event.description.is_empty());
        assert_eq!(event.order_key(), "t");
    }
}
