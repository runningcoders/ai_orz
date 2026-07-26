//! Handler: GET /api/v1/finance/model-providers/rebuild-progress - Get rebuild progress

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{GetRebuildProgressRequest, RebuildProgressResponse};
use common::error::{Error, Result};

/// Get vector index rebuild progress by task_id
#[generate_http_handler]
pub async fn get_rebuild_progress(
    ctx: RequestContext,
    params: GetRebuildProgressRequest,
) -> Result<RebuildProgressResponse> {
    let progress = domain()
        .model_provider_manage()
        .get_rebuild_progress(ctx, &params.task_id)
        .await?
        .ok_or_else(|| Error::not_found(format!("Rebuild task {} not found", params.task_id)))?;

    Ok(progress)
}
