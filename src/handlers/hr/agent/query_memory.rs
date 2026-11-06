//! Handler: 查询记忆 - Neural Tool

use super::property_attach::{AttachOptions, attach_properties};
use crate::models::memory::Memory;
use crate::pkg::RequestContext;
use crate::service::dao::memory::MemoryQuery;
use crate::service::domain::runtime::domain as runtime_domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{MemoryResult, QueryMemoryParams, QueryMemoryResponse};
use common::enums::{MemoryStatus, MemoryType};
use common::error::{Result, bail_err, err};

/// Query memory entries by filter conditions
#[register_handler_tool(
    id = "query_memory",
    name = "Query Agent Memory",
    description = "Query memory entries by structured filters: ids (exact fetch by memory id — use this to expand an index line into its full content), agent_id, memory_type (short_term/knowledge_node/trace/relation/all), status (active/settled/forgotten), tags, and task_id. ids takes a list of exact memory ids and combines with other filters (AND semantics); an empty list or blank-only ids is rejected with invalid_request instead of silently returning everything. Values outside those lists are rejected with invalid_request rather than silently ignored. Knowledge nodes are hive-shared (any agent can read all of them), so agent_id is an optional ownership filter rather than a permission gate; short-term memory is scoped to the calling agent. For relevance-ranked free-text lookup use search_memory.",
    params = "common::api::QueryMemoryParams",
    neural
)]
#[generate_http_handler]
pub async fn query_memory(
    ctx: RequestContext,
    params: QueryMemoryParams,
) -> Result<QueryMemoryResponse> {
    // 调用主体：人类用户（HTTP）或 Agent（唤醒 / 休息沉淀链路）都可以操作记忆。
    // ⚠️ 不能只认 user —— 休息沉淀的 ctx 由 `RequestContext::new_system()` 还原，
    // 天生没有 user_id（只有 agent_id），而沉淀 prompt 明确要求 Agent 调用本工具。
    // 只认 user 会让这类调用全部 400「当前请求缺少用户上下文」（实测 call_trace 已复现）。
    // 记忆的归属维度是 Agent（短期私有）与蜂巢（知识节点共享），本就与 user 无关。
    if ctx.uid().is_empty() && ctx.agent_id().is_none() {
        bail_err!(InvalidRequest, "当前请求缺少用户/Agent 上下文");
    }

    // 类型/状态统一走枚举自带的 SSOT 解析，非法值直接 400。
    // ⚠️ 刻意**不**用 `_ =>` 兜底成 All / Active：静默降级会让调用方拼错一个词就拿到
    // 全量（或「只 active」）的结果，而响应看起来完全成功 —— 最难察觉的一类参数错误。
    let memory_type = match params.memory_type.as_deref() {
        None => MemoryType::All,
        Some(raw) => MemoryType::parse(raw).ok_or_else(|| {
            err!(
                InvalidRequest,
                "不支持的 memory_type: `{}`，合法取值：{}",
                raw,
                MemoryType::ACCEPTED_VALUES
            )
        })?,
    };

    let status = match params.status.as_deref() {
        None => None,
        Some(raw) => Some(MemoryStatus::parse(raw).ok_or_else(|| {
            err!(
                InvalidRequest,
                "不支持的 status: `{}`，合法取值：{}",
                raw,
                MemoryStatus::ACCEPTED_VALUES
            )
        })?),
    };

    // 归属：可选筛选，不是权限门槛，且**只认显式传入的 `params.agent_id`**。
    // ⚠️ 刻意不回退 `ctx.agent_id()`：知识节点蜂巢共享，回退会让 Agent 查询静默收窄成
    // 「只看自己的节点」。短期记忆（私有）需要的归属回退在 DAL 内部完成
    // （`private_agent_scope`：缺省回退 ctx 自己的 Agent，Agent 调用天然只看自己）。
    let query_agent_id = params.agent_id.clone().filter(|s| !s.is_empty());

    // ids = 索引块的反查通道：点名直取，不参与语义排序。
    // ⚠️ 空数组必须 400 而不是退化成「不过滤」——后者会静默返回全量记忆，
    // 既贵又最难察觉；索引层永远会带至少一个 id。顺手 trim + 保序去重，
    // 避免 `id IN ()` 这种 SQLite 语法错误和重复参数把 SQL 撑大。
    let ids = params.ids.as_ref().map(|raw| {
        let mut seen: Vec<&str> = Vec::new();
        for id in raw {
            let id = id.trim();
            if !id.is_empty() && !seen.contains(&id) {
                seen.push(id);
            }
        }
        seen.into_iter().map(str::to_string).collect::<Vec<_>>()
    });
    if ids.as_ref().is_some_and(Vec::is_empty) {
        bail_err!(
            InvalidRequest,
            "ids 至少需要一个非空 id；空数组不会退化为查询全量"
        );
    }

    let query = MemoryQuery {
        ids,
        agent_id: query_agent_id,
        memory_type: Some(memory_type),
        limit: params.limit.map(|l| l as usize),
        tags: params.tags.clone().filter(|t| !t.is_empty()),
        task_id: params.task_id.clone(),
        status,
        ..Default::default()
    };

    let memories = runtime_domain().memory().query(ctx.clone(), query).await?;
    let mut results = memories_to_results(memories);

    // 属性回显：与 search_memory 共用 `property_attach::attach_properties` ——
    // (d)「唯一装配源」：结构化通道此前完全不回显属性，与语义通道形成硬缺口。
    // 默认 `include_history = false`，只回生效值。
    attach_properties(
        &ctx,
        &mut results,
        AttachOptions::from_param(params.include_history),
    )
    .await?;

    Ok(QueryMemoryResponse { results })
}

