//! Handler: 保存短期记忆 - Neural Tool

use crate::models::memory::{MemoryCreateParams, MemoryPo, ShortTermMemoryIndexPo};
use crate::pkg::RequestContext;
use crate::service::domain::runtime::domain as runtime_domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{SaveShortTermMemoryParams, SaveShortTermMemoryResponse};
use common::error::{Result, err};
use serde_json;

#[register_handler_tool(
    id = "save_short_term_memory",
    name = "Save to Working Memory",
    description = "Save an entry into the agent's working memory: a summary plus tags, optional task_id, and trace_ids linking back to the originating thinking traces. Returns the memory_id. For durable knowledge use save_long_term_memory; for type-dispatched generic creation use create_memory.",
    params = "common::api::SaveShortTermMemoryParams",
    neural
)]
#[generate_http_handler]
pub async fn save_short_term_memory(
    ctx: RequestContext,
    params: SaveShortTermMemoryParams,
) -> Result<SaveShortTermMemoryResponse> {
    let now = chrono::Utc::now().timestamp();
    let tags_json = serde_json::to_string(&params.tags.unwrap_or_default())?;
    let trace_ids_json = serde_json::to_string(&params.trace_ids.unwrap_or_default())?;

    let id_content = format!("{}{}", params.summary, now);
    let id = format!("st_{}", sha256::digest(id_content));

    let agent_id = ctx.agent_id().cloned().unwrap_or_default();

    let index = ShortTermMemoryIndexPo {
        id: id.clone(),
        agent_id,
        task_id: params.task_id.clone(),
        role: "assistant".to_string(),
        summary: params.summary,
        tags: tags_json,
        trace_ids: trace_ids_json,
        status: common::enums::MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };

    let create_params = MemoryCreateParams::CreateShortTerm(index);
    let results = runtime_domain().memory().create(ctx, create_params).await?;

    let memory_id = results
        .first()
        .map(|m| match &m.po {
            MemoryPo::ShortTerm(st) => st.id.clone(),
            _ => id.clone(),
        })
        .ok_or_else(|| err!(Internal, "保存短期记忆失败，未返回结果"))?;

    Ok(SaveShortTermMemoryResponse { memory_id })
}
