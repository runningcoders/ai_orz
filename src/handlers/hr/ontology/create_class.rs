//! Handler: POST /api/v1/hr/ontology/classes - 新增实体类词条

use crate::models::ontology::{OntologyClass, OntologyClassPo};
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ontology::{CreateOntologyClassRequest, CreateOntologyClassResponse};
use common::error::Result;

/// Create an ontology class lexicon entry
#[register_handler_tool(
    id = "create_ontology_class",
    name = "Create Ontology Class",
    description = "Create an entity-class lexicon entry (a node type in the knowledge graph). term_key is the lowercase snake_case canonical key and must be unique; it cannot be changed after creation (retire + re-create instead). required_fields lists the mandatory attributes for entities of this class. Fails with Conflict if term_key already exists.",
    params = "common::api::CreateOntologyClassRequest",
    tags = "ontology_management"
)]
#[generate_http_handler]
pub async fn create_ontology_class(
    ctx: RequestContext,
    params: CreateOntologyClassRequest,
) -> Result<CreateOntologyClassResponse> {
    let po = OntologyClassPo::new(
        params.term_key,
        params.display_name,
        params.description,
        serde_json::to_string(&params.required_fields).unwrap_or_default(),
    );
    domain()
        .ontology_domain()
        .create_class(ctx, &OntologyClass::from_po(po.clone()))
        .await?;
    Ok(super::class_item(po))
}
