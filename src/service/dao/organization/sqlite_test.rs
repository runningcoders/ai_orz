//! Organization DAO SQLite 单元测试

use crate::models::organization::OrganizationPo;
use crate::pkg::RequestContext;
use crate::service::dao::organization::{self, OrganizationDao};
use common::enums::{OrganizationScope, OrganizationStatus};
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;
use common::bail_err;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    RequestContext::new_simple(user_id, pool)
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
