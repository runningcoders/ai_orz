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
    name = "search_memory",
    description = "Search memory by keyword or semantic query, returns matching memory entries",
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

    let agent_id = ctx.agent_id().cloned().unwrap_or_default();

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
    memories.into_iter().map(|m| memory_to_result(&m)).collect()
}

fn memory_to_result(memory: &Memory) -> MemoryResult {
    match &memory.po {
        MemoryPo::Trace(trace) => MemoryResult {
            id: trace.id.clone(),
            content: trace.input.clone(),
            memory_type: "trace".to_string(),
            score: memory.search_match.as_ref().and_then(|m| m.vector_distance),
            summary: None,
            source_node_id: None,
            target_node_id: None,
            relation_type: None,
            tags: None,
        },
        MemoryPo::ShortTerm(st) => MemoryResult {
            id: st.id.clone(),
            content: st.summary.clone(),
            memory_type: "short_term".to_string(),
            score: memory.search_match.as_ref().and_then(|m| m.vector_distance),
            summary: Some(st.summary.clone()),
            source_node_id: None,
            target_node_id: None,
            relation_type: None,
            tags: Some(parse_tags_json(&st.tags)),
        },
        MemoryPo::KnowledgeNode(kn) => MemoryResult {
            id: kn.id.clone(),
            content: kn.node_description.clone(),
            memory_type: "knowledge_node".to_string(),
            score: memory.search_match.as_ref().and_then(|m| m.vector_distance),
            summary: Some(kn.summary.clone()),
            source_node_id: None,
            target_node_id: None,
            relation_type: None,
            tags: Some(parse_tags_json(&kn.tags)),
        },
        MemoryPo::Relation(rel) => MemoryResult {
            id: rel.id.clone(),
            content: format!("{:?}", rel.relation_type),
            memory_type: "relation".to_string(),
            score: memory.search_match.as_ref().and_then(|m| m.vector_distance),
            summary: None,
            source_node_id: Some(rel.source_node_id.clone()),
            target_node_id: Some(rel.target_node_id.clone()),
            relation_type: Some(format!("{:?}", rel.relation_type)),
            tags: None,
        },
    }
}

/// 解析 tags JSON 数组字符串为 Vec<String>，解析失败返回空 Vec
fn parse_tags_json(tags_json: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(tags_json).unwrap_or_default()
}
