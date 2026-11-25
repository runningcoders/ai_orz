//! Handler 工具：Agent 自主创建日程
//!
//! 与系统级 `create_cron_trigger` 的区别：
//! - 本工具产生**带 Who 组身份**的日程记录（归属用户 / 创建者 / 负责 Agent）
//! - 身份一律从 `RequestContext` 固化，**不接受模型传入**，杜绝越权；
//!   唯一的业务字段 `agent_id`（负责 Agent）可以指定，但要过「存在且在役」校验
//! - 只产出异步行为模板（`AsyncNotify` / `AsyncAgentWakeup`），系统维护类走管理接口

use crate::handlers::system::cron_trigger::response::to_detail;
use crate::pkg::RequestContext;
use crate::service::domain::system::domain as system_domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ScheduleAgentTaskRequest, ScheduleAgentTaskResponse};
use common::enums::{CreatorType, TriggerBehavior, TriggerType};
use common::error::{Result, bail_err};

/// 计算日程的下一次触发时间：cron > interval > run_at 三选一
///
/// 优先级与 `create_cron_trigger` 的 Schedule 分支一致（A-设计 §7）。
/// 抽成纯函数是为了单测——时间规则错一位就是「日程永不到点」。
fn resolve_next_run_at(params: &ScheduleAgentTaskRequest, now: i64) -> Result<i64> {
    if let Some(expression) = params
        .cron_expression
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        let timezone = crate::config::timezone();
        return common::schedule::next_run_at(expression, &timezone, chrono::Utc::now());
    }
    // 显式给了 interval 就按 interval 校验：非正数 / 溢出直接报错，
    // 不再静默回落到 run_at（那会让「间隔写错」变成「只跑一次」）。
    if let Some(interval) = params.interval_seconds {
        return common::schedule::interval_next_run_at(now, interval);
    }
    params.run_at.ok_or_else(|| {
        common::error::err!(
            InvalidRequest,
            "one of cron_expression / interval_seconds / run_at is required for Schedule"
        )
    })
}

/// 归属用户兜底：优先上下文 uid；没有归属时，创建者是 User 就用它兜底。
///
/// 返回 `None` 表示无法确定归属（创建者是 Agent / System 且上下文没有用户），
/// 调用方必须拒绝创建——没有归属的日程到点无法推送、也无法还原工具凭据。
fn resolve_owner_user_id(uid: &str, creator_type: CreatorType, creator_id: &str) -> Option<String> {
    if !uid.is_empty() {
        return Some(uid.to_string());
    }
    (creator_type == CreatorType::User && !creator_id.is_empty()).then(|| creator_id.to_string())
}

