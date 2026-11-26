//! Handler: GET /api/v1/system/cron-triggers/{trigger_id}/exec-records
//!
//! 查询某触发器（日程维度）的执行明细：分页 + 总数。
//! 统计概览/分布走 `/exec-stats` 独立接口。时间窗缺省「最近 7 天」，
//! 分页上限 200；归一化在 handler 完成，domain/dal/dao 只接收确定值。

use ai_orz_macros::generate_http_handler;
use common::api::{QueryTriggerExecRequest, QueryTriggerExecResponse};
use common::error::{Error, Result};

use crate::pkg::RequestContext;
use crate::service::dao::cron_trigger::TriggerExecQuery;
use crate::service::domain::system::domain;

/// 默认最近 7 天（毫秒）；执行明细与统计两个接口共用
pub(super) const DEFAULT_WINDOW_MS: i64 = 7 * 24 * 60 * 60 * 1000;
/// 每页默认条数与硬上限
const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 200;

#[generate_http_handler]
pub async fn query_trigger_exec(
    ctx: RequestContext,
    params: QueryTriggerExecRequest,
) -> Result<QueryTriggerExecResponse> {
    let now = chrono::Utc::now().timestamp_millis();
    let start_at = params.start_at.unwrap_or(now - DEFAULT_WINDOW_MS);
    let end_at = params.end_at.unwrap_or(now);
    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);

    // 归属校验：执行记录必须挂在真实存在的 trigger 上。
    let org_id = resolve_exec_org(&ctx, &params.trigger_id).await?;

    let result = domain()
        .cron_manager()
        .query_trigger_exec(
            ctx,
            TriggerExecQuery {
                trigger_id: params.trigger_id,
                org_id,
                time_range: (start_at, end_at),
                offset,
                limit,
            },
        )
        .await?;

    Ok(QueryTriggerExecResponse {
        records: result.records,
        total: result.total,
    })
}

/// 两个执行查询接口共用：确认 trigger 存在并解析归属组织（跨组织 Admin 拒绝）。
pub(super) async fn resolve_exec_org(
    ctx: &RequestContext,
    trigger_id: &str,
) -> Result<Option<String>> {
    let trigger = domain()
        .cron_manager()
        .get_trigger(ctx.clone(), trigger_id)
        .await?
        .ok_or_else(|| Error::not_found(format!("CronTrigger {trigger_id} not found")))?;
    if let (Some(caller_org), Some(owner_org)) = (&ctx.organization_id, &trigger.org_id)
        && caller_org != owner_org
    {
        return Err(Error::forbidden(
            "no permission to read exec records of another organization",
        ));
    }
    Ok(trigger.org_id.clone())
}
