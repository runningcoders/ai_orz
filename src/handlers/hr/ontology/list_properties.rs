//! Handler: GET /api/v1/hr/ontology/properties - 属性定义词表分页列表

use crate::pkg::RequestContext;
use crate::service::dao::ontology::OntologyPropertyQuery;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ontology::{ListOntologyPropertiesRequest, ListOntologyPropertiesResponse};
use common::error::Result;

/// List ontology property definitions (paged)
#[register_handler_tool(
    id = "list_ontology_properties",
    name = "List Ontology Properties",
    description = "List ontology property definitions with pagination. Filter by status (active/retired, defaults to all), domain class, carrier type (node/relation), and keyword (matches term_key, display_name, or description). Each item declares the property's value type, value range and cardinality.",
    params = "common::api::ListOntologyPropertiesRequest",
    neural,
    tags = "ontology_management,ontology"
)]
#[generate_http_handler]
pub async fn list_ontology_properties(
    ctx: RequestContext,
    params: ListOntologyPropertiesRequest,
) -> Result<ListOntologyPropertiesResponse> {
    let page = domain()
        .ontology_domain()
        .list_properties(
            ctx,
            OntologyPropertyQuery {
                status: params.status,
                domain_class: params.domain_class,
                target_type: params.target_type,
                keyword: params.keyword,
                pagination: params.pagination,
            },
        )
        .await?;
    Ok(page.map(|e| super::property_item(e.into_po())))
}
