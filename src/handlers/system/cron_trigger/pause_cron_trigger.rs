//! Handler: POST /api/v1/system/cron-triggers/{trigger_id}/pause - Pause Cron Trigger.

use ai_orz_macros::generate_http_handler;
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::system::domain;

use super::response::{PauseCronTriggerRequest, PauseCronTriggerResponse};

#[generate_http_handler]
pub async fn pause_cron_trigger(
    ctx: RequestContext,
    params: PauseCronTriggerRequest,
) -> Result<PauseCronTriggerResponse> {
    domain()
        .cron_manager()
        .pause_trigger(ctx, &params.trigger_id)
        .await?;

    Ok(PauseCronTriggerResponse { success: true })
}
