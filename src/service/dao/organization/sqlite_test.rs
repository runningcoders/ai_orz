//! Organization DAO SQLite 单元测试

use crate::models::organization::OrganizationPo;
use crate::pkg::RequestContext;
use crate::service::dao::organization::{self, OrganizationDao, PeerOrgUpsert};
use common::api::OrganizationConfig;
use common::enums::{OrganizationScope, OrganizationStatus};
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx(user_id, pool)
}

/// 初始化测试环境
fn init_test_env() -> Arc<dyn OrganizationDao + Send + Sync> {
    crate::service::dao::organization::init();
    organization::dao()
}

/// 创建测试 OrganizationPo
fn create_test_organization(name: &str, description: &str, created_by: &str) -> OrganizationPo {
    OrganizationPo::new(
        Uuid::now_v7().to_string(),
        name.to_string(),
        description.to_string(),
        None,
        created_by.to_string(),
    )
}

/// 测试插入组织并按 ID 查询
#[sqlx::test]
async fn test_insert_and_find_by_id(pool: SqlitePool) {
    let org_dao = init_test_env();

    let org = create_test_organization("测试组织", "组织描述", "test-user");
    let org_id = org.id.clone();

    org_dao
        .insert(new_ctx("test-user", pool.clone()), &org)
        .await
        .unwrap();

    let found: Option<OrganizationPo> = org_dao
        .find_by_id(new_ctx("test-user", pool.clone()), &org_id)
        .await
        .unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, org_id);
    assert_eq!(found.name, "测试组织");
    assert_eq!(found.description, "组织描述");
    assert_eq!(found.status, OrganizationStatus::Active);
}

/// 测试查询所有组织
#[sqlx::test]
async fn test_find_all(pool: SqlitePool) {
    let org_dao = init_test_env();

    let org1 = create_test_organization("组织一", "", "test-user");
    let org2 = create_test_organization("组织二", "", "test-user");

    org_dao
        .insert(new_ctx("test-user", pool.clone()), &org1)
        .await
        .unwrap();
    org_dao
        .insert(new_ctx("test-user", pool.clone()), &org2)
        .await
        .unwrap();

    let all: Vec<OrganizationPo> = org_dao
        .find_all(new_ctx("test-user", pool.clone()))
        .await
        .unwrap();
    assert_eq!(all.len(), 2);
}

/// 测试统计组织数量
#[sqlx::test]
async fn test_count_all(pool: SqlitePool) {
    let org_dao = init_test_env();

    let org1 = create_test_organization("组织一", "", "test-user");
    let org2 = create_test_organization("组织二", "", "test-user");

    org_dao
        .insert(new_ctx("test-user", pool.clone()), &org1)
        .await
        .unwrap();
    org_dao
        .insert(new_ctx("test-user", pool.clone()), &org2)
        .await
        .unwrap();

    let count = org_dao
        .count_all(new_ctx("test-user", pool.clone()))
        .await
        .unwrap();
    assert_eq!(count, 2);
}

/// 测试更新组织
#[sqlx::test]
async fn test_update(pool: SqlitePool) {
    let org_dao = init_test_env();

    let mut org = create_test_organization("旧名称", "旧描述", "test-user");
    let org_id = org.id.clone();

    org_dao
        .insert(new_ctx("test-user", pool.clone()), &org)
        .await
        .unwrap();

    org.name = "新名称".to_string();
    org.description = "新描述".to_string();
    org_dao
        .update(new_ctx("test-user", pool.clone()), &org)
        .await
        .unwrap();

    let found: Option<OrganizationPo> = org_dao
        .find_by_id(new_ctx("test-user", pool.clone()), &org_id)
        .await
        .unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.name, "新名称");
    assert_eq!(found.description, "新描述");
}

/// 测试软删除组织
#[sqlx::test]
async fn test_soft_delete(pool: SqlitePool) {
    let org_dao = init_test_env();

    let org = create_test_organization("待删除组织", "", "test-user");
    let org_id = org.id.clone();

    org_dao
        .insert(new_ctx("test-user", pool.clone()), &org)
        .await
        .unwrap();

    // 删除前查询存在
    let found: Option<OrganizationPo> = org_dao
        .find_by_id(new_ctx("test-user", pool.clone()), &org_id)
        .await
        .unwrap();
    assert!(found.is_some());

    // 执行软删除
    org_dao
        .delete(new_ctx("test-user", pool.clone()), &org_id)
        .await
        .unwrap();

    // 删除后查询不存在
    let found: Option<OrganizationPo> = org_dao
        .find_by_id(new_ctx("test-user", pool.clone()), &org_id)
        .await
        .unwrap();
    assert!(found.is_none());
}

