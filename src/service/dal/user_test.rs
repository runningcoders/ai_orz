//! User DAL 单元测试

use common::enums::{UserRole, UserStatus};
use crate::models::user::UserPo;
use crate::pkg::RequestContext;
use crate::service::dao::user::UserQuery;
use sqlx::SqlitePool;
use uuid::Uuid;

#[sqlx::test]
async fn test_create_and_find_by_id(pool: SqlitePool) {
    crate::service::dao::user::init();
    crate::service::dal::user::init();
    let dal = crate::service::dal::user::dal();
    let ctx = RequestContext::new_simple("admin", pool);

    let org_id = Uuid::now_v7().to_string();
    let user = UserPo::new(
        "user-001".to_string(),
        org_id.clone(),
        "testuser".to_string(),
        "Test User".to_string(),
        "test@example.com".to_string(),
        "hashed-password".to_string(),
        UserRole::Admin,
        "admin".to_string(),
    );

    dal.create(ctx.clone(), &user).await.unwrap();
    let found = dal.find_by_id(ctx, "user-001").await.unwrap().unwrap();

    assert_eq!(found.id, "user-001");
    assert_eq!(found.organization_id, org_id);
    assert_eq!(found.username, "testuser");
    assert_eq!(found.display_name, "Test User");
    assert_eq!(found.email, "test@example.com");
    assert_eq!(found.role, UserRole::Admin);
    assert_eq!(found.created_by, "admin");
    assert_eq!(found.status, UserStatus::Active);
}

#[sqlx::test]
async fn test_find_by_username(pool: SqlitePool) {
    crate::service::dao::user::init();
    crate::service::dal::user::init();
    let dal = crate::service::dal::user::dal();
    let ctx = RequestContext::new_simple("admin", pool);

    let org_id = Uuid::now_v7().to_string();
    let user = UserPo::new(
        "user-001".to_string(),
        org_id,
        "testuser".to_string(),
        "Test User".to_string(),
        "test@example.com".to_string(),
        "hashed-password".to_string(),
        UserRole::Admin,
        "admin".to_string(),
    );

    dal.create(ctx.clone(), &user).await.unwrap();
    let found = dal.find_by_username(ctx, "testuser").await.unwrap().unwrap();

    assert_eq!(found.id, "user-001");
    assert_eq!(found.username, "testuser");
}

#[sqlx::test]
async fn test_find_by_organization_id(pool: SqlitePool) {
    crate::service::dao::user::init();
    crate::service::dal::user::init();
    let dal = crate::service::dal::user::dal();
    let ctx = RequestContext::new_simple("admin", pool);

    let org_id = Uuid::now_v7().to_string();
    let other_org_id = Uuid::now_v7().to_string();

    let users: [(&str, String, &str); 3] = [
        ("user-001", org_id.clone(), "user1"),
        ("user-002", org_id.clone(), "user2"),
        ("user-003", other_org_id, "user3"),
    ];

    for (id, oid, username) in users {
        let user = UserPo::new(
            id.to_string(),
            oid,
            username.to_string(),
            username.to_string(),
            format!("{}@example.com", username),
            "hashed-password".to_string(),
            UserRole::Member,
            "admin".to_string(),
        );
        dal.create(ctx.clone(), &user).await.unwrap();
    }

    let org_users = dal.find_by_organization_id(ctx, &org_id).await.unwrap();
    assert_eq!(org_users.len(), 2);
    assert!(org_users.iter().any(|u| u.username == "user1"));
    assert!(org_users.iter().any(|u| u.username == "user2"));
}

#[sqlx::test]
async fn test_query_with_organization_filter(pool: SqlitePool) {
    crate::service::dao::user::init();
    crate::service::dal::user::init();
    let dal = crate::service::dal::user::dal();
    let ctx = RequestContext::new_simple("admin", pool);

    let org_id = Uuid::now_v7().to_string();
    let other_org_id = Uuid::now_v7().to_string();

    // 组织1的用户
    let user1 = UserPo::new(
        "user-001".to_string(),
        org_id.clone(),
        "user1".to_string(),
        "User One".to_string(),
        "user1@example.com".to_string(),
        "hashed-password".to_string(),
        UserRole::Member,
        "admin".to_string(),
    );
    dal.create(ctx.clone(), &user1).await.unwrap();

    // 组织2的用户
    let user2 = UserPo::new(
        "user-002".to_string(),
        other_org_id,
        "user2".to_string(),
        "User Two".to_string(),
        "user2@example.com".to_string(),
        "hashed-password".to_string(),
        UserRole::Member,
        "admin".to_string(),
    );
    dal.create(ctx.clone(), &user2).await.unwrap();

    // 按组织过滤
    let results = dal.query(ctx, UserQuery {
        organization_id: Some(org_id),
        limit: None,
    }).await.unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].username, "user1");
}

