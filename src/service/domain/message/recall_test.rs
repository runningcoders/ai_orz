//! Message Recall 单元测试
//!
//! 覆盖三条最容易回归的边界：
//! 1. 权限 gate 必须认「用户 **或** Agent」双主体（Agent 唤醒态只有 agent_id、无 user_id）；
//! 2. 在飞判定看**运行时状态**而非 `messages.status`（后者当前恒不写 `Processing`）；
//! 3. 撤回是**终态**，重复撤回幂等、已处理消息撤回是 no-op（都不得报 error）。

use super::{MessageDomain, RecallOutcome, domain};
use crate::pkg::RequestContext;
use crate::pkg::agent_runtime_state::{AgentRuntimeStateManager, AgentThinkRuntime};
use crate::service::domain::message::SendToAgentCommand;
use common::enums::{MessageRole, MessageStatus, MessageType};
use common::error::ErrorCode;
use sqlx::SqlitePool;
use std::sync::Arc;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx(user_id, pool)
}

/// 初始化测试环境（复用共享 setup，保证单例注入链完整）
fn init_test_env(pool: SqlitePool) -> (Arc<dyn MessageDomain>, RequestContext) {
    crate::pkg::request_context_test_support::init_service_for_test();
    let ctx = new_ctx("admin", pool);
    (domain(), ctx)
}

/// 造一条「发送方 → 目标 Agent」的普通文本消息，返回 message_id
async fn seed_incoming_message(
    domain: &Arc<dyn MessageDomain>,
    ctx: &RequestContext,
    from_id: &str,
    from_role: MessageRole,
) -> String {
    domain
        .delivery()
        .send_to_agent(
            ctx.clone(),
            SendToAgentCommand {
                from_id,
                from_role,
                to_agent_id: "agent-target",
                content: "一条已经过时的指令",
                project_id: None,
                task_id: None,
                reply_to_id: None,
                external_key: None,
                thread_id: None,
                attachment_ids: None,
                message_type: MessageType::Text,
            },
        )
        .await
        .unwrap()
        .po
        .id
}

/// 直读消息原始状态（含撤回态；`find_by_id` 会把撤回态过滤成 None）
async fn raw_status(ctx: &RequestContext, message_id: &str) -> Option<MessageStatus> {
    crate::service::dal::message::dal()
        .find_by_id_with_recalled(ctx.clone(), message_id)
        .await
        .unwrap()
        .map(|m| m.po.status)
}

/// 发送方撤回自己的未处理消息 → `Recalled`，且对业务读路径不可见；重复撤回幂等
#[sqlx::test]
async fn test_recall_pending_message_by_sender(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let message_id = seed_incoming_message(&domain, &ctx, "admin", MessageRole::User).await;

    let outcome = domain
        .recall_message(ctx.clone(), &message_id, Some("指令已过时"))
        .await
        .unwrap();
    assert_eq!(outcome, RecallOutcome::Recalled);
    assert_eq!(
        raw_status(&ctx, &message_id).await,
        Some(MessageStatus::Recalled)
    );

    // 撤回 = 逻辑作废：业务读路径（带 `status != 0` 过滤）不再可见
    assert!(
        domain
            .management()
            .get_by_id(ctx.clone(), &message_id)
            .await
            .unwrap()
            .is_none(),
        "撤回后业务读路径必须看不到该消息"
    );

    // 幂等：重复撤回不算错误，也不重复写库
    let again = domain
        .recall_message(ctx.clone(), &message_id, None)
        .await
        .unwrap();
    assert_eq!(again, RecallOutcome::AlreadyRecalled);
}

/// 与消息无关的第三方撤回 → `Forbidden`（无 project 上下文时只有发送方 / SuperAdmin 可撤）
#[sqlx::test]
async fn test_recall_rejected_for_outsider(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let message_id = seed_incoming_message(&domain, &ctx, "user-owner", MessageRole::User).await;

    let outsider = new_ctx("someone-else", ctx.db_pool().clone());
    let err = domain
        .recall_message(outsider, &message_id, None)
        .await
        .unwrap_err();
    assert_eq!(err.code_enum(), ErrorCode::Forbidden);
    assert_eq!(
        raw_status(&ctx, &message_id).await,
        Some(MessageStatus::Pending),
        "被拒的撤回不得改动状态"
    );
}

/// 权限 gate 必须认 Agent 主体：唤醒态 ctx（`caller_type = System`、只有 agent_id、无 user_id）
/// 撤回自己发的消息必须放行
///
/// 这正是「只认 user_id 的 gate 会让整族工具在沉淀期恒被拒」的红线回归。
#[sqlx::test]
async fn test_recall_allowed_for_agent_origin_caller(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let message_id = seed_incoming_message(&domain, &ctx, "agent-sender", MessageRole::Agent).await;

    let agent_ctx = crate::pkg::request_context_test_support::new_test_agent_ctx(
        "agent-sender",
        ctx.db_pool().clone(),
    );
    assert!(
        agent_ctx.user_id().is_none(),
        "前提：Agent 唤醒态没有 user_id"
    );

    let outcome = domain
        .recall_message(agent_ctx, &message_id, None)
        .await
        .unwrap();
    assert_eq!(outcome, RecallOutcome::Recalled);
}

