//! Handler: 查询记忆 - Neural Tool

use crate::models::memory::Memory;
use crate::pkg::RequestContext;
use crate::service::dao::memory::MemoryQuery;
use crate::service::domain::runtime::domain as runtime_domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{MemoryResult, QueryMemoryParams, QueryMemoryResponse};
use common::enums::{MemoryStatus, MemoryType};
use common::error::{Result, bail_err, err};

/// Query memory entries by filter conditions
#[register_handler_tool(
    id = "query_memory",
    name = "Query Agent Memory",
    description = "Query memory entries by structured filters: agent_id, memory_type (short_term/knowledge_node/trace/relation/all), status (active/settled/forgotten), tags, and task_id. Values outside those lists are rejected with invalid_request rather than silently ignored. Knowledge nodes are hive-shared (any agent can read all of them), so agent_id is an optional ownership filter rather than a permission gate; short-term memory is scoped to the calling agent. For relevance-ranked free-text lookup use search_memory.",
    params = "common::api::QueryMemoryParams",
    neural
)]
#[generate_http_handler]
pub async fn query_memory(
    ctx: RequestContext,
    params: QueryMemoryParams,
) -> Result<QueryMemoryResponse> {
    // 调用主体：人类用户（HTTP）或 Agent（唤醒 / 休息沉淀链路）都可以操作记忆。
    // ⚠️ 不能只认 user —— 休息沉淀的 ctx 由 `RequestContext::new_system()` 还原，
    // 天生没有 user_id（只有 agent_id），而沉淀 prompt 明确要求 Agent 调用本工具。
    // 只认 user 会让这类调用全部 400「当前请求缺少用户上下文」（实测 call_trace 已复现）。
    // 记忆的归属维度是 Agent（短期私有）与蜂巢（知识节点共享），本就与 user 无关。
    if ctx.uid().is_empty() && ctx.agent_id().is_none() {
        bail_err!(InvalidRequest, "当前请求缺少用户/Agent 上下文");
    }

    // 类型/状态统一走枚举自带的 SSOT 解析，非法值直接 400。
    // ⚠️ 刻意**不**用 `_ =>` 兜底成 All / Active：静默降级会让调用方拼错一个词就拿到
    // 全量（或「只 active」）的结果，而响应看起来完全成功 —— 最难察觉的一类参数错误。
    let memory_type = match params.memory_type.as_deref() {
        None => MemoryType::All,
        Some(raw) => MemoryType::parse(raw).ok_or_else(|| {
            err!(
                InvalidRequest,
                "不支持的 memory_type: `{}`，合法取值：{}",
                raw,
                MemoryType::ACCEPTED_VALUES
            )
        })?,
    };

    let status = match params.status.as_deref() {
        None => None,
        Some(raw) => Some(MemoryStatus::parse(raw).ok_or_else(|| {
            err!(
                InvalidRequest,
                "不支持的 status: `{}`，合法取值：{}",
                raw,
                MemoryStatus::ACCEPTED_VALUES
            )
        })?),
    };

    // 归属：可选筛选，不是权限门槛，且**只认显式传入的 `params.agent_id`**。
    // ⚠️ 刻意不回退 `ctx.agent_id()`：知识节点蜂巢共享，回退会让 Agent 查询静默收窄成
    // 「只看自己的节点」。短期记忆（私有）需要的归属回退在 DAL 内部完成
    // （`private_agent_scope`：缺省回退 ctx 自己的 Agent，Agent 调用天然只看自己）。
    let query_agent_id = params.agent_id.clone().filter(|s| !s.is_empty());

    let query = MemoryQuery {
        agent_id: query_agent_id,
        memory_type: Some(memory_type),
        limit: params.limit.map(|l| l as usize),
        tags: params.tags.clone().filter(|t| !t.is_empty()),
        task_id: params.task_id.clone(),
        status,
        ..Default::default()
    };

    let memories = runtime_domain().memory().query(ctx, query).await?;
    let results = memories_to_results(memories);

    Ok(QueryMemoryResponse { results })
}

fn memories_to_results(memories: Vec<Memory>) -> Vec<MemoryResult> {
    // 与 search_memory 共用 `Memory::to_api_result`：
    // 结构化查询的 Memory 不带 search_match，转换后 score / search_match 自然为空
    memories.iter().map(Memory::to_api_result).collect()
}

// （原地的 `parse_memory_status` 已删除：解析收敛到 `MemoryStatus::parse`，
//   且非法值不再兜底成 Active，而是报 400。）

#[cfg(test)]
mod tests {
    use super::*;

    fn init_env(pool: sqlx::SqlitePool) -> RequestContext {
        let _ = crate::config::init();
        let base_path = crate::config::get().base_data_path();
        crate::pkg::tool_tracing::logger::ToolCallLogger::init(base_path);
        crate::service::dao::init_all();
        crate::service::dal::init_all();
        crate::service::domain::runtime::init();
        crate::pkg::request_context_test_support::new_test_ctx("test-user", pool)
    }

    fn params(memory_type: Option<&str>, status: Option<&str>) -> QueryMemoryParams {
        QueryMemoryParams {
            agent_id: None,
            memory_type: memory_type.map(|s| s.to_string()),
            limit: Some(5),
            tags: None,
            task_id: None,
            status: status.map(|s| s.to_string()),
        }
    }

    /// 结构化过滤里的非法枚举值必须报 400。
    ///
    /// 回归背景：`memory_type` 曾 `_ => All`、`status` 曾 `_ => Active` 兜底 ——
    /// 「参数写错」会伪装成「查到了」（只是结果集悄悄变成全量 / 只剩 active）。
    #[sqlx::test]
    async fn invalid_type_or_status_is_rejected_not_defaulted(pool: sqlx::SqlitePool) {
        let ctx = init_env(pool);

        let e = query_memory(ctx.clone(), params(Some("knowlege_node"), None))
            .await
            .expect_err("拼错的 memory_type 必须报错");
        let msg = e.to_string();
        assert!(msg.contains("invalid_request"), "错误码不对: {msg}");
        assert!(msg.contains("memory_type"), "错误信息应指明字段: {msg}");
        assert!(msg.contains("knowledge_node"), "应列出合法取值: {msg}");

        let e = query_memory(ctx.clone(), params(None, Some("actived")))
            .await
            .expect_err("拼错的 status 必须报错");
        let msg = e.to_string();
        assert!(msg.contains("invalid_request"), "错误码不对: {msg}");
        assert!(msg.contains("status"), "错误信息应指明字段: {msg}");

        // 合法值照常放行：snake_case / PascalCase / 显式 all / 不传
        for good in [
            Some("knowledge_node"),
            Some("KnowledgeNode"),
            Some("all"),
            None,
        ] {
            query_memory(ctx.clone(), params(good, Some("active")))
                .await
                .unwrap_or_else(|e| panic!("合法 memory_type {good:?} 不该报错: {e}"));
        }
        // 判别值字符串（历史调用方形式）仍然等价
        query_memory(ctx.clone(), params(None, Some("2")))
            .await
            .expect("判别值 \"2\" 应等价于 settled");
    }
}