#[sqlx::test]
async fn test_update(pool: SqlitePool) {
    crate::service::dao::user::init();
    crate::service::dal::user::init();
    let dal = crate::service::dal::user::dal();
    let ctx = RequestContext::new_simple("admin", pool);

    let org_id = Uuid::now_v7().to_string();
    let mut user = UserPo::new(
        "user-001".to_string(),
        org_id,
        "testuser".to_string(),
        "Old Name".to_string(),
        "old@example.com".to_string(),
        "hashed-password".to_string(),
        UserRole::Member,
        "admin".to_string(),
    );

    dal.create(ctx.clone(), &user).await.unwrap();

    // 更新用户信息
    user.display_name = "New Name".to_string();
    user.email = "new@example.com".to_string();
    dal.update(ctx.clone(), &user).await.unwrap();

    let updated = dal.find_by_id(ctx, "user-001").await.unwrap().unwrap();
    assert_eq!(updated.display_name, "New Name");
    assert_eq!(updated.email, "new@example.com");
}

#[sqlx::test]
async fn test_delete(pool: SqlitePool) {
    crate::service::dao::user::init();
    crate::service::dal::user::init();
    let dal = crate::service::dal::user::dal();
    let ctx = RequestContext::new_simple("admin", pool);

    let org_id = Uuid::now_v7().to_string();
    let user = UserPo::new(
        "user-001".to_string(),
        org_id,
        "testuser".to_string(),
        "Test User".to_string(),
        "test@example.com".to_string(),
        "hashed-password".to_string(),
        UserRole::Member,
        "admin".to_string(),
    );

    dal.create(ctx.clone(), &user).await.unwrap();

    // 删除前能找到
    let before = dal.find_by_id(ctx.clone(), "user-001").await.unwrap();
    assert!(before.is_some());

    dal.delete(ctx.clone(), "user-001").await.unwrap();

    // 删除后找不到（因为 find_by_id 自动过滤 status=0）
    let after = dal.find_by_id(ctx, "user-001").await.unwrap();
    assert!(after.is_none());
}

#[sqlx::test]
async fn test_exists_by_username(pool: SqlitePool) {
    crate::service::dao::user::init();
    crate::service::dal::user::init();
    let dal = crate::service::dal::user::dal();
    let ctx = RequestContext::new_simple("admin", pool);

    let org_id = Uuid::now_v7().to_string();
    let user = UserPo::new(
        "user-001".to_string(),
        org_id,
        "existing".to_string(),
        "Existing User".to_string(),
        "existing@example.com".to_string(),
        "hashed-password".to_string(),
        UserRole::Member,
        "admin".to_string(),
    );

    dal.create(ctx.clone(), &user).await.unwrap();

    let exists = dal.exists_by_username(ctx.clone(), "existing").await.unwrap();
    let not_exists = dal.exists_by_username(ctx, "nonexistent").await.unwrap();

    assert!(exists);
    assert!(!not_exists);
}

#[sqlx::test]
async fn test_count_by_organization_id(pool: SqlitePool) {
    crate::service::dao::user::init();
    crate::service::dal::user::init();
    let dal = crate::service::dal::user::dal();
    let ctx = RequestContext::new_simple("admin", pool);

    let org_id = Uuid::now_v7().to_string();
    let other_org_id = Uuid::now_v7().to_string();

    for i in 1..=3 {
        let user = UserPo::new(
            format!("user-{:03}", i),
            org_id.clone(),
            format!("user{}", i),
            format!("User {}", i),
            format!("user{}@example.com", i),
            "hashed-password".to_string(),
            UserRole::Member,
            "admin".to_string(),
        );
        dal.create(ctx.clone(), &user).await.unwrap();
    }

    let count = dal.count_by_organization_id(ctx.clone(), &org_id).await.unwrap();
    let other_count = dal.count_by_organization_id(ctx, &other_org_id).await.unwrap();

    assert_eq!(count, 3);
    assert_eq!(other_count, 0);
}