/// 收件方（判据⑤）撤回「发给自己的未处理消息」→ `Recalled`
///
/// 这是「消息按接收方串行排队、队列只认先来后到」这条痛点的**收件侧解法**：需求已
/// 变更时，Agent 有权把自己队列里那条作废的指令丢掉，而不必干等发送方发现。
/// 判据①（发送方）对它是**不成立**的——必须靠第⑤条才放行。
#[sqlx::test]
async fn test_recall_pending_message_by_recipient(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    // seed_incoming_message 的收件方固定为 "agent-target"，此处发送方是无关第三方
    let message_id = seed_incoming_message(&domain, &ctx, "user-owner", MessageRole::User).await;

    let recipient_ctx = crate::pkg::request_context_test_support::new_test_agent_ctx(
        "agent-target",
        ctx.db_pool().clone(),
    );

    let outcome = domain
        .recall_message(recipient_ctx, &message_id, Some("需求已变更，拒收该指令"))
        .await
        .unwrap();
    assert_eq!(outcome, RecallOutcome::Recalled);
    assert_eq!(
        raw_status(&ctx, &message_id).await,
        Some(MessageStatus::Recalled)
    );
}

/// 在飞消息：靠**运行时状态**定位持有它的 Agent → 发取消信号 + 标撤回
#[sqlx::test]
async fn test_recall_in_flight_cancels_holding_agent(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let message_id = seed_incoming_message(&domain, &ctx, "admin", MessageRole::User).await;

    // 复用生产链路的形状：先 set_busy 登记 current_message_id，再挂 think_runtime
    // （`cancel_thinking` 只有 think_runtime 存在时才会真正翻转 cancel_flag）
    let agent_id = "agent-inflight-recall";
    let mgr = AgentRuntimeStateManager::global();
    assert!(mgr.try_set_busy(agent_id, &message_id, None, None));
    mgr.set_think_runtime(
        agent_id,
        Arc::new(AgentThinkRuntime::new(
            agent_id.to_string(),
            "trace-1".to_string(),
        )),
    );

    let outcome = domain
        .recall_message(ctx.clone(), &message_id, Some("撤回正在处理的过时指令"))
        .await
        .unwrap();

    assert_eq!(
        outcome,
        RecallOutcome::RecalledInFlight {
            agent_id: agent_id.to_string(),
            cancelled: true,
        }
    );
    assert_eq!(
        raw_status(&ctx, &message_id).await,
        Some(MessageStatus::Recalled)
    );

    mgr.set_idle(agent_id);
}

/// 撤回者不得是「正在处理这条消息的那个 Agent 自己」→ `InvalidRequest`（引导改用 cancel_thinking）
#[sqlx::test]
async fn test_recall_self_while_processing_is_rejected(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let agent_id = "agent-self-recall";
    let message_id = seed_incoming_message(&domain, &ctx, agent_id, MessageRole::Agent).await;

    let mgr = AgentRuntimeStateManager::global();
    assert!(mgr.try_set_busy(agent_id, &message_id, None, None));
    mgr.set_think_runtime(
        agent_id,
        Arc::new(AgentThinkRuntime::new(
            agent_id.to_string(),
            "trace-2".to_string(),
        )),
    );

    let agent_ctx = crate::pkg::request_context_test_support::new_test_agent_ctx(
        agent_id,
        ctx.db_pool().clone(),
    );
    let err = domain
        .recall_message(agent_ctx, &message_id, None)
        .await
        .unwrap_err();

    assert_eq!(err.code_enum(), ErrorCode::InvalidRequest);
    assert!(
        err.to_string().contains("cancel_thinking"),
        "错误文案必须点名正确的替代工具（会原样回灌给模型）: {err}"
    );

    mgr.set_idle(agent_id);
}

/// 已处理的消息撤回是 **no-op**（`not_recallable`），不得报 error
#[sqlx::test]
async fn test_recall_processed_message_is_noop(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let message_id = seed_incoming_message(&domain, &ctx, "admin", MessageRole::User).await;

    crate::service::dal::message::dal()
        .update_status(ctx.clone(), &message_id, MessageStatus::Processed)
        .await
        .unwrap();

    let outcome = domain
        .recall_message(ctx.clone(), &message_id, None)
        .await
        .unwrap();
    assert_eq!(
        outcome,
        RecallOutcome::NotRecallable {
            status: MessageStatus::Processed
        }
    );
    assert_eq!(
        raw_status(&ctx, &message_id).await,
        Some(MessageStatus::Processed),
        "no-op 分支不得改动状态"
    );
}
