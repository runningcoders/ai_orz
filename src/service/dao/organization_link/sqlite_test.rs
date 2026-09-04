//! OrganizationLink DAO SQLite 单元测试

use crate::models::organization::OrganizationPo;
use crate::models::organization_link::OrganizationLinkPo;
use crate::pkg::RequestContext;
use crate::service::dao::organization::OrganizationDao;
use crate::service::dao::organization_link::{self, OrganizationLinkDao, OrganizationLinkQuery};
use common::enums::{OrganizationLinkStatus, OrganizationScope};
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx(user_id, pool)
}

/// 初始化测试环境（links DAO + organizations DAO 均需要）
fn init_test_env() -> (
    Arc<dyn OrganizationLinkDao + Send + Sync>,
    Arc<dyn OrganizationDao + Send + Sync>,
) {
    crate::service::dao::organization::init();
    organization_link::init();
    (
        organization_link::dao(),
        crate::service::dao::organization::dao(),
    )
}

fn create_test_org(name: &str, scope: OrganizationScope) -> OrganizationPo {
    let mut org = OrganizationPo::new(
        Uuid::now_v7().to_string(),
        name.to_string(),
        String::new(),
        None,
        "test-user".to_string(),
    );
    org.scope = scope;
    org
}

fn create_test_link(local_org_id: &str, peer_org_id: &str) -> OrganizationLinkPo {
    OrganizationLinkPo::new(
        Uuid::now_v7().to_string(),
        local_org_id.to_string(),
        peer_org_id.to_string(),
        "https://peer.example.com".to_string(),
        "a".repeat(64),
        "b".repeat(64),
        "test-user".to_string(),
    )
}

/// 建联：插入 + 按 id / 按组织对查询
#[sqlx::test]
async fn test_insert_and_find(pool: SqlitePool) {
    let (link_dao, org_dao) = init_test_env();
    let ctx = new_ctx("test-user", pool.clone());

    let local = create_test_org("本地组织", OrganizationScope::Local);
    let peer = create_test_org("对端组织", OrganizationScope::Linked);
    org_dao.insert(ctx.clone(), &local).await.unwrap();
    org_dao.insert(ctx.clone(), &peer).await.unwrap();

    let link = create_test_link(&local.id, &peer.id);
    link_dao.insert(ctx.clone(), &link).await.unwrap();

    let by_id = link_dao.find_by_id(ctx.clone(), &link.id).await.unwrap();
    assert!(by_id.is_some());
    assert_eq!(by_id.unwrap().peer_org_id, peer.id);

    let by_pair = link_dao
        .find_by_pair(ctx.clone(), &local.id, &peer.id)
        .await
        .unwrap();
    assert!(by_pair.is_some());

    // 反向对不存在（连接是有向的契约记录）
    assert!(
        link_dao
            .find_by_pair(ctx.clone(), &peer.id, &local.id)
            .await
            .unwrap()
            .is_none()
    );
}

/// 重复建联：唯一约束 (local_org_id, peer_org_id) 拒绝第二条
#[sqlx::test]
async fn test_duplicate_pair_rejected(pool: SqlitePool) {
    let (link_dao, org_dao) = init_test_env();
    let ctx = new_ctx("test-user", pool.clone());

    let local = create_test_org("本地组织", OrganizationScope::Local);
    let peer = create_test_org("对端组织", OrganizationScope::Linked);
    org_dao.insert(ctx.clone(), &local).await.unwrap();
    org_dao.insert(ctx.clone(), &peer).await.unwrap();

    link_dao
        .insert(ctx.clone(), &create_test_link(&local.id, &peer.id))
        .await
        .unwrap();
    assert!(
        link_dao
            .insert(ctx.clone(), &create_test_link(&local.id, &peer.id))
            .await
            .is_err()
    );
}

/// 查询过滤：local_org_id + status
#[sqlx::test]
async fn test_query_filters(pool: SqlitePool) {
    let (link_dao, org_dao) = init_test_env();
    let ctx = new_ctx("test-user", pool.clone());

    let local = create_test_org("本地组织", OrganizationScope::Local);
    let peer1 = create_test_org("对端一", OrganizationScope::Linked);
    let peer2 = create_test_org("对端二", OrganizationScope::Linked);
    for org in [&local, &peer1, &peer2] {
        org_dao.insert(ctx.clone(), org).await.unwrap();
    }

    let link1 = create_test_link(&local.id, &peer1.id);
    let mut link2 = create_test_link(&local.id, &peer2.id);
    link2.status = OrganizationLinkStatus::Revoked;
    link_dao.insert(ctx.clone(), &link1).await.unwrap();
    link_dao.insert(ctx.clone(), &link2).await.unwrap();

    let all = link_dao
        .query(
            ctx.clone(),
            OrganizationLinkQuery {
                local_org_id: Some(local.id.clone()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(all.len(), 2);

    let active = link_dao
        .query(
            ctx.clone(),
            OrganizationLinkQuery {
                local_org_id: Some(local.id.clone()),
                status: Some(OrganizationLinkStatus::Active),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, link1.id);
}

/// 断联：连接置 Revoked（影子降级在 org DAO/DAL 侧，见 organization/sqlite_test.rs）
#[sqlx::test]
async fn test_revoke_marks_link_revoked(pool: SqlitePool) {
    let (link_dao, org_dao) = init_test_env();
    let ctx = new_ctx("test-user", pool.clone());

    let local = create_test_org("本地组织", OrganizationScope::Local);
    let peer = create_test_org("对端组织", OrganizationScope::Linked);
    org_dao.insert(ctx.clone(), &local).await.unwrap();
    org_dao.insert(ctx.clone(), &peer).await.unwrap();

    let link = create_test_link(&local.id, &peer.id);
    link_dao.insert(ctx.clone(), &link).await.unwrap();

    link_dao.revoke(ctx.clone(), &link.id).await.unwrap();

    let revoked = link_dao
        .find_by_id(ctx.clone(), &link.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(revoked.status, OrganizationLinkStatus::Revoked);
}

/// 机器侧鉴权：按对端出站凭证哈希查连接（仅 Active 命中；未知/吊销不命中）
#[sqlx::test]
async fn test_find_active_by_peer_token_hash(pool: SqlitePool) {
    let (link_dao, org_dao) = init_test_env();
    let ctx = new_ctx("test-user", pool.clone());

    let local = create_test_org("本地组织", OrganizationScope::Local);
    let peer = create_test_org("对端组织", OrganizationScope::Linked);
    org_dao.insert(ctx.clone(), &local).await.unwrap();
    org_dao.insert(ctx.clone(), &peer).await.unwrap();

    let mut link = create_test_link(&local.id, &peer.id);
    link.peer_token_hash = "hash-active".to_string();
    link_dao.insert(ctx.clone(), &link).await.unwrap();

    // Active + 哈希匹配 → 命中
    let hit = link_dao
        .find_active_by_peer_token_hash(ctx.clone(), "hash-active")
        .await
        .unwrap();
    assert_eq!(hit.expect("should hit active link").id, link.id);

    // 未知哈希不命中（防枚举：与无效凭证统一 None）
    assert!(
        link_dao
            .find_active_by_peer_token_hash(ctx.clone(), "hash-unknown")
            .await
            .unwrap()
            .is_none()
    );

    // 吊销后不命中（仅 Active 参与鉴权）
    link_dao.revoke(ctx.clone(), &link.id).await.unwrap();
    assert!(
        link_dao
            .find_active_by_peer_token_hash(ctx.clone(), "hash-active")
            .await
            .unwrap()
            .is_none()
    );
}
