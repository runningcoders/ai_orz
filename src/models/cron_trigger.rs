//! Cron Trigger 模型
//!
//! CronTrigger 是定时触发器配置，管理定时任务的触发规则。
//! 统一调度器后，一条记录完整描述 When / Who / What / How 四组参数；
//! 系统默认触发器只填 When/What，Who 组留空表示无归属约束。

use common::enums::{CreatorType, TriggerBehavior, TriggerType};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// CronTriggerPo 持久化对象。
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct CronTriggerPo {
    // ===== When：什么时间 =====
    pub id: String,
    pub name: String,
    pub trigger_type: TriggerType,
    pub cron_expression: Option<String>,
    pub interval_seconds: Option<i64>,
    pub run_at: Option<i64>,
    pub next_run_at: i64,
    pub start_at: Option<i64>,
    pub end_at: Option<i64>,

    // ===== Who：谁（归属用户 / 创建者 / 负责 Agent）=====
    //
    // 三个字段各司其职，不要互相兼任：
    // - `owner_user_id`：**归属**用户（可见范围 + 推送受众）；
    // - `creator_type` / `creator_id`：**创建者**，可以是 User、Agent 或 System；
    // - `agent_id`：**负责**该日程的 Agent（到点唤醒它，或由它发出提醒）。
    // 用户经 HTTP 创建时 creator 是用户、agent_id 由参数指定；Agent 自建时
    // creator 与 agent 通常都指向它自己。
    pub org_id: Option<String>,
    pub owner_user_id: Option<String>,
    pub creator_type: Option<i32>,
    pub creator_id: Option<String>,
    pub agent_id: Option<String>,

    // ===== What：做什么事 =====
    pub description: String,
    pub action: Option<String>,
    pub payload: String,

    // ===== How：怎么做 =====
    pub behavior: TriggerBehavior,
    pub max_runs: Option<i64>,
    pub total_runs: i64,
    pub last_error: Option<String>,
    pub lease_owner: Option<String>,
    pub lease_until: Option<i64>,

    // ===== 通用审计与执行 =====
    pub is_enabled: i32,
    pub last_run_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub created_by: Option<String>,
    pub updated_by: Option<String>,
}

impl CronTriggerPo {
    pub fn new(
        id: String,
        name: String,
        trigger_type: TriggerType,
        next_run_at: i64,
        creator: Option<String>,
    ) -> Self {
        let id = if id.is_empty() {
            common::new_id!(CronTrigger)
        } else {
            id
        };
        let now = common::constants::utils::current_timestamp_ms();
        Self {
            id,
            name,
            trigger_type,
            cron_expression: None,
            interval_seconds: None,
            run_at: None,
            next_run_at,
            start_at: None,
            end_at: None,
            org_id: None,
            owner_user_id: None,
            creator_type: None,
            creator_id: None,
            agent_id: None,
            description: String::new(),
            action: None,
            payload: "{}".to_string(),
            behavior: TriggerBehavior::SyncMaintenance,
            max_runs: None,
            total_runs: 0,
            last_error: None,
            lease_owner: None,
            lease_until: None,
            is_enabled: 1,
            last_run_at: None,
            created_at: now,
            updated_at: now,
            created_by: creator.clone(),
            updated_by: creator,
        }
    }

    pub fn touch(&mut self, modifier: Option<String>) {
        self.updated_at = common::constants::utils::current_timestamp_ms();
        self.updated_by = modifier;
    }

    /// 业务动作标签：顶层 `action` 列优先，存量记录回落到 `payload.action`。
    ///
    /// 存量 6 条系统触发器把 action 放在 payload 里（迁移已回填到列，但保留回落
    /// 以兼容「写入时只有 payload」的旁路）。不修改 payload 本身，避免破坏
    /// `extra` 具名字段的既有解析。
    pub fn resolved_action(&self) -> Option<String> {
        if let Some(action) = self.action.as_deref()
            && !action.is_empty()
        {
            return Some(action.to_string());
        }
        let value: serde_json::Value = serde_json::from_str(&self.payload).ok()?;
        value.get("action")?.as_str().map(str::to_string)
    }

    /// 是否仍在调度生效窗口内（窗口字段为 NULL 表示无限制）
    pub fn within_window(&self, now: i64) -> bool {
        self.start_at.is_none_or(|start| now >= start) && self.end_at.is_none_or(|end| now <= end)
    }

    /// 是否已达到最大执行次数（`total_runs` 已含历史全部执行）
    pub fn runs_exhausted(&self) -> bool {
        self.max_runs.is_some_and(|max| self.total_runs >= max)
    }

    /// 本次执行完成后是否达到最大次数。
    ///
    /// `mark_executed` 在「消费成功之后、`total_runs` 尚未 +1」时调用，
    /// 此时计数不含本次，必须 +1 再比较；直接用 `runs_exhausted()` 会少判一次，
    /// 使 `max_runs=N` 实际执行 N+1 次。
    pub fn exhausts_after_current_run(&self) -> bool {
        self.max_runs.is_some_and(|max| self.total_runs + 1 >= max)
    }

    /// 创建者类型（存量记录无此字段 → 视为 System）
    pub fn creator(&self) -> CreatorType {
        self.creator_type
            .map(CreatorType::from)
            .unwrap_or(CreatorType::System)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolved_action_prefers_column() {
        let mut trigger = CronTriggerPo::new(
            String::new(),
            "t".to_string(),
            TriggerType::Schedule,
            100,
            None,
        );
        trigger.action = Some("agent_schedule".to_string());
        trigger.payload = r#"{"action":"legacy","extra":{}}"#.to_string();
        assert_eq!(trigger.resolved_action().as_deref(), Some("agent_schedule"));
    }

    #[test]
    fn test_resolved_action_falls_back_to_payload() {
        let mut trigger =
            CronTriggerPo::new(String::new(), "t".to_string(), TriggerType::Cron, 100, None);
        trigger.payload = r#"{"action":"agent_rest","extra":{"settle_limit":10}}"#.to_string();
        assert_eq!(trigger.resolved_action().as_deref(), Some("agent_rest"));
    }

    #[test]
    fn test_resolved_action_handles_legacy_and_malformed() {
        let mut trigger =
            CronTriggerPo::new(String::new(), "t".to_string(), TriggerType::Cron, 100, None);
        assert_eq!(trigger.resolved_action(), None);
        trigger.payload = "not-json".to_string();
        assert_eq!(trigger.resolved_action(), None);
    }

    #[test]
    fn test_window_and_runs_guards() {
        let mut trigger = CronTriggerPo::new(
            String::new(),
            "t".to_string(),
            TriggerType::Schedule,
            100,
            None,
        );
        assert!(trigger.within_window(100));
        trigger.start_at = Some(90);
        trigger.end_at = Some(110);
        assert!(trigger.within_window(100));
        assert!(!trigger.within_window(89));
        assert!(!trigger.within_window(111));
        assert!(!trigger.runs_exhausted());
        trigger.max_runs = Some(2);
        trigger.total_runs = 2;
        assert!(trigger.runs_exhausted());

        // 执行后判定：total_runs 不含本次，故 1 次历史 + 本次 = 达上限
        trigger.total_runs = 1;
        assert!(trigger.exhausts_after_current_run());
        trigger.total_runs = 0;
        assert!(!trigger.exhausts_after_current_run());
    }
}
