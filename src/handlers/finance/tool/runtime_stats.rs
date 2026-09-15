//! Handler: GET /api/v1/finance/tools/runtime-stats - 组织级工具调用汇总
//!
//! 为工作台顶栏提供窗口内的工具调用读数（调用次数 / 失败次数 / 平均耗时）。
//! 数据源为 DuckDB `tool_call_events`；组织隔离由 DAL 层按 `ctx.organization_id`
//! 注入 filter（见 `ToolDal::tool_call_stats`）。
//!
//! 与 `/finance/model-providers/token-stats` 的分工：后者给模型的 Token 时序 +
//! 合计，本接口给工具的合计读数（不返回时序点）。
//!
//! 注意：统计事件是批次刷盘（缓冲满才 flush），故最近一段时间的调用可能尚未落库，
//! 读数偏低属预期表现，前端按固定间隔轮询即可。

use ai_orz_macros::generate_http_handler;
use common::api::{GetToolRuntimeStatsRequest, ToolRuntimeStatsResponse};
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain as finance_domain;

/// GET /api/v1/finance/tools/runtime-stats
#[generate_http_handler]
pub async fn get_tool_runtime_stats(
    ctx: RequestContext,
    params: GetToolRuntimeStatsRequest,
) -> Result<ToolRuntimeStatsResponse> {
    // clamp 在 DAL 内也会做一次（那里是唯一的权威口径），此处先算出来仅为回填响应字段
    let minutes = params.minutes.unwrap_or(60).clamp(1, 1440);

    let stats = finance_domain()
        .tool_provider_manage()
        .tool_call_stats(ctx, minutes)
        .await?;

    Ok(ToolRuntimeStatsResponse {
        window_minutes: minutes,
        total_calls: stats.call_summary.map(|c| c.total_calls).unwrap_or(0),
        failed_calls: stats.failed_count.unwrap_or(0),
        avg_duration_ms: stats.avg_duration_ms,
    })
}
