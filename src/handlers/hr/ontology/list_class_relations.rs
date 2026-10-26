//! Handler: GET /api/v1/hr/ontology/class-relations - 骨架三元组分页列表

use crate::pkg::RequestContext;
use crate::service::dao::ontology::OntologyClassRelationQuery;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ontology::{
    ListOntologyClassRelationsRequest, ListOntologyClassRelationsResponse,
};
use common::error::Result;

/// List ontology skeleton triples (paged)
#[register_handler_tool(
    id = "list_ontology_class_relations",
    name = "List Ontology Class Relations",
    description = "List skeleton triples (class -[relation]-> class) with pagination. Filter by subject_class_key, relation_key, object_class_key, and status (active/retired, defaults to all).",
    params = "common::api::ListOntologyClassRelationsRequest",
    neural,
    tags = "ontology_management,ontology"
)]
#[generate_http_handler]
pub async fn list_ontology_class_relations(
    ctx: RequestContext,
    params: ListOntologyClassRelationsRequest,
) -> Result<ListOntologyClassRelationsResponse> {
    let page = domain()
        .ontology_domain()
        .list_class_relations(
            ctx,
            OntologyClassRelationQuery {
                subject_class_key: params.subject_class_key,
                relation_key: params.relation_key,
                object_class_key: params.object_class_key,
                status: params.status,
                pagination: params.pagination,
            },
        )
        .await?;
    Ok(page.map(|e| super::class_relation_item(e.into_po())))
}
