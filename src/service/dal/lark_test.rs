//! LarkDalImpl 单元测试
//!
//! 覆盖凭证引用模型核心路径：
//! - `find_channel_by_lark_identity`：app_id + open_id 二维定位（跨应用隔离）
//! - `resolve_credentials_for_user`：渠道引用 ID → 凭证行 → 解密凭证 + 身份模式
//! - 监听生命周期无网络路径：无渠道引用时 ensure/release 均安全返回

use crate::models::message_channel::{ChannelConfig, MessageChannel, MessageChannelPo};
use crate::models::user::UserPo;
use crate::models::user_credential::UserCredentialPo;
use crate::pkg::RequestContext;
use crate::service::dal::lark::test_support::new_for_test_with_credential_dao;
use crate::service::dal::lark::{LarkCredentialDal, LarkListenerDal};
use crate::service::dal::message_channel::init as message_channel_dal_init;
use crate::service::dao::a2a_callback::init as a2a_callback_dao_init;
use crate::service::dao::email::init as email_dao_init;
use crate::service::dao::lark::init as lark_dao_init;
use crate::service::dao::message_channel::init as message_channel_dao_init;
use crate::service::dao::slack::init as slack_dao_init;
use crate::service::dao::user::init as user_dao_init;
use crate::service::dao::user_credential::init as user_credential_dao_init;
use crate::service::dao::webhook::init as webhook_dao_init;
use crate::service::dao::wechat::init as wechat_dao_init;
use common::enums::{ChannelStatus, ChannelType};
use common::models::{CredentialDetail, CredentialKind, CredentialVisibility};
use sqlx::SqlitePool;

fn init_all_test_daos() {
    message_channel_dao_init();
    a2a_callback_dao_init();
    user_dao_init();
    user_credential_dao_init();
    lark_dao_init();
    wechat_dao_init();
    slack_dao_init();
    email_dao_init();
    webhook_dao_init();
    message_channel_dal_init();
}

/// 创建测试用户（凭证已独立建表，用户行不再携带凭证信息）
async fn seed_user(ctx: &RequestContext, user_id: &str) {
    let user = UserPo::new(
        user_id.to_string(),
        "org-1".to_string(),
        format!("{}-name", user_id),
        format!("{}-display", user_id),
        String::new(),
        String::new(),
        common::enums::UserRole::Member,
        "admin".to_string(),
    );
    crate::service::dao::user::dao()
        .insert(ctx.clone(), &user)
        .await
        .unwrap();
}

/// 创建携带 LarkApp 凭证的测试用户（明文直通兼容：解密路径对未加密值原样返回）
async fn seed_lark_user(ctx: &RequestContext, user_id: &str, app_id: &str, app_secret: &str) {
    seed_user(ctx, user_id).await;
    let po = UserCredentialPo::new(
        format!("cred-{}", user_id),
        "org-1".to_string(),
        user_id.to_string(),
        CredentialKind::LarkApp,
        format!("凭证-{}", user_id),
        CredentialDetail::LarkApp {
            app_id: app_id.to_string(),
            app_secret: app_secret.to_string(),
            encrypt_key: None,
            verification_token: None,
        },
        CredentialVisibility::Private,
        "admin".to_string(),
    );
    crate::service::dao::user_credential::dao()
        .insert(ctx.clone(), &po)
        .await
        .unwrap();
}

/// 创建飞书测试渠道（凭证仅存引用 ID，app_id/secret 在凭证表）
fn lark_channel(
    channel_id: &str,
    user_id: &str,
    credential_id: Option<&str>,
    open_id: Option<&str>,
) -> MessageChannel {
    let po = MessageChannelPo::new(
        channel_id.to_string(),
        "org-1".to_string(),
        user_id.to_string(),
        None,
        ChannelType::Lark,
        format!("lark-{}", channel_id),
        None,
        None,
        None,
        ChannelConfig {
            lark_credential_id: credential_id.map(|s| s.to_string()),
            lark_open_id: open_id.map(|s| s.to_string()),
            ..Default::default()
        },
        "admin".to_string(),
    );
    MessageChannel::from_po(po)
}

async fn init_env(pool: SqlitePool) -> RequestContext {
    init_all_test_daos();
    // 监听生命周期内部使用系统上下文（依赖全局 storage）
    crate::pkg::storage::test_support::init_for_test().await;
    crate::pkg::request_context_test_support::new_test_ctx("admin", pool)
}

/// 注入凭证 DAO 的测试 DAL（凭证引用解析可用）
fn test_dal() -> std::sync::Arc<crate::service::dal::lark::LarkDalImpl> {
    new_for_test_with_credential_dao(
        crate::service::dal::message_channel::dal(),
        crate::service::dao::lark::dao(),
        crate::service::dao::user_credential::dao(),
    )
}

