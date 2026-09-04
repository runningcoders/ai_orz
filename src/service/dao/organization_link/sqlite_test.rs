//! OrganizationLink DAO SQLite 单元测试

use crate::models::organization::OrganizationPo;
use crate::models::organization_link::OrganizationLinkPo;
use crate::pkg::RequestContext;
use crate::service::dao::organization::OrganizationDao;
use crate::service::dao::organization_link::{
    self, OrganizationLinkDao, OrganizationLinkQuery, PeerOrgUpsert,
};
use common::enums::{OrganizationLinkStatus, OrganizationScope, OrganizationStatus};
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

fn peer_upsert(id: &str, name: &str, updated_at: i64) -> PeerOrgUpsert {
    PeerOrgUpsert {
        id: id.to_string(),
        name: name.to_string(),
        description: format!("{} 的描述", name),
        base_url: "https://peer.example.com".to_string(),
        group_name: Some("示例集团".to_string()),
        status: OrganizationStatus::Active,
        updated_at,
    }
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

/// 断联：连接置 Revoked + 对端影子 Linked → Remote；本端组织 scope 不受影响
#[sqlx::test]
async fn test_revoke_downgrades_peer_scope(pool: SqlitePool) {
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

    let peer_org = org_dao
        .find_by_id(ctx.clone(), &peer.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(peer_org.scope, OrganizationScope::Remote);

    let local_org = org_dao
        .find_by_id(ctx.clone(), &local.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(local_org.scope, OrganizationScope::Local);
}

/// 影子 upsert：新对端 → 插入 scope=Remote 影子
#[sqlx::test]
async fn test_upsert_inserts_new_shadow(pool: SqlitePool) {
    let (link_dao, org_dao) = init_test_env();
    let ctx = new_ctx("test-user", pool.clone());

    let wrote = link_dao
        .upsert_peer_org(ctx.clone(), &peer_upsert("peer-1", "远端组织", 1000))
        .await
        .unwrap();
    assert!(wrote);

    let shadow = org_dao
        .find_by_id(ctx.clone(), "peer-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(shadow.scope, OrganizationScope::Remote);
    assert_eq!(shadow.name, "远端组织");
    assert_eq!(shadow.group_name.as_deref(), Some("示例集团"));
}

/// 影子 upsert：新者胜——更新的数据覆盖元信息且不动 scope；更旧的数据跳过
#[sqlx::test]
async fn test_upsert_new_wins_and_scope_preserved(pool: SqlitePool) {
    let (link_dao, org_dao) = init_test_env();
    let ctx = new_ctx("test-user", pool.clone());

    link_dao
        .upsert_peer_org(ctx.clone(), &peer_upsert("peer-1", "旧名", 1000))
        .await
        .unwrap();

    // 更新的数据：元信息更新，scope 保持 Remote
    let wrote = link_dao
        .upsert_peer_org(ctx.clone(), &peer_upsert("peer-1", "新名", 2000))
        .await
        .unwrap();
    assert!(wrote);
    let shadow = org_dao
        .find_by_id(ctx.clone(), "peer-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(shadow.name, "新名");
    assert_eq!(shadow.scope, OrganizationScope::Remote);

    // 更旧的数据：跳过
    let wrote = link_dao
        .upsert_peer_org(ctx.clone(), &peer_upsert("peer-1", "过期名", 1500))
        .await
        .unwrap();
    assert!(!wrote);
    let shadow = org_dao
        .find_by_id(ctx.clone(), "peer-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(shadow.name, "新名");
}

/// 影子 upsert：已建联（Linked）的对端只更新元信息，绝不动 scope
#[sqlx::test]
async fn test_upsert_linked_scope_preserved(pool: SqlitePool) {
    let (link_dao, org_dao) = init_test_env();
    let ctx = new_ctx("test-user", pool.clone());

    let linked = create_test_org("已建联组织", OrganizationScope::Linked);
    org_dao.insert(ctx.clone(), &linked).await.unwrap();

    // updated_at 必须比新建组织（当前时间戳）新，否则被新者胜规则正确拒绝
    let newer = chrono::Utc::now().timestamp_millis() + 5000;
    let wrote = link_dao
        .upsert_peer_org(ctx.clone(), &peer_upsert(&linked.id, "对端新名", newer))
        .await
        .unwrap();
    assert!(wrote);

    let org = org_dao
        .find_by_id(ctx.clone(), &linked.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(org.name, "对端新名");
    assert_eq!(org.scope, OrganizationScope::Linked);
}

/// 影子 upsert：本地组织（scope=Local）绝不覆盖（id 撞车防护，评审稿 R5）
#[sqlx::test]
async fn test_upsert_never_overwrites_local_org(pool: SqlitePool) {
    let (link_dao, org_dao) = init_test_env();
    let ctx = new_ctx("test-user", pool.clone());

    let local = create_test_org("我自己的组织", OrganizationScope::Local);
    org_dao.insert(ctx.clone(), &local).await.unwrap();

    let wrote = link_dao
        .upsert_peer_org(ctx.clone(), &peer_upsert(&local.id, "冒名顶替", 9999))
        .await
        .unwrap();
    assert!(!wrote);

    let org = org_dao
        .find_by_id(ctx.clone(), &local.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(org.name, "我自己的组织");
    assert_eq!(org.scope, OrganizationScope::Local);
}
