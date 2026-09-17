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
use common::error::{Result, bail_err};

/// Search memory by keyword or semantic query
#[register_handler_tool(
    id = "search_memory",
    name = "Search Memory (Semantic)",
    description = "Search memories with a free-text query via hybrid keyword + vector semantic matching; results are ranked and annotated with match_type, vector_distance, and fts_rank. Optionally pass seed_node_ids with traversal_depth/breadth to expand through the knowledge graph. For structured field filtering use query_memory.",
    params = "common::api::SearchMemoryParams",
    neural
)]
#[generate_http_handler]
pub async fn search_memory(
    ctx: RequestContext,
    params: SearchMemoryParams,
) -> Result<SearchMemoryResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let agent_id = params
        .agent_id
        .clone()
        .or_else(|| ctx.agent_id().cloned())
        .unwrap_or_default();

    let memory_type = params
        .memory_type
        .as_deref()
        .map(|t| match t {
            "short_term" | "ShortTerm" => MemoryType::ShortTerm,
            "knowledge_node" | "KnowledgeNode" => MemoryType::KnowledgeNode,
            "trace" | "Trace" => MemoryType::Trace,
            "relation" | "Relation" => MemoryType::Relation,
            _ => MemoryType::All,
        })
        .unwrap_or(MemoryType::All);

    // 短期记忆是私有的，不共享；KnowledgeNode 和 All 类型包含 published 共享节点
    let include_shared = matches!(memory_type, MemoryType::KnowledgeNode | MemoryType::All);

    let traversal_depth = params.traversal_depth.unwrap_or(0);
    let traversal_breadth = params.traversal_breadth.unwrap_or(0);
    let traversal_strategy = match params.traversal_strategy.as_deref() {
        Some("depth_first") => TraversalStrategy::DepthFirst,
        _ => TraversalStrategy::BreadthFirst,
    };
    let seed_node_ids = params.seed_node_ids.clone().unwrap_or_default();

    let has_seeds = !seed_node_ids.is_empty();
    let do_traversal = traversal_depth > 0;

    let mut all_memories: Vec<Memory> = Vec::new();

    if has_seeds && do_traversal {
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
    } else if !has_seeds && do_traversal {
        let search = MemorySearch {
            keyword: Some(params.query.clone()),
            top_k: params.max_results,
            filters: crate::service::dao::memory::MemoryQuery {
                memory_type: Some(MemoryType::KnowledgeNode),
                limit: params.max_results.map(|l| l as usize),
                tags: params.tags.clone(),
                agent_id: Some(agent_id.clone()),
                include_shared: true,
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
                agent_id: Some(agent_id.clone()),
                include_shared,
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
mod tests {
    use super::*;
    use crate::models::memory::{
        KnowledgeNodeRelationPo, LongTermKnowledgeNodePo, MemoryCreateParams,
    };
    use crate::service::dao::memory::{MemoryQuery, MemorySearch};
    use common::enums::MemoryStatus;

    fn init_env(pool: sqlx::SqlitePool) -> RequestContext {
        let _ = crate::config::init();
        let base_path = crate::config::get().base_data_path();
        crate::pkg::tool_tracing::logger::ToolCallLogger::init(base_path);
        crate::service::dao::init_all();
        crate::service::dal::init_all();
        crate::service::domain::runtime::init();
        crate::pkg::request_context_test_support::new_test_ctx("test-user", pool)
    }

    async fn seed_node(ctx: &RequestContext, id: &str, name: &str, desc: &str, summary: &str) {
        let now = chrono::Utc::now().timestamp();
        let node = LongTermKnowledgeNodePo {
            id: id.to_string(),
            agent_id: "agent-kg".to_string(),
            node_name: name.to_string(),
            node_description: desc.to_string(),
            node_type: "general".to_string(),
            summary: summary.to_string(),
            tags: r#"["published"]"#.to_string(),
            status: MemoryStatus::Active,
            is_published: true,
            created_at: now,
            updated_at: now,
        };
        runtime_domain()
            .memory()
            .create(
                ctx.clone(),
                MemoryCreateParams::CreateKnowledgeNode {
                    node,
                    references: vec![],
                },
            )
            .await
            .unwrap();
    }

    /// 读链路端到端回归：知识图谱页拿到的字段必须是**人可读**的。
    ///
    /// 覆盖用户反馈的三条（「打分 / 摘要 / 内容都是错的、没有信息量」）：
    /// 1. 关系边的 content / relation_type 曾是 `format!("{:?}")` 的 Rust 变体名
    ///    （`"Causes"`），前端关系标签词表只认 `"causes"` → 连线标签退化成英文；
    /// 2. 关系边没有独立正文，至少要能读出中文关系名；
    /// 3. 遍历展开出来的邻居节点/关系边没有匹配过程，`score` 保持 `None`，
    ///    而不是伪造一个 0 让前端显示成「匹配度 0%」。
    #[sqlx::test]
    async fn knowledge_graph_payload_is_human_readable(pool: sqlx::SqlitePool) {
        let ctx = init_env(pool);

        let desc =
            "订单状态机描述了订单从创建到完成的完整状态流转：待支付、已支付、已发货、已完成。";
        // 模拟「调用方没给摘要」的写入：此处显式落空串，验证读侧会归一成 None
        seed_node(&ctx, "kn_a", "订单状态机", desc, "").await;
        seed_node(
            &ctx,
            "kn_b",
            "订单超时补偿",
            "订单超时补偿负责在订单超时后触发回滚。",
            "",
        )
        .await;

        let rel = KnowledgeNodeRelationPo {
            id: "kr_1".to_string(),
            source_node_id: "kn_a".to_string(),
            target_node_id: "kn_b".to_string(),
            relation_type: common::enums::KnowledgeRelationType::Causes,
            // 强度要能一路穿过「迁移 → INSERT → SELECT → PO → DTO」，
            // 任何一层漏掉列，图谱上的线宽就又退化成统一粗细
            weight: Some(0.8),
            created_at: 0,
            updated_at: 0,
        };
        runtime_domain()
            .memory()
            .create(ctx.clone(), MemoryCreateParams::CreateRelations(vec![rel]))
            .await
            .unwrap();

        let search = MemorySearch {
            keyword: Some("订单状态机".to_string()),
            top_k: Some(50),
            filters: MemoryQuery {
                memory_type: Some(MemoryType::KnowledgeNode),
                limit: Some(50),
                agent_id: Some(String::new()),
                include_shared: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let hits = runtime_domain()
            .memory()
            .search(ctx.clone(), search)
            .await
            .unwrap();
        let seed_ids: Vec<String> = hits
            .iter()
            .filter_map(|m| match &m.po {
                MemoryPo::KnowledgeNode(kn) => Some(kn.id.clone()),
                _ => None,
            })
            .collect();

        let mut all = hits;
        all.extend(
            runtime_domain()
                .memory()
                .traverse_graph(
                    ctx.clone(),
                    &seed_ids,
                    1,
                    10,
                    TraversalStrategy::BreadthFirst,
                )
                .await
                .unwrap(),
        );

        let results = memories_to_results(all);

        let relation = results
            .iter()
            .find(|r| r.memory_type == "relation")
            .expect("应返回关系边");
        assert_eq!(
            relation.relation_type.as_deref(),
            Some("causes"),
            "关系类型必须是 Display 的 snake_case，前端关系标签词表以此为 key"
        );
        assert_eq!(relation.content, "导致", "关系边内容应为中文标签");
        assert_eq!(relation.name.as_deref(), Some("导致"));
        assert_eq!(relation.source_node_id.as_deref(), Some("kn_a"));
        assert_eq!(relation.target_node_id.as_deref(), Some("kn_b"));
        assert_eq!(
            relation.weight,
            Some(0.8),
            "关系强度要从库里读回来，否则图谱上的粗细与 hover 读数都是空的"
        );
        assert!(
            !relation.content.contains("Causes"),
            "不应再出现 Rust 变体名: {}",
            relation.content
        );

        let neighbor = results
            .iter()
            .find(|r| r.id == "kn_b")
            .expect("应返回邻居节点");
        assert!(
            neighbor.score.is_none(),
            "遍历展开的邻居没有匹配过程，不应有分值"
        );
        assert!(
            neighbor.summary.is_none(),
            "空摘要要归一成 None，否则前端会渲染一个空的摘要块"
        );
        assert_eq!(neighbor.name.as_deref(), Some("订单超时补偿"));
        assert!(!neighbor.content.is_empty(), "正文不能为空");
        assert!(
            neighbor.weight.is_none(),
            "强度只属于关系边，节点/记忆条目不应带值"
        );
        assert_eq!(
            neighbor.tags,
            Some(Vec::new()),
            "published 是可见性控制位，不该出现在业务标签里"
        );
    }
}
