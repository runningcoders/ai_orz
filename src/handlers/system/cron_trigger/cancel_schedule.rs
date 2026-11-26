//! Handler：取消日程（Agent 工具 + HTTP 接口）。
//!
//! 本层只做身份固化与 DTO 透传；归属校验、幂等语义、软删落库都在
//! `domain::system::CronManager::cancel_schedule`，consumer 可直接复用。

use crate::pkg::RequestContext;
use crate::service::domain::system::domain as system_domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CancelScheduleRequest, CancelScheduleResponse};
use common::error::Result;

/// 取消自己创建的一条日程。
///
/// 幂等：已取消的日程再次调用返回 `cancelled=false`，不是错误。
/// 只能取消归属自己的异步日程；系统维护模板与其它用户的日程统一报 NotFound。
#[register_handler_tool(
    id = "cancel_schedule",
    name = "Cancel Schedule",
    description = "Cancel a schedule owned by the current user (any agent acting for that user may cancel it; creator does not grant permission). Pass trigger_id from schedule_agent_task or query_schedule. Idempotent: cancelling again returns cancelled=false, not an error. System maintenance triggers and other users' schedules are not visible. Use update_schedule to change only the time or title.",
    params = "common::api::CancelScheduleRequest",
    neural,
    tags = "scheduling,autonomy"
)]
#[generate_http_handler]
pub async fn cancel_schedule(
    ctx: RequestContext,
    params: CancelScheduleRequest,
) -> Result<CancelScheduleResponse> {
    let cancelled = system_domain()
        .cron_manager()
        .cancel_schedule(ctx, &params.trigger_id)
        .await?;
    Ok(CancelScheduleResponse {
        cancelled,
        message: if cancelled {
            "日程已取消".to_string()
        } else {
            "日程此前已取消（幂等 no-op）".to_string()
        },
    })
}
