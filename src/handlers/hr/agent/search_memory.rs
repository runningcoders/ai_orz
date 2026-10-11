//! Handler: 搜索记忆 - Neural Tool

use std::collections::HashSet;

use crate::models::memory::{Memory, MemoryPo};
use crate::pkg::RequestContext;
use crate::service::dal::memory::TraversalStrategy;
use crate::service::dao::memory::MemorySearch;
use crate::service::domain::runtime::domain as runtime_domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{
    KnowledgePropertyItem, MemoryResult, PropertyFilterParam, SearchMemoryParams,
    SearchMemoryResponse,
};
use common::enums::MemoryType;
use common::enums::ontology::PropertyTargetType;
use common::error::{Result, bail_err, err};
use common::ontology_property::PropertyValue;

/// Search memory by keyword or semantic query
#[register_handler_tool(
    id = "search_memory",
    name = "Search Memory (Semantic)",
    description = "Search memories with a free-text query via hybrid keyword + vector semantic matching; results are ranked and annotated with match_type, vector_distance, and fts_rank. memory_type accepts short_term/knowledge_node/trace/relation/all; traversal_strategy accepts breadth_first/depth_first. Values outside those lists are rejected with invalid_request rather than silently defaulted. Knowledge nodes are hive-shared: every agent can read all of them, and agent_id is only an optional ownership filter on the seeds (omit it to search the whole hive). Pass seed_node_ids to expand through the knowledge graph instead of searching: the seed nodes themselves are always returned (the center node is never dropped), traversal_depth/breadth/strategy shape the expansion, and neighbors are never filtered by ownership. Pass property_filters for exact structured filtering on node properties (term_key + operator + typed value, AND semantics): it runs as an independent recall channel that resolves matching node ids first instead of re-filtering the semantic top_k, and combines with query as an intersection; it cannot be combined with seed_node_ids, and comparison operators other than eq require a number value. Returned knowledge nodes carry their current properties, so you can see what is already attached (and re-send an item to correct it via update_memory). For structured field filtering on tags/agent/status use query_memory.",
    params = "common::api::SearchMemoryParams",
    neural,
    tags = "ontology"
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

    // 属性过滤入参先落地：后面据此收敛记忆类型与命中集（见「精确召回通道」注释）
    let property_filters: Vec<PropertyFilterParam> =
        params.property_filters.clone().unwrap_or_default();
    let has_property_filters = !property_filters.is_empty();

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
    // 属性过滤是针对**知识节点**的结构化条件（工具面只开放节点载体）：
    // 命中集里只可能是节点 id，混查其他记忆类型只会稀释结果。
    let effective_memory_type = if has_property_filters {
        MemoryType::KnowledgeNode
    } else {
        memory_type
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

    // 🎯 属性过滤：结构化**精确召回**通道（与语义检索互补）
    //
    // 先按属性反查命中节点 id，再把 id 注入 `MemoryQuery.ids` 让 DAO 直取节点——
    // **不是**在语义 top_k 里再筛一遍（那样命中率会被语义排序吃掉）。
    let property_hit_ids: Option<Vec<String>> = match property_filters.as_slice() {
        [] => None,
        filters => {
            // 与点名种子互斥：种子恒返回（遍历不变量），叠加过滤会造成
            // 「传了过滤却仍返回未命中节点」的静默失真，直接报 400 而非悄悄忽略。
            if has_seeds {
                bail_err!(
                    InvalidRequest,
                    "property_filters 与 seed_node_ids 不可同时使用（点名种子展开时种子恒返回，属性过滤无法生效）"
                );
            }
            // 数值运算符必须配 number 值：DAL 侧同样会拒，但那里是
            // 「查询无结果」，这里是「调用方写错参数」，报错要早且要说清。
            for f in filters {
                if let Some(op) = f.op
                    && op.requires_number()
                    && !matches!(f.value, PropertyValue::Number(_))
                {
                    bail_err!(
                        InvalidRequest,
                        "属性 {} 的比较运算 {:?} 仅支持 number 值，当前给了 {}",
                        f.term_key,
                        op,
                        f.value.data_type().as_str()
                    );
                }
            }
            let filters: Vec<common::ontology_property::PropertyFilter> =
                filters.iter().map(|f| f.clone().into_filter()).collect();
            Some(
                runtime_domain()
                    .memory()
                    .query_entities_by_properties(ctx.clone(), &filters)
                    .await?,
            )
        }
    };

    // 命中集为空 = 确实没有符合条件的节点，**短路返回空**而不是回退成全量：
    // ids 为空集若被当成「不过滤」，调用方会拿到全量结果并误以为筛过了。
    if let Some(ids) = &property_hit_ids
        && ids.is_empty()
    {
        return Ok(SearchMemoryResponse {
            results: Vec::new(),
        });
    }

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
                ids: property_hit_ids.clone(),
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
                memory_type: Some(effective_memory_type),
                limit: params.max_results.map(|l| l as usize),
                tags: params.tags.clone(),
                task_id: params.task_id.clone(),
                agent_id: agent_filter,
                ids: property_hit_ids.clone(),
                ..Default::default()
            },
            ..Default::default()
        };

        let search_results = runtime_domain()
            .memory()
            .search(ctx.clone(), search)
            .await?;
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

    // 属性条件是**硬条件**：任何召回通道（FTS / 向量 / 遍历）都不能越过命中集。
    // DAO 的 FTS 通道已经把候选集下推到 SQL，这里再兜一道——混合搜索会合并
    // 多个通道的结果，只改一条通道不足以保证「命中集之外一个都不出现」。
    if let Some(hit_ids) = &property_hit_ids {
        unique_memories.retain(|m| hit_ids.contains(&memory_id(m)));
    }

    let mut results = memories_to_results(unique_memories);

    // 属性回显：一趟批量查完再回填（防 N+1）。
    // 只回显**生效值**——历史版本留在库里可追溯，但不进模型视野（避免它对着旧值推理）。
    attach_properties(&ctx, &mut results).await?;

    Ok(SearchMemoryResponse { results })
}

/// 把命中节点的生效属性回填进结果（无属性的节点保持 `None`，不是空数组）
async fn attach_properties(ctx: &RequestContext, results: &mut [MemoryResult]) -> Result<()> {
    let targets: Vec<(PropertyTargetType, String)> = results
        .iter()
        .filter(|r| r.memory_type == "knowledge_node")
        .map(|r| (PropertyTargetType::Node, r.id.clone()))
        .collect();
    if targets.is_empty() {
        return Ok(());
    }

    let map = runtime_domain()
        .memory()
        .list_entity_properties_batch(ctx.clone(), &targets)
        .await?;

    let key_prefix = PropertyTargetType::Node.as_str();
    for r in results.iter_mut() {
        if r.memory_type != "knowledge_node" {
            continue;
        }
        let Some(pos) = map.get(&(key_prefix.to_string(), r.id.clone())) else {
            continue;
        };
        // 指派列缺失的行 = 脏数据，跳过而不是给一个错类型的值
        let items: Vec<KnowledgePropertyItem> = pos
            .iter()
            .filter_map(|po| {
                Some(KnowledgePropertyItem {
                    term_key: po.term_key.clone(),
                    value: po.value()?,
                    ordinal: po.ordinal,
                })
            })
            .collect();
        if !items.is_empty() {
            r.properties = Some(items);
        }
    }
    Ok(())
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
