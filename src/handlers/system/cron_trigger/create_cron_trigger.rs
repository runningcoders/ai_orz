//! Handler: POST /api/v1/system/cron-triggers - Create a new Cron Trigger.

use ai_orz_macros::generate_http_handler;
use common::enums::TriggerType;
use common::error::Result;
use uuid::Uuid;

use crate::models::cron_trigger::CronTriggerPo;
use crate::pkg::RequestContext;
use crate::service::domain::system::domain;

use super::response::{CreateCronTriggerRequest, CreateCronTriggerResponse, to_detail};

#[generate_http_handler]
pub async fn create_cron_trigger(
    ctx: RequestContext,
    params: CreateCronTriggerRequest,
) -> Result<CreateCronTriggerResponse> {
    let next_run_at = match params.trigger_type {
        TriggerType::Once => params.run_at.ok_or_else(|| {
            common::error::err!(InvalidRequest, "run_at is required for Once trigger")
        })?,
        TriggerType::Interval => {
            let interval = params.interval_seconds.ok_or_else(|| {
                common::error::err!(
                    InvalidRequest,
                    "interval_seconds is required for Interval trigger"
                )
            })?;
            common::constants::utils::current_timestamp() + interval
        }
        TriggerType::Cron => {
            let expression = params.cron_expression.as_deref().ok_or_else(|| {
                common::error::err!(InvalidRequest, "cron_expression is required for Cron trigger")
            })?;
            let timezone = crate::pkg::cron::system_timezone();
            crate::pkg::cron::next_run_at(expression, &timezone, chrono::Utc::now())?
        }
    };

    let mut trigger = CronTriggerPo::new(
        Uuid::now_v7().to_string(),
        params.name,
        params.trigger_type,
        next_run_at,
        Some(ctx.uid()),
    );

    trigger.cron_expression = params.cron_expression;
    trigger.interval_seconds = params.interval_seconds;
    trigger.run_at = params.run_at;
    trigger.payload = params.payload;

    let trigger_id = trigger.id.clone();

    domain()
        .cron_manager()
        .create_trigger(ctx.clone(), &trigger)
        .await?;

    let trigger = domain()
        .cron_manager()
        .get_trigger(ctx, &trigger_id)
        .await?
        .unwrap_or(trigger);

    Ok(to_detail(&trigger))
}
