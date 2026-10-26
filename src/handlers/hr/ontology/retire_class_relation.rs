//! Handler: DELETE /api/v1/hr/ontology/class-relations/{id} - 退役骨架三元组（软删除）

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ontology::{
    RetireOntologyClassRelationRequest, RetireOntologyClassRelationResponse,
};
use common::error::{Result, err};

/// Retire an ontology skeleton triple (soft delete)
#[register_handler_tool(
    id = "retire_ontology_class_relation",
    name = "Retire Ontology Class Relation",
    description = "Retire a skeleton triple (soft delete, status=retired). Historical interpretation is preserved; the triple no longer constrains runtime writes. Retiring an already-retired triple succeeds idempotently. Fails with NotFound if the triple does not exist.",
    params = "common::api::RetireOntologyClassRelationRequest",
    tags = "ontology_management"
)]
#[generate_http_handler]
pub async fn retire_ontology_class_relation(
    ctx: RequestContext,
    params: RetireOntologyClassRelationRequest,
) -> Result<RetireOntologyClassRelationResponse> {
    // 校验存在性（不存在即 404；重复退役由 DAO 幂等处理）
    domain()
        .ontology_domain()
        .get_class_relation(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| err!(NotFound, "骨架三元组 {} 不存在", params.id))?;
    domain()
        .ontology_domain()
        .retire_class_relation(ctx.clone(), &params.id)
        .await?;
    // 重查返回退役后的最新行（status=retired + DAO 刷新的 updated_at）
    let latest = domain()
        .ontology_domain()
        .get_class_relation(ctx, &params.id)
        .await?
        .ok_or_else(|| err!(NotFound, "骨架三元组 {} 不存在", params.id))?;
    Ok(super::class_relation_item(latest.into_po()))
}
