//! User DAO 单元测试
//!
//! 使用内存数据库进行测试

use super::*;
use crate::models::user::UserPo;
use crate::pkg::constants::UserRole;
use crate::pkg::storage::sql;
use crate::pkg::RequestContext;
use rusqlite::Connection;

/// 测试插入用户并查询
#[test]
fn test_insert_and_find_by_id() {
    // 创建内存数据库
    let mut conn = Connection::open_in_memory().unwrap();
    // 创建表
    conn.execute(sql::SQLITE_CREATE_TABLE_ORGANIZATIONS, ()).unwrap();
    conn.execute(sql::SQLITE_CREATE_TABLE_USERS, ()).unwrap();

    let ctx = RequestContext::new("test-user".to_string());
    let user = UserPo::new(
        "test-user-1".to_string(),
        "test-org-1".to_string(),
        "admin".to_string(),
        "超级管理员".to_string(),
        "admin@example.com".to_string(),
        "$2a$10$...hash...".to_string(),
        UserRole::SuperAdmin,
        "test-user-1".to_string(),
    );

    let dao = UserDaoImpl::new();
    let result = dao.insert(ctx.clone(), &user);
    assert!(result.is_ok());

    // 查询
    let found = dao.find_by_id(ctx, "test-user-1").unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, "test-user-1");
    assert_eq!(found.organization_id, "test-org-1");
    assert_eq!(found.username, "admin");
    assert_eq!(found.display_name, "超级管理员");
    assert_eq!(found.email, "admin@example.com");
    assert_eq!(found.role, UserRole::SuperAdmin.to_str());
    assert_eq!(found.status, 1);
}

/// 测试根据用户名查询（用于登录）
#[test]
fn test_find_by_username() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute(sql::SQLITE_CREATE_TABLE_ORGANIZATIONS, ()).unwrap();
    conn.execute(sql::SQLITE_CREATE_TABLE_USERS, ()).unwrap();

    let ctx = RequestContext::new("test-user".to_string());
    let dao = UserDaoImpl::new();

    let user = UserPo::new(
        "test-user-1".to_string(),
        "test-org-1".to_string(),
        "admin".to_string(),
        "超级管理员".to_string(),
        "admin@example.com".to_string(),
        "$2a$10$...hash...".to_string(),
        UserRole::SuperAdmin,
        "test-user-1".to_string(),
    );
    dao.insert(ctx.clone(), &user).unwrap();

    // 根据用户名查询
    let found = dao.find_by_username(ctx, "admin").unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.username, "admin");
}

/// 测试检查用户名是否已存在
#[test]
fn test_exists_by_username() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute(sql::SQLITE_CREATE_TABLE_ORGANIZATIONS, ()).unwrap();
    conn.execute(sql::SQLITE_CREATE_TABLE_USERS, ()).unwrap();

    let ctx = RequestContext::new("test-user".to_string());
    let dao = UserDaoImpl::new();

    // 不存在
    let exists = dao.exists_by_username(ctx.clone(), "admin").unwrap();
    assert!(!exists);

    // 插入
    let user = UserPo::new(
        "test-user-1".to_string(),
        "test-org-1".to_string(),
        "admin".to_string(),
        "超级管理员".to_string(),
        "admin@example.com".to_string(),
        "$2a$10$...hash...".to_string(),
        UserRole::SuperAdmin,
        "test-user-1".to_string(),
    );
    dao.insert(ctx.clone(), &user).unwrap();

    // 存在
    let exists = dao.exists_by_username(ctx, "admin").unwrap();
    assert!(exists);
}

