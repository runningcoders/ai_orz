//! Organization DAO 单元测试
//!
//! 使用单个临时数据库文件运行所有测试

use crate::models::organization::OrganizationPo;
use crate::pkg::storage;
use common::constants::{RequestContext, OrganizationStatus, OrganizationScope};
use crate::service::dao::organization::{OrganizationDaoTrait, sqlite::OrganizationDaoImpl};
use uuid::Uuid;

/// 运行所有测试在同一个数据库初始化中，避免 OnceLock 重复初始化问题
#[test]
fn test_all_organization_dao_functions() {
    // 准备工作：删除旧的临时数据库，初始化全局存储
    let test_db_path = "/tmp/ai_orz_test_organization_all.db".to_string();
    let _ = std::fs::remove_file(&test_db_path);

    // 初始化全局存储
    let _ = storage::init(&test_db_path);

    // 初始化数据库表和索引
    let _ = storage::get().conn().execute(storage::sql::SQLITE_CREATE_TABLE_ORGANIZATIONS, ());
    let _ = storage::get().conn().execute(storage::sql::SQLITE_CREATE_INDEX_ORGANIZATIONS_ID, ());

    let ctx = RequestContext::new(Some("test-user".to_string()), None);
    let dao = OrganizationDaoImpl::new();

    // ========== 第一步: 插入第一个组织 ==========
    let id1 = Uuid::now_v7().to_string();
    let org = OrganizationPo::new(
        id1.clone(),
        "我的组织".to_string(),
        "这是我的第一个组织".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    let result = dao.insert(ctx.clone(), &org);
    assert!(result.is_ok());

    let found = dao.find_by_id(ctx.clone(), &id1).unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, id1);
    assert_eq!(found.name, "我的组织");
    assert_eq!(found.description, "这是我的第一个组织");
    assert_eq!(found.status, OrganizationStatus::Active);
    assert_eq!(found.scope, OrganizationScope::default());

    // 此时只有 1 个组织
    let all = dao.find_all(ctx.clone()).unwrap();
    assert_eq!(all.len(), 1);
    let count = dao.count_all(ctx.clone()).unwrap();
    assert_eq!(count, 1);

    // ========== 第二步: 插入第二个组织 ==========
    let id2 = Uuid::now_v7().to_string();
    let org2 = OrganizationPo::new(
        id2.clone(),
        "组织二".to_string(),
        "".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    dao.insert(ctx.clone(), &org2).unwrap();

    // 插入第二个组织之后，现在共有 2 个组织
    let all = dao.find_all(ctx.clone()).unwrap();
    assert_eq!(all.len(), 2);
    let count = dao.count_all(ctx.clone()).unwrap();
    assert_eq!(count, 2);

    // ========== 第三步: 更新组织 ==========
    let id3 = Uuid::now_v7().to_string();
    let mut org_update = OrganizationPo::new(
        id3.clone(),
        "旧名称".to_string(),
        "旧描述".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    dao.insert(ctx.clone(), &org_update).unwrap();

    // 插入第三个组织之后，现在共有 3 个组织
    let count = dao.count_all(ctx.clone()).unwrap();
    assert_eq!(count, 3);

    org_update.name = "新名称".to_string();
    org_update.description = "新描述".to_string();
    let _result = dao.update(ctx.clone(), &org_update);
    assert!(_result.is_ok());

    let found = dao.find_by_id(ctx.clone(), &id3).unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.name, "新名称");
    assert_eq!(found.description, "新描述");

    // ========== 第四步: 删除组织（软删除） ==========
    let id4 = Uuid::now_v7().to_string();
    let org_delete = OrganizationPo::new(
        id4.clone(),
        "我的组织".to_string(),
        "".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    let result = dao.insert(ctx.clone(), &org_delete);
    assert!(result.is_ok());

    // 插入第四个组织之后，现在共有 4 个组织
    let count = dao.count_all(ctx.clone()).unwrap();
    assert_eq!(count, 4);

    let result = dao.delete(ctx.clone(), &id4);
    assert!(result.is_ok());

    // delete is soft delete, found will be None because query filters out disabled
    let found = dao.find_by_id(ctx.clone(), &id4).unwrap();
    assert!(found.is_none());

    // 删除后 active 组织减少一个 → 总数 3
    let count = dao.count_all(ctx.clone()).unwrap();
    assert_eq!(count, 3);

    // ========== 第五步: 插入第五个组织 ==========
    let id5 = Uuid::now_v7().to_string();
    let org_count = OrganizationPo::new(
        id5.clone(),
        "我的组织".to_string(),
        "".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    let result = dao.insert(ctx.clone(), &org_count);
    assert!(result.is_ok());

    // id1(active), id2(active), id3(active), id4(deleted), id5(active) → 总共 4 active
    let count = dao.count_all(ctx).unwrap();
    assert_eq!(count, 4);
}
