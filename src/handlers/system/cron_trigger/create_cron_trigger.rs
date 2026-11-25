//! Handler: POST /api/v1/system/cron-triggers - Create a new Cron Trigger.

use ai_orz_macros::generate_http_handler;
use common::enums::TriggerType;
use common::error::Result;

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
            // 受检乘加：interval<=0 或极端值直接报错，不 wrap 成过去时间
            common::schedule::interval_next_run_at(
                common::constants::utils::current_timestamp_ms(),
                interval,
            )?
        }
        TriggerType::Cron => {
            let expression = params.cron_expression.as_deref().ok_or_else(|| {
                common::error::err!(
                    InvalidRequest,
                    "cron_expression is required for Cron trigger"
                )
            })?;
            let timezone = crate::config::timezone();
            common::schedule::next_run_at(expression, &timezone, chrono::Utc::now())?
        }
        // Schedule 是形态标签：时间规则三选一（cron > interval > run_at）。
        // 归属与行为模板由 P2 的 schedule_agent_task 工具固化；此处仅支持裸时间规则。
        TriggerType::Schedule => {
            if let Some(expression) = params.cron_expression.as_deref() {
                let timezone = crate::config::timezone();
                common::schedule::next_run_at(expression, &timezone, chrono::Utc::now())?
            } else if let Some(interval) = params.interval_seconds {
                common::schedule::interval_next_run_at(
                    common::constants::utils::current_timestamp_ms(),
                    interval,
                )?
            } else {
                params.run_at.ok_or_else(|| {
                    common::error::err!(
                        InvalidRequest,
                        "one of cron_expression / interval_seconds / run_at is required for Schedule"
                    )
                })?
            }
        }
    };

    let mut trigger = CronTriggerPo::new(
        common::new_id!(CronTrigger),
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
