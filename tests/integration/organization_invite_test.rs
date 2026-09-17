//! Integration tests for org invite-code issuance/rotation and the create-user
//! route path.
//!
//! Covers two fixes:
//! 1. `POST /organization/user` must be hit WITHOUT trailing slash — axum 0.8
//!    strict matching makes `/user/` fall through to the explicit API 404.
//! 2. Admin invite-code endpoints: lazy issue (idempotent GET), rotation
//!    (old code invalid immediately), public validation + self-registration,
//!    and Admin-only role gating.
//!
//! 共享 DB 串行化：邀请码是组织单例行，懒签发/轮换/注册用例通过文件内
//! `INVITE_MUTEX` 互斥（也覆盖内部会触碰 invite_code 的 register_fresh_member）。

#[path = "../common/mod.rs"]
mod common;

use crate::common::TestApp;
use ::common::api::RegisterByInviteRequest;
use sqlx::SqlitePool;

static INVITE_MUTEX: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// 手动添加用户：无尾斜杠路径命中 handler（200），带尾斜杠路径 404（回归守卫）。
#[sqlx::test]
async fn test_create_user_path_trailing_slash_regression(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let body = serde_json::json!({
        "username": format!("invite-test-user-{}", uuid::Uuid::now_v7()),
        "password": format!("pw-{}", uuid::Uuid::now_v7()),
        "display_name": "Invite Test User",
        "role": 2, // Member
    });

    // 正确路径：无尾斜杠 → 200
    let (status, resp) = app
        .post_with_jwt("/api/v1/organization/user", &body, &jwt)
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "create user without trailing slash should succeed, got: {}",
        resp
    );
    let data = crate::common::assert_api_ok(status, &resp);
    assert!(
        data.get("user_id").and_then(|v| v.as_str()).is_some(),
        "create user response should contain user_id, got: {}",
        resp
    );

    // 错误路径：带尾斜杠 → axum 0.8 严格匹配不命中嵌套根路由，/api/ 回退显式 404
    let dup_body = serde_json::json!({
        "username": format!("invite-test-user-{}", uuid::Uuid::now_v7()),
        "password": format!("pw-{}", uuid::Uuid::now_v7()),
        "role": 2,
    });
    let (status, _resp) = app
        .post_with_jwt("/api/v1/organization/user/", &dup_body, &jwt)
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::NOT_FOUND,
        "create user with trailing slash must 404 under axum 0.8 strict matching"
    );
}

/// 邀请码全生命周期：懒签发（幂等）→ 公开校验 → 轮换（旧码失效）→ 凭新码注册。
#[sqlx::test]
async fn test_invite_code_lifecycle(pool: SqlitePool) {
    let _guard = INVITE_MUTEX.lock().await;

    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, admin_jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 1. 首次 GET：懒签发 24 字符邀请码
    let (status, resp) = app
        .get_with_jwt("/api/v1/organization/me/invite-code", &admin_jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &resp);
    let code1 = data
        .get("invite_code")
        .and_then(|v| v.as_str())
        .expect("invite_code should be returned")
        .to_string();
    assert_eq!(code1.len(), 24, "invite code should be 24 chars");

    // 2. 再次 GET：幂等返回同一码（查看不轮换）
    let (status, resp) = app
        .get_with_jwt("/api/v1/organization/me/invite-code", &admin_jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &resp);
    assert_eq!(
        data.get("invite_code").and_then(|v| v.as_str()),
        Some(code1.as_str()),
        "repeated GET must not rotate the code"
    );

    // 3. 公开校验：当前码有效
    let (status, resp) = app
        .get(&format!(
            "/api/v1/organization/auth/invite/validate?invite_code={code1}"
        ))
        .await;
    let data = crate::common::assert_api_ok(status, &resp);
    assert_eq!(
        data.get("valid").and_then(|v| v.as_bool()),
        Some(true),
        "freshly issued code should validate, got: {}",
        resp
    );

    // 4. 轮换：新码与旧码不同
    let (status, resp) = app
        .post_with_jwt(
            "/api/v1/organization/me/invite-code/regenerate",
            &serde_json::json!({}),
            &admin_jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &resp);
    let code2 = data
        .get("invite_code")
        .and_then(|v| v.as_str())
        .expect("new invite_code should be returned")
        .to_string();
    assert_ne!(code1, code2, "rotated code must differ from old one");
    assert_eq!(code2.len(), 24);

    // 5. 旧码立即失效，新码有效
    let (_, old_resp) = app
        .get(&format!(
            "/api/v1/organization/auth/invite/validate?invite_code={code1}"
        ))
        .await;
    assert_eq!(
        old_resp
            .get("data")
            .and_then(|d| d.get("valid"))
            .and_then(|v| v.as_bool()),
        Some(false),
        "old code must be invalid after rotation"
    );
    let (_, new_resp) = app
        .get(&format!(
            "/api/v1/organization/auth/invite/validate?invite_code={code2}"
        ))
        .await;
    assert_eq!(
        new_resp
            .get("data")
            .and_then(|d| d.get("valid"))
            .and_then(|v| v.as_bool()),
        Some(true),
        "new code must be valid after rotation"
    );

    // 6. 凭新码公开注册：直接登录态返回 token
    let register_req = RegisterByInviteRequest {
        invite_code: code2.clone(),
        username: format!("member-{}", uuid::Uuid::now_v7()),
        password: format!("pw-{}", uuid::Uuid::now_v7()),
        display_name: Some("Invited Member".to_string()),
    };
    let (status, resp) = app
        .post("/api/v1/organization/auth/register", &register_req)
        .await;
    let data = crate::common::assert_api_ok(status, &resp);
    assert!(
        data.get("token").and_then(|v| v.as_str()).is_some(),
        "register by invite should return a login token, got: {}",
        resp
    );

    // 7. 注册不消耗邀请码：码仍可被再次校验
    let (status, _) = app
        .get(&format!(
            "/api/v1/organization/auth/invite/validate?invite_code={code2}"
        ))
        .await;
    assert_eq!(status, axum::http::StatusCode::OK);
}

/// 角色门控：普通成员不能查看/轮换邀请码（403）；管理员可用。
#[sqlx::test]
async fn test_invite_code_admin_only(pool: SqlitePool) {
    let _guard = INVITE_MUTEX.lock().await;

    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, admin_jwt) = crate::common::factories::bootstrap_and_login(&app).await;
    // register_fresh_member 内部走邀请码注册链路，返回全新 Member 的 JWT
    let (member_jwt, _user_id, _org_id) =
        crate::common::factories::register_fresh_member(&app).await;

    let (status, _) = app
        .get_with_jwt("/api/v1/organization/me/invite-code", &member_jwt)
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::FORBIDDEN,
        "member must not read the invite code"
    );

    let (status, _) = app
        .post_with_jwt(
            "/api/v1/organization/me/invite-code/regenerate",
            &serde_json::json!({}),
            &member_jwt,
        )
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::FORBIDDEN,
        "member must not rotate the invite code"
    );

    // 管理员仍可正常访问
    let (status, resp) = app
        .get_with_jwt("/api/v1/organization/me/invite-code", &admin_jwt)
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "admin should read invite code, got: {}",
        resp
    );
}
