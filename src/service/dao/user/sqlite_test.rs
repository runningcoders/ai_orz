//! User DAO SQLite 单元测试

use crate::models::user::UserPo;
use crate::pkg::RequestContext;
use common::enums::{UserRole, UserStatus};
use crate::service::dao::user::{self, UserDaoTrait};
use uuid::Uuid;
use sqlx::SqlitePool;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    RequestContext::new_simple(user_id, pool)
}

/// 测试所有 User DAO 功能
#[sqlx::test]
async fn test_all_user_dao_functions(pool: SqlitePool) {
    // Storage 自动迁移完成
    crate::service::dao::user::init();
    let user_dao = crate::service::dao::user::dao();

    // 伪造随机 organization_id，不需要真实插入组织（无外键约束）
    let org_id1 = Uuid::now_v7().to_string();

    // 测试 1: 插入用户并查询
    let user_id1 = Uuid::now_v7().to_string();
    let username1 = format!("admin_{}", Uuid::now_v7());
    let user = UserPo::new(
        user_id1.clone(),
        org_id1.clone(),
        username1.clone(),
        "超级管理员".to_string(),
        "admin@example.com".to_string(),
        "$2a$10$...hash...".to_string(),
        UserRole::SuperAdmin,
        "test-user-1".to_string(),
    );
    let result = user_dao.insert(new_ctx("test-user", pool.clone()), &user).await;
    if let Err(e) = &result {
        panic!("insert user failed: {}", e);
    }
    assert!(result.is_ok());

    let found: Option<UserPo> = user_dao.find_by_id(new_ctx("test-user", pool.clone()), &user_id1).await.unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, user_id1);
    assert_eq!(found.organization_id, org_id1);
    assert_eq!(found.username, username1);
    assert_eq!(found.display_name, "超级管理员".to_string());
    assert_eq!(found.email, "admin@example.com".to_string());
    assert_eq!(found.role, UserRole::SuperAdmin);
    assert_eq!(found.status, UserStatus::Active);

    // 测试 2: 根据用户名查询（用于登录）
    let user_id_login = Uuid::now_v7().to_string();
    let username_login = format!("loginuser_{}", Uuid::now_v7());
    let user_login = UserPo::new(
        user_id_login.clone(),
        org_id1.clone(),
        username_login.clone(),
        "登录用户".to_string(),
        "login@example.com".to_string(),
        "hash".to_string(),
        UserRole::Member,
        "test-user-1".to_string(),
    );
    let result = user_dao.insert(new_ctx("test-user", pool.clone()), &user_login).await;
    assert!(result.is_ok());

    let found: Option<UserPo> = user_dao.find_by_username(new_ctx("test-user", pool.clone()), &username_login).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, user_id_login);

    // 创建两个伪造组织 id 用于计数测试
    let count_org_id1 = Uuid::now_v7().to_string();
    let count_org_id2 = Uuid::now_v7().to_string();

    // 测试 3: 根据组织 ID 查询所有用户
    let user_id_count1 = Uuid::now_v7().to_string();
    let username_count1 = format!("user1_{}", Uuid::now_v7());
    let user1 = UserPo::new(
        user_id_count1.clone(),
        count_org_id1.clone(),
        username_count1.clone(),
        "User 1".to_string(),
        "user1@example.com".to_string(),
        "hash".to_string(),
        UserRole::Member,
        "test-user-1".to_string(),
    );
    let result = user_dao.insert(new_ctx("test-user", pool.clone()), &user1).await;
    assert!(result.is_ok());

    let user_id_count2 = Uuid::now_v7().to_string();
    let username_count2 = format!("user2_{}", Uuid::now_v7());
    let user2 = UserPo::new(
        user_id_count2.clone(),
        count_org_id1.clone(),
        username_count2.clone(),
        "User 2".to_string(),
        "user2@example.com".to_string(),
        "hash".to_string(),
        UserRole::Member,
        "test-user-1".to_string(),
    );
    let result = user_dao.insert(new_ctx("test-user", pool.clone()), &user2).await;
    assert!(result.is_ok());

    let user_id_count3 = Uuid::now_v7().to_string();
    let username_count3 = format!("user3_{}", Uuid::now_v7());
    let user3 = UserPo::new(
        user_id_count3.clone(),
        count_org_id2.clone(),
        username_count3.clone(),
        "User 3".to_string(),
        "user3@example.com".to_string(),
        "hash".to_string(),
        UserRole::Member,
        "test-user-1".to_string(),
    );
    let result = user_dao.insert(new_ctx("test-user", pool.clone()), &user3).await;
    assert!(result.is_ok());

    let users: Vec<UserPo> = user_dao.find_by_organization_id(new_ctx("test-user", pool.clone()), &count_org_id1).await.unwrap();
    assert_eq!(users.len(), 2);

    // 测试 4: 更新用户
    let user_id_update = Uuid::now_v7().to_string();
    let username_old = format!("olduser_{}", Uuid::now_v7());
    let mut user_update = UserPo::new(
        user_id_update.clone(),
        org_id1.clone(),
        username_old.clone(),
        "旧名称".to_string(),
        "old@example.com".to_string(),
        "oldhash".to_string(),
        UserRole::Member,
        "test-user-1".to_string(),
    );
    let result = user_dao.insert(new_ctx("test-user", pool.clone()), &user_update).await;
    assert!(result.is_ok());

    user_update.username = format!("newuser_{}", Uuid::now_v7());
    user_update.display_name = "新名称".to_string();
    user_update.email = "new@example.com".to_string();
    let result = user_dao.update(new_ctx("test-user", pool.clone()), &user_update).await;
    assert!(result.is_ok());

    let found: Option<UserPo> = user_dao.find_by_id(new_ctx("test-user", pool.clone()), &user_id_update).await.unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.username, user_update.username);
    assert_eq!(found.display_name, "新名称".to_string());
    assert_eq!(found.email, "new@example.com".to_string());

    // 测试 5: 删除用户（软删除）
    let user_id_delete = Uuid::now_v7().to_string();
    let username_delete = format!("deleteuser_{}", Uuid::now_v7());
    let user_delete = UserPo::new(
        user_id_delete.clone(),
        org_id1.clone(),
        username_delete.clone(),
        "删除用户".to_string(),
        "delete@example.com".to_string(),
        "hash".to_string(),
        UserRole::Member,
        "test-user-1".to_string(),
    );
    let result = user_dao.insert(new_ctx("test-user", pool.clone()), &user_delete).await;
    assert!(result.is_ok());

    let result = user_dao.delete(new_ctx("test-user", pool.clone()), &user_id_delete).await;
    assert!(result.is_ok());

    // delete is soft delete, found will be None because query filters out disabled
    let found: Option<UserPo> = user_dao.find_by_id(new_ctx("test-user", pool.clone()), &user_id_delete).await.unwrap();
    assert!(found.is_none());
}
