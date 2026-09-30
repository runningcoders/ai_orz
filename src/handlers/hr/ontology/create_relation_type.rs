//! Handler: POST /api/v1/hr/ontology/relation-types - 新增关系类型词条

use crate::models::ontology::{OntologyRelationType, OntologyRelationTypePo};
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ontology::{
    CreateOntologyRelationTypeRequest, CreateOntologyRelationTypeResponse,
};
use common::error::{Result, bail_err, err};
use common::ontology::Direction;

/// Create an ontology relation type lexicon entry
#[register_handler_tool(
    id = "create_ontology_relation_type",
    name = "Create Ontology Relation Type",
    description = "Create a relation-type lexicon entry (an edge type in the knowledge graph). term_key is the lowercase snake_case canonical key and must be unique; it cannot be changed after creation. domain_classes / range_classes constrain the head/tail entity classes (empty = unconstrained, keys must exist); inverse_key optionally links a symmetric reverse relation; direction is the edge orientation, either 'directed' or 'undirected' (default undirected) - directed edges render an arrowhead at the target end, and undirected entries must not declare inverse_key. Fails with Conflict if term_key already exists, or InvalidRequest if referenced classes do not exist or direction is invalid / conflicts with inverse_key.",
    params = "common::api::CreateOntologyRelationTypeRequest",
    tags = "ontology_management"
)]
#[generate_http_handler]
pub async fn create_ontology_relation_type(
    ctx: RequestContext,
    params: CreateOntologyRelationTypeRequest,
) -> Result<CreateOntologyRelationTypeResponse> {
    // direction 校验（方案 §5.3）：解析失败 → 400；undirected 携带 inverse_key → 400
    //（无向词不允许声明互逆；directed 可空可显式，自逆词合法）
    let direction = params
        .direction
        .as_deref()
        .unwrap_or("undirected")
        .to_string();
    let parsed: Direction = direction.parse().map_err(|_| {
        err!(
            InvalidRequest,
            "direction 取值非法: {}（仅接受 directed / undirected）",
            direction
        )
    })?;
    if parsed == Direction::Undirected && params.inverse_key.is_some() {
        bail_err!(
            InvalidRequest,
            "direction=undirected 与 inverse_key 互斥：无向词不允许声明互逆，请改用 directed 或去掉 inverse_key"
        );
    }
    let po = OntologyRelationTypePo::new(
        params.term_key,
        params.display_name,
        params.description,
        serde_json::to_string(&params.domain_classes).unwrap_or_default(),
        serde_json::to_string(&params.range_classes).unwrap_or_default(),
        params.weight_base.unwrap_or(1.0),
        params.inverse_key,
        &direction,
    );
    domain()
        .ontology_domain()
        .create_relation_type(ctx, &OntologyRelationType::from_po(po.clone()))
        .await?;
    Ok(super::relation_type_item(po))
}
