//! Handler: GET /api/v1/system/workspace/metrics - 工作台顶栏聚合指标
//!
//! 为工作台顶栏提供单一聚合端点：项目/Agent 概览 + 运行态三色计数 +
//! 模型/工具窗口读数 + AOP 队列积压，一个响应覆盖顶栏全部数字，
//! 前端 30 秒单轮询即可，所有数字出自同一份快照。
//!
//! 数据源分工（全部复用各域现成能力，本 handler 只做编排）：
//! - 概览计数：project / hr domain 的 count 查询（SQLite）
//! - 运行态三色计数：runtime domain 内存态（权威实时，未注册运行的 Agent 不计入）
//! - 模型/工具读数：finance domain 的 DuckDB 统计（批次刷盘，最近 1~2 分钟
//!   未落库属预期，读数偏低不是故障）
//! - 队列积压：system domain 的 AOP monitor 内存态
//!
//! 降级策略：单维度失败按 0 呈现并记 error 日志，不影响整体快照返回。

use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{
    AgentRuntimeCounts, GetWorkspaceMetricsRequest, ModelUsageMetrics, ToolUsageMetrics,
    WorkspaceMetricsResponse,
};
use common::enums::{AgentRuntimeState, AgentStatus, ProjectStatus};
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::dao::agent::AgentQuery;
use crate::service::dao::project::ProjectQuery;
use crate::service::domain::finance::domain as finance_domain;
use crate::service::domain::hr::domain as hr_domain;
use crate::service::domain::project::domain as project_domain;
use crate::service::domain::runtime::domain as runtime_domain;
use crate::service::domain::system::domain as system_domain;

/// 查询工作台顶栏聚合指标（项目/Agent 概览 + 运行态计数 + 窗口统计 + 队列积压）
#[register_handler_tool(
    id = "get_workspace_metrics",
    name = "Get Workspace Metrics",
    description = "Get aggregated workspace top-bar metrics in one snapshot: project/agent counts, active project count, agent runtime state counts (idle/busy/resting), model token usage and tool call stats within a time window, and AOP queue backlog. Use it as a unified monitoring readout instead of polling multiple endpoints.",
    params = "common::api::GetWorkspaceMetricsRequest",
    tags = "system,monitor,query"
)]
#[generate_http_handler]
pub async fn get_workspace_metrics(
    ctx: RequestContext,
    params: GetWorkspaceMetricsRequest,
) -> Result<WorkspaceMetricsResponse> {
    let minutes = params.minutes.unwrap_or(60).clamp(1, 1440);

    // 概览计数（SQLite；项目 DAO 默认排除 Deleted）
    let project_count = project_domain()
        .project_manage()
        .count_projects(ctx.clone(), ProjectQuery::default())
        .await
        .unwrap_or(0);

    // 运行中项目：status 1..=3，与前端 is_active_project 同口径
    let active_project_count = project_domain()
        .project_manage()
        .count_projects(
            ctx.clone(),
            ProjectQuery {
                status_in: Some(vec![
                    ProjectStatus::InProgress,
                    ProjectStatus::Completed,
                    ProjectStatus::Archived,
                ]),
                ..Default::default()
            },
        )
        .await
        .unwrap_or(0);

    let agent_count = hr_domain()
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

    // 运行态三色计数（内存实时）
    let mut runtime = AgentRuntimeCounts::default();
    for (_, info) in runtime_domain().list_runtime_agents(None, None, None) {
        match info.state {
            AgentRuntimeState::Idle => runtime.idle += 1,
            AgentRuntimeState::Busy => runtime.busy += 1,
            AgentRuntimeState::Resting => runtime.resting += 1,
        }
    }

    // 模型/工具窗口读数（DuckDB 批次刷盘；失败降级为 0 并记日志）
    let model = match finance_domain()
        .model_provider_manage()
        .model_call_time_series(ctx.clone(), minutes)
        .await
    {
        Ok(points) => ModelUsageMetrics {
            total_calls: points.iter().map(|p| p.call_count).sum(),
            total_tokens_input: points.iter().map(|p| p.tokens_input).sum(),
            total_tokens_output: points.iter().map(|p| p.tokens_output).sum(),
        },
        Err(e) => {
            log_error!(&ctx, "get_workspace_metrics", "model stats degraded: {}", e);
            ModelUsageMetrics::default()
        }
    };

    let tool = match finance_domain()
        .tool_provider_manage()
        .tool_call_stats(ctx.clone(), minutes)
        .await
    {
        Ok(stats) => ToolUsageMetrics {
            total_calls: stats.call_summary.map(|c| c.total_calls).unwrap_or(0),
            failed_calls: stats.failed_count.unwrap_or(0),
            avg_duration_ms: stats.avg_duration_ms,
        },
        Err(e) => {
            log_error!(&ctx, "get_workspace_metrics", "tool stats degraded: {}", e);
            ToolUsageMetrics::default()
        }
    };

    // AOP 队列积压（内存实时，所有消费者累加）
    let mut queue_backlog: u64 = 0;
    for (_, s) in system_domain().aop_monitor().all_queue_stats().iter() {
        queue_backlog = queue_backlog.saturating_add(s.pending_count as u64);
    }

    Ok(WorkspaceMetricsResponse {
        project_count,
        agent_count,
        active_project_count,
        runtime,
        window_minutes: minutes,
        model,
        tool,
        queue_backlog,
    })
}
