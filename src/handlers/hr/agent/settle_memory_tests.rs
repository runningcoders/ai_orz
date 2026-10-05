//! tests 单元测试（拆分自 settle_memory.rs）
//!
//! 文件瘦身：原 674 行 → 409 行，测试体 266 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use crate::models::agent::Agent;
use crate::models::memory::{MemoryCreateParams, ShortTermMemoryIndexPo};

/// 初始化测试环境：config + 所有 DAO/DAL 单例 + Runtime Domain
///
/// 本模块是 Adapter 层，只通过 Domain 访问记忆，因此测试也必须把 Domain
/// 依赖的单例全部拉起。池由 `new_test_ctx` 注入（sqlx::test 的内存库），
/// 各 DAO 单例本身无状态，不会串到真实库。
fn init_settle_test_env(pool: sqlx::SqlitePool) -> RequestContext {
    let _ = crate::config::init();
    let base_path = crate::config::get().base_data_path();
    crate::pkg::tool_tracing::logger::ToolCallLogger::init(base_path);

    crate::service::dao::init_all();
    crate::service::dal::init_all();
    crate::service::domain::runtime::init();
    // settle_agent_exclusive 走 hr domain 加载 Agent（旧测试只碰记忆，不需要）
    crate::service::domain::hr::init();

    crate::pkg::request_context_test_support::new_test_ctx("test-user", pool)
}

async fn seed(ctx: &RequestContext, id: &str, offset_ms: i64) {
    let now = chrono::Utc::now().timestamp_millis() + offset_ms;
    runtime_domain()
        .memory()
        .create(
            ctx.clone(),
            MemoryCreateParams::CreateShortTerm(ShortTermMemoryIndexPo {
                id: id.to_string(),
                agent_id: "agent-settle".to_string(),
                task_id: None,
                role: "user".to_string(),
                summary: format!("摘要 {}", id),
                tags: "[]".to_string(),
                trace_ids: "[]".to_string(),
                status: MemoryStatus::Active,
                created_at: now,
                updated_at: now,
            }),
        )
        .await
        .unwrap();
}

