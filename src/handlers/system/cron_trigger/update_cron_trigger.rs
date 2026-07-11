//! Handler: PUT /api/v1/system/cron-triggers/{trigger_id} - Update Cron Trigger.

use ai_orz_macros::generate_http_handler;
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::system::domain;

use super::response::{UpdateCronTriggerRequest, UpdateCronTriggerResponse, to_detail};

#[generate_http_handler]
pub async fn update_cron_trigger(
    ctx: RequestContext,
    params: UpdateCronTriggerRequest,
) -> Result<UpdateCronTriggerResponse> {
    let mut trigger = domain()
        .cron_manager()
        .get_trigger(ctx.clone(), &params.trigger_id)
        .await?
        .ok_or_else(|| {
            common::error::Error::not_found(format!(
                "CronTrigger {} not found",
                params.trigger_id
            ))
        })?;

    if let Some(name) = params.name {
        trigger.name = name;
    }
    if let Some(trigger_type) = params.trigger_type {
        trigger.trigger_type = trigger_type;
    }
    if params.cron_expression.is_some() {
        trigger.cron_expression = params.cron_expression;
    }
    if params.interval_seconds.is_some() {
        trigger.interval_seconds = params.interval_seconds;
    }
    if params.run_at.is_some() {
        trigger.run_at = params.run_at;
    }
    if let Some(payload) = params.payload {
        trigger.payload = payload;
    }

    trigger.touch(Some(ctx.uid()));

    domain()
        .cron_manager()
        .update_trigger(ctx.clone(), &trigger)
        .await?;

    let trigger = domain()
        .cron_manager()
        .get_trigger(ctx, &params.trigger_id)
        .await?
        .unwrap_or(trigger);

    Ok(to_detail(&trigger))
}
