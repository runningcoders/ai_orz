//! Handler: GET /api/v1/hr/ontology/synonyms - 同义映射分页列表

use crate::pkg::RequestContext;
use crate::service::dao::ontology::OntologySynonymQuery;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ontology::{ListOntologySynonymsRequest, ListOntologySynonymsResponse};
use common::error::Result;

/// List synonym mappings (paged)
#[register_handler_tool(
    id = "list_ontology_synonyms",
    name = "List Ontology Synonyms",
    description = "List synonym mappings with pagination. Filter by target_kind (class/relation) and target_key (exact match on the canonical term key).",
    params = "common::api::ListOntologySynonymsRequest",
    tags = "ontology_management"
)]
#[generate_http_handler]
pub async fn list_ontology_synonyms(
    ctx: RequestContext,
    params: ListOntologySynonymsRequest,
) -> Result<ListOntologySynonymsResponse> {
    let page = domain()
        .ontology_domain()
        .list_synonyms(
            ctx,
            OntologySynonymQuery {
                target_kind: params.target_kind,
                target_key: params.target_key,
                keyword: None,
                pagination: params.pagination,
            },
        )
        .await?;
    Ok(page.map(|e| super::synonym_item(e.into_po())))
}
