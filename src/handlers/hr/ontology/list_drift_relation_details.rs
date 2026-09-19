//! Handler: GET /api/v1/hr/ontology/drift/relations - 漂移词下钻：关系（边）明细

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ontology::{ListDriftRelationDetailsRequest, ListDriftRelationDetailsResponse};
use common::error::Result;

/// Drill down into drift relation (edge) details
#[register_handler_tool(
    id = "list_ontology_drift_relation_details",
    name = "List Ontology Drift Relation Details",
    description = "Drill down into knowledge-graph edge instances for a term. raw_term accepts either a drifted raw term or a canonical term key (drill-down is not limited to drifted words). Filter by agent_id optionally. Returns paged edge details: id, producer agent, source/target node names, created_at.",
    params = "common::api::ListDriftRelationDetailsRequest",
    tags = "ontology_management"
)]
#[generate_http_handler]
pub async fn list_ontology_drift_relation_details(
    ctx: RequestContext,
    params: ListDriftRelationDetailsRequest,
) -> Result<ListDriftRelationDetailsResponse> {
    domain()
        .ontology_domain()
        .list_drift_relation_details(ctx, params)
        .await
}
