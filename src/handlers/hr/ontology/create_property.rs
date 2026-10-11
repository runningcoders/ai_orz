//! Handler: POST /api/v1/hr/ontology/properties - 新增属性定义词条

use crate::models::ontology::{OntologyProperty, OntologyPropertyPo};
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ontology::{CreateOntologyPropertyRequest, CreateOntologyPropertyResponse};
use common::error::Result;

/// Create an ontology property definition
#[register_handler_tool(
    id = "create_ontology_property",
    name = "Create Ontology Property",
    description = "Create a property definition in the ontology lexicon: declares what a named property means, which value type it holds (string/number/boolean/date/reference/enum), its value range or enum whitelist, cardinality (single/many), and which knowledge-graph carriers (node/relation) it may attach to. Property instance values are stored separately on entities. term_key is the canonical snake_case key and is immutable after creation. Fails with Conflict if term_key already exists.",
    params = "common::api::CreateOntologyPropertyRequest",
    tags = "ontology_management"
)]
#[generate_http_handler]
pub async fn create_ontology_property(
    ctx: RequestContext,
    params: CreateOntologyPropertyRequest,
) -> Result<CreateOntologyPropertyResponse> {
    let po = OntologyPropertyPo::new(
        params.term_key,
        params.display_name,
        params.description,
        serde_json::to_string(&params.domain_classes).unwrap_or_default(),
        serde_json::to_string(&params.applies_to).unwrap_or_default(),
        params.data_type.as_str(),
        serde_json::to_string(&params.value_range).unwrap_or_default(),
        params.cardinality.as_str(),
        params.unit,
        params.required,
    );
    domain()
        .ontology_domain()
        .create_property(ctx, &OntologyProperty::from_po(po.clone()))
        .await?;
    Ok(super::property_item(po))
}
