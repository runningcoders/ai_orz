//! 管理面三 handler 集成测试（create_request / list / revoke）

use common::api::{
    AuthorizationQueryRequest, AuthorizationStatusDto, CreateAuthorizationRequest,
    RevokeAuthorizationRequest,
};
use sqlx::SqlitePool;

use super::create_request_authorization::create_request_authorization;
use super::list_authorizations::list_authorizations;
use super::revoke_authorization::revoke_authorization;
use crate::pkg::RequestContext;
use crate::pkg::authorization::command_signature;
use crate::pkg::request_context_test_support::new_test_ctx;

fn init_test_singletons() {
    crate::pkg::request_context_test_support::init_service_for_test();
}

// 各测试唯一命令串（受控 git 子命令形态，无受限字面序列）：
// 授权服务为进程级共享单例且测试默认并行，唯一化避免「同签名 Pending 防重」跨测试误伤
fn uniq_cmd(tag: &str) -> String {
    format!("git reset --hard HEAD~{tag}")
}

fn create_params(agent_id: &str, command: &str) -> CreateAuthorizationRequest {
    CreateAuthorizationRequest {
        agent_id: agent_id.to_string(),
        tool_id: "shell_exec".to_string(),
        reason: "T6 管理面集成测试".to_string(),
        command: Some(command.to_string()),
        repository: None,
        branch: None,
        org_id: None,
    }
}

async fn agent_ctx(pool: &SqlitePool, agent_id: &str) -> RequestContext {
    let mut ctx = new_test_ctx("user-aman", pool.clone());
    ctx.agent_id = Some(agent_id.to_string());
    ctx
}

// ① 纵深校验：未命中任何拦截规则 → Conflict 非受限操作
#[sqlx::test(migrations = "./migrations")]
async fn create_request_rejects_unrestricted_command(pool: SqlitePool) {
    init_test_singletons();
    let ctx = agent_ctx(&pool, "agent-t6a").await;
    let params = create_params("agent-t6a", "docker push registry.local/t6a:1.0");
    let err = create_request_authorization(ctx, params)
        .await
        .expect_err("未命中拦截规则应拒绝建单");
    assert_eq!(err.code_enum(), common::error::ErrorCode::Conflict);
    assert!(err.to_string().contains("非受限操作"));
}

// ② 建单成功：Agent ctx 强制归属自身（params 携带他者 ID 也被覆盖）+ 归属过滤查询
#[sqlx::test(migrations = "./migrations")]
async fn create_request_agent_ctx_forces_own_agent_id(pool: SqlitePool) {
    init_test_singletons();
    let ctx = agent_ctx(&pool, "agent-t6b").await;
    let resp =
        create_request_authorization(ctx, create_params("agent-elsewhere", &uniq_cmd("t6b")))
            .await
            .expect("Agent ctx 建单成功");
    assert!(matches!(resp.status, AuthorizationStatusDto::Pending));

    // 归属校验：agent-t6b 可见且归属正确；他者 Agent 视角不可见
    let own = list_authorizations(
        agent_ctx(&pool, "agent-t6b").await,
        AuthorizationQueryRequest::default(),
    )
    .await
    .expect("列表查询成功");
    assert_eq!(own.len(), 1);
    assert_eq!(own[0].agent_id, "agent-t6b");
    let others = list_authorizations(
        agent_ctx(&pool, "agent-stranger").await,
        AuthorizationQueryRequest::default(),
    )
    .await
    .expect("列表查询成功");
    assert!(others.iter().all(|d| d.agent_id != "agent-t6b"));
}

// ② 补充：user ctx 建单使用 params.agent_id（代指定）+ 状态过滤查询
#[sqlx::test(migrations = "./migrations")]
async fn create_request_user_ctx_uses_params_agent_id(pool: SqlitePool) {
    init_test_singletons();
    let sig = command_signature(&uniq_cmd("t6c"));
    let ctx = new_test_ctx("user-aman", pool.clone());
    let resp = create_request_authorization(ctx, create_params("agent-t6c", &uniq_cmd("t6c")))
        .await
        .expect("user ctx 代指定建单成功");
    assert!(matches!(resp.status, AuthorizationStatusDto::Pending));

    // user ctx 列表：归属基准=审批人本人（user_id 强制注入），按唯一签名定位
    let items = list_authorizations(
        new_test_ctx("user-aman", pool.clone()),
        AuthorizationQueryRequest::default(),
    )
    .await
    .expect("列表查询成功");
    let hit = items
        .iter()
        .find(|d| d.command_signature == sig)
        .expect("user ctx 建单可见");
    assert_eq!(hit.agent_id, "agent-t6c");

    // ④ 状态过滤：Pending 过滤只返回待审批单
    let pending = list_authorizations(
        new_test_ctx("user-aman", pool),
        AuthorizationQueryRequest {
            status: Some(AuthorizationStatusDto::Pending),
            ..Default::default()
        },
    )
    .await
    .expect("状态过滤查询成功");
    assert!(
        pending
            .iter()
            .all(|d| matches!(d.status, AuthorizationStatusDto::Pending))
    );
    assert!(pending.iter().any(|d| d.command_signature == sig));
}

