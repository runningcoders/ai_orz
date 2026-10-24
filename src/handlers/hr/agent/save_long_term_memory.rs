//! Handler: 保存长期记忆 - Neural Tool

use super::{
    property_issue_entries, property_issue_strings, save_node_properties,
    write_relations_with_receipt,
};
use crate::models::memory::{
    KnowledgeReferencePo, LongTermKnowledgeNodePo, MemoryCreateParams, MemoryPo,
};
use crate::pkg::RequestContext;
use crate::service::dao::memory::MemoryQuery;
use crate::service::domain::runtime::domain as runtime_domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{SaveLongTermMemoryParams, SaveLongTermMemoryResponse};
use common::error::{Result, err};
use serde_json;

#[register_handler_tool(
    id = "save_long_term_memory",
    name = "Save to Long-Term Memory",
    description = "Persist durable knowledge as a long-term knowledge node (node_name, node_description, node_type, tags) and optionally create typed relations to other nodes in the same call. Each relation accepts an optional weight (0.0-1.0) stating how strong/confirmed the association is; it drives edge thickness and color in the knowledge graph and is shown on hover, so omit it when unsure rather than guessing. You may also attach structured properties (term_key + typed value) for facts that need to be filtered or compared, e.g. model_name, effective_from, confidence - leave them out for narrative content, which belongs in node_description. Relation writes are per-item keyed: relations_result[] reports each entry as {key, status, relation_id (success only), error_code, message} — placeholder node IDs (SELF / kn_new_self) and nonexistent endpoints are rejected per-item without failing the batch, and relation_ids lists ONLY the IDs actually persisted (strictly the success items). The node itself is still created even if every relation fails. Returns node_id, relation_ids, relations_result, property_issues (V1 strings, deprecated) and property_issues_v2 (structured {code,message,field}); property issues are per-item partial-success and never fail the node creation. source_short_term_ids declares which short-term memories this node was distilled from: fill it with the [id=...] values of the memories you consolidated, because a node keeps only a summary while the raw detail stays in traces — declaring the source lets the framework record a node -> short-term memory -> original trace chain so the node can be traced back to the original records; omitting it leaves the node an unsourced assertion. For transient working-memory entries use save_short_term_memory.",
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

    // 引用链：知识节点 → 短期记忆 → trace。
    //
    // 节点本身只存摘要（图谱是「知道有这份笔记、大概讲什么」），原始细节住在
    // trace 的 JSONL 里。`source_short_term_ids` 由模型声明「本节点从哪些短期记忆
    // 归纳而来」，框架据此展开 trace 并落物理坐标，让节点能追回当时的原始记录。
    let references = build_knowledge_references(
        &ctx,
        &node_id,
        &agent_id,
        params.source_short_term_ids.as_deref(),
        now,
    )
    .await?;

    let create_params = MemoryCreateParams::CreateKnowledgeNode { node, references };
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

/// 构建「知识节点 → 短期记忆 → trace」引用行
///
/// 粒度是**一个 trace 一行**：引用关系的主体是「谁引用了谁」，一条短期记忆聚合了
/// N 个 trace 就产生 N 行；挤成一行会让 `trace_id` 只能存一个，溯源链从中间断掉。
///
/// # 为什么需要模型声明来源
/// 框架不知道「哪个知识节点来自哪条短期记忆」（那是模型归纳的结果），所以只能由
/// 调用方通过 `source_short_term_ids` 声明；框架负责把它展开成可回溯的引用行。
///
/// # 物理坐标缺失的处理
/// trace 可能因轮转/清理读不到坐标，此时 `date_path` 为空、`line_number = -1`
/// （哨兵，0 是合法行号不能借用）。**引用关系本身照写**——「引用过」是事实，
/// 「还能读到原文」只是附加能力，不该反过来决定引用是否存在。
async fn build_knowledge_references(
    ctx: &RequestContext,
    knowledge_id: &str,
    agent_id: &str,
    source_ids: Option<&[String]>,
    now: i64,
) -> Result<Vec<KnowledgeReferencePo>> {
    let Some(source_ids) = source_ids.filter(|ids| !ids.is_empty()) else {
        return Ok(Vec::new());
    };

    // 1. 取回来源短期记忆（限定本 Agent：模型填错 id 时不该牵出别人的记忆）
    let memories = runtime_domain()
        .memory()
        .query(
            ctx.clone(),
            MemoryQuery {
                ids: Some(source_ids.to_vec()),
                agent_id: Some(agent_id.to_string()),
                memory_type: Some(common::enums::MemoryType::ShortTerm),
                ..Default::default()
            },
        )
        .await?;

    if memories.len() < source_ids.len() {
        log_warn!(
            "知识节点 {} 的部分来源短期记忆未命中: 声明 {} 条, 命中 {} 条（该节点溯源链不完整）",
            knowledge_id,
            source_ids.len(),
            memories.len()
        );
    }

    // 2. 展开每条短期记忆的 trace_ids → (short_term_id, trace_id) 对
    let mut pairs: Vec<(String, String)> = Vec::new();
    for memory in &memories {
        let MemoryPo::ShortTerm(index) = &memory.po else {
            continue;
        };
        let trace_ids: Vec<String> = serde_json::from_str(&index.trace_ids).unwrap_or_default();
        for trace_id in trace_ids {
            pairs.push((index.id.clone(), trace_id));
        }
    }
    if pairs.is_empty() {
        return Ok(Vec::new());
    }

    // 3. 一次解析这批 trace 的物理坐标（单 Agent 目录内扫描，全部命中即提前返回）
    let trace_ids: Vec<String> = pairs.iter().map(|(_, trace_id)| trace_id.clone()).collect();
    // 解析失败只降级为「无坐标」，不牵连节点创建：引用是附属元数据，
    // 它的成败不该影响「知识有没有沉淀下来」这件正事。
    let positions = runtime_domain()
        .memory()
        .resolve_trace_positions(ctx.clone(), agent_id, &trace_ids)
        .await
        .unwrap_or_default();

    // 4. 一个 trace 一行
    let mut references = Vec::with_capacity(pairs.len());
    for (short_term_id, trace_id) in pairs {
        let position = positions.get(&trace_id);
        references.push(KnowledgeReferencePo {
            id: format!("kr_{}", uuid::Uuid::now_v7().simple()),
            knowledge_id: knowledge_id.to_string(),
            short_term_id,
            trace_id,
            date_path: position
                .map(|p| p.date_filename.clone())
                .unwrap_or_default(),
            line_number: position.map(|p| p.line_number as i64).unwrap_or(-1),
            created_at: now,
        });
    }
    Ok(references)
}
