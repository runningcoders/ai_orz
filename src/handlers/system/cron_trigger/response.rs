//! Cron Trigger 请求/响应 DTO —— 全部复用 `common::api::cron_trigger` 单一事实源。
//!
//! 这里不再重复定义结构体（之前与 common 同名同形两份，长期易漂移），只保留
//! 依赖后端 PO 的 `to_detail` 转换函数。所有请求/响应类型由 common 经 `pub use` 再次导出，
//! 既有 `super::response::*` 导入路径保持不变。

use crate::models::cron_trigger::CronTriggerPo;
use common::enums::CreatorType;

pub use common::api::cron_trigger::*;

pub(super) fn to_detail(trigger: &CronTriggerPo) -> common::api::CronTriggerDetail {
    common::api::CronTriggerDetail {
        id: trigger.id.clone(),
        name: trigger.name.clone(),
        trigger_type: trigger.trigger_type,
        cron_expression: trigger.cron_expression.clone(),
        interval_seconds: trigger.interval_seconds,
        run_at: trigger.run_at,
        next_run_at: trigger.next_run_at,
        is_enabled: trigger.is_enabled == 1,
        payload: trigger.payload.clone(),
        last_run_at: trigger.last_run_at,
        created_at: trigger.created_at,
        updated_at: trigger.updated_at,
        created_by: trigger.created_by.clone(),
        updated_by: trigger.updated_by.clone(),
        start_at: trigger.start_at,
        end_at: trigger.end_at,
        org_id: trigger.org_id.clone(),
        owner_user_id: trigger.owner_user_id.clone(),
        creator_type: trigger.creator_type.map(CreatorType::from),
        creator_id: trigger.creator_id.clone(),
        agent_id: trigger.agent_id.clone(),
        description: trigger.description.clone(),
        action: trigger.resolved_action(),
        behavior: trigger.behavior,
        max_runs: trigger.max_runs,
        total_runs: trigger.total_runs,
        last_error: trigger.last_error.clone(),
    }
}
