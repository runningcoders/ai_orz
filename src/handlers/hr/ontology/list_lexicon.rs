//! Handler: GET /api/v1/hr/ontology/lexicon - 词表注入视图（神经技能 / 管理页共用）

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ontology::{ListOntologyLexiconRequest, ListOntologyLexiconResponse};
use common::error::Result;

/// Get the ontology lexicon injection view
#[register_handler_tool(
    id = "list_ontology_lexicon",
    name = "List Ontology Lexicon",
    description = "Get the ontology lexicon injection view in three sections: relation types, entity classes, and synonym samples. Field order reflects the token-trimming priority for neural skill prompt injection (relations first, then classes, then synonym samples). Shared by the neural skill and the management page.",
    params = "common::api::ListOntologyLexiconRequest",
    tags = "ontology_management"
)]
#[generate_http_handler]
pub async fn list_ontology_lexicon(
    ctx: RequestContext,
    _params: ListOntologyLexiconRequest,
) -> Result<ListOntologyLexiconResponse> {
    domain().ontology_domain().list_lexicon(ctx).await
}
