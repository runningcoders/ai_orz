//! User DAL 单元测试

use crate::models::user::UserPo;
use crate::pkg::RequestContext;
use crate::service::dal::user::UserDal;
use crate::service::dao::user::UserQuery;
use common::enums::{UserRole, UserStatus};
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

/// 初始化测试环境
async fn init_test_env(pool: SqlitePool) -> (Arc<dyn UserDal + Send + Sync>, RequestContext) {
    crate::service::dao::user::init();
    crate::service::dal::user::init();
    let dal = crate::service::dal::user::dal();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);
    (dal, ctx)
}

/// 创建测试用户
fn create_test_user(id: &str, org_id: &str, username: &str, role: UserRole) -> UserPo {
    UserPo::new(
        id.to_string(),
        org_id.to_string(),
        username.to_string(),
        format!("User {}", username),
        format!("{}@example.com", username),
        "hashed-password".to_string(),
        role,
        "admin".to_string(),
    )
}

#[sqlx::test]
async fn test_create_and_find_by_id(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let org_id = Uuid::now_v7().to_string();
    let user = create_test_user("user-001", &org_id, "testuser", UserRole::Admin);

    dal.create(ctx.clone(), &user).await.unwrap();
    let found = dal.find_by_id(ctx, "user-001").await.unwrap().unwrap();

    assert_eq!(found.id, "user-001");
    assert_eq!(found.organization_id, org_id);
    assert_eq!(found.username, "testuser");
    assert_eq!(found.display_name, "User testuser");
    assert_eq!(found.email, "testuser@example.com");
    assert_eq!(found.role, UserRole::Admin);
    assert_eq!(found.created_by, "admin");
    assert_eq!(found.status, UserStatus::Active);
}

#[sqlx::test]
async fn test_find_by_username(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let org_id = Uuid::now_v7().to_string();
    let user = create_test_user("user-001", &org_id, "testuser", UserRole::Admin);

    dal.create(ctx.clone(), &user).await.unwrap();
    let found = dal
        .find_by_username(ctx, "testuser")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(found.id, "user-001");
    assert_eq!(found.username, "testuser");
}

#[sqlx::test]
async fn test_find_by_organization_id(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let org_id = Uuid::now_v7().to_string();
    let other_org_id = Uuid::now_v7().to_string();

    let users: [(&str, &str, &str); 3] = [
        ("user-001", &org_id, "user1"),
        ("user-002", &org_id, "user2"),
        ("user-003", &other_org_id, "user3"),
    ];

    for (id, oid, username) in users {
        let user = create_test_user(id, oid, username, UserRole::Member);
        dal.create(ctx.clone(), &user).await.unwrap();
    }

    let org_users = dal.find_by_organization_id(ctx, &org_id).await.unwrap();
    assert_eq!(org_users.len(), 2);
    assert!(org_users.iter().any(|u| u.username == "user1"));
    assert!(org_users.iter().any(|u| u.username == "user2"));
}

#[sqlx::test]
async fn test_query_with_organization_filter(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let org_id = Uuid::now_v7().to_string();
    let other_org_id = Uuid::now_v7().to_string();

    let user1 = create_test_user("user-001", &org_id, "user1", UserRole::Member);
    let user2 = create_test_user("user-002", &other_org_id, "user2", UserRole::Member);

    dal.create(ctx.clone(), &user1).await.unwrap();
    dal.create(ctx.clone(), &user2).await.unwrap();

    // 按组织过滤
    let results = dal
        .query(
            ctx,
            UserQuery {
                organization_id: Some(org_id),
                limit: None,
            },
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].username, "user1");
}

#[sqlx::test]
async fn test_update(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let org_id = Uuid::now_v7().to_string();
    let mut user = create_test_user("user-001", &org_id, "testuser", UserRole::Member);

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
    let (dal, ctx) = init_test_env(pool).await;

    let org_id = Uuid::now_v7().to_string();
    let user = create_test_user("user-001", &org_id, "testuser", UserRole::Member);

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
    let (dal, ctx) = init_test_env(pool).await;

    let org_id = Uuid::now_v7().to_string();
    let user = create_test_user("user-001", &org_id, "existing", UserRole::Member);

    dal.create(ctx.clone(), &user).await.unwrap();

    let exists = dal
        .exists_by_username(ctx.clone(), "existing")
        .await
        .unwrap();
    let not_exists = dal.exists_by_username(ctx, "nonexistent").await.unwrap();

    assert!(exists);
    assert!(!not_exists);
}

#[sqlx::test]
async fn test_count_by_organization_id(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let org_id = Uuid::now_v7().to_string();
    let other_org_id = Uuid::now_v7().to_string();

    for i in 1..=3 {
        let user = create_test_user(
            &format!("user-{:03}", i),
            &org_id,
            &format!("user{}", i),
            UserRole::Member,
        );
        dal.create(ctx.clone(), &user).await.unwrap();
    }

    let count = dal
        .count_by_organization_id(ctx.clone(), &org_id)
        .await
        .unwrap();
    let other_count = dal
        .count_by_organization_id(ctx, &other_org_id)
        .await
        .unwrap();

    assert_eq!(count, 3);
    assert_eq!(other_count, 0);
}
