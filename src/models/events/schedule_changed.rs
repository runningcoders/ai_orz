use crate::pkg::aop::Event;
use common::enums::EventTopic;
use serde::{Deserialize, Serialize};

/// 日程配置变更的原始事件。
///
/// 由 domain 在 create / update / delete / pause / resume / cancel 成功后发出，
/// 只描述「这条日程变成了什么」，不碰派发队列 —— 入队是 cron producer 的职责。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScheduleChangedEvent {
    pub event_id: String,
    pub trigger_id: String,
    pub op: ScheduleChangeOp,
    /// Upsert 时携带的最新执行时刻；Remove 时为 None
    #[serde(default)]
    pub next_run_at: Option<i64>,
    /// Upsert 时是否启用；None 表示未知（Remove 时不用）
    #[serde(default)]
    pub is_enabled: Option<bool>,
    /// 变更后的 DB 版本（`cron_triggers.updated_at`）。派发队列把它作为载荷，
    /// 消费时业务层按 id 回查：版本 / 时间对不上即跳过，避免旧快照被误执行。
    #[serde(default)]
    pub version: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ScheduleChangeOp {
    Upsert,
    Remove,
}

impl Event for ScheduleChangedEvent {
    fn kind(&self) -> EventTopic {
        EventTopic::ScheduleChanged
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

impl ScheduleChangedEvent {
    pub fn upsert(
        trigger_id: &str,
        next_run_at: i64,
        is_enabled: bool,
        version: i64,
        created_at: i64,
    ) -> Self {
        Self {
            event_id: format!("schedule-{}-{}", trigger_id, created_at),
            trigger_id: trigger_id.to_string(),
            op: ScheduleChangeOp::Upsert,
            next_run_at: Some(next_run_at),
            is_enabled: Some(is_enabled),
            version,
            created_at,
        }
    }

    pub fn remove(trigger_id: &str, created_at: i64) -> Self {
        Self {
            event_id: format!("schedule-remove-{}-{}", trigger_id, created_at),
            trigger_id: trigger_id.to_string(),
            op: ScheduleChangeOp::Remove,
            next_run_at: None,
            is_enabled: None,
            version: 0,
            created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_and_remove_carry_only_their_own_fields() {
        let upsert = ScheduleChangedEvent::upsert("t-1", 123, true, 88, 100);
        assert_eq!(upsert.op, ScheduleChangeOp::Upsert);
        assert_eq!(upsert.next_run_at, Some(123));
        assert_eq!(upsert.is_enabled, Some(true));
        assert_eq!(upsert.version, 88);
        assert_eq!(upsert.kind(), EventTopic::ScheduleChanged);

        let remove = ScheduleChangedEvent::remove("t-1", 101);
        assert_eq!(remove.op, ScheduleChangeOp::Remove);
        assert_eq!(remove.next_run_at, None);
        assert_ne!(upsert.id(), remove.id());
    }

    #[test]
    fn envelope_survives_aop_metadata_injection() {
        let event = ScheduleChangedEvent::upsert("t-2", 500, false, 77, 10);
        let mut json = serde_json::to_value(&event).unwrap();
        json.as_object_mut()
            .unwrap()
            .insert("attempt".to_string(), serde_json::json!(1));
        let parsed: ScheduleChangedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(parsed, event);
    }
}
