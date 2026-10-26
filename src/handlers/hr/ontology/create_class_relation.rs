//! Handler: POST /api/v1/hr/ontology/class-relations - 新增骨架三元组

use crate::models::ontology::{OntologyClassRelation, OntologyClassRelationPo};
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ontology::{
    CreateOntologyClassRelationRequest, CreateOntologyClassRelationResponse,
};
use common::error::Result;

/// Create an ontology skeleton triple (subject class -> relation -> object class)
#[register_handler_tool(
    id = "create_ontology_class_relation",
    name = "Create Ontology Class Relation",
    description = "Create a skeleton triple (subject_class_key -[relation_key]-> object_class_key), the first-class representation of which entity classes may be connected by which relation. subject/relation/object keys must reference existing active classes/relation types; use '*' as a class key to mean unconstrained on that side. cardinality is 'single' or 'many' (default many); required marks whether the edge is mandatory. Fails with Conflict if the exact triple already exists.",
    params = "common::api::CreateOntologyClassRelationRequest",
    tags = "ontology_management"
)]
#[generate_http_handler]
pub async fn create_ontology_class_relation(
    ctx: RequestContext,
    params: CreateOntologyClassRelationRequest,
) -> Result<CreateOntologyClassRelationResponse> {
    let po = OntologyClassRelationPo::new(
        params.subject_class_key,
        params.relation_key,
        params.object_class_key,
        params.cardinality.unwrap_or_else(|| "many".to_string()),
        params.required,
    );
    domain()
        .ontology_domain()
        .create_class_relation(ctx, &OntologyClassRelation::from_po(po.clone()))
        .await?;
    Ok(super::class_relation_item(po))
}
