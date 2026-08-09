//! Handler: GET /api/v1/system/logs - 查询应用日志。
//!
//! 路由层 `require_role_middleware(UserRole::Admin)` 已确保 Admin/SuperAdmin 可访问。

use ai_orz_macros::generate_http_handler;
use common::api::{LogQueryRequest, QueryLogsResponse};
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::dal::log_query::LogQuery;
use crate::service::domain::system::domain;

#[generate_http_handler]
pub async fn query_logs(
    ctx: RequestContext,
    params: LogQueryRequest,
) -> Result<QueryLogsResponse> {
    let query = LogQuery {
        keyword: params.keyword,
        log_id: params.log_id,
        level: params.level,
        start_time: params.start_time,
        end_time: params.end_time,
        page: params.page.unwrap_or(1),
        page_size: params.page_size.unwrap_or(20),
    };

    let result = domain().log_query().query_logs(ctx, query).await?;

    Ok(result)
}
