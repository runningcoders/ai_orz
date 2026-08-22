//! UserCredential DAO SQLite 单元测试
//!
//! 覆盖 platform 维度（匹配键 (kind, platform) 二元组）核心路径：
//! - `find_default`：platform 精确匹配 / None 排除带值行（专用 kind 语义）
//! - `set_default` / `clear_default`：同 kind 不同 platform 默认槽位互不干扰

use crate::models::user::UserPo;
use crate::models::user_credential::UserCredentialPo;
use crate::pkg::RequestContext;
use common::enums::UserRole;
use common::models::{CredentialDetail, CredentialKind, CredentialVisibility};
use sqlx::SqlitePool;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx(user_id, pool)
}

/// 创建测试用户（find_default 经 JOIN users 取目标用户组织）
async fn seed_user(ctx: &RequestContext, user_id: &str) {
    let user = UserPo::new(
        user_id.to_string(),
        "org-plat".to_string(),
        format!("{}-name", user_id),
        format!("{}-display", user_id),
        String::new(),
        String::new(),
        UserRole::Member,
        "admin".to_string(),
    );
    crate::service::dao::user::dao()
        .insert(ctx.clone(), &user)
        .await
        .unwrap();
}

/// 创建 generic_token 凭证（platform 由测试显式指定；明文直通兼容：解密路径对未加密值原样返回）
async fn seed_generic_token(
    ctx: &RequestContext,
    cred_id: &str,
    user_id: &str,
    platform: Option<&str>,
) {
    let mut po = UserCredentialPo::new(
        cred_id.to_string(),
        "org-plat".to_string(),
        user_id.to_string(),
        CredentialKind::GenericToken,
        format!("凭证-{}", cred_id),
        CredentialDetail::GenericToken {
            token: format!("token-{}", cred_id),
        },
        CredentialVisibility::Private,
        "admin".to_string(),
    );
    po.platform = platform.map(|s| s.to_string());
    crate::service::dao::user_credential::dao()
        .insert(ctx.clone(), &po)
        .await
        .unwrap();
}

/// platform 精确匹配：同 kind 双平台各持一行，Some("linear") 命中 linear 行
#[sqlx::test]
async fn find_default_matches_platform_exact(pool: SqlitePool) {
    crate::service::dao::user::init();
    crate::service::dao::user_credential::init();
    let ctx = new_ctx("test-user", pool);
    let dao = crate::service::dao::user_credential::dao();

    seed_user(&ctx, "user-1").await;
    seed_generic_token(&ctx, "cred-linear", "user-1", Some("linear")).await;
    seed_generic_token(&ctx, "cred-notion", "user-1", Some("notion")).await;

    let hit = dao
        .find_default(ctx, "user-1", CredentialKind::GenericToken, Some("linear"))
        .await
        .unwrap()
        .expect("should hit linear credential");
    assert_eq!(hit.id, "cred-linear");
    assert_eq!(hit.platform.as_deref(), Some("linear"));
}

/// None 语义 = platform IS NULL：不匹配带值行（专用 kind 语义）
#[sqlx::test]
async fn find_default_none_platform_excludes_valued(pool: SqlitePool) {
    crate::service::dao::user::init();
    crate::service::dao::user_credential::init();
    let ctx = new_ctx("test-user", pool);
    let dao = crate::service::dao::user_credential::dao();

    seed_user(&ctx, "user-1").await;
    seed_generic_token(&ctx, "cred-linear", "user-1", Some("linear")).await;
    seed_generic_token(&ctx, "cred-notion", "user-1", Some("notion")).await;

    let miss = dao
        .find_default(ctx, "user-1", CredentialKind::GenericToken, None)
        .await
        .unwrap();
    assert!(miss.is_none(), "None platform must not match valued rows");
}

/// 默认槽位按 (kind, platform) 隔离：同 kind 不同 platform 可各自持有默认
#[sqlx::test]
async fn set_default_scoped_by_platform(pool: SqlitePool) {
    crate::service::dao::user::init();
    crate::service::dao::user_credential::init();
    let ctx = new_ctx("test-user", pool);
    let dao = crate::service::dao::user_credential::dao();

    seed_user(&ctx, "user-1").await;
    seed_generic_token(&ctx, "cred-linear", "user-1", Some("linear")).await;
    seed_generic_token(&ctx, "cred-notion", "user-1", Some("notion")).await;

    dao.set_default(ctx.clone(), "cred-linear").await.unwrap();
    dao.set_default(ctx.clone(), "cred-notion").await.unwrap();

    // 两平台默认并存（不再互斥）
    let linear = dao
        .find_by_id(ctx.clone(), "cred-linear")
        .await
        .unwrap()
        .unwrap();
    let notion = dao
        .find_by_id(ctx.clone(), "cred-notion")
        .await
        .unwrap()
        .unwrap();
    assert!(
        linear.is_default,
        "linear default must survive notion set_default"
    );
    assert!(notion.is_default);

    // 清 linear 默认不影响 notion
    dao.clear_default(
        ctx.clone(),
        "user-1",
        CredentialKind::GenericToken,
        Some("linear"),
    )
    .await
    .unwrap();
    let linear = dao
        .find_by_id(ctx.clone(), "cred-linear")
        .await
        .unwrap()
        .unwrap();
    let notion = dao
        .find_by_id(ctx.clone(), "cred-notion")
        .await
        .unwrap()
        .unwrap();
    assert!(!linear.is_default);
    assert!(
        notion.is_default,
        "clear linear default must not touch notion default"
    );
}
