//! Handler: 更新记忆 - Neural Tool

use crate::models::memory::{KnowledgeNodeRelationPo, Memory, MemoryCreateParams, MemoryPo};
use crate::pkg::RequestContext;
use crate::service::dao::memory::MemoryQuery;
use crate::service::domain::runtime::domain as runtime_domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateMemoryParams, UpdateMemoryResponse};
use common::enums::{KnowledgeRelationStatus, MemoryStatus};
use common::error::{Result, bail_err, err};
use serde_json;

/// Update an existing memory entry
#[register_handler_tool(
    id = "update_memory",
    name = "Update Memory Entry",
    description = "Update an existing short_term memory or knowledge_node by id: change content, summary, tags, or status (active/settled/forgotten; any other value is rejected with invalid_request rather than silently ignored); knowledge nodes also accept node_tags to toggle the published flag, and relations to create typed edges to other nodes — this is the ONLY way to add edges to an existing node (mentioning another node in the body text does NOT create an edge: graph traversal only follows the relation table, in-text references are unreachable). Trace and Relation entries cannot be modified. Returns the memory_id.",
    params = "common::api::UpdateMemoryParams",
    neural
)]
#[generate_http_handler]
pub async fn update_memory(
    ctx: RequestContext,
    params: UpdateMemoryParams,
) -> Result<UpdateMemoryResponse> {
    // 调用主体：人类用户（HTTP）或 Agent（唤醒 / 休息沉淀链路）都可以操作记忆。
    // ⚠️ 不能只认 user —— 休息沉淀的 ctx 由 `RequestContext::new_system()` 还原，
    // 天生没有 user_id（只有 agent_id），而沉淀 prompt 明确要求 Agent 调用本工具
    // （把已处理的短期记忆标 `Settled`）。只认 user 会让这类调用全部 400，实测
    // call_trace 已复现（update_memory 失败 10 次）。记忆归属是 Agent / 蜂巢维度。
    if ctx.uid().is_empty() && ctx.agent_id().is_none() {
        bail_err!(InvalidRequest, "当前请求缺少用户/Agent 上下文");
    }

    let query = MemoryQuery {
        ids: Some(vec![params.memory_id.clone()]),
        ..Default::default()
    };

    let memories = runtime_domain().memory().query(ctx.clone(), query).await?;
    let memory = memories
        .into_iter()
        .next()
        .ok_or_else(|| err!(NotFound, "记忆 {} 不存在", params.memory_id))?;

    // relations 只对知识节点有意义（短期记忆没有节点间关系语义）。
    // 静默忽略会让调用方以为边已建立——沉淀联想悄悄丢失，比报错更糟。
    if params.relations.is_some() && !matches!(memory.po, MemoryPo::KnowledgeNode(_)) {
        bail_err!(
            InvalidRequest,
            "relations 仅支持知识节点；短期记忆不支持建边（联想请沉淀为知识节点后再建关系）"
        );
    }

    let updated_memory = match memory.po {
        MemoryPo::ShortTerm(st) => {
            let mut updated = st.clone();
            let now = chrono::Utc::now().timestamp();

            if let Some(content) = &params.content {
                updated.summary = content.clone();
            }
            if let Some(summary) = &params.summary {
                updated.summary = summary.clone();
            }
            if let Some(tags) = &params.tags {
                updated.tags = serde_json::to_string(tags)?;
            }
            // 支持 status 更新（如标记为 Settled）。
            // ⚠️ 非法值报 400，**不**兜底成 Active —— 否则调用方拼错会把节点悄悄改成活跃。
            if let Some(status_str) = &params.status {
                updated.status = MemoryStatus::parse(status_str).ok_or_else(|| {
                    err!(
                        InvalidRequest,
                        "不支持的 status: `{}`，合法取值：{}",
                        status_str,
                        MemoryStatus::ACCEPTED_VALUES
                    )
                })?;
            }
            updated.updated_at = now;

            Memory {
                po: MemoryPo::ShortTerm(updated),
                search_match: memory.search_match,
            }
        }
        MemoryPo::KnowledgeNode(kn) => {
            let mut updated = kn.clone();
            let now = chrono::Utc::now().timestamp();

            if let Some(content) = &params.content {
                updated.node_description = content.clone();
            }
            if let Some(summary) = &params.summary {
                updated.summary = summary.clone();
            }
            // 新增：支持 KnowledgeNode tags 更新（用于加 published 标签等）
            // 同步 is_published 冗余字段，保证 DB 查询走索引而非 json_each 全表扫描
            if let Some(node_tags) = &params.node_tags {
                updated.is_published = node_tags.iter().any(|t| t == "published");
                updated.tags = serde_json::to_string(node_tags)?;
            }
            // 支持 status 更新（如遗忘节点）。同上：非法值 400，不静默兜底。
            if let Some(status_str) = &params.status {
                updated.status = MemoryStatus::parse(status_str).ok_or_else(|| {
                    err!(
                        InvalidRequest,
                        "不支持的 status: `{}`，合法取值：{}",
                        status_str,
                        MemoryStatus::ACCEPTED_VALUES
                    )
                })?;
            }
            updated.updated_at = now;

            Memory {
                po: MemoryPo::KnowledgeNode(updated),
                search_match: memory.search_match,
            }
        }
        MemoryPo::Trace(_) => {
            bail_err!(UnsupportedOperation, "原始记忆 Trace 不可修改");
        }
        MemoryPo::Relation(_) => {
            bail_err!(UnsupportedOperation, "记忆 Relation 不可修改");
        }
    };

    let result = runtime_domain()
        .memory()
        .update(ctx.clone(), updated_memory)
        .await?;

    // 知识节点更新成功后落地关系边（复用 save_long_term_memory 的建边路径）。
    // 放在 update 之后：内容写失败时不应留下孤儿边。
    if let Some(relations) = params.relations.filter(|rs| !rs.is_empty()) {
        let now = chrono::Utc::now().timestamp();
        let relation_pos: Vec<KnowledgeNodeRelationPo> = relations
            .iter()
            .map(|r| KnowledgeNodeRelationPo {
                id: format!("kr_{}", uuid::Uuid::now_v7().simple()),
                source_node_id: r.source_node_id.clone(),
                target_node_id: r.target_node_id.clone(),
                // 原文直落：词表外的标注原样保留（与 save_long_term_memory 一致）
                relation_type: r.relation_type.trim().to_string(),
                weight: r.normalized_weight(),
                status: KnowledgeRelationStatus::Active,
                created_at: now,
                updated_at: now,
            })
            .collect();
        runtime_domain()
            .memory()
            .create(ctx, MemoryCreateParams::CreateRelations(relation_pos))
            .await?;
    }

    let memory_id = match &result.po {
        MemoryPo::ShortTerm(st) => st.id.clone(),
        MemoryPo::KnowledgeNode(kn) => kn.id.clone(),
        _ => params.memory_id.clone(),
    };

    Ok(UpdateMemoryResponse { memory_id })
}

// （原地的 `parse_memory_status` 已删除：解析收敛到 `MemoryStatus::parse`，
//   且非法值不再兜底成 Active，而是报 400。）
#[cfg(test)]
#[path = "update_memory_tests.rs"]
mod tests;
