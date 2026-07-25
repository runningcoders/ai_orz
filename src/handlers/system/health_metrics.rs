//! Handler: GET /api/v1/system/health/metrics - 系统健康指标聚合
//!
//! 为前端 HUD 仪表盘墙提供单一聚合端点，避免前端并发请求多个域接口。
//!
//! 实际填充维度：
//! - `backend_online`: handler 能响应即 true
//! - `aop_pending` / `aop_in_progress`: 通过 SystemDomain.aop_monitor() 聚合所有消费者队列
//! - `uptime_secs`: 通过全局 OnceLock<Instant> 在首次调用时初始化（近似进程运行时长）
//!
//! 降级为 0 的维度（跨域获取成本高，按计划文档约束先返回 0）：
//! - `active_agents` / `total_agents`
//! - `active_projects` / `total_projects`
//! - `pending_tasks` / `total_tasks`

use axum::Json;
use common::api::{ApiResponse, HealthMetricsResponse};
use std::sync::OnceLock;
use std::time::Instant;

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

    // 跨域维度降级为 0：active_agents/total_agents/active_projects/total_projects/pending_tasks/total_tasks
    // 这些维度需要跨 hr/project 域调用，成本较高；前端 UI 仍可正常渲染（显示 0/0）。
    Json(ApiResponse::success(HealthMetricsResponse {
        backend_online: true,
        aop_pending,
        aop_in_progress,
        active_agents: 0,
        total_agents: 0,
        active_projects: 0,
        total_projects: 0,
        pending_tasks: 0,
        total_tasks: 0,
        uptime_secs,
    }))
}
