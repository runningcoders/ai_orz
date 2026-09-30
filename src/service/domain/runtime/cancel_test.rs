//! Cancel Thinking 单元测试
//!
//! 覆盖两条最容易回归的边界：
//! 1. **存在性校验**：目标 Agent 不存在必须报 `NotFound`，不得与「Agent 空闲」混同
//!    —— 此前二者返回完全一样的 `success=false`，会把拼错 / 已删除的 ID 静默吞掉；
//! 2. 「Agent 存在但当前空闲」是**幂等 no-op**（`NotThinking`），不得报 error。

use super::{CancelOutcome, domain};
use crate::pkg::RequestContext;
use crate::pkg::agent_runtime_state::{AgentRuntimeStateManager, AgentThinkRuntime};
use common::error::ErrorCode;
use sqlx::SqlitePool;
use std::sync::Arc;
use std::sync::atomic::Ordering;

fn init_test_env(pool: SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::init_service_for_test();
    crate::pkg::request_context_test_support::new_test_ctx("admin", pool)
}

/// 建一条真实的 agents 行（存在性校验只判定「该 ID 能否在 agents 表命中」）
async fn seed_agent(ctx: &RequestContext, name: &str) -> String {
    let po = crate::models::agent::AgentPo::new(
        name.to_string(),
        vec!["worker".to_string()],
        String::new(),
        Vec::new(),
        String::new(),
        "provider-test".to_string(),
        "admin".to_string(),
    );
    let agent = crate::models::agent::Agent::from_po(po);
    let id = agent.po.id.clone();
    crate::service::dal::agent::dal()
        .create(ctx.clone(), &agent)
        .await
        .expect("seed agent");
    id
}

/// 不存在的 Agent → `NotFound`（不得静默返回「未在思考」）
#[sqlx::test]
async fn test_cancel_unknown_agent_is_not_found(pool: SqlitePool) {
    let ctx = init_test_env(pool);

    let err = domain()
        .cancel_thinking(ctx, "no-such-agent")
        .await
        .unwrap_err();
    assert_eq!(
        err.code_enum(),
        ErrorCode::NotFound,
        "目标不存在必须报 NotFound，否则调用方分不清「没有这个 Agent」和「Agent 空闲」: {err}"
    );
}

/// Agent 存在但未在思考 → `NotThinking`（幂等 no-op，不是错误）
#[sqlx::test]
async fn test_cancel_idle_agent_is_noop(pool: SqlitePool) {
    let ctx = init_test_env(pool);
    let agent_id = seed_agent(&ctx, "idle-agent").await;

    let outcome = domain().cancel_thinking(ctx, &agent_id).await.unwrap();
    assert_eq!(outcome, CancelOutcome::NotThinking);
}

/// Agent 正在思考 → `Cancelled`，且 `cancel_flag` 真的被翻转
#[sqlx::test]
async fn test_cancel_thinking_agent_cancels(pool: SqlitePool) {
    let ctx = init_test_env(pool);
    let agent_id = seed_agent(&ctx, "busy-agent").await;

    // 复用生产链路的形状：先登记运行时条目（set_think_runtime 只对已存在的条目生效），
    // 再挂 think_runtime —— `cancel_thinking` 只有 think_runtime 存在时才会真正翻 flag
    let mgr = AgentRuntimeStateManager::global();
    assert!(mgr.try_set_busy(&agent_id, "msg-cancel", None, None));
    let runtime = Arc::new(AgentThinkRuntime::new(
        agent_id.clone(),
        "trace-cancel".to_string(),
    ));
    mgr.set_think_runtime(&agent_id, runtime.clone());

    let outcome = domain().cancel_thinking(ctx, &agent_id).await.unwrap();
    assert_eq!(outcome, CancelOutcome::Cancelled);
    assert!(
        runtime.cancel_flag().load(Ordering::SeqCst),
        "cancel_flag 必须被真实翻转（否则 Agent 不会在轮次边界退出）"
    );

    mgr.set_idle(&agent_id);
}
