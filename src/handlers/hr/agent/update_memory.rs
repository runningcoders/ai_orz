//! Handler: 更新记忆 - Neural Tool

use super::{
    RelationBatchReceipt, property_issue_entries, property_issue_strings, save_node_properties,
    write_relations_with_receipt,
};
use crate::models::memory::{Memory, MemoryPo, PUBLISHED_TAG, legacy_tags_json};
use crate::pkg::RequestContext;
use crate::service::dao::memory::MemoryQuery;
use crate::service::domain::runtime::domain as runtime_domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateMemoryParams, UpdateMemoryResponse};
use common::enums::MemoryStatus;
use common::error::{Result, bail_err, err};
use serde_json;

/// Update an existing memory entry
#[register_handler_tool(
    id = "update_memory",
    name = "Update Memory Entry",
    description = "Update an existing short_term memory or knowledge_node by id: change content, summary, tags, or status (active/settled/forgotten; any other value is rejected with invalid_request rather than silently ignored); knowledge nodes also accept node_tags for their WRITABLE business tags (a `published` entry there is captured as is_published=true and is NOT stored as a tag) and is_published: Option<bool> (omit / null = leave the publish state untouched, true = publish, false = unpublish - this is the only supported way to change publication), relations to create typed edges to other nodes — this is the ONLY way to add edges to an existing node (mentioning another node in the body text does NOT create an edge: graph traversal only follows the relation table, in-text references are unreachable) — and properties to attach or correct structured facts (term_key + typed value), the ONLY way to put properties on an existing node. Re-writing a single-valued property supersedes the old value (history is kept, only the newest is read back), so correcting a property is just another write, not a delete-then-create. Relation writes are per-item keyed: relations_result[] reports each entry as {key, status, relation_id (success only), error_code, message} — placeholder node IDs (SELF / kn_new_self) and nonexistent endpoints are rejected per-item without failing the update. Rejected property entries are listed in property_issues (V1 strings, deprecated) and property_issues_v2 (structured {code,message,field}), never failing the update itself. Trace and Relation entries cannot be modified, and neither relations nor properties are accepted for short_term memories. Returns memory_id, relations_result, property_issues and property_issues_v2.",
    params = "common::api::UpdateMemoryParams",
    neural,
    tags = "ontology"
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
    let is_node = matches!(memory.po, MemoryPo::KnowledgeNode(_));

    if params.relations.is_some() && !is_node {
        bail_err!(
            InvalidRequest,
            "relations 仅支持知识节点；短期记忆不支持建边（联想请沉淀为知识节点后再建关系）"
        );
    }
    // 属性同样只对知识节点有意义（ABox 属性挂在图谱载体上，短期记忆没有载体语义）。
    // 与 relations 同口径报 400：静默忽略会让模型以为属性已补上。
    if params.properties.is_some() && !is_node {
        bail_err!(
            InvalidRequest,
            "properties 仅支持知识节点；短期记忆不支持挂属性（结构化事实请沉淀为知识节点后再补）"
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
            // 标签写入（拆分后）：`node_tags` 是**可写业务标签** → 落 `user_tags`；
            // `published` 属系统标记 → **不写入 user_tags**，改折算 `is_published = true`
            // （语义收编：系统标记不该躺在业务标签里，回显走 `system_flags`）。
            if let Some(node_tags) = &params.node_tags {
                let user_tags: Vec<String> = node_tags
                    .iter()
                    .filter(|t| t.as_str() != PUBLISHED_TAG)
                    .cloned()
                    .collect();
                if node_tags.iter().any(|t| t == PUBLISHED_TAG) {
                    updated.is_published = true;
                }
                updated.user_tags = serde_json::to_string(&user_tags)?;
                updated.tags = legacy_tags_json(&user_tags, updated.is_published)?;
            }
            // 发布标记：`None` = **保持不变** —— 这条是「读全字段原样回写不再静默
            // 清 published」的关键：不开这个口子，调用方只能靠往 tags 里塞
            // `published` 来保发布，而那正是本次要消灭的双义写法。
            if let Some(is_published) = params.is_published {
                updated.is_published = is_published;
                // 旧列双写跟随，否则 FTS / 旧读侧与 is_published 会脱节
                let user_tags = crate::models::memory::parse_tags_json(&updated.user_tags);
                updated.tags = legacy_tags_json(&user_tags, is_published)?;
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

    let node_id = match &result.po {
        MemoryPo::KnowledgeNode(kn) => Some(kn.id.clone()),
        _ => None,
    };

    // 属性写入独立一步（与 `save_long_term_memory` 共用 `save_node_properties`）：
    // 放在 update 之后——内容写失败时不应留下孤儿属性；写失败也不回滚节点本身。
    // ⚠️ 单值属性重复写 = **软取代**（旧值留历史、读侧只见新值），不是冲突，
    // 所以「修正属性」走 update_memory 是对的，不必先删后建。
    let property_report = match &node_id {
        Some(id) => save_node_properties(&ctx, id, params.properties).await?,
        None => Default::default(),
    };
    let property_issues = property_issue_strings(&property_report);
    let property_issues_v2 = property_issue_entries(&property_report);

    // 知识节点更新成功后落地关系边（与 save_long_term_memory 共用同一份共享实现：
    // 校验口径与回执形态一致——占位符 / 格式 / 批内重复 / 端点存在性逐项校验 +
    // 逐条独立落库 + keyed 回执；修复旧实现「不回执边 ID」的同构缺陷）。
    // 放在 update 之后：内容写失败时不应留下孤儿边。
    let receipt = match params.relations.filter(|rs| !rs.is_empty()) {
        Some(relations) => {
            let now = chrono::Utc::now().timestamp();
            write_relations_with_receipt(&ctx, relations, now).await
        }
        None => RelationBatchReceipt {
            results: Vec::new(),
            relation_ids: Vec::new(),
        },
    };

    let memory_id = match &result.po {
        MemoryPo::ShortTerm(st) => st.id.clone(),
        MemoryPo::KnowledgeNode(kn) => kn.id.clone(),
        _ => params.memory_id.clone(),
    };

    Ok(UpdateMemoryResponse {
        memory_id,
        property_issues,
        relations_result: (!receipt.results.is_empty()).then_some(receipt.results),
        property_issues_v2: (!property_issues_v2.is_empty()).then_some(property_issues_v2),
    })
}

// （原地的 `parse_memory_status` 已删除：解析收敛到 `MemoryStatus::parse`，
//   且非法值不再兜底成 Active，而是报 400。）
#[cfg(test)]
#[path = "update_memory_tests.rs"]
mod tests;
