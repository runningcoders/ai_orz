//! Handler: 保存长期记忆 - Neural Tool

use crate::models::memory::{
    KnowledgeNodeRelationPo, LongTermKnowledgeNodePo, MemoryCreateParams, MemoryPo,
};
use crate::pkg::RequestContext;
use crate::service::domain::runtime::domain as runtime_domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{SaveLongTermMemoryParams, SaveLongTermMemoryResponse};
use common::error::{Result, err};
use serde_json;

#[register_handler_tool(
    id = "save_long_term_memory",
    name = "Save to Long-Term Memory",
    description = "Persist durable knowledge as a long-term knowledge node (node_name, node_description, node_type, tags) and optionally create typed relations to other nodes in the same call. Returns node_id and relation_ids. For transient working-memory entries use save_short_term_memory.",
    params = "common::api::SaveLongTermMemoryParams",
    neural
)]
#[generate_http_handler]
pub async fn save_long_term_memory(
    ctx: RequestContext,
    params: SaveLongTermMemoryParams,
) -> Result<SaveLongTermMemoryResponse> {
    let now = chrono::Utc::now().timestamp();

    let summary = params
        .summary
        .unwrap_or_else(|| params.node_description.chars().take(100).collect());

    // 根据 tags 是否包含 "published" 设置冗余字段 is_published
    let is_published = params
        .tags
        .as_ref()
        .map(|tags| tags.iter().any(|t| t == "published"))
        .unwrap_or(false);

    let tags_json = serde_json::to_string(&params.tags.unwrap_or_default())?;

    let node_id = format!("kn_{}", uuid::Uuid::now_v7().simple());

    let agent_id = ctx.agent_id().cloned().unwrap_or_default();

    let node = LongTermKnowledgeNodePo {
        id: node_id.clone(),
        agent_id: agent_id.clone(),
        node_name: params.node_name.clone(),
        node_description: params.node_description.clone(),
        node_type: params.node_type.clone(),
        summary,
        tags: tags_json,
        status: common::enums::MemoryStatus::Active,
        is_published,
        created_at: now,
        updated_at: now,
    };

    let create_params = MemoryCreateParams::CreateKnowledgeNode {
        node,
        references: vec![],
    };
    let results = runtime_domain()
        .memory()
        .create(ctx.clone(), create_params)
        .await?;

    let _ = results
        .first()
        .map(|m| match &m.po {
            MemoryPo::KnowledgeNode(kn) => kn.id.clone(),
            _ => node_id.clone(),
        })
        .ok_or_else(|| err!(Internal, "创建知识节点失败，未返回结果"))?;

    let mut relation_ids: Vec<String> = Vec::new();

    if let Some(relations) = params.relations
        && !relations.is_empty()
    {
        let relation_pos: Vec<KnowledgeNodeRelationPo> = relations
            .iter()
            .map(|r| {
                let relation_id = format!("kr_{}", uuid::Uuid::now_v7().simple());
                relation_ids.push(relation_id.clone());
                KnowledgeNodeRelationPo {
                    id: relation_id,
                    source_node_id: r.source_node_id.clone(),
                    target_node_id: r.target_node_id.clone(),
                    relation_type: common::enums::KnowledgeRelationType::from(
                        r.relation_type.clone(),
                    ),
                    created_at: now,
                    updated_at: now,
                }
            })
            .collect();

        let create_relations_params = MemoryCreateParams::CreateRelations(relation_pos);
        let _relation_results = runtime_domain()
            .memory()
            .create(ctx, create_relations_params)
            .await?;
    }

    Ok(SaveLongTermMemoryResponse {
        node_id,
        relation_ids,
    })
}
