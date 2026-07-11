//! Cron Trigger 模型
//!
//! CronTrigger 是定时触发器配置，管理定时任务的触发规则。

use common::enums::TriggerType;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// CronTriggerPo 持久化对象。
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct CronTriggerPo {
    pub id: String,
    pub name: String,
    pub trigger_type: TriggerType,
    pub cron_expression: Option<String>,
    pub interval_seconds: Option<i64>,
    pub run_at: Option<i64>,
    pub next_run_at: i64,
    pub is_enabled: i32,
    pub payload: String,
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
            Uuid::now_v7().to_string()
        } else {
            id
        };
        let now = common::constants::utils::current_timestamp();
        Self {
            id,
            name,
            trigger_type,
            cron_expression: None,
            interval_seconds: None,
            run_at: None,
            next_run_at,
            is_enabled: 1,
            payload: "{}".to_string(),
            last_run_at: None,
            created_at: now,
            updated_at: now,
            created_by: creator.clone(),
            updated_by: creator,
        }
    }

    pub fn touch(&mut self, modifier: Option<String>) {
        self.updated_at = common::constants::utils::current_timestamp();
        self.updated_by = modifier;
    }
}