/// 测试组织级配置：写穿缓存 + 读穿缓存
///
/// 验证 `set_org_config` 写入 DB 并同步刷新缓存后，`get_org_config`
/// 能命中缓存返回最新值（即 message DAL 门控所依赖的读取路径）。
#[sqlx::test]
async fn test_org_config_set_get_cache(pool: SqlitePool) {
    let org_dao = init_test_env();

    let org = create_test_organization("配置测试组织", "", "test-user");
    let org_id = org.id.clone();
    org_dao
        .insert(new_ctx("test-user", pool.clone()), &org)
        .await
        .unwrap();

    // 默认未设置：回退 DB 读到默认（enable_message_vector = false）
    let default_cfg = org_dao
        .get_org_config(new_ctx("test-user", pool.clone()), &org_id)
        .await
        .unwrap();
    assert!(!default_cfg.enable_message_vector);

    // 开启消息向量开关并写穿
    let cfg = OrganizationConfig {
        enable_message_vector: true,
    };
    org_dao
        .set_org_config(new_ctx("test-user", pool.clone()), &org_id, &cfg)
        .await
        .unwrap();

    // 再次读取：命中缓存，返回开启后的值
    let updated_cfg = org_dao
        .get_org_config(new_ctx("test-user", pool.clone()), &org_id)
        .await
        .unwrap();
    assert!(updated_cfg.enable_message_vector);
}

/// 测试组织级配置：DB 回退默认值
///
/// 不调用 `set_org_config`，直接读取未写入配置的组织的 config，
/// 应回退到 `OrganizationConfig::default()`（enable_message_vector = false）。
#[sqlx::test]
async fn test_org_config_default_fallback(pool: SqlitePool) {
    let org_dao = init_test_env();

    let org = create_test_organization("默认配置组织", "", "test-user");
    let org_id = org.id.clone();
    org_dao
        .insert(new_ctx("test-user", pool.clone()), &org)
        .await
        .unwrap();

    let cfg = org_dao
        .get_org_config(new_ctx("test-user", pool.clone()), &org_id)
        .await
        .unwrap();

    // 未设置 → 回退默认，且默认关闭
    assert_eq!(cfg, OrganizationConfig::default());
    assert!(!cfg.enable_message_vector);
}

// ==================== 联邦影子（静默写入，不发布事件）====================
// 从 dao/organization_link 迁入：organizations 表属主语义在属主 DAO 处测试。

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

fn create_org_with_scope(name: &str, scope: OrganizationScope) -> OrganizationPo {
    let mut org = create_test_organization(name, "", "test-user");
    org.scope = scope;
    org
}

