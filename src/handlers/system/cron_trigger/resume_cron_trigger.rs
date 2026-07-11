//! Handler: POST /api/v1/system/cron-triggers/{trigger_id}/resume - Resume Cron Trigger.

use ai_orz_macros::generate_http_handler;
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::system::domain;

use super::response::{ResumeCronTriggerRequest, ResumeCronTriggerResponse};

#[generate_http_handler]
pub async fn resume_cron_trigger(
    ctx: RequestContext,
    params: ResumeCronTriggerRequest,
) -> Result<ResumeCronTriggerResponse> {
    domain()
        .cron_manager()
        .resume_trigger(ctx, &params.trigger_id)
        .await?;

    Ok(ResumeCronTriggerResponse { success: true })
}
