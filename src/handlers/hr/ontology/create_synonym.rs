//! Handler: POST /api/v1/hr/ontology/synonyms - 新增同义映射

use crate::models::ontology::{OntologySynonymMapping, OntologySynonymMappingPo};
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ontology::{CreateOntologySynonymRequest, CreateOntologySynonymResponse};
use common::error::Result;
use common::ontology::normalize;

/// Create a synonym mapping (drift repair rule)
#[register_handler_tool(
    id = "create_ontology_synonym",
    name = "Create Ontology Synonym",
    description = "Create a synonym mapping that redirects a drifted raw term to a canonical lexicon entry. raw_term is normalized (trimmed + lowercased) before insert. The target must exist in the lexicon for the given target_kind (class or relation). Fails with Conflict if the same (raw_term, target_kind) pair already exists, or InvalidRequest if the target does not exist.",
    params = "common::api::CreateOntologySynonymRequest",
    tags = "ontology_management"
)]
#[generate_http_handler]
pub async fn create_ontology_synonym(
    ctx: RequestContext,
    params: CreateOntologySynonymRequest,
) -> Result<CreateOntologySynonymResponse> {
    // 入参先归一化（与 domain 入库覆写同源幂等），保证返回值与落库行一致
    let po = OntologySynonymMappingPo::new(
        normalize(&params.raw_term),
        params.target_kind.as_str(),
        params.target_key,
    );
    domain()
        .ontology_domain()
        .create_synonym(ctx, &OntologySynonymMapping::from_po(po.clone()))
        .await?;
    Ok(super::synonym_item(po))
}
