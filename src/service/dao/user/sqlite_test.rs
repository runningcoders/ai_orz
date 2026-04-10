//! User DAO 单元测试
//!
//! 使用单个临时数据库文件运行所有测试

use crate::models::organization::OrganizationPo;
use crate::models::user::UserPo;
use crate::pkg::storage;
use common::enums::UserRole;
use common::constants::{RequestContext, UserStatus};
use crate::service::dao::organization::{OrganizationDaoTrait, sqlite::OrganizationDaoImpl};
use crate::service::dao::user::{UserDaoTrait, sqlite::UserDaoImpl};
use uuid::Uuid;

/// 运行所有测试在同一个数据库初始化中，避免 OnceLock 重复初始化问题
#[test]
fn test_all_user_dao_functions() {
    // 准备工作：删除旧的临时数据库，初始化全局存储
    let test_db_path = "/tmp/ai_orz_test_user_all.db".to_string();
    let _ = std::fs::remove_file(&test_db_path);

    // 初始化全局存储
    let _ = storage::init(&test_db_path);

    // 初始化数据库表和索引
    let _ = storage::get().conn().execute(storage::sql::SQLITE_CREATE_TABLE_ORGANIZATIONS, ());
    let _ = storage::get().conn().execute(storage::sql::SQLITE_CREATE_INDEX_ORGANIZATIONS_ID, ());
    let _ = storage::get().conn().execute(storage::sql::SQLITE_CREATE_TABLE_USERS, ());
    let _ = storage::get().conn().execute(storage::sql::SQLITE_CREATE_INDEX_USERS_ID, ());
    let _ = storage::get().conn().execute(storage::sql::SQLITE_CREATE_INDEX_USERS_ORGANIZATION_ID, ());
    let _ = storage::get().conn().execute(storage::sql::SQLITE_CREATE_INDEX_USERS_USERNAME, ());

    let ctx = RequestContext::new(Some("test-user-1".to_string()), None);
    let org_dao = OrganizationDaoImpl::new();
    let user_dao = UserDaoImpl::new();

    // ========== 第一步: 插入第一个组织 ==========
    let org_id1 = Uuid::now_v7().to_string();
    let test_org_1 = OrganizationPo::new(
        org_id1.clone(),
        "测试组织 1".to_string(),
        "".to_string(),
        "".to_string(),
        "test-user-1".to_string(),
    );
    let result = org_dao.insert(ctx.clone(), &test_org_1);
    assert!(result.is_ok());

    // 此时只有 1 个组织
    let count_org = org_dao.count_all(ctx.clone()).unwrap();
    assert_eq!(count_org, 1);

    // ========== 第二步: 插入第二个组织 ==========
    let org_id2 = Uuid::now_v7().to_string();
    let test_org_2 = OrganizationPo::new(
        org_id2.clone(),
        "测试组织 2".to_string(),
        "".to_string(),
        "".to_string(),
        "test-user-1".to_string(),
    );
    let result = org_dao.insert(ctx.clone(), &test_org_2);
    assert!(result.is_ok());

    // 插入第二个组织后，现在共有 2 个组织
    let count_org = org_dao.count_all(ctx.clone()).unwrap();
    assert_eq!(count_org, 2);

    // ========== 测试 1: 插入用户并查询 ==========
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
    let result = user_dao.insert(ctx.clone(), &user);
    assert!(result.is_ok());

    let found = user_dao.find_by_id(ctx.clone(), &user_id1).unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, user_id1);
    assert_eq!(found.organization_id, org_id1);
    assert_eq!(found.username, username1);
    assert_eq!(found.display_name, "超级管理员");
    assert_eq!(found.email, "admin@example.com");
    assert_eq!(found.role, UserRole::SuperAdmin);
    assert_eq!(found.status, UserStatus::Active);

    // ========== 测试 2: 根据用户名查询（用于登录） ==========
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
    let result = user_dao.insert(ctx.clone(), &user_login);
    assert!(result.is_ok());

    let found = user_dao.find_by_username(ctx.clone(), &username_login).unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, user_id_login);

    // ========== 第三步: 为计数测试创建两个组织 ==========
    let count_org_id1 = Uuid::now_v7().to_string();
    let test_org_1_count = OrganizationPo::new(
        count_org_id1.clone(),
        "Org 1".to_string(),
        "".to_string(),
        "".to_string(),
        "test-user-1".to_string(),
    );
    let result = org_dao.insert(ctx.clone(), &test_org_1_count);
    assert!(result.is_ok());

    // 插入第一个计数组织后，总数增加 1 → 现在有 3 个组织
    let count_org_step = org_dao.count_all(ctx.clone()).unwrap();
    assert_eq!(count_org_step, 3);

    let count_org_id2 = Uuid::now_v7().to_string();
    let test_org_2_count = OrganizationPo::new(
        count_org_id2.clone(),
        "Org 2".to_string(),
        "".to_string(),
        "".to_string(),
        "test-user-1".to_string(),
    );
    let result = org_dao.insert(ctx.clone(), &test_org_2_count);
    assert!(result.is_ok());

    // 插入第二个计数组织后，总数增加 1 → 现在共有 4 个组织，都 active
    let count_org_after = org_dao.count_all(ctx.clone()).unwrap();
    assert_eq!(count_org_after, 4);

    // ========== 测试 3: 根据组织 ID 查询所有用户 ==========
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
    let result = user_dao.insert(ctx.clone(), &user1);
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
    let result = user_dao.insert(ctx.clone(), &user2);
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
    let result = user_dao.insert(ctx.clone(), &user3);
    assert!(result.is_ok());

    let users = user_dao.find_by_organization_id(ctx.clone(), &count_org_id1).unwrap();
    assert_eq!(users.len(), 2);

    // ========== 测试 4: 更新用户 ==========
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
    let result = user_dao.insert(ctx.clone(), &user_update);
    assert!(result.is_ok());

    user_update.username = format!("newuser_{}", Uuid::now_v7());
    user_update.display_name = "新名称".to_string();
    user_update.email = "new@example.com".to_string();
    let result = user_dao.update(ctx.clone(), &user_update);
    assert!(result.is_ok());

    let found = user_dao.find_by_id(ctx.clone(), &user_id_update).unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.username, user_update.username);
    assert_eq!(found.display_name, "新名称");
    assert_eq!(found.email, "new@example.com");

    // ========== 测试 5: 删除用户（软删除） ==========
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
    let result = user_dao.insert(ctx.clone(), &user_delete);
    assert!(result.is_ok());

    let result = user_dao.delete(ctx.clone(), &user_id_delete);
    assert!(result.is_ok());

    // delete is soft delete, found will be None because query filters out disabled
    let found = user_dao.find_by_id(ctx.clone(), &user_id_delete).unwrap();
    assert!(found.is_none());

    // ========== 测试 6: 检查用户名是否存在 ==========
    let exists = user_dao.exists_by_username(ctx.clone(), &user_update.username).unwrap();
    assert!(exists);

    let not_exists = user_dao.exists_by_username(ctx.clone(), "nonexistent").unwrap();
    assert!(!not_exists);

    // ========== 测试 7: 统计组织下用户数量 ==========
    // 已经插入 user1 + user2 到 count_org_id1，两者都是 active
    let count = user_dao.count_by_organization_id(ctx.clone(), &count_org_id1).unwrap();
    assert_eq!(count, 2);
}