fn memories_to_results(memories: Vec<Memory>) -> Vec<MemoryResult> {
    // 与 search_memory 共用 `Memory::to_api_result`：
    // 结构化查询的 Memory 不带 search_match，转换后 score / search_match 自然为空
    memories.iter().map(Memory::to_api_result).collect()
}

// （原地的 `parse_memory_status` 已删除：解析收敛到 `MemoryStatus::parse`，
//   且非法值不再兜底成 Active，而是报 400。）

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::memory::{MemoryCreateParams, ShortTermMemoryIndexPo};
    use common::enums::MemoryStatus;

    fn init_env(pool: sqlx::SqlitePool) -> RequestContext {
        let _ = crate::config::init();
        let base_path = crate::config::get().base_data_path();
        crate::pkg::tool_tracing::logger::ToolCallLogger::init(base_path);
        crate::service::dao::init_all();
        crate::service::dal::init_all();
        crate::service::domain::runtime::init();
        crate::pkg::request_context_test_support::new_test_ctx("test-user", pool)
    }

    fn params(memory_type: Option<&str>, status: Option<&str>) -> QueryMemoryParams {
        QueryMemoryParams {
            ids: None,
            agent_id: None,
            memory_type: memory_type.map(|s| s.to_string()),
            limit: Some(5),
            tags: None,
            task_id: None,
            status: status.map(|s| s.to_string()),
            include_history: None,
        }
    }

    /// 结构化过滤里的非法枚举值必须报 400。
    ///
    /// 回归背景：`memory_type` 曾 `_ => All`、`status` 曾 `_ => Active` 兜底 ——
    /// 「参数写错」会伪装成「查到了」（只是结果集悄悄变成全量 / 只剩 active）。
    #[sqlx::test]
    async fn invalid_type_or_status_is_rejected_not_defaulted(pool: sqlx::SqlitePool) {
        let ctx = init_env(pool);

        let e = query_memory(ctx.clone(), params(Some("knowlege_node"), None))
            .await
            .expect_err("拼错的 memory_type 必须报错");
        let msg = e.to_string();
        assert!(msg.contains("invalid_request"), "错误码不对: {msg}");
        assert!(msg.contains("memory_type"), "错误信息应指明字段: {msg}");
        assert!(msg.contains("knowledge_node"), "应列出合法取值: {msg}");

        let e = query_memory(ctx.clone(), params(None, Some("actived")))
            .await
            .expect_err("拼错的 status 必须报错");
        let msg = e.to_string();
        assert!(msg.contains("invalid_request"), "错误码不对: {msg}");
        assert!(msg.contains("status"), "错误信息应指明字段: {msg}");

        // 合法值照常放行：snake_case / PascalCase / 显式 all / 不传
        for good in [
            Some("knowledge_node"),
            Some("KnowledgeNode"),
            Some("all"),
            None,
        ] {
            query_memory(ctx.clone(), params(good, Some("active")))
                .await
                .unwrap_or_else(|e| panic!("合法 memory_type {good:?} 不该报错: {e}"));
        }
        // 判别值字符串（历史调用方形式）仍然等价
        query_memory(ctx.clone(), params(None, Some("2")))
            .await
            .expect("判别值 \"2\" 应等价于 settled");
    }

    async fn seed_short_term(ctx: &RequestContext, id: &str) {
        let now = chrono::Utc::now().timestamp();
        runtime_domain()
            .memory()
            .create(
                ctx.clone(),
                MemoryCreateParams::CreateShortTerm(ShortTermMemoryIndexPo {
                    id: id.to_string(),
                    agent_id: "agent-ids-test".to_string(),
                    task_id: None,
                    role: "user".to_string(),
                    summary: format!("索引行正文 {id}"),
                    tags: "[]".to_string(),
                    trace_ids: "[]".to_string(),
                    status: MemoryStatus::Active,
                    created_at: now,
                    updated_at: now,
                }),
            )
            .await
            .expect("seed 短期记忆");
    }

    fn ids_params(ids: Option<Vec<String>>, status: Option<&str>) -> QueryMemoryParams {
        QueryMemoryParams {
            ids,
            agent_id: Some("agent-ids-test".to_string()),
            ..params(Some("short_term"), status)
        }
    }

    /// 索引 → 详情反查通道：点名直取、空数组不退化成全量、与其它过滤 AND。
    ///
    /// 护栏意义：索引块只给 id + 短摘要，详情必须能按 id 原样取回；
    /// 若空 ids 被当成「不过滤」，一次误传就把全量记忆灌进上下文，
    /// 既烧 token 又最难察觉（响应看起来完全成功）。
    #[sqlx::test]
    async fn ids_fetch_exact_memories_and_empty_ids_is_rejected(pool: sqlx::SqlitePool) {
        let ctx = init_env(pool);
        seed_short_term(&ctx, "st_ids_a").await;
        seed_short_term(&ctx, "st_ids_b").await;
        seed_short_term(&ctx, "st_ids_c").await;

        // 1. 点名直取：只回点名的两条，且带回完整正文
        let resp = query_memory(
            ctx.clone(),
            ids_params(Some(vec!["st_ids_a".into(), "st_ids_c".into()]), None),
        )
        .await
        .expect("按 id 精确取回应成功");
        let got: Vec<&str> = resp.results.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(got.len(), 2, "点名两条就该只回两条: {got:?}");
        assert!(got.contains(&"st_ids_a") && got.contains(&"st_ids_c"));
        assert!(
            resp.results
                .iter()
                .all(|r| r.content.contains("索引行正文")),
            "反查必须带回完整内容，而不是索引里的短摘要"
        );

        // 2. 未知 id → 空结果，不是全量
        let miss = query_memory(
            ctx.clone(),
            ids_params(Some(vec!["st_missing".into()]), None),
        )
        .await
        .expect("未知 id 应是合法查询");
        assert!(miss.results.is_empty(), "未知 id 不得返回任何记忆");

        // 3. trim + 保序去重：重复点名不产生重复行
        let dedup = query_memory(
            ctx.clone(),
            ids_params(
                Some(vec![
                    "  st_ids_a  ".into(),
                    "st_ids_a".into(),
                    "st_ids_b".into(),
                ]),
                None,
            ),
        )
        .await
        .expect("trim + 去重后应成功");
        assert_eq!(dedup.results.len(), 2, "去重后应恰好两条");

        // 4. 与其它过滤条件 AND：status=settled 与 active 的 seed 不相交
        let filtered = query_memory(
            ctx.clone(),
            ids_params(Some(vec!["st_ids_a".into()]), Some("settled")),
        )
        .await
        .expect("AND 过滤应正常返回");
        assert!(filtered.results.is_empty(), "ids 不得架空 status 过滤");

        // 5. 空数组 / 全空串 → 400，绝不静默退化成查询全量
        for bad in [Some(vec![]), Some(vec!["  ".to_string(), String::new()])] {
            let e = query_memory(ctx.clone(), ids_params(bad, None))
                .await
                .expect_err("空 ids 必须报错");
            assert!(
                e.to_string().contains("invalid_request"),
                "空 ids 应报 invalid_request: {e}"
            );
        }
    }
}
