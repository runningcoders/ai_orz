//! Organization DAO SQLite 单元测试

use crate::models::organization::OrganizationPo;
use crate::pkg::RequestContext;
use crate::service::dao::organization::{self, OrganizationDao};
use common::api::OrganizationConfig;
use common::enums::OrganizationStatus;
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
