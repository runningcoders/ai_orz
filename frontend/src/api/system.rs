//! System 域 API - 健康检查、定时触发器、备份管理、日志查询

use serde::{Deserialize, Serialize};

use super::{api_delete, api_get, api_get_or_default, api_post, api_post_empty, api_put, ApiError};

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

pub async fn create_cron_trigger(req: common::api::CreateCronTriggerRequest) -> Result<common::api::CreateCronTriggerResponse, ApiError> {
    api_post("/api/v1/system/cron-triggers", &req).await
}

pub async fn update_cron_trigger(id: &str, req: common::api::UpdateCronTriggerRequest) -> Result<common::api::UpdateCronTriggerResponse, ApiError> {
    api_put(&format!("/api/v1/system/cron-triggers/{}", id), &req).await
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
    api_post_empty(&format!("/api/v1/system/cron-triggers/{}/resume", id), &body).await
}

// ===== 备份管理 =====

/// 单个备份的元信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    /// 备份版本号（单调递增）
    pub version: u64,
    /// ISO8601 格式时间戳
    pub timestamp: String,
    /// 归档文件名
    pub file_name: String,
    /// 归档文件字节数
    pub size_bytes: u64,
    /// 归档文件 MD5（十六进制小写）
    pub md5: String,
}

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

/// 单条日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// ISO8601 格式时间戳
    pub timestamp: String,
    /// 日志级别（INFO / WARN / ERROR / DEBUG / TRACE）
    pub level: String,
    /// 日志消息
    pub message: String,
    /// 请求追踪 ID
    pub log_id: Option<String>,
    /// 用户 ID
    pub user_id: Option<String>,
    /// 操作名称
    pub operation: Option<String>,
    /// 原始 JSON 对象（用于展开查看完整信息）
    #[serde(default)]
    pub raw: Option<serde_json::Value>,
}

/// 分页结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogPageResult {
    /// 匹配总数
    pub total: usize,
    /// 当前页日志条目
    pub entries: Vec<LogEntry>,
    /// 当前页码
    pub page: usize,
    /// 每页条数
    pub page_size: usize,
}

/// 日志查询参数
#[derive(Debug, Clone, Default)]
pub struct LogQueryParams {
    /// 关键词（message 字段包含，不区分大小写）
    pub keyword: Option<String>,
    /// 调用链 ID 精确匹配
    pub log_id: Option<String>,
    /// 日志级别过滤（INFO / WARN / ERROR / DEBUG）
    pub level: Option<String>,
    /// 起始时间（unix timestamp ms，含）
    pub start_time: Option<i64>,
    /// 结束时间（unix timestamp ms，含）
    pub end_time: Option<i64>,
    /// 页码（从 1 开始）
    pub page: usize,
    /// 每页条数
    pub page_size: usize,
}

impl LogQueryParams {
    /// 构造 URL query string（不含前导 `?`）
    pub fn to_query_string(&self) -> String {
        let mut params: Vec<String> = Vec::new();
        if let Some(kw) = &self.keyword {
            let kw = kw.trim();
            if !kw.is_empty() {
                params.push(format!("keyword={}", url_encode(kw)));
            }
        }
        if let Some(id) = &self.log_id {
            let id = id.trim();
            if !id.is_empty() {
                params.push(format!("log_id={}", url_encode(id)));
            }
        }
        if let Some(lvl) = &self.level {
            let lvl = lvl.trim();
            if !lvl.is_empty() {
                params.push(format!("level={}", url_encode(lvl)));
            }
        }
        if let Some(start) = self.start_time {
            params.push(format!("start_time={}", start));
        }
        if let Some(end) = self.end_time {
            params.push(format!("end_time={}", end));
        }
        if self.page > 0 {
            params.push(format!("page={}", self.page));
        }
        if self.page_size > 0 {
            params.push(format!("page_size={}", self.page_size));
        }
        params.join("&")
    }
}

/// 查询日志
pub async fn query_logs(params: &LogQueryParams) -> Result<LogPageResult, ApiError> {
    let qs = params.to_query_string();
    let path = if qs.is_empty() {
        "/api/v1/system/logs".to_string()
    } else {
        format!("/api/v1/system/logs?{}", qs)
    };
    api_get(&path).await
}