// ③ 同签名防重：第二单 Conflict
#[sqlx::test(migrations = "./migrations")]
async fn create_request_same_signature_is_deduplicated(pool: SqlitePool) {
    init_test_singletons();
    let cmd = uniq_cmd("t6d");
    create_request_authorization(
        agent_ctx(&pool, "agent-t6d").await,
        create_params("agent-t6d", &cmd),
    )
    .await
    .expect("首单成功");
    let err = create_request_authorization(
        agent_ctx(&pool, "agent-t6d").await,
        create_params("agent-t6d", &cmd),
    )
    .await
    .expect_err("同签名 Pending 防重应拒绝");
    assert!(err.to_string().contains("同签名待审批授权单已存在"));
}

// ⑤ 撤销即时生效 + 终态守卫
#[sqlx::test(migrations = "./migrations")]
async fn revoke_takes_effect_immediately_and_terminal_state_guarded(pool: SqlitePool) {
    init_test_singletons();
    let resp = create_request_authorization(
        agent_ctx(&pool, "agent-t6e").await,
        create_params("agent-t6e", &uniq_cmd("t6e")),
    )
    .await
    .expect("建单成功");
    let auth_id = resp.authorization_id;

    let revoked = revoke_authorization(
        new_test_ctx("user-aman", pool.clone()),
        RevokeAuthorizationRequest {
            authorization_id: auth_id.clone(),
            reason: Some("T6 撤销即时生效验证".to_string()),
        },
    )
    .await
    .expect("撤销成功");
    assert!(matches!(revoked.status, AuthorizationStatusDto::Revoked));
    assert!(revoked.grant_id.is_none());

    let items = list_authorizations(
        agent_ctx(&pool, "agent-t6e").await,
        AuthorizationQueryRequest::default(),
    )
    .await
    .expect("列表查询成功");
    let rec = items
        .iter()
        .find(|d| d.authorization_id == auth_id)
        .expect("已撤销记录仍可见（审计留档）");
    assert!(matches!(rec.status, AuthorizationStatusDto::Revoked));

    let err = revoke_authorization(
        new_test_ctx("user-aman", pool),
        RevokeAuthorizationRequest {
            authorization_id: auth_id,
            reason: None,
        },
    )
    .await
    .expect_err("终态记录不可再撤销");
    assert!(err.to_string().contains("已终态不可撤销"));
}

// ⑥ 撤销后同签名可重建（防重仅针对 Pending）——拦截面重新武装语义
#[sqlx::test(migrations = "./migrations")]
async fn recreate_after_revoke_re_arms_interception(pool: SqlitePool) {
    init_test_singletons();
    let cmd = uniq_cmd("t6f");
    let first = create_request_authorization(
        agent_ctx(&pool, "agent-t6f").await,
        create_params("agent-t6f", &cmd),
    )
    .await
    .expect("首单成功");
    revoke_authorization(
        new_test_ctx("user-aman", pool.clone()),
        RevokeAuthorizationRequest {
            authorization_id: first.authorization_id,
            reason: None,
        },
    )
    .await
    .expect("撤销成功");

    let second = create_request_authorization(
        agent_ctx(&pool, "agent-t6f").await,
        create_params("agent-t6f", &cmd),
    )
    .await
    .expect("撤销后同签名重建成功");
    assert!(matches!(second.status, AuthorizationStatusDto::Pending));
}

// ⑦ 仅命中审计规则的命令 → InvalidRequest 无需授权
#[sqlx::test(migrations = "./migrations")]
async fn create_request_rejects_audit_only_command(pool: SqlitePool) {
    init_test_singletons();
    let ctx = agent_ctx(&pool, "agent-t6g").await;
    let params = create_params("agent-t6g", "git commit -m t6g-audit-only");
    let err = create_request_authorization(ctx, params)
        .await
        .expect_err("审计级命令无需授权应拒绝");
    assert_eq!(err.code_enum(), common::error::ErrorCode::InvalidRequest);
    assert!(err.to_string().contains("无需授权"));
}
