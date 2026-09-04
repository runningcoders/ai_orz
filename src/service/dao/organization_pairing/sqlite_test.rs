//! OrganizationPairingCode DAO SQLite 单元测试

use crate::models::organization::OrganizationPo;
use crate::models::organization_pairing_code::OrganizationPairingCodePo;
use crate::pkg::RequestContext;
use crate::service::dao::organization;
use crate::service::dao::organization_pairing;
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx(user_id, pool)
}

fn new_code(org_id: &str, code_hash: &str, expires_at: i64) -> OrganizationPairingCodePo {
    OrganizationPairingCodePo {
        id: Uuid::now_v7().to_string(),
        org_id: org_id.to_string(),
        code_hash: code_hash.to_string(),
        expires_at,
        consumed_at: None,
        created_by: "test-user".to_string(),
        created_at: Utc::now().timestamp_millis(),
    }
}

/// 插入签发组织（满足 organization_pairing_codes.org_id 外键约束）
async fn seed_issuer_org(ctx: &RequestContext, org_id: &str) {
    organization::init();
    let org = OrganizationPo::new(
        org_id.to_string(),
        "issuer".to_string(),
        String::new(),
        None,
        org_id.to_string(),
    );
    organization::dao().insert(ctx.clone(), &org).await.unwrap();
}

/// 有效码消费成功，返回签发组织 ID
#[sqlx::test]
async fn test_consume_valid_returns_org_id(pool: SqlitePool) {
    organization_pairing::init();
    let dao = organization_pairing::dao();
    let ctx = new_ctx("u", pool.clone());
    seed_issuer_org(&ctx, "ORG-A").await;
    let now = Utc::now().timestamp_millis();
    dao.insert(ctx.clone(), &new_code("ORG-A", "hash-valid", now + 60_000))
        .await
        .unwrap();
    assert_eq!(
        dao.consume(ctx.clone(), "hash-valid", now).await.unwrap(),
        Some("ORG-A".to_string())
    );
}

/// 单用途：同一码第二次消费返回 None（防重放）
#[sqlx::test]
async fn test_consume_replay_rejected(pool: SqlitePool) {
    organization_pairing::init();
    let dao = organization_pairing::dao();
    let ctx = new_ctx("u", pool.clone());
    seed_issuer_org(&ctx, "ORG-A").await;
    let now = Utc::now().timestamp_millis();
    dao.insert(ctx.clone(), &new_code("ORG-A", "hash-replay", now + 60_000))
        .await
        .unwrap();
    assert_eq!(
        dao.consume(ctx.clone(), "hash-replay", now).await.unwrap(),
        Some("ORG-A".to_string())
    );
    assert_eq!(
        dao.consume(ctx.clone(), "hash-replay", now).await.unwrap(),
        None
    );
}

/// 过期码消费返回 None（TTL 判定，以本端时钟为准，评审稿 R4）
#[sqlx::test]
async fn test_consume_expired_rejected(pool: SqlitePool) {
    organization_pairing::init();
    let dao = organization_pairing::dao();
    let ctx = new_ctx("u", pool.clone());
    seed_issuer_org(&ctx, "ORG-A").await;
    let now = Utc::now().timestamp_millis();
    // 已过期（expires_at 早于 now）
    dao.insert(ctx.clone(), &new_code("ORG-A", "hash-expired", now - 1))
        .await
        .unwrap();
    assert_eq!(
        dao.consume(ctx.clone(), "hash-expired", now).await.unwrap(),
        None
    );
}

/// 未知哈希消费返回 None（不区分无效/过期/已用，防枚举探测，评审稿 §6.3）
#[sqlx::test]
async fn test_consume_unknown_hash_rejected(pool: SqlitePool) {
    organization_pairing::init();
    let dao = organization_pairing::dao();
    let ctx = new_ctx("u", pool.clone());
    let now = Utc::now().timestamp_millis();
    assert_eq!(
        dao.consume(ctx.clone(), "hash-unknown", now).await.unwrap(),
        None
    );
}
