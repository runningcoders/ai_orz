//! Handler: 创建记忆 - Neural Tool

use crate::models::memory::{
    LongTermKnowledgeNodePo, MemoryCreateParams, MemoryPo, ShortTermMemoryIndexPo,
};
use crate::pkg::RequestContext;
use crate::service::domain::runtime::domain as runtime_domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CreateMemoryParams, CreateMemoryResponse};
use common::error::{Result, bail_err, err};
use serde_json;

/// Create a new memory entry (short_term or knowledge_node)
#[register_handler_tool(
    id = "create_memory",
    name = "create_memory",
    description = "Create a new memory entry, supports short_term and knowledge_node types",
    params = "common::api::CreateMemoryParams",
    neural
)]
#[generate_http_handler]
pub async fn create_memory(
    ctx: RequestContext,
    params: CreateMemoryParams,
) -> Result<CreateMemoryResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let memory_id = match params.memory_type.as_str() {
        "short_term" => create_short_term(ctx.clone(), params).await?,
        "knowledge_node" => create_knowledge_node(ctx.clone(), params).await?,
        _ => bail_err!(
            InvalidRequest,
            "不支持的记忆类型: {}, 仅支持 short_term 和 knowledge_node",
            params.memory_type
        ),
    };

    Ok(CreateMemoryResponse { memory_id })
}

async fn create_short_term(ctx: RequestContext, params: CreateMemoryParams) -> Result<String> {
    let now = chrono::Utc::now().timestamp();
    let tags_json = serde_json::to_string(&params.tags.unwrap_or_default())?;

    let summary = params.summary.unwrap_or_else(|| params.content.clone());

    let id_content = format!("{}{}", summary, now);
    let id = format!("st_{}", sha256::digest(id_content));

    let agent_id = ctx.agent_id().cloned().unwrap_or_default();

    let index = ShortTermMemoryIndexPo {
        id: id.clone(),
        agent_id,
        task_id: params.task_id.clone(),
        role: "assistant".to_string(),
        summary,
        tags: tags_json,
        trace_ids: "[]".to_string(),
        status: common::enums::MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };

    let create_params = MemoryCreateParams::CreateShortTerm(index);
    let results = runtime_domain().memory().create(ctx, create_params).await?;

    results
        .first()
        .map(|m| match &m.po {
            MemoryPo::ShortTerm(st) => st.id.clone(),
            _ => id.clone(),
        })
        .ok_or_else(|| err!(Internal, "创建短期记忆失败，未返回结果"))
}

async fn create_knowledge_node(ctx: RequestContext, params: CreateMemoryParams) -> Result<String> {
    let now = chrono::Utc::now().timestamp();
    let summary = params.summary.unwrap_or_else(|| params.content.clone());

    let id_content = format!("{}{}", params.content, now);
    let id = format!("kn_{}", sha256::digest(id_content));

    let agent_id = ctx.agent_id().cloned().unwrap_or_default();

    let node = LongTermKnowledgeNodePo {
        id: id.clone(),
        agent_id,
        node_name: params.content.chars().take(50).collect(),
        node_description: params.content.clone(),
        node_type: "general".to_string(),
        summary,
        status: common::enums::MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };

    let create_params = MemoryCreateParams::CreateKnowledgeNode {
        node,
        references: vec![],
    };
    let results = runtime_domain().memory().create(ctx, create_params).await?;

    results
        .first()
        .map(|m| match &m.po {
            MemoryPo::KnowledgeNode(kn) => kn.id.clone(),
            _ => id.clone(),
        })
        .ok_or_else(|| err!(Internal, "创建知识节点失败，未返回结果"))
}
