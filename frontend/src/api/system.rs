//! System 域 API - 健康检查、定时触发器、备份管理、日志查询
//!
//! 协议化改造：DTO 全部复用 `common::api` 共享定义（此前本地镜像了一份，存在漂移风险），
//! 此处 re-export 保持既有 `crate::api::system::*` 导入路径可用。

pub use common::api::{
    AopStatsDistributionItem, AopStatsDistributionResponse, AopStatsOverviewResponse,
    AopStatsTimeSeriesPoint, AopStatsTimeSeriesResponse, BackupInfo, CleanupToolLogsRequest,
    CleanupToolLogsResponse, EventDetailResponse, EventSummaryResponse, HealthMetricsResponse,
    LogEntry, QueryLogsResponse, QueueStatsResponse, ToolLogStorageResponse,
};

use super::{ApiError, api_delete, api_get, api_get_or_default, api_post, api_post_empty, api_put};

/// 健康检查（返回纯文本）
pub async fn check_health() -> Result<String, ApiError> {
    super::api_get_text("/health").await
}

// ===== 定时触发器 =====

pub async fn list_cron_triggers() -> Result<common::api::ListCronTriggersResponse, ApiError> {
    api_get_or_default("/api/v1/system/cron-triggers").await
}

pub async fn get_cron_trigger(id: &str) -> Result<common::api::GetCronTriggerResponse, ApiError> {
    api_get(&format!("/api/v1/system/cron-triggers/{}", id)).await
}

pub async fn create_cron_trigger(
    req: common::api::CreateCronTriggerRequest,
) -> Result<common::api::CreateCronTriggerResponse, ApiError> {
    api_post("/api/v1/system/cron-triggers", &req).await
}

pub async fn update_cron_trigger(
    req: common::api::UpdateCronTriggerRequest,
) -> Result<common::api::UpdateCronTriggerResponse, ApiError> {
    api_put(
        &format!("/api/v1/system/cron-triggers/{}", req.trigger_id),
        &req,
    )
    .await
}

pub async fn delete_cron_trigger(id: &str) -> Result<(), ApiError> {
    api_delete(&format!("/api/v1/system/cron-triggers/{}", id)).await
}

pub async fn pause_cron_trigger(id: &str) -> Result<(), ApiError> {
    let body = serde_json::json!({});
    api_post_empty(&format!("/api/v1/system/cron-triggers/{}/pause", id), &body).await
}

pub async fn resume_cron_trigger(id: &str) -> Result<(), ApiError> {
    let body = serde_json::json!({});
    api_post_empty(
        &format!("/api/v1/system/cron-triggers/{}/resume", id),
        &body,
    )
    .await
}

// ===== 备份管理 =====

/// 列出所有备份（按 version 降序）
pub async fn list_backups() -> Result<Vec<BackupInfo>, ApiError> {
    api_get_or_default("/api/v1/system/backups").await
}

/// 创建新备份，返回其元信息
pub async fn create_backup() -> Result<BackupInfo, ApiError> {
    let body = serde_json::json!({});
    api_post("/api/v1/system/backups", &body).await
}

/// 删除指定版本的备份
pub async fn delete_backup(version: u64) -> Result<(), ApiError> {
    api_delete(&format!("/api/v1/system/backups/{}", version)).await
}

/// 获取指定版本的恢复脚本（POST，返回纯文本 bash 脚本）
///
/// 后端以 text/plain 返回，不经过 ApiResponse 包装，
/// 因此直接使用底层 reqwest 客户端处理。
pub async fn get_restore_script(version: u64) -> Result<String, ApiError> {
    let path = format!("/api/v1/system/backups/{}/restore", version);
    let url = crate::config::current_config().api_url(&path);
    let body = serde_json::json!({});
    let resp = super::client()
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(super::network_err)?;
    let status = resp.status();
    if !status.is_success() {
        super::handle_unauthorized(status.as_u16());
        return Err(super::parse_error_response(resp).await);
    }
    resp.text().await.map_err(|e| ApiError {
        http_status: 200,
        error_code: None,
        message: e.to_string(),
    })
}

// ===== 日志查询 =====

/// 查询日志
pub async fn query_logs(req: &common::api::LogQueryRequest) -> Result<QueryLogsResponse, ApiError> {
    let qs = super::build_query_string(&[
        ("keyword", req.keyword.clone()),
        ("log_id", req.log_id.clone()),
        ("level", req.level.clone()),
        ("start_time", req.start_time.map(|v| v.to_string())),
        ("end_time", req.end_time.map(|v| v.to_string())),
        ("page", req.page.map(|v| v.to_string())),
        ("page_size", req.page_size.map(|v| v.to_string())),
    ]);
    api_get(&format!("/api/v1/system/logs{}", qs)).await
}

