//! Handler: GET /api/v1/hr/ontology/classes - 实体类词表分页列表

use crate::pkg::RequestContext;
use crate::service::dao::ontology::OntologyClassQuery;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ontology::{ListOntologyClassesRequest, ListOntologyClassesResponse};
use common::error::Result;

/// List ontology class lexicon entries (paged)
#[register_handler_tool(
    id = "list_ontology_classes",
    name = "List Ontology Classes",
    description = "List entity-class lexicon entries (knowledge graph node types) with pagination. Filter by status (active/retired, defaults to all) and keyword (matches term_key, display_name, or description).",
    params = "common::api::ListOntologyClassesRequest",
    tags = "ontology_management"
)]
#[generate_http_handler]
pub async fn list_ontology_classes(
    ctx: RequestContext,
    params: ListOntologyClassesRequest,
) -> Result<ListOntologyClassesResponse> {
    let page = domain()
        .ontology_domain()
        .list_classes(
            ctx,
            OntologyClassQuery {
                status: params.status,
                keyword: params.keyword,
                pagination: params.pagination,
            },
        )
        .await?;
    Ok(page.map(|e| super::class_item(e.into_po())))
}
