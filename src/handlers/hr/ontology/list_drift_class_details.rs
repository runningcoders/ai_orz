//! Handler: GET /api/v1/hr/ontology/drift/classes - 漂移词下钻：节点明细

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ontology::{ListDriftClassDetailsRequest, ListDriftClassDetailsResponse};
use common::error::Result;

/// Drill down into drift class (node) details
#[register_handler_tool(
    id = "list_ontology_drift_class_details",
    name = "List Ontology Drift Class Details",
    description = "Drill down into knowledge-graph node instances for a term. raw_term accepts either a drifted raw term or a canonical term key (drill-down is not limited to drifted words). Filter by agent_id optionally. Returns paged node details: id, producer agent, node name, created_at.",
    params = "common::api::ListDriftClassDetailsRequest",
    tags = "ontology_management"
)]
#[generate_http_handler]
pub async fn list_ontology_drift_class_details(
    ctx: RequestContext,
    params: ListDriftClassDetailsRequest,
) -> Result<ListDriftClassDetailsResponse> {
    domain()
        .ontology_domain()
        .list_drift_class_details(ctx, params)
        .await
}