// ===== AOP 队列监控 =====

/// 获取所有队列统计
pub async fn get_all_queue_stats() -> Result<Vec<QueueStatsResponse>, ApiError> {
    api_get_or_default("/api/v1/system/aop/stats").await
}

/// 获取指定消费者队列统计
#[allow(dead_code)]
pub async fn get_queue_stats(consumer: &str) -> Result<QueueStatsResponse, ApiError> {
    api_get(&format!("/api/v1/system/aop/{}/stats", consumer)).await
}

/// 查询事件列表
pub async fn list_events(
    req: common::api::ListEventsRequest,
) -> Result<Vec<EventSummaryResponse>, ApiError> {
    let qs = super::build_query_string(&[
        ("order_key", req.order_key.clone()),
        ("status", req.status.clone()),
        ("limit", req.limit.map(|v| v.to_string())),
        ("offset", req.offset.map(|v| v.to_string())),
    ]);
    let url = format!("/api/v1/system/aop/{}/events{}", req.consumer, qs);
    api_get_or_default(&url).await
}

/// 获取事件详情
pub async fn get_event(req: common::api::GetEventRequest) -> Result<EventDetailResponse, ApiError> {
    api_get(&format!(
        "/api/v1/system/aop/{}/events/{}",
        req.consumer, req.event_id
    ))
    .await
}

// ===== AOP 实时统计 =====

/// 获取 AOP 统计概览
pub async fn get_aop_stats_overview() -> Result<AopStatsOverviewResponse, ApiError> {
    api_get("/api/v1/system/aop/stats/overview").await
}

/// 获取 AOP 统计时序数据
pub async fn get_aop_stats_time_series(
    req: common::api::GetStatsTimeSeriesRequest,
) -> Result<AopStatsTimeSeriesResponse, ApiError> {
    let qs = super::build_query_string(&[
        ("event_kind", req.event_kind.clone()),
        ("consumer_name", req.consumer_name.clone()),
        ("status", req.status.clone()),
    ]);
    api_get(&format!("/api/v1/system/aop/stats/time-series{}", qs)).await
}

/// 获取 AOP 统计分布
pub async fn get_aop_stats_distribution(
    req: common::api::GetStatsDistributionRequest,
) -> Result<AopStatsDistributionResponse, ApiError> {
    let qs = super::build_query_string(&[
        ("group_by", Some(req.group_by.clone())),
        ("status", req.status.clone()),
    ]);
    api_get(&format!("/api/v1/system/aop/stats/distribution{}", qs)).await
}

// ===== 系统健康指标（HUD 仪表盘墙用） =====

/// 获取系统健康指标（HUD 仪表盘墙用）
pub async fn get_health_metrics() -> Result<HealthMetricsResponse, ApiError> {
    api_get("/api/v1/system/health/metrics").await
}

// ===== 工具日志存储监控与清理（① 运行时输出层治理） =====

/// 工具日志存储占用统计（按天分区 + 保留策略）
pub async fn get_tool_log_storage() -> Result<ToolLogStorageResponse, ApiError> {
    api_get("/api/v1/system/storage/tool-logs").await
}

/// 手动清理超期工具日志（retention_days 缺省读服务端 [tool_log] 配置）
pub async fn cleanup_tool_logs(
    req: common::api::CleanupToolLogsRequest,
) -> Result<CleanupToolLogsResponse, ApiError> {
    api_post("/api/v1/system/storage/tool-logs/cleanup", &req).await
}

// ===== 统一后台进程管理（shell_list / shell_status / shell_kill 的 HTTP 面） =====

/// 列出后台进程（可见范围由后端按调用方身份过滤）
pub async fn list_processes() -> Result<common::api::ListProcessesResponse, ApiError> {
    api_get("/api/v1/system/processes").await
}

/// 查询单个进程状态（探活 + 日志尾部）
pub async fn get_process_status(
    pid: u32,
    tail_lines: Option<usize>,
) -> Result<common::api::ShellStatusResponse, ApiError> {
    let qs = super::build_query_string(&[("tail_lines", tail_lines.map(|v| v.to_string()))]);
    api_get(&format!("/api/v1/system/processes/{}{}", pid, qs)).await
}

/// 终止进程（killed=false 表示进程已退出）
pub async fn kill_process(pid: u32) -> Result<common::api::ShellKillResponse, ApiError> {
    api_post(
        &format!("/api/v1/system/processes/{}/kill", pid),
        &serde_json::json!({}),
    )
    .await
}
