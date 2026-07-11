//! Handler: GET /api/v1/system/cron-triggers/{trigger_id} - Get Cron Trigger detail.

use ai_orz_macros::generate_http_handler;
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::system::domain;

use super::response::{GetCronTriggerRequest, GetCronTriggerResponse, to_detail};

#[generate_http_handler]
pub async fn get_cron_trigger(
    ctx: RequestContext,
    params: GetCronTriggerRequest,
) -> Result<GetCronTriggerResponse> {
    let trigger = domain()
        .cron_manager()
        .get_trigger(ctx, &params.trigger_id)
        .await?
        .ok_or_else(|| {
            common::error::Error::not_found(format!(
                "CronTrigger {} not found",
                params.trigger_id
            ))
        })?;

    Ok(to_detail(&trigger))
}
