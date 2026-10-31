//! Handler: GET /api/v1/hr/ontology/properties/effectiveness - 属性有效性统计

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ontology::{GetPropertyEffectivenessRequest, GetPropertyEffectivenessResponse};
use common::error::Result;

/// Get property effectiveness statistics
#[register_handler_tool(
    id = "get_property_effectiveness",
    name = "Get Property Effectiveness",
    description = "Get ontology property effectiveness statistics grouped by (term_key, node_type): total values, distinct values, the mode value and its count, mode_ratio, non_default_ratio, and a flagged marker when mode_ratio is strictly greater than the configured threshold (the applied value is echoed back in the response field `threshold`). The default value is the mode (most frequent value) rather than the first enum value; ties are broken by the lexicographically smallest value. Answers 'this property has become a constant on this entity class' when flagged. Read-only SQLite aggregation. Filter by term_key / node_type / min_mode_ratio, capped by limit (default 100).",
    params = "common::api::GetPropertyEffectivenessRequest",
    neural,
    tags = "ontology_management,ontology"
)]
#[generate_http_handler]
pub async fn get_property_effectiveness(
    ctx: RequestContext,
    params: GetPropertyEffectivenessRequest,
) -> Result<GetPropertyEffectivenessResponse> {
    domain()
        .ontology_domain()
        .get_property_effectiveness(ctx, params)
        .await
}
