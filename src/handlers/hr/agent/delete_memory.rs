//! Handler: 删除记忆 - Neural Tool

use crate::models::memory::MemoryPo;
use crate::pkg::RequestContext;
use crate::service::dao::memory::MemoryQuery;
use crate::service::domain::runtime::domain as runtime_domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{DeleteMemoryParams, DeleteMemoryResponse};
use common::error::{Result, bail_err, err};

/// Delete a memory entry by ID
#[register_handler_tool(
    id = "delete_memory",
    name = "Delete Memory Entry",
    description = "Delete a memory entry by id; supports short_term entries, knowledge_nodes, and knowledge relations (relations are soft-deleted and can be restored). Traces are protected. Returns the deleted memory_id. Fails with NotFound if the id does not exist.",
    params = "common::api::DeleteMemoryParams",
    neural
)]
#[generate_http_handler]
pub async fn delete_memory(
    ctx: RequestContext,
    params: DeleteMemoryParams,
) -> Result<DeleteMemoryResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let query = MemoryQuery {
        ids: Some(vec![params.memory_id.clone()]),
        ..Default::default()
    };

    let memories = runtime_domain().memory().query(ctx.clone(), query).await?;
    let memory = memories
        .into_iter()
        .next()
        .ok_or_else(|| err!(NotFound, "记忆 {} 不存在", params.memory_id))?;

    // 仅拦截 Trace：短期记忆/知识节点走删除，关系边走软删除（标记 Deleted 可恢复）
    if let MemoryPo::Trace(_) = &memory.po {
        bail_err!(UnsupportedOperation, "原始记忆 Trace 不可删除");
    }

    runtime_domain().memory().delete(ctx, memory).await?;

    Ok(DeleteMemoryResponse {
        memory_id: params.memory_id,
    })
}
