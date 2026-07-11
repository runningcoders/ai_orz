//! Handler: DELETE /api/v1/system/cron-triggers/{trigger_id} - Delete Cron Trigger.

use ai_orz_macros::generate_http_handler;
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::system::domain;

use super::response::{DeleteCronTriggerRequest, DeleteCronTriggerResponse};

#[generate_http_handler]
pub async fn delete_cron_trigger(
    ctx: RequestContext,
    params: DeleteCronTriggerRequest,
) -> Result<DeleteCronTriggerResponse> {
    domain()
        .cron_manager()
        .delete_trigger(ctx, &params.trigger_id)
        .await?;

    Ok(DeleteCronTriggerResponse { success: true })
}
