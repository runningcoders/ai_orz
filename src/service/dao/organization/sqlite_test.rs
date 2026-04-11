//! Organization DAO SQLite 单元测试

use crate::models::organization::OrganizationPo;
use crate::pkg::storage::Storage;
use crate::pkg::RequestContext;
use common::enums::{OrganizationStatus, OrganizationScope};
use crate::service::dao::organization::{OrganizationDao, OrganizationDaoTrait};
use uuid::Uuid;

/// 测试所有 Organization DAO 功能
#[tokio::test]
async fn test_all_organization_dao_functions() {
    // 使用随机文件名，避免冲突 → 每个测试独立数据库，彻底隔离
    let random_name = format!("/tmp/ai_orz_test_org_{}.db", Uuid::now_v7());
    let _ = std::fs::remove_file(&random_name);

    // Storage 自动运行迁移，创建所有表
    let storage = Storage::new(&random_name).await.expect("Failed to create storage");
    let pool = storage.pool();

    let ctx = RequestContext::new(Some("test-user".to_string()), None);
    let org_dao = OrganizationDao::new(pool.clone());

    // 第一步: 插入第一个组织
    let id1 = Uuid::now_v7().to_string();
    let org = OrganizationPo::new(
        id1.clone(),
        "我的组织".to_string(),
        "这是我的第一个组织".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    let result = org_dao.insert(ctx.clone(), &org).await;
    assert!(result.is_ok());

    let found = org_dao.find_by_id(ctx.clone(), &id1).await.unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, Some(id1));
    assert_eq!(found.name, Some("我的组织".to_string()));
    assert_eq!(found.description, Some("这是我的第一个组织".to_string()));
    assert_eq!(found.status, OrganizationStatus::Active);
    assert_eq!(found.scope, OrganizationScope::default());

    // 此时只有 1 个组织
    let all = org_dao.find_all(ctx.clone()).await.unwrap();
    assert_eq!(all.len(), 1);
    let count = org_dao.count_all(ctx.clone()).await.unwrap();
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
    org_dao.insert(ctx.clone(), &org2).await.unwrap();

    // 插入第二个组织之后，现在共有 2 个组织
    let all = org_dao.find_all(ctx.clone()).await.unwrap();
    assert_eq!(all.len(), 2);
    let count = org_dao.count_all(ctx.clone()).await.unwrap();
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
    org_dao.insert(ctx.clone(), &org_update).await.unwrap();

    // 插入第三个组织之后，现在共有 3 个组织
    let count = org_dao.count_all(ctx.clone()).await.unwrap();
    assert_eq!(count, 3);

    org_update.name = Some("新名称".to_string());
    org_update.description = Some("新描述".to_string());
    let _result = org_dao.update(ctx.clone(), &org_update).await;
    assert!(_result.is_ok());

    let found = org_dao.find_by_id(ctx.clone(), &id3).await.unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.name, Some("新名称".to_string()));
    assert_eq!(found.description, Some("新描述".to_string()));

    // 第四步: 删除组织（软删除）
    let id4 = Uuid::now_v7().to_string();
    let org_delete = OrganizationPo::new(
        id4.clone(),
        "我的组织".to_string(),
        "".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    let result = org_dao.insert(ctx.clone(), &org_delete).await;
    assert!(result.is_ok());

    // 插入第四个组织之后，现在共有 4 个组织
    let count = org_dao.count_all(ctx.clone()).await.unwrap();
    assert_eq!(count, 4);

    let result = org_dao.delete(ctx.clone(), &id4).await;
    assert!(result.is_ok());

    // delete is soft delete, found will be None because query filters out disabled
    let found = org_dao.find_by_id(ctx.clone(), &id4).await.unwrap();
    assert!(found.is_none());

    // 删除后 active 组织减少一个 → 总数 3
    let count = org_dao.count_all(ctx.clone()).await.unwrap();
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
    let result = org_dao.insert(ctx.clone(), &org_count).await;
    assert!(result.is_ok());

    // id1(active), id2(active), id3(active), id4(deleted), id5(active) → 总共 4 active
    let count = org_dao.count_all(ctx.clone()).await.unwrap();
    assert_eq!(count, 4);
}
