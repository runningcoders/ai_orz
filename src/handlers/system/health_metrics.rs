//! Handler: GET /api/v1/system/health/metrics - 系统健康指标聚合
//!
//! 为前端 HUD 仪表盘墙提供单一聚合端点，避免前端并发请求多个域接口。
//!
//! 实际填充维度：
//! - `backend_online`: handler 能响应即 true
//! - `aop_pending` / `aop_in_progress`: 通过 SystemDomain.aop_monitor() 聚合所有消费者队列
//! - `uptime_secs`: 通过全局 OnceLock<Instant> 在首次调用时初始化（近似进程运行时长）
//! - `active_agents` / `total_agents`: 通过 HrDomain.agent_manage() 统计
//! - `active_projects` / `total_projects`: 通过 ProjectDomain.project_manage() 统计
//! - `pending_tasks` / `total_tasks`: 通过 ProjectDomain.task_manage() 统计

use axum::Json;
use common::api::{ApiResponse, HealthMetricsResponse};
use std::sync::OnceLock;
use std::time::Instant;

use common::enums::{AgentStatus, ProjectStatus, TaskStatus};
use crate::pkg::RequestContext;
use crate::service::dao::agent::AgentQuery;
use crate::service::dao::project::ProjectQuery;
use crate::service::dao::task::TaskQuery;
use crate::service::domain::hr::domain as hr_domain;
use crate::service::domain::project::domain as project_domain;
use crate::service::domain::system::domain;

/// 全局启动时间锚点：首次调用 get_health_metrics 时初始化。
/// 严格来说这是"首次健康指标请求时间"而非进程启动时间，
/// 但作为 HUD 展示用途足够准确，避免侵入 lib.rs 启动流程。
static START_TIME: OnceLock<Instant> = OnceLock::new();

/// GET /api/v1/system/health/metrics
pub async fn get_health_metrics() -> Json<ApiResponse<HealthMetricsResponse>> {
    let start = *START_TIME.get_or_init(Instant::now);
    let uptime_secs = start.elapsed().as_secs();

    // 聚合 AOP 队列：通过 SystemDomain.aop_monitor() 拿所有消费者快照
    let all_stats = domain().aop_monitor().all_queue_stats();
    let mut aop_pending: u64 = 0;
    let mut aop_in_progress: u64 = 0;
    for (_, s) in all_stats.iter() {
        aop_pending = aop_pending.saturating_add(s.pending_count as u64);
        aop_in_progress = aop_in_progress.saturating_add(s.in_progress_count as u64);
    }

    // 系统级调用约定：无 user/agent 上下文
    let ctx = RequestContext::new(None, None);

    // Agents：total 排除 Deleted，active 仅 Onboarded
    let total_agents = hr_domain()
        .agent_manage()
        .count_agents(
            ctx.clone(),
            AgentQuery {
                exclude_status: Some(AgentStatus::Deleted),
                ..Default::default()
            },
        )
        .await
        .unwrap_or(0);

    let active_agents = hr_domain()
        .agent_manage()
        .count_agents(
            ctx.clone(),
            AgentQuery {
                status: Some(AgentStatus::Onboarded),
                ..Default::default()
            },
        )
        .await
        .unwrap_or(0);

    // Projects：total 默认（DAO 自动加 status != 0），active 限定 Active/PendingReview/InProgress
    let total_projects = project_domain()
        .project_manage()
        .count_projects(ctx.clone(), ProjectQuery::default())
        .await
        .unwrap_or(0);

    let active_projects = project_domain()
        .project_manage()
        .count_projects(
            ctx.clone(),
            ProjectQuery {
                status_in: Some(vec![
                    ProjectStatus::Active,
                    ProjectStatus::PendingReview,
                    ProjectStatus::InProgress,
                ]),
                ..Default::default()
            },
        )
        .await
        .unwrap_or(0);

    // Tasks：total 默认（DAO 自动加 status != 0），pending 限定 PendingReview/Pending/InProgress
    let total_tasks = project_domain()
        .task_manage()
        .count_tasks(ctx.clone(), TaskQuery::default())
        .await
        .unwrap_or(0);

    let pending_tasks = project_domain()
        .task_manage()
        .count_tasks(
            ctx,
            TaskQuery {
                status_in: Some(vec![
                    TaskStatus::PendingReview,
                    TaskStatus::Pending,
                    TaskStatus::InProgress,
                ]),
                ..Default::default()
            },
        )
        .await
        .unwrap_or(0);

    Json(ApiResponse::success(HealthMetricsResponse {
        backend_online: true,
        aop_pending,
        aop_in_progress,
        active_agents,
        total_agents,
        active_projects,
        total_projects,
        pending_tasks,
        total_tasks,
        uptime_secs,
    }))
}
