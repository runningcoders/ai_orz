//! Handler: PUT /api/v1/hr/ontology/relation-types/{id} - 更新关系类型词条

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ontology::{
    UpdateOntologyRelationTypeRequest, UpdateOntologyRelationTypeResponse,
};
use common::error::{Result, err};

/// Update an ontology relation type lexicon entry (term_key immutable)
#[register_handler_tool(
    id = "update_ontology_relation_type",
    name = "Update Ontology Relation Type",
    description = "Update a relation-type lexicon entry's display name, description, domain/range class constraints, weight_base, and inverse_key. The term_key is immutable - to rename, retire the old entry and create a new one. Referenced classes must exist. Fails with NotFound if the entry does not exist.",
    params = "common::api::UpdateOntologyRelationTypeRequest",
    tags = "ontology_management"
)]
#[generate_http_handler]
pub async fn update_ontology_relation_type(
    ctx: RequestContext,
    params: UpdateOntologyRelationTypeRequest,
) -> Result<UpdateOntologyRelationTypeResponse> {
    let mut entity = domain()
        .ontology_domain()
        .get_relation_type(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| err!(NotFound, "关系类型 {} 不存在", params.id))?;
    entity.po.display_name = params.display_name;
    entity.po.description = params.description;
    entity.po.domain_classes = serde_json::to_string(&params.domain_classes).unwrap_or_default();
    entity.po.range_classes = serde_json::to_string(&params.range_classes).unwrap_or_default();
    entity.po.weight_base = params.weight_base;
    entity.po.inverse_key = params.inverse_key;
    domain()
        .ontology_domain()
        .update_relation_type(ctx.clone(), &entity)
        .await?;
    // DAO 写入时刷新 updated_at，重查返回最新行
    let latest = domain()
        .ontology_domain()
        .get_relation_type(ctx, &params.id)
        .await?
        .ok_or_else(|| err!(NotFound, "关系类型 {} 不存在", params.id))?;
    Ok(super::relation_type_item(latest.into_po()))
}
