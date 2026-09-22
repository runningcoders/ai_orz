//! Handler: 更新记忆 - Neural Tool

use crate::models::memory::{Memory, MemoryPo};
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
    description = "Update an existing short_term memory or knowledge_node by id: change content, summary, tags, or status (active/settled/forgotten; any other value is rejected with invalid_request rather than silently ignored); knowledge nodes also accept node_tags to toggle the published flag. Trace and Relation entries cannot be modified. Returns the memory_id.",
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
        .update(ctx, updated_memory)
        .await?;

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
mod tests {
    use super::*;
    use crate::handlers::hr::agent::save_short_term_memory::save_short_term_memory;
    use crate::service::dao::memory::MemoryQuery;
    use common::api::{SaveShortTermMemoryParams, UpdateMemoryParams};

    fn init_env(pool: sqlx::SqlitePool) -> RequestContext {
        let _ = crate::config::init();
        let base_path = crate::config::get().base_data_path();
        crate::pkg::tool_tracing::logger::ToolCallLogger::init(base_path);
        crate::service::dao::init_all();
        crate::service::dal::init_all();
        crate::service::domain::runtime::init();
        crate::pkg::request_context_test_support::new_test_ctx("test-user", pool)
    }

    /// 造一条真实的短期记忆（走真实写入路径，不手搓 PO）。
    async fn seed_short_term(ctx: &RequestContext) -> String {
        save_short_term_memory(
            ctx.clone(),
            SaveShortTermMemoryParams {
                summary: "待更新的短期记忆".to_string(),
                tags: None,
                task_id: None,
                content: None,
                trace_ids: None,
            },
        )
        .await
        .expect("保存短期记忆应成功")
        .memory_id
    }

    /// 断言指定 id 的短期记忆当前状态为 `expected`。
    ///
    /// ⚠️ 用「ids + **显式** status」查询：通用查询在 status 未指定时会默认排除
    /// Forgotten（`status != 0`，软删除语义），而本测试恰恰要读取被遗忘的记忆；
    /// 按 id 直取的 `get_short_term_index` 同样带 `status != 0` 过滤，两者都会让
    /// Forgotten 行不可见。显式指定 status 才能覆盖全部状态。
    async fn assert_status_is(ctx: &RequestContext, id: &str, expected: MemoryStatus) {
        let pos = crate::service::dao::memory::dao()
            .query_short_term(
                ctx.clone(),
                MemoryQuery {
                    ids: Some(vec![id.to_string()]),
                    status: Some(expected),
                    ..Default::default()
                },
            )
            .await
            .expect("查询应成功");
        assert_eq!(
            pos.len(),
            1,
            "应恰好查到 1 条状态为 {expected:?} 的记忆，实际: {pos:?}"
        );
        assert_eq!(pos[0].status, expected);
    }

    fn update_params(memory_id: &str, status: Option<&str>) -> UpdateMemoryParams {
        UpdateMemoryParams {
            memory_id: memory_id.to_string(),
            content: None,
            summary: None,
            tags: None,
            status: status.map(|s| s.to_string()),
            node_tags: None,
        }
    }

    /// 非法 `status` 必须报 400，且**绝不能顺手把记忆写成 Active**。
    ///
    /// 回归背景：原 `_ => MemoryStatus::Active` 兜底把「拼错状态」变成一次**静默写入** ——
    /// 调用方以为只是参数无效被忽略，实际记忆的活跃度已经被改掉了。
    #[sqlx::test]
    async fn invalid_status_is_rejected_and_never_silently_written(pool: sqlx::SqlitePool) {
        let ctx = init_env(pool);
        let id = seed_short_term(&ctx).await;
        assert_status_is(&ctx, &id, MemoryStatus::Active).await;

        let e = update_memory(ctx.clone(), update_params(&id, Some("actived")))
            .await
            .expect_err("拼错的 status 必须报错");
        let msg = e.to_string();
        assert!(msg.contains("invalid_request"), "错误码不对: {msg}");
        assert!(msg.contains("status"), "错误信息应指明字段: {msg}");
        assert!(
            msg.contains("forgotten"),
            "应列出合法取值供调用方纠正: {msg}"
        );
        // 状态原封不动（既没被兜底成别的值，也没被写坏）
        assert_status_is(&ctx, &id, MemoryStatus::Active).await;

        // 判别值字符串仍等价于名称（在遗忘之前做：被遗忘的记忆对
        // update_memory 的默认取数不可见（软删除语义），无法再被更新）
        update_memory(ctx.clone(), update_params(&id, Some("2")))
            .await
            .expect("判别值 \"2\" 应等价于 settled");
        assert_status_is(&ctx, &id, MemoryStatus::Settled).await;

        // 合法值是生效的：forgotten 必须真的落库（而不是被兜底成 Active）
        update_memory(ctx.clone(), update_params(&id, Some("forgotten")))
            .await
            .expect("合法 status 应更新成功");
        assert_status_is(&ctx, &id, MemoryStatus::Forgotten).await;
    }
}