/// Agent 自主创建日程
///
/// 到点后按行为模板派发：`AsyncAgentWakeup` 唤醒 `agent_id`（负责 Agent）执行
/// `description`，`AsyncNotify` 由负责 Agent 给归属用户推送提醒。
/// 归属用户取当前 `ctx.uid()`（root_user_id 还原）；负责 Agent 缺省为创建者自己。
#[register_handler_tool(
    id = "schedule_agent_task",
    name = "Schedule Agent Task",
    description = "Create a schedule for an agent to autonomously wake up and act at a later time. Provide a title, the full user intent (description), and one time rule: cron_expression (5-field cron), interval_seconds, or run_at (Unix ms). Optionally set start_at/end_at window, agent_id (the responsible agent; defaults to yourself when you are an agent), behavior (AsyncAgentWakeup by default, or AsyncNotify to remind the user instead), and max_runs (positive integer). Identity fields (owner, org, creator) are taken from the request context and cannot be overridden. Returns the created schedule detail with its id.",
    params = "common::api::ScheduleAgentTaskRequest",
    neural,
    tags = "scheduling,autonomy"
)]
#[generate_http_handler]
pub async fn schedule_agent_task(
    ctx: RequestContext,
    params: ScheduleAgentTaskRequest,
) -> Result<ScheduleAgentTaskResponse> {
    // 身份固化：一律从 ctx 取，模型传参不可覆盖（A-设计 §5.2）。
    if params.title.trim().is_empty() {
        bail_err!(InvalidRequest, "日程标题不能为空");
    }

    // 创建者（谁安排的）：以 ctx 的 Agent 身份为准（后台唤醒场景 caller_type=System，
    // 但执行者是被唤醒的 Agent，可靠判据是 agent_id.is_some()）；HTTP 用户调用时是用户。
    let sender_id = ctx.message_sender_id();
    let (creator_type, creator_id) = if ctx.agent_id().is_some() {
        (CreatorType::Agent, sender_id)
    } else if ctx.caller_type() == common::enums::CallerType::User {
        (CreatorType::User, sender_id)
    } else {
        (CreatorType::System, sender_id)
    };

    // 归属用户（算谁的 / 推给谁）：优先 `ctx.uid()`（root_user_id 还原）。
    // 上下文没有归属时用创建者兜底——创建者是 User，就归它：没有更外层委托
    // 关系时，「谁安排的」就是归属人。创建者是 Agent/System 且无归属则拒绝：
    // 日程落库后 root_user_id="" 会导致 SSE 不推、工具凭据丢失。
    let Some(owner_user_id) = resolve_owner_user_id(&ctx.uid(), creator_type, &creator_id) else {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文，无法创建日程");
    };

    let now = common::constants::utils::current_timestamp_ms();
    let next_run_at = resolve_next_run_at(&params, now)?;

    // 行为白名单：用户接口只能产出异步两族，SyncMaintenance 仅系统 seed 可建。
    let behavior = params.behavior.unwrap_or(TriggerBehavior::AsyncAgentWakeup);
    if matches!(behavior, TriggerBehavior::SyncMaintenance) {
        bail_err!(
            InvalidRequest,
            "schedule_agent_task 不支持 SyncMaintenance 模板（仅系统维护触发器可用）"
        );
    }
    // 窗口校验（A-设计 §7）
    if let (Some(start), Some(end)) = (params.start_at, params.end_at)
        && end <= start
    {
        bail_err!(InvalidRequest, "end_at 必须大于 start_at");
    }

    // 负责 Agent（谁来干）：显式参数优先（校验存在且在役）；上下文没有负责
    // Agent 时用创建者兜底——创建者是 Agent，就由它自己负责。
    // 创建者是 User / System 时仍为空：`AsyncAgentWakeup` 必须显式指定（否则
    // 到点无人可唤醒）；`AsyncNotify` 允许为空，表示由系统哨兵发提醒
    // （`send_to_user` 会按 ID 推导出 `from_role=System`，不会身份错位）。
    let agent_id = match params
        .agent_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(id) => {
            super::agent_guard::ensure_onboarded(&ctx, id).await?;
            id.to_string()
        }
        None if creator_type == CreatorType::Agent && !creator_id.is_empty() => creator_id.clone(),
        None => String::new(),
    };
    if behavior == TriggerBehavior::AsyncAgentWakeup && agent_id.is_empty() {
        bail_err!(
            InvalidRequest,
            "无法确定负责 Agent：创建者不是 Agent 时必须显式提供 agent_id"
        );
    }

    let name = params.title.clone();
    let action = params
        .action
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| name.clone());
    let org_id = ctx.organization_id().cloned();

    let mut trigger = crate::models::cron_trigger::CronTriggerPo::new(
        common::new_id!(CronTrigger),
        name,
        TriggerType::Schedule,
        next_run_at,
        Some(creator_id.clone()),
    );
    trigger.cron_expression = params.cron_expression;
    trigger.interval_seconds = params.interval_seconds;
    trigger.run_at = params.run_at;
    trigger.start_at = params.start_at;
    trigger.end_at = params.end_at;
    trigger.org_id = org_id;
    trigger.owner_user_id = Some(owner_user_id);
    trigger.creator_type = Some(creator_type.to_i32());
    trigger.creator_id = Some(creator_id);
    trigger.agent_id = (!agent_id.is_empty()).then_some(agent_id);
    trigger.description = params.description;
    trigger.action = Some(action);
    trigger.behavior = behavior;
    trigger.max_runs = params.max_runs.map(i64::from);
    // payload 只做兼容容器；新记录的业务动作写顶层 action 列
    trigger.payload = "{}".to_string();

    system_domain()
        .cron_manager()
        .create_trigger(ctx.clone(), &trigger)
        .await?;

    let stored = system_domain()
        .cron_manager()
        .get_trigger(ctx, &trigger.id)
        .await?
        .unwrap_or(trigger);
    Ok(to_detail(&stored))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> ScheduleAgentTaskRequest {
        ScheduleAgentTaskRequest {
            title: "晨间简报".to_string(),
            description: "汇总昨日进度并汇报".to_string(),
            cron_expression: None,
            interval_seconds: None,
            run_at: Some(1_800_000_000_000),
            start_at: None,
            end_at: None,
            agent_id: None,
            behavior: None,
            action: None,
            max_runs: None,
        }
    }

    #[test]
    fn resolve_run_at_when_only_timestamp() {
        assert_eq!(
            resolve_next_run_at(&params(), 1000).unwrap(),
            1_800_000_000_000
        );
    }

    #[test]
    fn resolve_interval_beats_run_at() {
        let mut p = params();
        p.interval_seconds = Some(60);
        assert_eq!(resolve_next_run_at(&p, 1000).unwrap(), 61_000);
    }

    #[test]
    fn resolve_rejects_all_missing() {
        let mut p = params();
        p.run_at = None;
        assert!(resolve_next_run_at(&p, 1000).is_err());
    }

    #[test]
    fn resolve_rejects_non_positive_interval() {
        let mut p = params();
        p.run_at = None;
        p.interval_seconds = Some(0);
        assert!(resolve_next_run_at(&p, 1000).is_err());
    }

    #[test]
    fn owner_prefers_context_then_user_creator() {
        // 上下文有归属 → 直接用，不被创建者覆盖
        assert_eq!(
            resolve_owner_user_id("user-ctx", CreatorType::Agent, "agent-1").as_deref(),
            Some("user-ctx")
        );
        // 上下文无归属、创建者是 User → 创建者兜底
        assert_eq!(
            resolve_owner_user_id("", CreatorType::User, "user-1").as_deref(),
            Some("user-1")
        );
        // 创建者是 Agent / System 且无归属 → 无法确定，必须拒绝
        assert_eq!(
            resolve_owner_user_id("", CreatorType::Agent, "agent-1"),
            None
        );
        assert_eq!(
            resolve_owner_user_id("", CreatorType::System, "system"),
            None
        );
    }
}
