//! Handler: 更新记忆 - Neural Tool

use crate::models::memory::{Memory, MemoryPo};
use crate::pkg::RequestContext;
use crate::service::dao::memory::MemoryQuery;
use crate::service::domain::runtime::domain as runtime_domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateMemoryParams, UpdateMemoryResponse};
use common::error::{Result, bail_err, err};
use serde_json;

/// Update an existing memory entry
#[register_handler_tool(
    id = "update_memory",
    name = "Update Memory Entry",
    description = "Update an existing short_term memory or knowledge_node by id: change content, summary, tags, or status (e.g. mark Settled or Forgotten); knowledge nodes also accept node_tags to toggle the published flag. Trace and Relation entries cannot be modified. Returns the memory_id.",
    params = "common::api::UpdateMemoryParams",
    neural
)]
#[generate_http_handler]
pub async fn update_memory(
    ctx: RequestContext,
    params: UpdateMemoryParams,
) -> Result<UpdateMemoryResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
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
            // 新增：支持 status 更新（如标记为 Settled）
            if let Some(status_str) = &params.status {
                updated.status = parse_memory_status(status_str);
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
            // 新增：支持 status 更新（如遗忘节点）
            if let Some(status_str) = &params.status {
                updated.status = parse_memory_status(status_str);
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
        .update(ctx, updated_memory)
        .await?;

    let memory_id = match &result.po {
        MemoryPo::ShortTerm(st) => st.id.clone(),
        MemoryPo::KnowledgeNode(kn) => kn.id.clone(),
        _ => params.memory_id.clone(),
    };

    Ok(UpdateMemoryResponse { memory_id })
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
