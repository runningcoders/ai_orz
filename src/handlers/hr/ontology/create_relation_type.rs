//! Handler: POST /api/v1/hr/ontology/relation-types - 新增关系类型词条

use crate::models::ontology::{OntologyRelationType, OntologyRelationTypePo};
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ontology::{
    CreateOntologyRelationTypeRequest, CreateOntologyRelationTypeResponse,
};
use common::error::Result;

/// Create an ontology relation type lexicon entry
#[register_handler_tool(
    id = "create_ontology_relation_type",
    name = "Create Ontology Relation Type",
    description = "Create a relation-type lexicon entry (an edge type in the knowledge graph). term_key is the lowercase snake_case canonical key and must be unique; it cannot be changed after creation. domain_classes / range_classes constrain the head/tail entity classes (empty = unconstrained, keys must exist); inverse_key optionally links a symmetric reverse relation. Fails with Conflict if term_key already exists, or InvalidRequest if referenced classes do not exist.",
    params = "common::api::CreateOntologyRelationTypeRequest",
    tags = "ontology_management"
)]
#[generate_http_handler]
pub async fn create_ontology_relation_type(
    ctx: RequestContext,
    params: CreateOntologyRelationTypeRequest,
) -> Result<CreateOntologyRelationTypeResponse> {
    let po = OntologyRelationTypePo::new(
        params.term_key,
        params.display_name,
        params.description,
        serde_json::to_string(&params.domain_classes).unwrap_or_default(),
        serde_json::to_string(&params.range_classes).unwrap_or_default(),
        params.weight_base.unwrap_or(1.0),
        params.inverse_key,
    );
    domain()
        .ontology_domain()
        .create_relation_type(ctx, &OntologyRelationType::from_po(po.clone()))
        .await?;
    Ok(super::relation_type_item(po))
}
