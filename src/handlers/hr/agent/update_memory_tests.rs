//! tests 单元测试（拆分自 update_memory.rs）
//!
//! 文件瘦身：原 400 行 → 174 行，测试体 227 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use crate::handlers::hr::agent::save_short_term_memory::save_short_term_memory;
use crate::service::dao::memory::MemoryQuery;
use common::api::{KnowledgeRelationParam, SaveShortTermMemoryParams, UpdateMemoryParams};

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
        relations: None,
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

/// 短期记忆传 relations 必须报 400：静默忽略会让调用方以为边已建立，
/// 沉淀联想悄悄丢失（正是「正文隐形边」问题的工具侧防线）。
#[sqlx::test]
async fn relations_on_short_term_memory_are_rejected(pool: sqlx::SqlitePool) {
    let ctx = init_env(pool);
    let id = seed_short_term(&ctx).await;

    let mut params = update_params(&id, None);
    params.relations = Some(vec![KnowledgeRelationParam {
        source_node_id: id.clone(),
        target_node_id: "kn_other".to_string(),
        relation_type: "related".to_string(),
        weight: None,
    }]);

    let e = update_memory(ctx.clone(), params)
        .await
        .expect_err("短期记忆建边必须报错");
    let msg = e.to_string();
    assert!(msg.contains("invalid_request"), "错误码不对: {msg}");
    assert!(msg.contains("relations"), "错误信息应指明字段: {msg}");
}

/// 既有知识节点用 update_memory 补边：沉淀联想落成显式关系边的唯一路径。
/// 覆盖：词表外关系名原样保留、weight 归一化穿透、内容更新与建边同调用生效。
#[sqlx::test]
async fn relations_create_edges_on_existing_knowledge_node(pool: sqlx::SqlitePool) {
    use crate::handlers::hr::agent::save_long_term_memory::save_long_term_memory;
    use crate::handlers::hr::agent::search_memory::search_memory;
    use common::api::{SaveLongTermMemoryParams, SearchMemoryParams};

    let ctx = init_env(pool);

    let node_a = save_long_term_memory(
        ctx.clone(),
        SaveLongTermMemoryParams {
            node_name: "飞书消息入站".to_string(),
            node_description: "旧描述".to_string(),
            node_type: "concept".to_string(),
            summary: None,
            tags: None,
            relations: None,
            task_id: None,
        },
    )
    .await
    .expect("建节点 A 应成功")
    .node_id;
    let node_b = save_long_term_memory(
        ctx.clone(),
        SaveLongTermMemoryParams {
            node_name: "异步回调骨架".to_string(),
            node_description: "五类出站渠道骨架".to_string(),
            node_type: "concept".to_string(),
            summary: None,
            tags: None,
            relations: None,
            task_id: None,
        },
    )
    .await
    .expect("建节点 B 应成功")
    .node_id;

    // 更新内容 + 补边一次调用完成：词表外关系名「实现」必须原样落库
    let mut params = update_params(&node_a, None);
    params.content = Some("新描述：入站后走两阶段唤醒".to_string());
    params.relations = Some(vec![KnowledgeRelationParam {
        source_node_id: node_a.clone(),
        target_node_id: node_b.clone(),
        relation_type: "实现".to_string(),
        weight: Some(0.8),
    }]);
    update_memory(ctx.clone(), params)
        .await
        .expect("更新节点 + 建边应成功");

    // 用纯图谱遍历验证边真实存在（正文里引用是查不到的——遍历只认关系表）
    let resp = search_memory(
        ctx.clone(),
        SearchMemoryParams {
            query: String::new(),
            max_results: Some(50),
            memory_type: None,
            traversal_depth: Some(1),
            traversal_breadth: Some(10),
            traversal_strategy: Some("breadth_first".to_string()),
            seed_node_ids: Some(vec![node_a.clone()]),
            tags: None,
            task_id: None,
            agent_id: None,
        },
    )
    .await
    .expect("遍历应成功");

    let relation = resp
        .results
        .iter()
        .find(|r| r.memory_type == "relation")
        .expect("更新建立的边必须对遍历可见");
    assert_eq!(relation.source_node_id.as_deref(), Some(node_a.as_str()));
    assert_eq!(relation.target_node_id.as_deref(), Some(node_b.as_str()));
    assert_eq!(
        relation.relation_type.as_deref(),
        Some("实现"),
        "词表外关系名必须原样落库（与 save_long_term_memory 一致）"
    );
    assert_eq!(relation.weight, Some(0.8), "weight 必须穿透到遍历结果");

    let node = resp
        .results
        .iter()
        .find(|r| r.id == node_a)
        .expect("种子节点必须在结果里");
    assert_eq!(
        node.content, "新描述：入站后走两阶段唤醒",
        "内容更新与建边应同调用生效"
    );
}