/// 影子 upsert：新对端 → 插入 scope=Remote 影子
#[sqlx::test]
async fn test_upsert_remote_shadow_inserts_new(pool: SqlitePool) {
    let org_dao = init_test_env();
    let ctx = new_ctx("test-user", pool.clone());

    let wrote = org_dao
        .upsert_remote_shadow(ctx.clone(), &peer_upsert("peer-1", "远端组织", 1000))
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
async fn test_upsert_remote_shadow_new_wins(pool: SqlitePool) {
    let org_dao = init_test_env();
    let ctx = new_ctx("test-user", pool.clone());

    org_dao
        .upsert_remote_shadow(ctx.clone(), &peer_upsert("peer-1", "旧名", 1000))
        .await
        .unwrap();

    // 更新的数据：元信息更新，scope 保持 Remote
    let wrote = org_dao
        .upsert_remote_shadow(ctx.clone(), &peer_upsert("peer-1", "新名", 2000))
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
    let wrote = org_dao
        .upsert_remote_shadow(ctx.clone(), &peer_upsert("peer-1", "过期名", 1500))
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
async fn test_upsert_remote_shadow_linked_scope_preserved(pool: SqlitePool) {
    let org_dao = init_test_env();
    let ctx = new_ctx("test-user", pool.clone());

    let linked = create_org_with_scope("已建联组织", OrganizationScope::Linked);
    org_dao.insert(ctx.clone(), &linked).await.unwrap();

    // updated_at 必须比新建组织（当前时间戳）新，否则被新者胜规则正确拒绝
    let newer = chrono::Utc::now().timestamp_millis() + 5000;
    let wrote = org_dao
        .upsert_remote_shadow(ctx.clone(), &peer_upsert(&linked.id, "对端新名", newer))
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
async fn test_upsert_remote_shadow_never_overwrites_local(pool: SqlitePool) {
    let org_dao = init_test_env();
    let ctx = new_ctx("test-user", pool.clone());

    let local = create_org_with_scope("我自己的组织", OrganizationScope::Local);
    org_dao.insert(ctx.clone(), &local).await.unwrap();

    let wrote = org_dao
        .upsert_remote_shadow(ctx.clone(), &peer_upsert(&local.id, "冒名顶替", 9999))
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

/// 直接建联影子 upsert：插入即 Linked；更新强制置 Linked（权威动作，不依赖新者胜）
#[sqlx::test]
async fn test_upsert_linked_shadow_forces_scope(pool: SqlitePool) {
    let org_dao = init_test_env();
    let ctx = new_ctx("test-user", pool.clone());

    // 新对端 → 插入 Linked
    let wrote = org_dao
        .upsert_linked_shadow(ctx.clone(), &peer_upsert("peer-1", "对端组织", 1000))
        .await
        .unwrap();
    assert!(wrote);
    let shadow = org_dao
        .find_by_id(ctx.clone(), "peer-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(shadow.scope, OrganizationScope::Linked);

    // 已有 Remote 影子 → 强制升为 Linked（不依赖新者胜：旧 updated_at 也生效）
    let stale = org_dao
        .upsert_linked_shadow(ctx.clone(), &peer_upsert("peer-2", "对端二", 1000))
        .await
        .unwrap();
    assert!(stale);
    let older = org_dao
        .upsert_linked_shadow(ctx.clone(), &peer_upsert("peer-2", "对端二改", 500))
        .await
        .unwrap();
    assert!(older);
    let shadow = org_dao
        .find_by_id(ctx.clone(), "peer-2")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(shadow.scope, OrganizationScope::Linked);
    assert_eq!(shadow.name, "对端二改");

    // Local 组织绝不覆盖（R5）
    let local = create_org_with_scope("我自己的组织", OrganizationScope::Local);
    org_dao.insert(ctx.clone(), &local).await.unwrap();
    let wrote = org_dao
        .upsert_linked_shadow(ctx.clone(), &peer_upsert(&local.id, "冒名顶替", 9999))
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

/// 断联降级：Linked → Remote（幂等）；不动 updated_at；Local 不受影响
#[sqlx::test]
async fn test_degrade_shadow_to_remote(pool: SqlitePool) {
    let org_dao = init_test_env();
    let ctx = new_ctx("test-user", pool.clone());

    let linked = create_org_with_scope("对端组织", OrganizationScope::Linked);
    org_dao.insert(ctx.clone(), &linked).await.unwrap();
    let before = org_dao
        .find_by_id(ctx.clone(), &linked.id)
        .await
        .unwrap()
        .unwrap();

    let degraded = org_dao
        .degrade_shadow_to_remote(ctx.clone(), &linked.id)
        .await
        .unwrap();
    assert!(degraded);
    let after = org_dao
        .find_by_id(ctx.clone(), &linked.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.scope, OrganizationScope::Remote);
    // updated_at 是「对端数据版本」，本地投影状态变更不参与该比较
    assert_eq!(after.updated_at, before.updated_at);

    // 幂等：再降级一次返回 false
    let again = org_dao
        .degrade_shadow_to_remote(ctx.clone(), &linked.id)
        .await
        .unwrap();
    assert!(!again);

    // Local 组织不受影响
    let local = create_org_with_scope("本地组织", OrganizationScope::Local);
    org_dao.insert(ctx.clone(), &local).await.unwrap();
    let hit = org_dao
        .degrade_shadow_to_remote(ctx.clone(), &local.id)
        .await
        .unwrap();
    assert!(!hit);
}