/// 测试根据组织 ID 查询所有用户
#[test]
fn test_find_by_organization_id() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute(sql::SQLITE_CREATE_TABLE_ORGANIZATIONS, ()).unwrap();
    conn.execute(sql::SQLITE_CREATE_TABLE_USERS, ()).unwrap();

    let ctx = RequestContext::new("test-user".to_string());
    let dao = UserDaoImpl::new();

    // 组织 1 两个用户
    let user1 = UserPo::new(
        "user-1".to_string(),
        "org-1".to_string(),
        "user1".to_string(),
        "用户一".to_string(),
        "user1@example.com".to_string(),
        "hash".to_string(),
        UserRole::Member,
        "test-user".to_string(),
    );
    let user2 = UserPo::new(
        "user-2".to_string(),
        "org-1".to_string(),
        "user2".to_string(),
        "用户二".to_string(),
        "user2@example.com".to_string(),
        "hash".to_string(),
        UserRole::Admin,
        "test-user".to_string(),
    );
    // 组织 2 一个用户
    let user3 = UserPo::new(
        "user-3".to_string(),
        "org-2".to_string(),
        "user3".to_string(),
        "用户三".to_string(),
        "user3@example.com".to_string(),
        "hash".to_string(),
        UserRole::Member,
        "test-user".to_string(),
    );
    dao.insert(ctx.clone(), &user1).unwrap();
    dao.insert(ctx.clone(), &user2).unwrap();
    dao.insert(ctx.clone(), &user3).unwrap();

    // 查询组织 1
    let users = dao.find_by_organization_id(ctx, "org-1").unwrap();
    assert_eq!(users.len(), 2);
}

/// 测试统计组织下用户总数
#[test]
fn test_count_by_organization_id() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute(sql::SQLITE_CREATE_TABLE_ORGANIZATIONS, ()).unwrap();
    conn.execute(sql::SQLITE_CREATE_TABLE_USERS, ()).unwrap();

    let ctx = RequestContext::new("test-user".to_string());
    let dao = UserDaoImpl::new();

    let count = dao.count_by_organization_id(ctx.clone(), "org-1").unwrap();
    assert_eq!(count, 0);

    let user = UserPo::new(
        "user-1".to_string(),
        "org-1".to_string(),
        "user1".to_string(),
        "用户一".to_string(),
        "user1@example.com".to_string(),
        "hash".to_string(),
        UserRole::Member,
        "test-user".to_string(),
    );
    dao.insert(ctx.clone(), &user).unwrap();

    let count = dao.count_by_organization_id(ctx, "org-1").unwrap();
    assert_eq!(count, 1);
}

/// 测试软删除
#[test]
fn test_delete() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute(sql::SQLITE_CREATE_TABLE_ORGANIZATIONS, ()).unwrap();
    conn.execute(sql::SQLITE_CREATE_TABLE_USERS, ()).unwrap();

    let ctx = RequestContext::new("test-user".to_string());
    let dao = UserDaoImpl::new();

    let user = UserPo::new(
        "user-1".to_string(),
        "org-1".to_string(),
        "user1".to_string(),
        "用户一".to_string(),
        "user1@example.com".to_string(),
        "hash".to_string(),
        UserRole::Member,
        "test-user".to_string(),
    );
    dao.insert(ctx.clone(), &user).unwrap();

    // 删除
    let result = dao.delete(ctx.clone(), "user-1");
    assert!(result.is_ok());

    // 查询不到
    let found = dao.find_by_id(ctx, "user-1").unwrap();
    assert!(found.is_none());
}

/// 测试更新用户
#[test]
fn test_update() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute(sql::SQLITE_CREATE_TABLE_ORGANIZATIONS, ()).unwrap();
    conn.execute(sql::SQLITE_CREATE_TABLE_USERS, ()).unwrap();

    let ctx = RequestContext::new("test-user".to_string());
    let dao = UserDaoImpl::new();

    let mut user = UserPo::new(
        "user-1".to_string(),
        "org-1".to_string(),
        "old_username".to_string(),
        "旧显示名称".to_string(),
        "old@example.com".to_string(),
        "old_hash".to_string(),
        UserRole::Member,
        "test-user".to_string(),
    );
    dao.insert(ctx.clone(), &user).unwrap();

    // 修改
    user.username = "new_username".to_string();
    user.display_name = "新显示名称".to_string();
    user.email = "new@example.com".to_string();
    let result = dao.update(ctx.clone(), &user);
    assert!(result.is_ok());

    // 查询验证
    let found = dao.find_by_id(ctx, "user-1").unwrap().unwrap();
    assert_eq!(found.username, "new_username");
    assert_eq!(found.display_name, "新显示名称");
    assert_eq!(found.email, "new@example.com");
}