// ===== AOP 队列监控 =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderKeyInfo {
    pub order_key: String,
    pub pending_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStatsResponse {
    pub consumer_name: String,
    pub pending_count: usize,
    pub in_progress_count: usize,
    pub order_keys: Vec<OrderKeyInfo>,
    pub oldest_event_age_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSummaryResponse {
    pub event_id: String,
    pub event_kind: String,
    pub order_key: String,
    pub priority: u8,
    pub created_at: i64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDetailResponse {
    pub event_id: String,
    pub event_kind: String,
    pub order_key: String,
    pub priority: u8,
    pub created_at: i64,
    pub status: String,
    pub payload_preview: String,
}

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
pub async fn list_events(consumer: &str, order_key: Option<&str>, status: Option<&str>, limit: usize, offset: usize) -> Result<Vec<EventSummaryResponse>, ApiError> {
    let mut params = Vec::new();
    if let Some(ok) = order_key {
        params.push(format!("order_key={}", url_encode(ok)));
    }
    if let Some(s) = status {
        params.push(format!("status={}", url_encode(s)));
    }
    if limit > 0 {
        params.push(format!("limit={}", limit));
    }
    if offset > 0 {
        params.push(format!("offset={}", offset));
    }
    let qs = params.join("&");
    let path = if qs.is_empty() {
        format!("/api/v1/system/aop/{}/events", consumer)
    } else {
        format!("/api/v1/system/aop/{}/events?{}", consumer, qs)
    };
    api_get_or_default(&path).await
}

/// 获取事件详情
pub async fn get_event(consumer: &str, event_id: &str) -> Result<EventDetailResponse, ApiError> {
    api_get(&format!("/api/v1/system/aop/{}/events/{}", consumer, event_id)).await
}

// ===== AOP 实时统计 =====

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AopStatsOverviewResponse {
    pub total_published: u64,
    pub total_consumed: u64,
    pub total_success: u64,
    pub total_failed: u64,
    pub avg_duration_ms: f64,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AopStatsTimeSeriesPoint {
    pub interval_start: i64,
    pub call_count: u64,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AopStatsTimeSeriesResponse {
    pub points: Vec<AopStatsTimeSeriesPoint>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AopStatsDistributionItem {
    pub label: String,
    pub value: u64,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AopStatsDistributionResponse {
    pub items: Vec<AopStatsDistributionItem>,
}

/// 获取 AOP 统计概览
pub async fn get_aop_stats_overview() -> Result<AopStatsOverviewResponse, ApiError> {
    api_get("/api/v1/system/aop/stats/overview").await
}

/// 获取 AOP 统计时序数据
pub async fn get_aop_stats_time_series(
    event_kind: Option<&str>,
    consumer_name: Option<&str>,
    status: Option<&str>,
) -> Result<AopStatsTimeSeriesResponse, ApiError> {
    let mut params = Vec::new();
    if let Some(v) = event_kind {
        params.push(format!("event_kind={}", url_encode(v)));
    }
    if let Some(v) = consumer_name {
        params.push(format!("consumer_name={}", url_encode(v)));
    }
    if let Some(v) = status {
        params.push(format!("status={}", url_encode(v)));
    }
    let qs = params.join("&");
    let path = if qs.is_empty() {
        "/api/v1/system/aop/stats/time-series".to_string()
    } else {
        format!("/api/v1/system/aop/stats/time-series?{}", qs)
    };
    api_get(&path).await
}

/// 获取 AOP 统计分布
pub async fn get_aop_stats_distribution(
    group_by: &str,
    status: Option<&str>,
) -> Result<AopStatsDistributionResponse, ApiError> {
    let mut path = format!("/api/v1/system/aop/stats/distribution?group_by={}", url_encode(group_by));
    if let Some(s) = status {
        path.push_str(&format!("&status={}", url_encode(s)));
    }
    api_get(&path).await
}

/// 简单的 URL 编码（与 api/message.rs 中的实现一致）
fn url_encode(s: &str) -> String {
    s.replace('%', "%25")
        .replace(' ', "%20")
        .replace('&', "%26")
        .replace('=', "%3D")
        .replace('+', "%2B")
        .replace('#', "%23")
        .replace('?', "%3F")
}
