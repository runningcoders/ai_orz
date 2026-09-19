//! Handler: GET /api/v1/hr/ontology/relation-types - 关系类型词表分页列表

use crate::pkg::RequestContext;
use crate::service::dao::ontology::OntologyRelationTypeQuery;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ontology::{ListOntologyRelationTypesRequest, ListOntologyRelationTypesResponse};
use common::error::Result;

/// List ontology relation type lexicon entries (paged)
#[register_handler_tool(
    id = "list_ontology_relation_types",
    name = "List Ontology Relation Types",
    description = "List relation-type lexicon entries (knowledge graph edge types) with pagination. Filter by status (active/retired, defaults to all) and keyword (matches term_key, display_name, or description). Each entry carries domain/range class constraints, weight_base, and the inverse relation key.",
    params = "common::api::ListOntologyRelationTypesRequest",
    tags = "ontology_management"
)]
#[generate_http_handler]
pub async fn list_ontology_relation_types(
    ctx: RequestContext,
    params: ListOntologyRelationTypesRequest,
) -> Result<ListOntologyRelationTypesResponse> {
    let page = domain()
        .ontology_domain()
        .list_relation_types(
            ctx,
            OntologyRelationTypeQuery {
                status: params.status,
                keyword: params.keyword,
                pagination: params.pagination,
            },
        )
        .await?;
    Ok(page.map(|e| super::relation_type_item(e.into_po())))
}
