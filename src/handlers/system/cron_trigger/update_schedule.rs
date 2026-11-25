//! Handler：更新日程（Agent 工具 + HTTP 接口）。
//!
//! 本层只做 DTO → 领域参数映射；归属校验、可用字段投影、规则校验与
//! `next_run_at` 重算都在 `domain::system::CronManager::update_schedule`。

use crate::pkg::RequestContext;
use crate::service::domain::system::{ScheduleUpdate, domain as system_domain};
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateScheduleRequest, UpdateScheduleResponse};
use common::error::Result;

use super::response::to_detail;

/// HTTP 参数 → 领域更新参数。身份（owner）刻意不在参数里，由 domain 从 ctx 固化。
fn to_update(params: UpdateScheduleRequest) -> ScheduleUpdate {
    ScheduleUpdate {
        trigger_id: params.trigger_id,
        title: params.title,
        description: params.description,
        cron_expression: params.cron_expression,
        interval_seconds: params.interval_seconds,
        run_at: params.run_at,
        start_at: params.start_at,
        end_at: params.end_at,
        max_runs: params.max_runs.map(|v| v.map(i64::from)),
        agent_id: params.agent_id,
    }
}

/// 更新自己创建的一条日程。
///
/// 只能改标题、描述、时间规则、生效窗口与最大次数；归属、行为模板、统计字段
/// 一律不接受修改。改了时间规则会按新规则重算下一次触发时刻。
#[register_handler_tool(
    id = "update_schedule",
    name = "Update Schedule",
    description = "Update a schedule owned by the current user (any agent acting for that user may edit it; creator does not grant permission). Pass trigger_id plus only the fields to change: title, description, cron_expression (5-field cron), interval_seconds, run_at (Unix ms), start_at/end_at window, max_runs (positive integer), agent_id (reassign the responsible agent). start_at / end_at / max_runs are tri-state: omit to keep, pass null to clear, pass a value to override. Passing any time rule field (cron_expression / interval_seconds / run_at) replaces the whole time rule and recomputes next_run_at. Owner, behavior template and run statistics cannot be changed. System maintenance triggers and other users' schedules are not visible.",
    params = "common::api::UpdateScheduleRequest",
    neural,
    tags = "scheduling,autonomy"
)]
#[generate_http_handler]
pub async fn update_schedule(
    ctx: RequestContext,
    params: UpdateScheduleRequest,
) -> Result<UpdateScheduleResponse> {
    // 改派负责 Agent 属于跨域编排：校验「存在且在役」由 handler 做，
    // 领域层不引用 hr（ARCHITECTURE 原则 1）。
    if let Some(agent_id) = params
        .agent_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        super::agent_guard::ensure_onboarded(&ctx, agent_id).await?;
    }
    let updated = system_domain()
        .cron_manager()
        .update_schedule(ctx, to_update(params))
        .await?;
    Ok(to_detail(&updated))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_all_fields_to_domain() {
        let p = UpdateScheduleRequest {
            trigger_id: "sch-1".into(),
            title: Some("新标题".into()),
            description: Some("新描述".into()),
            cron_expression: Some("0 9 * * *".into()),
            interval_seconds: Some(600),
            run_at: Some(123),
            start_at: Some(Some(10)),
            end_at: Some(None),
            max_runs: Some(Some(5)),
            agent_id: Some("agent-2".into()),
        };
        let u = to_update(p);
        assert_eq!(u.trigger_id, "sch-1");
        assert_eq!(u.title.as_deref(), Some("新标题"));
        assert_eq!(u.interval_seconds, Some(600));
        // 三态原样透传：Some(Some(v)) 覆盖、Some(None) 清除、None 保持
        assert_eq!(u.start_at, Some(Some(10)));
        assert_eq!(u.end_at, Some(None));
        assert_eq!(u.max_runs, Some(Some(5)));
        assert_eq!(u.agent_id.as_deref(), Some("agent-2"));
    }
}