async fn status_of(ctx: &RequestContext, id: &str) -> MemoryStatus {
    let list = runtime_domain()
        .memory()
        .query(
            ctx.clone(),
            MemoryQuery {
                ids: Some(vec![id.to_string()]),
                agent_id: Some("agent-settle".to_string()),
                memory_type: Some(MemoryType::ShortTerm),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    short_term_of(list.first().expect("memory should exist"))
        .expect("should be short term")
        .status
}

/// 核心语义：兜底只翻转仍为 Active 的，Agent 已处理的不覆盖，批次外的不误伤
#[sqlx::test]
async fn mark_pending_settled_only_flips_still_active(pool: sqlx::SqlitePool) {
    let ctx = init_settle_test_env(pool);

    // st-1: 模型漏调 update_memory，仍为 Active → 应被兜底置为 Settled
    // st-2: 模型已自行置为 Settled → 不应被改动
    // st-3: 不在本批 id 列表里 → 不应被误伤
    seed(&ctx, "st-1", 0).await;
    seed(&ctx, "st-2", 1).await;
    seed(&ctx, "st-3", 2).await;

    // 模拟模型已处理 st-2
    let mut done = runtime_domain()
        .memory()
        .query(
            ctx.clone(),
            MemoryQuery {
                ids: Some(vec!["st-2".to_string()]),
                agent_id: Some("agent-settle".to_string()),
                memory_type: Some(MemoryType::ShortTerm),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    short_term_of_mut(&mut done).unwrap().status = MemoryStatus::Settled;
    runtime_domain()
        .memory()
        .update(ctx.clone(), done)
        .await
        .unwrap();

    let marked = runtime_domain()
        .memory()
        .mark_short_term_settled(
            ctx.clone(),
            "agent-settle",
            &["st-1".to_string(), "st-2".to_string()],
        )
        .await
        .unwrap();

    // 只翻转了 st-1
    assert_eq!(marked, 1);
    assert_eq!(status_of(&ctx, "st-1").await, MemoryStatus::Settled);
    // 模型已置位的保持 Settled，且未被重复处理
    assert_eq!(status_of(&ctx, "st-2").await, MemoryStatus::Settled);
    // 批次外不受影响
    assert_eq!(status_of(&ctx, "st-3").await, MemoryStatus::Active);
}

/// 空批次直接返回
#[sqlx::test]
async fn mark_pending_settled_noops_on_empty(pool: sqlx::SqlitePool) {
    let ctx = init_settle_test_env(pool);
    assert_eq!(
        runtime_domain()
            .memory()
            .mark_short_term_settled(ctx.clone(), "agent-settle", &[])
            .await
            .unwrap(),
        0
    );
}

/// 待沉淀队列按最早优先取，避免老记忆被新记忆挤出窗口
#[sqlx::test]
async fn build_pending_summary_uses_oldest_first(pool: sqlx::SqlitePool) {
    let ctx = init_settle_test_env(pool);

    seed(&ctx, "p-old", 0).await;
    seed(&ctx, "p-mid", 10).await;
    seed(&ctx, "p-new", 20).await;

    // 条数上限压到 2，预算给足
    let (summary, ids, truncated) = build_pending_memories_summary(&ctx, "agent-settle", 10_000, 2)
        .await
        .unwrap()
        .expect("应有待沉淀记忆");

    assert_eq!(ids, vec!["p-old".to_string(), "p-mid".to_string()]);
    assert!(summary.contains("[id=p-old]"));
    assert!(!summary.contains("[id=p-new]"));
    // 候选集被 limit 截断，保守标记为仍有剩余
    assert!(truncated);
}

/// 预算自适应：只要累计长度还在预算内就继续拼，而不是固定条数
#[sqlx::test]
async fn build_pending_summary_accumulates_within_budget(pool: sqlx::SqlitePool) {
    let ctx = init_settle_test_env(pool);

    for i in 0..5 {
        seed(&ctx, &format!("b-{}", i), i).await;
    }

    // 预算充足 → 全部纳入
    let (_summary, ids, truncated) =
        build_pending_memories_summary(&ctx, "agent-settle", 100_000, PENDING_MAX_ITEMS)
            .await
            .unwrap()
            .expect("应有待沉淀记忆");
    assert_eq!(ids.len(), 5);
    assert!(!truncated);

    // 预算极小 → 只纳入能放下的前几条，并标记截断
    let (summary, ids, truncated) =
        build_pending_memories_summary(&ctx, "agent-settle", 60, PENDING_MAX_ITEMS)
            .await
            .unwrap()
            .expect("应有待沉淀记忆");
    assert!(truncated, "预算不足应标记截断");
    assert!(ids.len() < 5, "预算不足应少纳入：实际 {}", ids.len());
    assert!(summary.chars().count() <= 60 + 40, "不应明显超出预算");
    // 仍按最早优先
    assert_eq!(ids[0], "b-0");
}

/// 无待沉淀记忆时返回 None，调用方据此跳过
#[sqlx::test]
async fn build_pending_summary_returns_none_when_empty(pool: sqlx::SqlitePool) {
    let ctx = init_settle_test_env(pool);
    assert!(
        build_pending_memories_summary(&ctx, "agent-settle", 10_000, PENDING_MAX_ITEMS)
            .await
            .unwrap()
            .is_none()
    );
}

/// 预算取法：拿不到模型配置时回落到兜底常量，且恒为正
#[test]
fn pending_budget_chars_falls_back_without_brain() {
    let mut po = crate::models::agent::AgentPo::new(
        "Test".to_string(),
        vec!["assistant".to_string()],
        "desc".to_string(),
        vec![],
        "".to_string(),
        "provider-001".to_string(),
        "test-user".to_string(),
    );
    po.id = "agent-budget".to_string();
    let agent = Agent::from_po(po);

    // 无 brain → 兜底
    assert_eq!(pending_budget_chars(&agent), DEFAULT_PENDING_BUDGET_CHARS);
    assert!(pending_budget_chars(&agent) > 0);
}

/// 定时触发路径的核心契约：Agent 正忙时**不能**「返回 0 了事」（旧行为 = 静默丢一天），
/// 必须如实回 `Busy` 让调用方排期重试，且不得改写运行中的状态。
#[sqlx::test]
async fn settle_exclusive_reports_busy_and_keeps_busy_state(pool: sqlx::SqlitePool) {
    let ctx = init_settle_test_env(pool);
    let mgr = crate::pkg::agent_runtime_state::AgentRuntimeStateManager::global();
    let agent_id = "agent-settle-exclusive-busy";

    // 模拟「同一时刻项目巡检正在唤醒这个 Agent」
    mgr.set_busy(agent_id, "msg-patrol", Some("task-1"), Some("proj-1"));

    let attempt = settle_agent_exclusive(ctx, agent_id, PENDING_MAX_ITEMS)
        .await
        .expect("抢占失败不应是错误，而是可重排的 Busy");
    assert_eq!(attempt, SettleAttempt::Busy);

    // 状态与业务上下文原样保留：沉淀没碰正在跑的巡检
    let info = mgr.get(agent_id).unwrap();
    assert_eq!(info.state, common::enums::AgentRuntimeState::Busy);
    assert_eq!(info.current_message_id, Some("msg-patrol".to_string()));
    assert_eq!(info.task_id, Some("task-1".to_string()));

    mgr.set_idle(agent_id);
}

/// 抢占成功但沉淀主体提前失败（Agent 不存在 / Brain 装配失败）时，
/// BusyGuard 必须把 Agent 放回 Idle —— 否则它会永久卡在 Resting，
/// 之后发给它的所有消息都只能 nack 重投。
#[sqlx::test]
async fn settle_exclusive_releases_state_on_error(pool: sqlx::SqlitePool) {
    let ctx = init_settle_test_env(pool);
    let mgr = crate::pkg::agent_runtime_state::AgentRuntimeStateManager::global();
    let agent_id = "agent-settle-exclusive-missing";

    mgr.set_idle(agent_id);
    let err = settle_agent_exclusive(ctx, agent_id, PENDING_MAX_ITEMS)
        .await
        .expect_err("Agent 不存在应上抛资源未找到");
    assert_eq!(err.code(), "resource_not_found");

    assert_eq!(
        mgr.get_state(agent_id),
        common::enums::AgentRuntimeState::Idle,
        "失败路径必须释放 Resting 抢占，不能把 Agent 留在不可用状态"
    );
}
