//! Handler: GET /api/v1/system/logs - 查询应用日志。
//!
//! 路由层 `require_role_middleware(UserRole::Admin)` 已确保 Admin/SuperAdmin 可访问。

use axum::{
    Json,
    extract::{Extension, Query},
};
use common::api::ApiResponse;
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::dal::log_query::{LogPageResult, LogQuery};
use crate::service::domain::system::domain;

/// 查询参数（从 URL query string 提取）
#[derive(serde::Deserialize)]
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
    /// 页码（从 1 开始，默认 1）
    pub page: Option<usize>,
    /// 每页条数（默认 20）
    pub page_size: Option<usize>,
}

pub async fn handler(
    Extension(ctx): Extension<RequestContext>,
    Query(params): Query<LogQueryParams>,
) -> Result<Json<ApiResponse<LogPageResult>>> {
    let query = LogQuery {
        keyword: params.keyword,
        log_id: params.log_id,
        level: params.level,
        start_time: params.start_time,
        end_time: params.end_time,
        page: params.page.unwrap_or(1),
        page_size: params.page_size.unwrap_or(20),
    };

    let result = domain()
        .log_query()
        .query_logs(ctx, query)
        .await?;

    Ok(Json(ApiResponse::success(result)))
}
