//! Handler: 查询记忆 - Neural Tool

use crate::models::memory::{Memory, MemoryPo};
use crate::pkg::RequestContext;
use crate::service::dao::memory::MemoryQuery;
use crate::service::domain::runtime::domain as runtime_domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{MemoryResult, QueryMemoryParams, QueryMemoryResponse};
use common::enums::MemoryType;
use common::error::{Result, bail_err};

/// Query memory entries by filter conditions
#[register_handler_tool(
    id = "query_memory",
    name = "query_memory",
    description = "Query memory entries by agent_id, memory_type, and other filter conditions",
    params = "common::api::QueryMemoryParams",
    neural
)]
#[generate_http_handler]
pub async fn query_memory(
    ctx: RequestContext,
    params: QueryMemoryParams,
) -> Result<QueryMemoryResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

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

    let query = MemoryQuery {
        agent_id: params.agent_id.clone(),
        memory_type: Some(memory_type),
        limit: params.limit.map(|l| l as usize),
        ..Default::default()
    };

    let memories = runtime_domain().memory().query(ctx, query).await?;
    let results = memories_to_results(memories);

    Ok(QueryMemoryResponse { results })
}

fn memories_to_results(memories: Vec<Memory>) -> Vec<MemoryResult> {
    memories
        .into_iter()
        .map(|m| memory_to_result(&m))
        .collect()
}

fn memory_to_result(memory: &Memory) -> MemoryResult {
    match &memory.po {
        MemoryPo::Trace(trace) => MemoryResult {
            id: trace.id.clone(),
            content: trace.input.clone(),
            memory_type: "trace".to_string(),
            score: memory
                .search_match
                .as_ref()
                .and_then(|m| m.vector_distance),
            summary: None,
        },
        MemoryPo::ShortTerm(st) => MemoryResult {
            id: st.id.clone(),
            content: st.summary.clone(),
            memory_type: "short_term".to_string(),
            score: memory
                .search_match
                .as_ref()
                .and_then(|m| m.vector_distance),
            summary: Some(st.summary.clone()),
        },
        MemoryPo::KnowledgeNode(kn) => MemoryResult {
            id: kn.id.clone(),
            content: kn.node_description.clone(),
            memory_type: "knowledge_node".to_string(),
            score: memory
                .search_match
                .as_ref()
                .and_then(|m| m.vector_distance),
            summary: Some(kn.summary.clone()),
        },
        MemoryPo::Relation(rel) => MemoryResult {
            id: rel.id.clone(),
            content: format!("{:?}", rel.relation_type),
            memory_type: "relation".to_string(),
            score: memory
                .search_match
                .as_ref()
                .and_then(|m| m.vector_distance),
            summary: None,
        },
    }
}
