//! Handler: GET /api/v1/finance/model-providers/token-stats - 组织级 Token 消耗时序
//!
//! 为工作台顶栏提供分钟级模型调用时序（Token QPS 曲线）。
//! 数据源为 DuckDB `model_call_events`；组织隔离由 DAL 层按 `ctx.organization_id` 注入 filter。
//!
//! 注意：统计事件是批次刷盘（缓冲满才 flush），故最近 1~2 分钟的数据可能尚未落库，
//! 曲线末端偏低属预期表现，前端按固定间隔轮询即可。

use ai_orz_macros::generate_http_handler;
use common::api::{GetTokenStatsRequest, TokenStatsResponse};
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain as finance_domain;

/// GET /api/v1/finance/model-providers/token-stats
#[generate_http_handler]
pub async fn get_token_stats(
    ctx: RequestContext,
    params: GetTokenStatsRequest,
) -> Result<TokenStatsResponse> {
    let points = finance_domain()
        .model_provider_manage()
        .model_call_time_series(ctx, params.minutes.unwrap_or(60))
        .await?;

    Ok(TokenStatsResponse {
        total_tokens_input: points.iter().map(|p| p.tokens_input).sum(),
        total_tokens_output: points.iter().map(|p| p.tokens_output).sum(),
        points,
    })
}
