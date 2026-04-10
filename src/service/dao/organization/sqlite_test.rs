//! Organization DAO SQLite 单元测试

use crate::models::organization::OrganizationPo;
use crate::pkg::storage;
use common::constants::{RequestContext, OrganizationStatus, OrganizationScope};
use crate::service::dao::organization::{OrganizationDaoTrait, sqlite::OrganizationDaoImpl};
use uuid::Uuid;

/// 测试所有 Organization DAO 功能
///
/// 由于 storage 使用全局 OnceLock 只能初始化一次，
/// 所以所有测试放在一个函数中顺序执行。
#[test]
fn test_all_organization_dao_functions() {
    // 使用随机文件名，避免冲突
    let random_name = format!("/tmp/ai_orz_test_org_{}.db", Uuid::now_v7());
    let _ = std::fs::remove_file(&random_name);
    let _ = storage::init(&random_name);

    // 创建表和索引
    let _ = storage::get().conn().execute(storage::sql::SQLITE_CREATE_TABLE_ORGANIZATIONS, ());
    let _ = storage::get().conn().execute(storage::sql::SQLITE_CREATE_INDEX_ORGANIZATIONS_ID, ());

    let ctx = RequestContext::new(Some("test-user".to_string()), None);
    let org_dao = OrganizationDaoImpl::new();

    // 第一步: 插入第一个组织
    let id1 = Uuid::now_v7().to_string();
    let org = OrganizationPo::new(
        id1.clone(),
        "我的组织".to_string(),
        "这是我的第一个组织".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    let result = org_dao.insert(ctx.clone(), &org);
    assert!(result.is_ok());

    let found = org_dao.find_by_id(ctx.clone(), &id1).unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, id1);
    assert_eq!(found.name, "我的组织");
    assert_eq!(found.description, "这是我的第一个组织");
    assert_eq!(found.status, OrganizationStatus::Active);
    assert_eq!(found.scope, OrganizationScope::default());

    // 此时只有 1 个组织
    let all = org_dao.find_all(ctx.clone()).unwrap();
    assert_eq!(all.len(), 1);
    let count = org_dao.count_all(ctx.clone()).unwrap();
    assert_eq!(count, 1);

    // 第二步: 插入第二个组织
    let id2 = Uuid::now_v7().to_string();
    let org2 = OrganizationPo::new(
        id2.clone(),
        "组织二".to_string(),
        "".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    org_dao.insert(ctx.clone(), &org2).unwrap();

    // 插入第二个组织之后，现在共有 2 个组织
    let all = org_dao.find_all(ctx.clone()).unwrap();
    assert_eq!(all.len(), 2);
    let count = org_dao.count_all(ctx.clone()).unwrap();
    assert_eq!(count, 2);

    // 第三步: 更新组织
    let id3 = Uuid::now_v7().to_string();
    let mut org_update = OrganizationPo::new(
        id3.clone(),
        "旧名称".to_string(),
        "旧描述".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    org_dao.insert(ctx.clone(), &org_update).unwrap();

    // 插入第三个组织之后，现在共有 3 个组织
    let count = org_dao.count_all(ctx.clone()).unwrap();
    assert_eq!(count, 3);

    org_update.name = "新名称".to_string();
    org_update.description = "新描述".to_string();
    let _result = org_dao.update(ctx.clone(), &org_update);
    assert!(_result.is_ok());

    let found = org_dao.find_by_id(ctx.clone(), &id3).unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.name, "新名称");
    assert_eq!(found.description, "新描述");

    // 第四步: 删除组织（软删除）
    let id4 = Uuid::now_v7().to_string();
    let org_delete = OrganizationPo::new(
        id4.clone(),
        "我的组织".to_string(),
        "".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    let result = org_dao.insert(ctx.clone(), &org_delete);
    assert!(result.is_ok());

    // 插入第四个组织之后，现在共有 4 个组织
    let count = org_dao.count_all(ctx.clone()).unwrap();
    assert_eq!(count, 4);

    let result = org_dao.delete(ctx.clone(), &id4);
    assert!(result.is_ok());

    // delete is soft delete, found will be None because query filters out disabled
    let found = org_dao.find_by_id(ctx.clone(), &id4).unwrap();
    assert!(found.is_none());

    // 删除后 active 组织减少一个 → 总数 3
    let count = org_dao.count_all(ctx.clone()).unwrap();
    assert_eq!(count, 3);

    // 第五步: 插入第五个组织
    let id5 = Uuid::now_v7().to_string();
    let org_count = OrganizationPo::new(
        id5.clone(),
        "我的组织".to_string(),
        "".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    let result = org_dao.insert(ctx.clone(), &org_count);
    assert!(result.is_ok());

    // id1(active), id2(active), id3(active), id4(deleted), id5(active) → 总共 4 active
    let count = org_dao.count_all(ctx.clone()).unwrap();
    assert_eq!(count, 4);
}
