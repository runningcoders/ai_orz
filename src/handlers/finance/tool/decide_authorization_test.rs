//! decide_authorization handler 集成测试（真 DAL + 共享授权服务）

use common::api::AuthorizationDecisionRequest;
use common::enums::{MessageRole, MessageStatus, MessageType};
use sqlx::SqlitePool;

use super::decide_authorization::decide_authorization;
use crate::models::message::MessagePo;
use crate::pkg::authorization::command_signature;
use crate::pkg::request_context_test_support::new_test_ctx;
use crate::service::domain::finance::{CreateAuthorizationCmd, domain};

fn init_test_singletons() {
    crate::pkg::request_context_test_support::init_service_for_test();
}

// 授权服务为进程级共享单例且测试默认并行：各测试用唯一命令签名避免
// 「同签名 Pending 防重」跨测试误伤（语义不受影响，handler 测试无签名匹配依赖）
fn uniq_sig(tag: &str) -> String {
    format!("docker push registry.local/{tag}:1.0")
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

fn approve_params(auth_id: &str) -> AuthorizationDecisionRequest {
    AuthorizationDecisionRequest {
        authorization_id: auth_id.to_string(),
        decision: "Approve".to_string(),
        scope: None,
        evidence_class: None,
        evidence_message_id: None,
        mediator_agent_id: None,
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn decide_ui_direct_approval_issues_grant(pool: SqlitePool) {
    init_test_singletons();
    let auth_id = make_pending(&pool, &uniq_sig("decide-ui")).await;
    let ctx = new_test_ctx("user-aman", pool);

    let resp = decide_authorization(ctx, approve_params(&auth_id))
        .await
        .expect("UI 直批应签发");

    assert_eq!(resp.authorization_id, auth_id);
    assert!(matches!(
        resp.status,
        common::api::AuthorizationStatusDto::Active
    ));
    assert!(resp.grant_id.is_some());
}

#[sqlx::test(migrations = "./migrations")]
async fn decide_agent_ctx_without_mediation_is_rejected(pool: SqlitePool) {
    init_test_singletons();
    let auth_id = make_pending(&pool, &uniq_sig("decide-agent")).await;
    let mut ctx = new_test_ctx("user-aman", pool.clone());
    ctx.agent_id = Some("agent-b".to_string());

    let err = decide_authorization(ctx, approve_params(&auth_id))
        .await
        .expect_err("Agent ctx 无代呈形态应结构性拒绝");

    assert!(err.code_enum() == common::error::ErrorCode::InvalidRequest);
    assert!(err.to_string().contains("Agent 不得自我审批"));
}

// ============ T5·S3 双向通知集成验证（真 DAL：落库即通知） ============

/// 系统通知消息落库后经 find_by_id 可查（走真 MessageDal 全路径）
async fn find_system_notification(pool: &SqlitePool, to_id: &str) -> Option<MessagePo> {
    let ctx = new_test_ctx("user-aman", pool.clone());
    let dal = crate::service::dal::message::dal();
    let msgs = dal
        .list_by_to_id(ctx, to_id, Some(20))
        .await
        .expect("查询通知消息");
    msgs.into_iter()
        .find(|m| m.po.from_role == MessageRole::System)
        .map(|m| m.po)
}

#[sqlx::test(migrations = "./migrations")]
async fn pending_notification_persisted_via_real_dal(pool: SqlitePool) {
    init_test_singletons();
    let auth_id = make_pending(&pool, &uniq_sig("t5-pending-notify")).await;
    let po = find_system_notification(&pool, "user-aman")
        .await
        .expect("建单推送应经真 DAL 落库可查");
    assert_eq!(po.from_role, MessageRole::System);
    assert_eq!(po.to_role, MessageRole::User);
    assert_eq!(po.to_id, "user-aman");
    assert_eq!(po.message_type, MessageType::Text);
    assert_eq!(po.status, MessageStatus::Pending);
    assert!(
        po.content.contains(&auth_id),
        "建单推送应携带 authorization_id，实际: {}",
        po.content
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn decided_notification_persisted_via_real_dal(pool: SqlitePool) {
    init_test_singletons();
    let auth_id = make_pending(&pool, &uniq_sig("t5-decided-notify")).await;
    let params = approve_params(&auth_id);
    decide_authorization(new_test_ctx("user-aman", pool.clone()), params)
        .await
        .expect("UI 直批成功");
    let po = find_system_notification(&pool, "agent-a")
        .await
        .expect("决策回推应经真 DAL 落库可查");
    assert_eq!(po.from_role, MessageRole::System);
    assert_eq!(po.to_role, MessageRole::Agent);
    assert_eq!(po.to_id, "agent-a");
    assert!(
        po.content.contains(&auth_id),
        "决策回推应携带 authorization_id，实际: {}",
        po.content
    );
}