/// 二维定位：同一 open_id 挂在不同应用下互不串扰
#[sqlx::test]
async fn find_channel_by_lark_identity_routes_by_app_and_open_id(pool: SqlitePool) {
    let ctx = init_env(pool).await;
    let base_dal = crate::service::dal::message_channel::dal();
    let dal = test_dal();

    seed_lark_user(&ctx, "user-1", "cli_app_a", "secret-a").await;
    seed_lark_user(&ctx, "user-2", "cli_app_b", "secret-b").await;

    let channel_a = lark_channel(
        "lark-route-a",
        "user-1",
        Some("cred-user-1"),
        Some("ou_same"),
    );
    let channel_b = lark_channel(
        "lark-route-b",
        "user-2",
        Some("cred-user-2"),
        Some("ou_same"),
    );
    base_dal
        .create_channel(ctx.clone(), &channel_a)
        .await
        .unwrap();
    base_dal
        .create_channel(ctx.clone(), &channel_b)
        .await
        .unwrap();

    // app_a + ou_same → channel_a
    let found = dal
        .find_channel_by_lark_identity(ctx.clone(), "cli_app_a", "ou_same")
        .await
        .unwrap();
    assert_eq!(
        found.as_ref().map(|c| c.po.id.as_str()),
        Some("lark-route-a")
    );

    // app_b + ou_same → channel_b（跨应用隔离）
    let found = dal
        .find_channel_by_lark_identity(ctx.clone(), "cli_app_b", "ou_same")
        .await
        .unwrap();
    assert_eq!(
        found.as_ref().map(|c| c.po.id.as_str()),
        Some("lark-route-b")
    );

    // 错误组合 → None
    let found = dal
        .find_channel_by_lark_identity(ctx.clone(), "cli_app_a", "ou_other")
        .await
        .unwrap();
    assert!(found.is_none());
}

/// 禁用渠道不参与二维定位（only_enabled 语义）
#[sqlx::test]
async fn find_channel_by_lark_identity_skips_disabled_channel(pool: SqlitePool) {
    let ctx = init_env(pool).await;
    let base_dal = crate::service::dal::message_channel::dal();
    let dal = test_dal();

    seed_lark_user(&ctx, "user-3", "cli_app_c", "secret-c").await;
    let channel = lark_channel(
        "lark-route-disabled",
        "user-3",
        Some("cred-user-3"),
        Some("ou_x"),
    );
    base_dal
        .create_channel(ctx.clone(), &channel)
        .await
        .unwrap();
    base_dal
        .set_channel_status(ctx.clone(), "lark-route-disabled", ChannelStatus::Disabled)
        .await
        .unwrap();

    let found = dal
        .find_channel_by_lark_identity(ctx.clone(), "cli_app_c", "ou_x")
        .await
        .unwrap();
    assert!(found.is_none());
}

/// 按用户解析凭证：渠道引用 → 凭证行解密凭证 + 身份模式；无渠道用户返回 None
#[sqlx::test]
async fn resolve_credentials_for_user_returns_enabled_channel_credentials(pool: SqlitePool) {
    let ctx = init_env(pool).await;
    let base_dal = crate::service::dal::message_channel::dal();
    let dal = test_dal();

    seed_lark_user(&ctx, "user-cred", "cli_app_cred", "plain-secret").await;
    let channel = lark_channel("lark-cred-1", "user-cred", Some("cred-user-cred"), None);
    base_dal
        .create_channel(ctx.clone(), &channel)
        .await
        .unwrap();

    // 渠道归属用户可解析凭证（secret 解密还原，身份模式缺省 auto）
    let user_ctx = RequestContext::builder()
        .user_id("user-cred")
        .storage(ctx.storage().clone())
        .build();
    let credentials = dal.resolve_credentials_for_user(&user_ctx).await.unwrap();
    let (cred, mode) = credentials.expect("bound user should resolve credentials");
    assert_eq!(cred.app_id, "cli_app_cred");
    assert_eq!(cred.app_secret, "plain-secret");
    assert_eq!(mode, "auto");

    // 未绑定用户返回 None（lark_cli 工具据此给出引导错误）
    let other_ctx = RequestContext::builder()
        .user_id("user-none")
        .storage(ctx.storage().clone())
        .build();
    let credentials = dal.resolve_credentials_for_user(&other_ctx).await.unwrap();
    assert!(credentials.is_none());
}

/// 引用悬空（凭证 ID 在凭证表中不存在）时解析返回 None 而非报错
#[sqlx::test]
async fn resolve_credentials_for_user_returns_none_for_dangling_ref(pool: SqlitePool) {
    let ctx = init_env(pool).await;
    let base_dal = crate::service::dal::message_channel::dal();
    let dal = test_dal();

    // 用户存在但渠道引用了不存在的凭证 ID
    seed_lark_user(&ctx, "user-dangle", "cli_app_d", "secret-d").await;
    let channel = lark_channel("lark-dangle", "user-dangle", Some("cred-missing"), None);
    base_dal
        .create_channel(ctx.clone(), &channel)
        .await
        .unwrap();

    let user_ctx = RequestContext::builder()
        .user_id("user-dangle")
        .storage(ctx.storage().clone())
        .build();
    let credentials = dal.resolve_credentials_for_user(&user_ctx).await.unwrap();
    assert!(credentials.is_none());
}

/// 监听生命周期无网络路径：无渠道引用时 ensure/release 均安全返回
#[sqlx::test]
async fn listener_lifecycle_is_safe_without_channel_reference(pool: SqlitePool) {
    let ctx = init_env(pool).await;
    let dal = test_dal();
    let _ = ctx;

    // 该 app 无任何渠道引用 → ensure 不建连、release 幂等
    dal.ensure_listener_for("cli_app_ghost").await.unwrap();
    assert!(
        !crate::service::dao::lark::dao()
            .is_listening("cli_app_ghost")
            .await
    );
    dal.release_listener_if_unused("cli_app_ghost")
        .await
        .unwrap();
}
