//! Handler: GET /api/v1/hr/ontology/drift/dashboard - 漂移看板

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ontology::{GetDriftDashboardRequest, GetDriftDashboardResponse};
use common::error::Result;

/// Get the ontology drift dashboard
#[register_handler_tool(
    id = "get_ontology_drift_dashboard",
    name = "Get Ontology Drift Dashboard",
    description = "Get the ontology drift dashboard: top N drifted raw terms (with frequency and distinct-agent multi-source signals), relation-dimension lexicon coverage (canonical hits + synonym merges vs all relations), and node drift statistics. Lazily aggregated from SQLite read paths and self-heals as the lexicon evolves. Filter by agent_id optionally.",
    params = "common::api::GetDriftDashboardRequest",
    tags = "ontology_management"
)]
#[generate_http_handler]
pub async fn get_ontology_drift_dashboard(
    ctx: RequestContext,
    params: GetDriftDashboardRequest,
) -> Result<GetDriftDashboardResponse> {
    domain()
        .ontology_domain()
        .get_drift_dashboard(ctx, params)
        .await
}
