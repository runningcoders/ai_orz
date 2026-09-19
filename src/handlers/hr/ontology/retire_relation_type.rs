//! Handler: DELETE /api/v1/hr/ontology/relation-types/{id} - 退役关系类型词条（软删除）

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ontology::{
    RetireOntologyRelationTypeRequest, RetireOntologyRelationTypeResponse,
};
use common::error::{Result, err};

/// Retire an ontology relation type lexicon entry (soft delete)
#[register_handler_tool(
    id = "retire_ontology_relation_type",
    name = "Retire Ontology Relation Type",
    description = "Retire a relation-type lexicon entry (soft delete, status=retired). Historical graph edges of this type remain interpretable; no new writes may reference a retired entry. Retiring an already-retired entry succeeds idempotently. Fails with NotFound if the entry does not exist.",
    params = "common::api::RetireOntologyRelationTypeRequest",
    tags = "ontology_management"
)]
#[generate_http_handler]
pub async fn retire_ontology_relation_type(
    ctx: RequestContext,
    params: RetireOntologyRelationTypeRequest,
) -> Result<RetireOntologyRelationTypeResponse> {
    domain()
        .ontology_domain()
        .get_relation_type(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| err!(NotFound, "关系类型 {} 不存在", params.id))?;
    domain()
        .ontology_domain()
        .retire_relation_type(ctx.clone(), &params.id)
        .await?;
    // 重查返回退役后的最新行（status=retired + DAO 刷新的 updated_at）
    let latest = domain()
        .ontology_domain()
        .get_relation_type(ctx, &params.id)
        .await?
        .ok_or_else(|| err!(NotFound, "关系类型 {} 不存在", params.id))?;
    Ok(super::relation_type_item(latest.into_po()))
}
