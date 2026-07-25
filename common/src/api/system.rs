//! System 域共享 API DTO - 系统健康指标聚合

use serde::{Deserialize, Serialize};

/// 系统健康指标聚合响应
///
/// 用于前端 HUD 仪表盘墙展示，由后端 `GET /api/v1/system/health/metrics` 返回。
/// 部分维度允许降级为 0（跨域获取成本高时）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMetricsResponse {
    /// 后端服务在线（handler 能响应即视为 true）
    pub backend_online: bool,
    /// AOP 队列总待处理数（所有消费者累加）
    pub aop_pending: u64,
    /// AOP 队列总处理中数（所有消费者累加）
    pub aop_in_progress: u64,
    /// 活跃 Agent 数（status != 0），降级为 0
    pub active_agents: u64,
    /// 总 Agent 数，降级为 0
    pub total_agents: u64,
    /// 活跃项目数，降级为 0
    pub active_projects: u64,
    /// 总项目数，降级为 0
    pub total_projects: u64,
    /// 待处理任务数（status != Done），降级为 0
    pub pending_tasks: u64,
    /// 总任务数，降级为 0
    pub total_tasks: u64,
    /// 进程运行时长（秒）
    pub uptime_secs: u64,
}
