//! Handler: 沉淀记忆 - Neural Tool
//!
//! 将未沉淀的短期记忆总结并沉淀为长期知识图谱。

use crate::pkg::RequestContext;
use crate::service::domain::runtime::domain as runtime_domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{SettleMemoryParams, SettleMemoryResponse};
use common::error::Result;

#[register_handler_tool(
    id = "settle_memory",
    name = "settle_memory",
    description = "Settle unprocessed short-term memories into long-term knowledge graph. This triggers the agent's 'rest' process where it consolidates recent experiences into structured knowledge.",
    params = "common::api::SettleMemoryParams",
    neural
)]
#[generate_http_handler]
pub async fn settle_memory(
    ctx: RequestContext,
    params: SettleMemoryParams,
) -> Result<SettleMemoryResponse> {
    let agent_id = ctx.agent_id().cloned().unwrap_or_default();
    let limit = params.limit.unwrap_or(10);

    let settled_count = runtime_domain()
        .rest_and_settle(ctx, &agent_id, limit)
        .await?;

    Ok(SettleMemoryResponse { settled_count })
}
