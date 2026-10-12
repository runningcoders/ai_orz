//! Handler: 保存长期记忆 - Neural Tool

use super::{
    property_issue_entries, property_issue_strings, save_node_properties,
    write_relations_with_receipt,
};
use crate::models::memory::{LongTermKnowledgeNodePo, MemoryCreateParams, MemoryPo};
use crate::pkg::RequestContext;
use crate::service::domain::runtime::domain as runtime_domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{SaveLongTermMemoryParams, SaveLongTermMemoryResponse};
use common::error::{Result, err};
use serde_json;

#[register_handler_tool(
    id = "save_long_term_memory",
    name = "Save to Long-Term Memory",
    description = "Persist durable knowledge as a long-term knowledge node (node_name, node_description, node_type, tags) and optionally create typed relations to other nodes in the same call. Each relation accepts an optional weight (0.0-1.0) stating how strong/confirmed the association is; it drives edge thickness and color in the knowledge graph and is shown on hover, so omit it when unsure rather than guessing. You may also attach structured properties (term_key + typed value) for facts that need to be filtered or compared, e.g. model_name, effective_from, confidence - leave them out for narrative content, which belongs in node_description. Relation writes are per-item keyed: relations_result[] reports each entry as {key, status, relation_id (success only), error_code, message} — placeholder node IDs (SELF / kn_new_self) and nonexistent endpoints are rejected per-item without failing the batch, and relation_ids lists ONLY the IDs actually persisted (strictly the success items). The node itself is still created even if every relation fails. Returns node_id, relation_ids, relations_result, property_issues (V1 strings, deprecated) and property_issues_v2 (structured {code,message,field}); property issues are per-item partial-success and never fail the node creation. For transient working-memory entries use save_short_term_memory.",
    params = "common::api::SaveLongTermMemoryParams",
    neural,
    tags = "ontology"
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

    // 属性写入**独立一步**，不并入 MemoryCreateParams：
    // 属性写失败不该连累节点（节点已经建成功了，回滚代价远大于留一个缺属性的节点）。
    // ⚠️ 结果必须回给模型：静默吞掉会让它以为写成功了，下次不再补。
    let property_report = save_node_properties(&ctx, &node_id, params.properties).await?;
    let property_issues = property_issue_strings(&property_report);
    let property_issues_v2 = property_issue_entries(&property_report);

    // 建边走共享实现（与 update_memory 同一份）：占位符 / 格式 / 批内重复 /
    // 端点存在性逐项校验 + 逐条独立落库 + keyed 回执。
    // ⚠️ 回执 relation_id 绑定**实际落库**的边 ID（修复假成功根因①：
    // 旧实现预生成 ID 后直出，节点不存在也会「成功」返回假 ID）。
    let receipt = match params.relations {
        Some(relations) if !relations.is_empty() => {
            write_relations_with_receipt(&ctx, relations, now).await
        }
        _ => super::RelationBatchReceipt {
            results: Vec::new(),
            relation_ids: Vec::new(),
        },
    };

    Ok(SaveLongTermMemoryResponse {
        node_id,
        relation_ids: receipt.relation_ids,
        property_issues,
        relations_result: (!receipt.results.is_empty()).then_some(receipt.results),
        property_issues_v2: (!property_issues_v2.is_empty()).then_some(property_issues_v2),
    })
}

// 属性写入的共享实现在 `super::save_node_properties`（与 update_memory 同一份）：
// 新建与更新两条路径的门闩口径必须一致，否则「新建时能写、补写时被拒」会成为薛定谔行为。
