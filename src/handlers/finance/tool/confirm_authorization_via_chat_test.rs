//! confirm_authorization_via_chat handler 集成测试（真 DAL 证据链）

use common::api::AuthorizationDecisionRequest;
use common::enums::{MessageRole, MessageStatus, MessageType};
use sqlx::SqlitePool;

use super::confirm_authorization_via_chat::confirm_authorization_via_chat;
use crate::models::message::MessagePo;
use crate::pkg::authorization::command_signature;
use crate::pkg::request_context_test_support::new_test_ctx;
use crate::service::dao::message;
use crate::service::domain::finance::{CreateAuthorizationCmd, domain};

fn init_test_singletons() {
    crate::pkg::request_context_test_support::init_service_for_test();
}

// 授权服务为进程级共享单例且测试默认并行：各测试用唯一命令签名避免
// 「同签名 Pending 防重」跨测试误伤（语义不受影响，handler 测试无签名匹配依赖）
fn uniq_sig(tag: &str) -> String {
    format!("docker push registry.local/{tag}:1.0")
}

async fn insert_user_evidence(pool: &SqlitePool, msg_id: &str, content: &str) {
    let ctx = new_test_ctx("user-aman", pool.clone());
    // 证据必须晚于建单（五要素第④条）：+1 天偏移保证测试执行窗口内恒成立
    let base = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64)
        + 86_400_000;
    let po = MessagePo {
        id: msg_id.to_string(),
        from_id: "user-aman".to_string(),
        to_id: "agent-a".to_string(),
        from_role: MessageRole::User,
        to_role: MessageRole::Agent,
        message_type: MessageType::Text,
        status: MessageStatus::Processed,
        content: content.to_string(),
        created_at: base,
        updated_at: base,
        created_by: "user-aman".to_string(),
        modified_by: "user-aman".to_string(),
        ..Default::default()
    };
    message::new().insert(ctx, &po).await.expect("证据消息落库");
}

async fn make_pending(pool: &SqlitePool, signature: &str) -> String {
    let ctx = new_test_ctx("user-aman", pool.clone());
    let pending = domain()
        .tool_authorization_manage()
        .create_pending_authorization(
            ctx,
            CreateAuthorizationCmd {
                agent_id: "agent-a".to_string(),
                tool_id: "shell_exec".to_string(),
                command_signature: command_signature(signature),
                blocking_rule: "git_dangerous_subcommand".to_string(),
                rule_idempotent: false,
                reason: Some("T4 handler 集成测试".to_string()),
            },
        )
        .await
        .expect("建单成功");
    pending.authorization_id
}

fn mediated_params(
    auth_id: &str,
    evidence: Option<&str>,
    decision: &str,
) -> AuthorizationDecisionRequest {
    AuthorizationDecisionRequest {
        authorization_id: auth_id.to_string(),
        decision: decision.to_string(),
        scope: None,
        // handler 强制注入 ChatMediated 与 ctx.agent_id（客户端传值应被覆盖）
        evidence_class: None,
        evidence_message_id: evidence.map(|s| s.to_string()),
        mediator_agent_id: Some("agent-evil".to_string()),
    }
}

fn agent_ctx(pool: SqlitePool, agent_id: &str) -> crate::pkg::RequestContext {
    let mut ctx = new_test_ctx("user-aman", pool);
    ctx.agent_id = Some(agent_id.to_string());
    ctx
}

#[sqlx::test(migrations = "./migrations")]
async fn confirm_via_chat_third_party_mediation_succeeds(pool: SqlitePool) {
    init_test_singletons();
    let auth_id = make_pending(&pool, &uniq_sig("chat-approve")).await;
    insert_user_evidence(&pool, "ev-1", "同意执行该命令").await;

    let resp = confirm_authorization_via_chat(
        agent_ctx(pool.clone(), "agent-b"),
        mediated_params(&auth_id, Some("ev-1"), "Approve"),
    )
    .await
    .expect("第三方代呈应签发");

    assert!(matches!(
        resp.status,
        common::api::AuthorizationStatusDto::Active
    ));
    assert!(resp.grant_id.is_some());
}

#[sqlx::test(migrations = "./migrations")]
async fn confirm_via_chat_reject_decision_is_archived(pool: SqlitePool) {
    init_test_singletons();
    let auth_id = make_pending(&pool, &uniq_sig("chat-reject")).await;
    insert_user_evidence(&pool, "ev-reject", "不同意，先不要执行").await;

    let resp = confirm_authorization_via_chat(
        agent_ctx(pool.clone(), "agent-b"),
        mediated_params(&auth_id, Some("ev-reject"), "Reject"),
    )
    .await
    .expect("拒绝代呈应落档");

    assert!(matches!(
        resp.status,
        common::api::AuthorizationStatusDto::Rejected
    ));
    assert!(resp.grant_id.is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn confirm_via_chat_self_mediation_is_rejected(pool: SqlitePool) {
    init_test_singletons();
    let auth_id = make_pending(&pool, &uniq_sig("chat-self")).await;
    insert_user_evidence(&pool, "ev-self", "同意").await;

    let err = confirm_authorization_via_chat(
        agent_ctx(pool.clone(), "agent-a"),
        mediated_params(&auth_id, Some("ev-self"), "Approve"),
    )
    .await
    .expect_err("申请人自代呈应拒绝");

    assert!(err.to_string().contains("申请人不得自代呈"));
}

#[sqlx::test(migrations = "./migrations")]
async fn confirm_via_chat_missing_evidence_is_rejected(pool: SqlitePool) {
    init_test_singletons();
    let auth_id = make_pending(&pool, &uniq_sig("chat-missing")).await;

    let err = confirm_authorization_via_chat(
        agent_ctx(pool.clone(), "agent-b"),
        mediated_params(&auth_id, None, "Approve"),
    )
    .await
    .expect_err("缺证据消息应拒绝");

    assert!(err.code_enum() == common::error::ErrorCode::InvalidRequest);
    assert!(err.to_string().contains("证据消息 ID"));
}

#[sqlx::test(migrations = "./migrations")]
async fn decide_evidence_replay_is_rejected(pool: SqlitePool) {
    init_test_singletons();
    let auth_1 = make_pending(&pool, &uniq_sig("replay-a")).await;
    let auth_2 = make_pending(&pool, &uniq_sig("replay-b")).await;
    insert_user_evidence(&pool, "ev-replay", "同意").await;

    let first = confirm_authorization_via_chat(
        agent_ctx(pool.clone(), "agent-b"),
        mediated_params(&auth_1, Some("ev-replay"), "Approve"),
    )
    .await
    .expect("首次消费应签发");
    assert!(first.grant_id.is_some());

    let err = confirm_authorization_via_chat(
        agent_ctx(pool.clone(), "agent-b"),
        mediated_params(&auth_2, Some("ev-replay"), "Approve"),
    )
    .await
    .expect_err("证据重放应拒绝");

    assert!(err.to_string().contains("防重放"));
}
