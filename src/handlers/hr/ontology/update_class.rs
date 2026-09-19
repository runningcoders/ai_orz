//! Handler: PUT /api/v1/hr/ontology/classes/{id} - 更新实体类词条

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ontology::{UpdateOntologyClassRequest, UpdateOntologyClassResponse};
use common::error::{Result, err};

/// Update an ontology class lexicon entry (term_key immutable)
#[register_handler_tool(
    id = "update_ontology_class",
    name = "Update Ontology Class",
    description = "Update an entity-class lexicon entry's display name, description, and required_fields. The term_key is immutable - to rename, retire the old entry and create a new one so historical parsing chains are preserved. Fails with NotFound if the entry does not exist.",
    params = "common::api::UpdateOntologyClassRequest",
    tags = "ontology_management"
)]
#[generate_http_handler]
pub async fn update_ontology_class(
    ctx: RequestContext,
    params: UpdateOntologyClassRequest,
) -> Result<UpdateOntologyClassResponse> {
    let mut entity = domain()
        .ontology_domain()
        .get_class(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| err!(NotFound, "实体类 {} 不存在", params.id))?;
    entity.po.display_name = params.display_name;
    entity.po.description = params.description;
    entity.po.required_fields = serde_json::to_string(&params.required_fields).unwrap_or_default();
    domain()
        .ontology_domain()
        .update_class(ctx.clone(), &entity)
        .await?;
    // DAO 写入时刷新 updated_at，重查返回最新行
    let latest = domain()
        .ontology_domain()
        .get_class(ctx, &params.id)
        .await?
        .ok_or_else(|| err!(NotFound, "实体类 {} 不存在", params.id))?;
    Ok(super::class_item(latest.into_po()))
}
