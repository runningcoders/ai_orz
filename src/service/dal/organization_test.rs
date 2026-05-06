//! Organization DAL 单元测试

use common::enums::OrganizationStatus;
use crate::models::organization::OrganizationPo;
use crate::pkg::RequestContext;
use crate::service::dao::organization::OrganizationQuery;
use sqlx::SqlitePool;

#[sqlx::test]
async fn test_create_and_get_by_id(pool: SqlitePool) {
    crate::service::dao::organization::init();
    crate::service::dal::organization::init();
    let dal = crate::service::dal::organization::dal();
    let ctx = RequestContext::new_simple("admin", pool);

    let org = OrganizationPo::new(
        "org-001".to_string(),
        "测试组织".to_string(),
        "这是一个测试组织".to_string(),
        Some("https://example.com".to_string()),
        "admin".to_string(),
    );

    dal.create(ctx.clone(), &org).await.unwrap();
    let found = dal.get_by_id(ctx, "org-001").await.unwrap().unwrap();

    assert_eq!(found.id, "org-001");
    assert_eq!(found.name, "测试组织");
    assert_eq!(found.description, "这是一个测试组织");
    assert_eq!(found.base_url, "https://example.com");
    assert_eq!(found.created_by, "admin");
    assert_eq!(found.status, OrganizationStatus::Active);
}

#[sqlx::test]
async fn test_is_initialized(pool: SqlitePool) {
    crate::service::dao::organization::init();
    crate::service::dal::organization::init();
    let dal = crate::service::dal::organization::dal();
    let ctx = RequestContext::new_simple("admin", pool);

    // 空数据库未初始化
    let initialized = dal.is_initialized(ctx.clone()).await.unwrap();
    assert!(!initialized);

    // 创建组织后应该已初始化
    let org = OrganizationPo::new(
        "org-001".to_string(),
        "测试组织".to_string(),
        "测试描述".to_string(),
        None,
        "admin".to_string(),
    );
    dal.create(ctx.clone(), &org).await.unwrap();

    let initialized = dal.is_initialized(ctx).await.unwrap();
    assert!(initialized);
}

#[sqlx::test]
async fn test_list_all(pool: SqlitePool) {
    crate::service::dao::organization::init();
    crate::service::dal::organization::init();
    let dal = crate::service::dal::organization::dal();
    let ctx = RequestContext::new_simple("admin", pool);

    // 初始为空
    let list = dal.list_all(ctx.clone()).await.unwrap();
    assert_eq!(list.len(), 0);

    // 创建两个组织
    let org1 = OrganizationPo::new(
        "org-001".to_string(),
        "组织一".to_string(),
        "描述一".to_string(),
        None,
        "admin".to_string(),
    );
    dal.create(ctx.clone(), &org1).await.unwrap();

    let org2 = OrganizationPo::new(
        "org-002".to_string(),
        "组织二".to_string(),
        "描述二".to_string(),
        None,
        "admin".to_string(),
    );
    dal.create(ctx.clone(), &org2).await.unwrap();

    // 查询所有
    let list = dal.list_all(ctx).await.unwrap();
    assert_eq!(list.len(), 2);
    assert!(list.iter().any(|o| o.name == "组织一"));
    assert!(list.iter().any(|o| o.name == "组织二"));
}

#[sqlx::test]
async fn test_query_with_limit(pool: SqlitePool) {
    crate::service::dao::organization::init();
    crate::service::dal::organization::init();
    let dal = crate::service::dal::organization::dal();
    let ctx = RequestContext::new_simple("admin", pool);

    // 创建3个组织
    for i in 1..=3 {
        let org = OrganizationPo::new(
            format!("org-{:03}", i),
            format!("组织{}", i),
            format!("描述{}", i),
            None,
            "admin".to_string(),
        );
        dal.create(ctx.clone(), &org).await.unwrap();
    }

    // 限制返回 2 条
    let results = dal.query(ctx, OrganizationQuery {
        limit: Some(2),
    }).await.unwrap();

    assert_eq!(results.len(), 2);
}

#[sqlx::test]
async fn test_update(pool: SqlitePool) {
    crate::service::dao::organization::init();
    crate::service::dal::organization::init();
    let dal = crate::service::dal::organization::dal();
    let ctx = RequestContext::new_simple("admin", pool);

    let mut org = OrganizationPo::new(
        "org-001".to_string(),
        "旧名称".to_string(),
        "旧描述".to_string(),
        Some("https://old.com".to_string()),
        "admin".to_string(),
    );

    dal.create(ctx.clone(), &org).await.unwrap();

    // 更新组织信息
    org.name = "新名称".to_string();
    org.description = "新描述".to_string();
    org.base_url = "https://new.com".to_string();
    dal.update(ctx.clone(), &org).await.unwrap();

    let updated = dal.get_by_id(ctx, "org-001").await.unwrap().unwrap();
    assert_eq!(updated.name, "新名称");
    assert_eq!(updated.description, "新描述");
    assert_eq!(updated.base_url, "https://new.com");
}

#[sqlx::test]
async fn test_delete(pool: SqlitePool) {
    crate::service::dao::organization::init();
    crate::service::dal::organization::init();
    let dal = crate::service::dal::organization::dal();
    let ctx = RequestContext::new_simple("admin", pool);

    let org = OrganizationPo::new(
        "org-001".to_string(),
        "测试组织".to_string(),
        "测试描述".to_string(),
        None,
        "admin".to_string(),
    );

    dal.create(ctx.clone(), &org).await.unwrap();

    // 删除前能找到
    let before = dal.get_by_id(ctx.clone(), "org-001").await.unwrap();
    assert!(before.is_some());

    dal.delete(ctx.clone(), "org-001").await.unwrap();

    // 删除后找不到（软删除自动过滤 status=0）
    let after = dal.get_by_id(ctx, "org-001").await.unwrap();
    assert!(after.is_none());
}

#[sqlx::test]
async fn test_count_organizations(pool: SqlitePool) {
    crate::service::dao::organization::init();
    crate::service::dal::organization::init();
    let dal = crate::service::dal::organization::dal();
    let ctx = RequestContext::new_simple("admin", pool);

    // 初始计数为 0
    let count = dal.count_organizations(ctx.clone()).await.unwrap();
    assert_eq!(count, 0);

    // 创建3个组织
    for i in 1..=3 {
        let org = OrganizationPo::new(
            format!("org-{:03}", i),
            format!("组织{}", i),
            format!("描述{}", i),
            None,
            "admin".to_string(),
        );
        dal.create(ctx.clone(), &org).await.unwrap();
    }

    // 计数应为 3
    let count = dal.count_organizations(ctx).await.unwrap();
    assert_eq!(count, 3);
}

#[sqlx::test]
async fn test_get_by_id_not_found(pool: SqlitePool) {
    crate::service::dao::organization::init();
    crate::service::dal::organization::init();
    let dal = crate::service::dal::organization::dal();
    let ctx = RequestContext::new_simple("admin", pool);

    // 查询不存在的组织应该返回 None
    let found = dal.get_by_id(ctx, "nonexistent").await.unwrap();
    assert!(found.is_none());
}
