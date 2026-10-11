//! Handler: PUT /api/v1/hr/ontology/properties/{id} - 更新属性定义词条

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ontology::{UpdateOntologyPropertyRequest, UpdateOntologyPropertyResponse};
use common::error::{Result, err};

/// Update an ontology property definition (term_key immutable)
#[register_handler_tool(
    id = "update_ontology_property",
    name = "Update Ontology Property",
    description = "Update an ontology property definition's display name, description, domain classes, carrier list, value type, value range, cardinality, unit and required flag. The term_key is immutable - to rename, retire the old definition and create a new one. Existing property values already stored on entities are NOT migrated or removed. Fails with NotFound if the definition does not exist.",
    params = "common::api::UpdateOntologyPropertyRequest",
    tags = "ontology_management"
)]
#[generate_http_handler]
pub async fn update_ontology_property(
    ctx: RequestContext,
    params: UpdateOntologyPropertyRequest,
) -> Result<UpdateOntologyPropertyResponse> {
    let mut entity = domain()
        .ontology_domain()
        .get_property(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| err!(NotFound, "属性 {} 不存在", params.id))?;

    // term_key 不可变：语义锚点是历史属性值的稳定性来源，改 key = 废弃旧属性
    entity.po.display_name = params.display_name;
    entity.po.description = params.description;
    entity.po.domain_classes = serde_json::to_string(&params.domain_classes).unwrap_or_default();
    entity.po.applies_to = serde_json::to_string(&params.applies_to).unwrap_or_default();
    entity.po.data_type = params.data_type.as_str().to_string();
    entity.po.value_range = serde_json::to_string(&params.value_range).unwrap_or_default();
    entity.po.cardinality = params.cardinality.as_str().to_string();
    entity.po.unit = params.unit;
    entity.po.required = i32::from(params.required);

    domain()
        .ontology_domain()
        .update_property(ctx.clone(), &entity)
        .await?;
    // DAO 写入时刷新 updated_at，重查返回最新行
    let latest = domain()
        .ontology_domain()
        .get_property(ctx, &params.id)
        .await?
        .ok_or_else(|| err!(NotFound, "属性 {} 不存在", params.id))?;
    Ok(super::property_item(latest.into_po()))
}
