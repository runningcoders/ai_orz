//! Handler: 搜索记忆 - Neural Tool

use std::collections::HashSet;

use crate::models::memory::{Memory, MemoryPo};
use crate::pkg::RequestContext;
use crate::service::dal::memory::TraversalStrategy;
use crate::service::dao::memory::MemorySearch;
use crate::service::domain::runtime::domain as runtime_domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{MemoryResult, SearchMemoryParams, SearchMemoryResponse};
use common::enums::MemoryType;
use common::error::{Result, bail_err, err};

/// Search memory by keyword or semantic query
#[register_handler_tool(
    id = "search_memory",
    name = "Search Memory (Semantic)",
    description = "Search memories with a free-text query via hybrid keyword + vector semantic matching; results are ranked and annotated with match_type, vector_distance, and fts_rank. memory_type accepts short_term/knowledge_node/trace/relation/all; traversal_strategy accepts breadth_first/depth_first. Values outside those lists are rejected with invalid_request rather than silently defaulted. Knowledge nodes are hive-shared: every agent can read all of them, and agent_id is only an optional ownership filter on the seeds (omit it to search the whole hive). Pass seed_node_ids to expand through the knowledge graph instead of searching: the seed nodes themselves are always returned (the center node is never dropped), traversal_depth/breadth/strategy shape the expansion, and neighbors are never filtered by ownership. For structured field filtering use query_memory.",
    params = "common::api::SearchMemoryParams",
    neural
)]
#[generate_http_handler]
pub async fn search_memory(
    ctx: RequestContext,
    params: SearchMemoryParams,
) -> Result<SearchMemoryResponse> {
    // 调用主体：人类用户（HTTP）或 Agent（唤醒 / 休息沉淀链路）都可以操作记忆。
    // ⚠️ 不能只认 user —— 休息沉淀的 ctx 由 `RequestContext::new_system()` 还原，
    // **天生没有 user_id**（只有 agent_id）：`agent_rest` cron → `agent.settle.requested`
    // → Settle 场景，而沉淀 prompt 明确要求 Agent 调用本工具（TEMPLATE_MEMORY_COGNITION）。
    // 只认 user 会让这类调用全部 400「当前请求缺少用户上下文」，实测 call_trace 已复现。
    // 记忆的归属维度是 Agent（短期私有）与蜂巢（知识节点共享），本就与 user 无关。
    if ctx.uid().is_empty() && ctx.agent_id().is_none() {
        bail_err!(InvalidRequest, "当前请求缺少用户/Agent 上下文");
    }

    // 归属筛选：**只认显式传入的 `params.agent_id`**（空串 = None = 不过滤）。
    // ⚠️ 刻意**不**回退 `ctx.agent_id()`：
    // - 知识节点是蜂巢共享资产，回退会让 Agent 的检索静默收窄成「只看自己沉淀的节点」，
    //   与「所有 Agent 都能看到所有知识节点」相反；
    // - 短期记忆（私有）需要的归属回退由 DAL 内部完成（`private_agent_scope`），
    //   那里才分得清「私有要作用域」和「共享不要门槛」。
    // 该参数只约束**起点**的选取，不约束沿图展开（见下方分支注释）。
    let agent_filter: Option<String> = params.agent_id.clone().filter(|s| !s.is_empty());

    // 类型/遍历策略统一走枚举自带的 SSOT 解析，非法值直接 400（不做 `_ =>` 兜底）。
    // ⚠️ 静默降级会让调用方拼错一个词就拿到全量结果 / 另一种形状的图，而响应看起来成功。
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

    let traversal_depth = params.traversal_depth.unwrap_or(0);
    let traversal_breadth = params.traversal_breadth.unwrap_or(0);
    let traversal_strategy = match params.traversal_strategy.as_deref() {
        None => TraversalStrategy::DEFAULT,
        Some(raw) => TraversalStrategy::parse(raw).ok_or_else(|| {
            err!(
                InvalidRequest,
                "不支持的 traversal_strategy: `{}`，合法取值：{}",
                raw,
                TraversalStrategy::ACCEPTED_VALUES
            )
        })?,
    };
    let seed_node_ids = params.seed_node_ids.clone().unwrap_or_default();

    let has_seeds = !seed_node_ids.is_empty();
    let do_traversal = traversal_depth > 0;

    let mut all_memories: Vec<Memory> = Vec::new();

    if has_seeds {
        // 🎯 点名种子（前端点击节点展开 / 调用方指定起点）：直接沿图展开，**不搜索**。
        // - 种子**必返回**（中心节点缺席的话整张展开图就没有意义了）；
        // - 展开不受归属筛选：知识节点蜂巢共享，邻居不该因为属于别人而被丢掉；
        // - `traversal_depth = 0` 表示「只看种子、不展开」，同样走这条路而不是回退到搜索。
        let traversed = runtime_domain()
            .memory()
            .traverse_graph(
                ctx.clone(),
                &seed_node_ids,
                traversal_depth,
                traversal_breadth,
                traversal_strategy,
            )
            .await?;
        all_memories.extend(traversed);
    } else if do_traversal {
        // 🔍 关键词搜索选起点（**这里**才受归属筛选：`agent_id` 决定从谁的节点起步）
        // → 再沿图展开（蜂巢全域，见 `has_seeds` 分支的说明）。
        let search = MemorySearch {
            keyword: Some(params.query.clone()),
            top_k: params.max_results,
            filters: crate::service::dao::memory::MemoryQuery {
                memory_type: Some(MemoryType::KnowledgeNode),
                limit: params.max_results.map(|l| l as usize),
                tags: params.tags.clone(),
                agent_id: agent_filter.clone(),
                ..Default::default()
            },
            ..Default::default()
        };

        let search_results = runtime_domain()
            .memory()
            .search(ctx.clone(), search)
            .await?;

        let seed_ids: Vec<String> = search_results
            .iter()
            .filter_map(|m| match &m.po {
                MemoryPo::KnowledgeNode(kn) => Some(kn.id.clone()),
                _ => None,
            })
            .collect();

        all_memories.extend(search_results);

        if !seed_ids.is_empty() {
            let traversed = runtime_domain()
                .memory()
                .traverse_graph(
                    ctx.clone(),
                    &seed_ids,
                    traversal_depth,
                    traversal_breadth,
                    traversal_strategy,
                )
                .await?;
            all_memories.extend(traversed);
        }
    } else {
        let search = MemorySearch {
            keyword: Some(params.query.clone()),
            top_k: params.max_results,
            filters: crate::service::dao::memory::MemoryQuery {
                memory_type: Some(memory_type),
                limit: params.max_results.map(|l| l as usize),
                tags: params.tags.clone(),
                task_id: params.task_id.clone(),
                agent_id: agent_filter,
                ..Default::default()
            },
            ..Default::default()
        };

        let search_results = runtime_domain().memory().search(ctx, search).await?;
        all_memories.extend(search_results);
    }

    let mut seen = HashSet::new();
    let mut unique_memories = Vec::new();
    for memory in all_memories {
        let id = memory_id(&memory);
        if seen.insert(id) {
            unique_memories.push(memory);
        }
    }

    let results = memories_to_results(unique_memories);

    Ok(SearchMemoryResponse { results })
}

fn memory_id(memory: &Memory) -> String {
    match &memory.po {
        MemoryPo::Trace(t) => t.id.clone(),
        MemoryPo::ShortTerm(st) => st.id.clone(),
        MemoryPo::KnowledgeNode(kn) => kn.id.clone(),
        MemoryPo::Relation(rel) => rel.id.clone(),
    }
}

fn memories_to_results(memories: Vec<Memory>) -> Vec<MemoryResult> {
    // 字段映射收敛在 `Memory::to_api_result`（与 query_memory 共用一份实现），
    // 这里只负责批量转换
    memories.iter().map(Memory::to_api_result).collect()
}

#[cfg(test)]
#[path = "search_memory_tests.rs"]
mod tests;
