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
    name = "Query Agent Memory",
    description = "Query memory entries by structured filters: agent_id, memory_type (short_term/knowledge_node/trace/relation), status (active/settled/forgotten), tags, and task_id. Querying another agent's memory yields only its published knowledge nodes. For relevance-ranked free-text lookup use search_memory.",
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

    let status = params.status.as_deref().map(parse_memory_status);

    // 权限校验：获取 ctx 的 agent_id 和查询目标 agent_id
    let ctx_agent_id = ctx.agent_id().cloned().unwrap_or_default();
    let query_agent_id = params
        .agent_id
        .clone()
        .unwrap_or_else(|| ctx_agent_id.clone());
    // 查询其他 Agent 的记忆时，只能看到 published 节点
    let is_querying_other = query_agent_id != ctx_agent_id && !ctx_agent_id.is_empty();

    // 查询他人时，强制只返回 published 节点（通过 tags 过滤实现）
    let mut tags = params.tags.clone().unwrap_or_default();
    if is_querying_other && !tags.contains(&"published".to_string()) {
        tags.push("published".to_string());
    }

    let query = MemoryQuery {
        agent_id: Some(query_agent_id),
        memory_type: Some(memory_type),
        limit: params.limit.map(|l| l as usize),
        tags: if tags.is_empty() { None } else { Some(tags) },
        task_id: params.task_id.clone(),
        status,
        // 查询自己时包含 published 共享节点；查询他人时不包含（仅返回该 agent 的 published 节点）
        include_shared: !is_querying_other,
        ..Default::default()
    };

    let memories = runtime_domain().memory().query(ctx, query).await?;
    let results = memories_to_results(memories);

    Ok(QueryMemoryResponse { results })
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
            search_match: None,
        },
        // 短期记忆 PO 只有一个文本字段 summary（无独立标题/正文）：
        // content 放完整 summary；summary 置 None，由前端显示层
        // 默认取 content 前几行作预览，避免「标题 + 摘要」两行重复。
        MemoryPo::ShortTerm(st) => MemoryResult {
            id: st.id.clone(),
            content: st.summary.clone(),
            memory_type: "short_term".to_string(),
            score: memory.search_match.as_ref().and_then(|m| m.vector_distance),
            summary: None,
            source_node_id: None,
            target_node_id: None,
            relation_type: None,
            tags: Some(parse_tags_json(&st.tags)),
            search_match: None,
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
            search_match: None,
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
            search_match: None,
        },
    }
}

/// 解析 tags JSON 数组字符串为 Vec<String>，解析失败返回空 Vec
fn parse_tags_json(tags_json: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(tags_json).unwrap_or_default()
}

/// 解析记忆状态字符串为 MemoryStatus 枚举。
///
/// 支持的输入（大小写不敏感）：
/// - "active" / "1" → Active
/// - "forgotten" / "0" → Forgotten
/// - "settled" / "2" → Settled
/// - 其他 → Active（兜底）
fn parse_memory_status(s: &str) -> common::enums::MemoryStatus {
    match s.to_lowercase().as_str() {
        "active" | "1" => common::enums::MemoryStatus::Active,
        "forgotten" | "0" => common::enums::MemoryStatus::Forgotten,
        "settled" | "2" => common::enums::MemoryStatus::Settled,
        _ => common::enums::MemoryStatus::Active,
    }
}
