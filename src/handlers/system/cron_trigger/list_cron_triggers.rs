//! Handler: GET /api/v1/system/cron-triggers - List Cron Triggers.

use ai_orz_macros::generate_http_handler;
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::dao::cron_trigger::CronTriggerQuery;
use crate::service::domain::system::domain;

use super::response::{CronTriggerDetail, ListCronTriggersRequest, ListCronTriggersResponse};

#[generate_http_handler]
pub async fn list_cron_triggers(
    ctx: RequestContext,
    params: ListCronTriggersRequest,
) -> Result<ListCronTriggersResponse> {
    let query = CronTriggerQuery {
        trigger_type: params.trigger_type,
        is_enabled: params.is_enabled,
        limit: params.limit,
    };

    let triggers = domain()
        .cron_manager()
        .list_triggers(ctx, query)
        .await?;

    let total = triggers.len();
    let triggers: Vec<CronTriggerDetail> = triggers
        .iter()
        .map(|t| super::response::to_detail(t))
        .collect();

    Ok(ListCronTriggersResponse { triggers, total })
}
