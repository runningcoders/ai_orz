//! Handler: DELETE /api/v1/hr/ontology/classes/{id} - 退役实体类词条（软删除）

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ontology::{RetireOntologyClassRequest, RetireOntologyClassResponse};
use common::error::{Result, err};

/// Retire an ontology class lexicon entry (soft delete)
#[register_handler_tool(
    id = "retire_ontology_class",
    name = "Retire Ontology Class",
    description = "Retire an entity-class lexicon entry (soft delete, status=retired). Historical graph nodes of this class remain interpretable; no new writes may reference a retired entry. Retiring an already-retired entry succeeds idempotently. Fails with NotFound if the entry does not exist.",
    params = "common::api::RetireOntologyClassRequest",
    tags = "ontology_management"
)]
#[generate_http_handler]
pub async fn retire_ontology_class(
    ctx: RequestContext,
    params: RetireOntologyClassRequest,
) -> Result<RetireOntologyClassResponse> {
    // 校验存在性（不存在即 404；重复退役由 DAO 幂等处理）
    domain()
        .ontology_domain()
        .get_class(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| err!(NotFound, "实体类 {} 不存在", params.id))?;
    domain()
        .ontology_domain()
        .retire_class(ctx.clone(), &params.id)
        .await?;
    // 重查返回退役后的最新行（status=retired + DAO 刷新的 updated_at）
    let latest = domain()
        .ontology_domain()
        .get_class(ctx, &params.id)
        .await?
        .ok_or_else(|| err!(NotFound, "实体类 {} 不存在", params.id))?;
    Ok(super::class_item(latest.into_po()))
}
