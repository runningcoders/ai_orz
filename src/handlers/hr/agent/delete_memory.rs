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
    // 调用主体：人类用户（HTTP）或 Agent（唤醒 / 休息沉淀链路）都可以操作记忆。
    // ⚠️ 不能只认 user —— 休息沉淀的 ctx 由 `RequestContext::new_system()` 还原，
    // 天生没有 user_id（只有 agent_id），而沉淀 prompt 明确要求 Agent 调用本工具。
    // 只认 user 会让这类调用全部 400「当前请求缺少用户上下文」（实测 call_trace 已复现）。
    // 记忆的归属维度是 Agent（短期私有）与蜂巢（知识节点共享），本就与 user 无关。
    if ctx.uid().is_empty() && ctx.agent_id().is_none() {
        bail_err!(InvalidRequest, "当前请求缺少用户/Agent 上下文");
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
