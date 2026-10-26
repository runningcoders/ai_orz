//! Handler: PUT /api/v1/hr/ontology/class-relations/{id} - 更新骨架三元组

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ontology::{
    UpdateOntologyClassRelationRequest, UpdateOntologyClassRelationResponse,
};
use common::error::{Result, err};

/// Update an ontology skeleton triple (all three keys are editable)
#[register_handler_tool(
    id = "update_ontology_class_relation",
    name = "Update Ontology Class Relation",
    description = "Update a skeleton triple's subject/relation/object keys, cardinality, and required flag. Referenced classes and relation types must exist and be active ('*' is allowed for a class side). Fails with NotFound if the triple does not exist, or Conflict if the new triple duplicates an existing one.",
    params = "common::api::UpdateOntologyClassRelationRequest",
    tags = "ontology_management"
)]
#[generate_http_handler]
pub async fn update_ontology_class_relation(
    ctx: RequestContext,
    params: UpdateOntologyClassRelationRequest,
) -> Result<UpdateOntologyClassRelationResponse> {
    let mut entity = domain()
        .ontology_domain()
        .get_class_relation(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| err!(NotFound, "骨架三元组 {} 不存在", params.id))?;
    entity.po.subject_class_key = params.subject_class_key;
    entity.po.relation_key = params.relation_key;
    entity.po.object_class_key = params.object_class_key;
    entity.po.cardinality = params.cardinality;
    entity.po.required = i32::from(params.required);
    domain()
        .ontology_domain()
        .update_class_relation(ctx.clone(), &entity)
        .await?;
    // DAO 写入时刷新 updated_at，重查返回最新行
    let latest = domain()
        .ontology_domain()
        .get_class_relation(ctx, &params.id)
        .await?
        .ok_or_else(|| err!(NotFound, "骨架三元组 {} 不存在", params.id))?;
    Ok(super::class_relation_item(latest.into_po()))
}
