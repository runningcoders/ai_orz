//! Handler: GET /api/v1/hr/ontology/skeleton - 本体骨架视图（活跃类 + 关系类型 + 三元组）

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ontology::{GetOntologySkeletonRequest, GetOntologySkeletonResponse};
use common::error::Result;

/// Get the ontology skeleton view
#[register_handler_tool(
    id = "get_ontology_skeleton",
    name = "Get Ontology Skeleton",
    description = "Get the full ontology skeleton in one call: active entity classes (with parent_term_key inheritance), active relation types, and active class-relation triples. The management page renders this as a directed graph (classes = nodes, triples = edges, inheritance as a distinct edge style). Only active entries are included; retired entries stay in the database for historical interpretation.",
    params = "common::api::GetOntologySkeletonRequest",
    neural,
    tags = "ontology_management,ontology"
)]
#[generate_http_handler]
pub async fn get_ontology_skeleton(
    ctx: RequestContext,
    _params: GetOntologySkeletonRequest,
) -> Result<GetOntologySkeletonResponse> {
    domain().ontology_domain().get_skeleton(ctx).await
}
