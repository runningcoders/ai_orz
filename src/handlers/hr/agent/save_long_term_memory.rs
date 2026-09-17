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
    description = "Persist durable knowledge as a long-term knowledge node (node_name, node_description, node_type, tags) and optionally create typed relations to other nodes in the same call. Each relation accepts an optional weight (0.0-1.0) stating how strong/confirmed the association is; it drives edge thickness and color in the knowledge graph and is shown on hover, so omit it when unsure rather than guessing. Returns node_id and relation_ids. For transient working-memory entries use save_short_term_memory.",
    params = "common::api::SaveLongTermMemoryParams",
    neural
)]
#[generate_http_handler]
pub async fn save_long_term_memory(
    ctx: RequestContext,
    params: SaveLongTermMemoryParams,
) -> Result<SaveLongTermMemoryResponse> {
    let now = chrono::Utc::now().timestamp();

    // ⚠️ 缺省**不伪造摘要**：此前缺省取描述前 100 字，描述短于 100 字时摘要
    // 与正文一字不差 → 图谱卡片第二行 / 详情面板「摘要」与「内容」重复显示。
    // 留空交给前端回退用描述渲染（`node_card::body_lines` 已支持），
    // 只有调用方真的给了摘要才落库。
    let summary = params.summary.clone().unwrap_or_default();

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
                    // 原文直落：词表外的标注（「实现」「implements」…）必须原样保留，
                    // 过一遍枚举会被塌成 Custom、写入方的语义就永久丢了
                    relation_type: r.relation_type.trim().to_string(),
                    // 强度归一化在 DTO 上：非有限值丢弃、越界夹紧到 0.0~1.0，
                    // 未给则 None（未标注 ≠ 0）
                    weight: r.normalized_weight(),
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
