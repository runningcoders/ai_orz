//! Handler: DELETE /api/v1/hr/ontology/synonyms/{id} - 删除同义映射（物理删除）

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ontology::{DeleteOntologySynonymRequest, DeleteOntologySynonymResponse};
use common::error::Result;

/// Delete a synonym mapping (physical delete)
#[register_handler_tool(
    id = "delete_ontology_synonym",
    name = "Delete Ontology Synonym",
    description = "Physically delete a synonym mapping by id. Deletion is idempotent - deleting a non-existent mapping still succeeds. Mappings are interpretation rules, not facts: after deletion the related terms naturally fall back to drift state, effective on the next resolution.",
    params = "common::api::DeleteOntologySynonymRequest",
    tags = "ontology_management"
)]
#[generate_http_handler]
pub async fn delete_ontology_synonym(
    ctx: RequestContext,
    params: DeleteOntologySynonymRequest,
) -> Result<DeleteOntologySynonymResponse> {
    domain()
        .ontology_domain()
        .delete_synonym(ctx, &params.id)
        .await?;
    Ok(DeleteOntologySynonymResponse { success: true })
}
