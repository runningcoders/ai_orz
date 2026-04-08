//! Organization DAO 单元测试
//!
//! 使用内存数据库进行测试

use super::*;
use crate::models::organization::OrganizationPo;
use crate::pkg::storage::sql;
use crate::pkg::RequestContext;
use rusqlite::Connection;

/// 测试插入组织并查询
#[test]
fn test_insert_and_find_by_id() {
    // 创建内存数据库
    let mut conn = Connection::open_in_memory().unwrap();
    // 创建表
    conn.execute(sql::SQLITE_CREATE_TABLE_ORGANIZATIONS, ()).unwrap();

    let ctx = RequestContext::new("test-user".to_string());
    let org = OrganizationPo::new(
        "test-org-1".to_string(),
        "我的组织".to_string(),
        "这是我的第一个组织".to_string(),
        "test-user".to_string(),
    );

    let dao = OrganizationDaoImpl::new();
    let result = dao.insert(ctx.clone(), &org);
    assert!(result.is_ok());

    // 查询
    let found = dao.find_by_id(ctx, "test-org-1").unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, "test-org-1");
    assert_eq!(found.name, "我的组织");
    assert_eq!(found.description, "这是我的第一个组织");
    assert_eq!(found.status, 1);
}

/// 测试查询所有组织
#[test]
fn test_find_all() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute(sql::SQLITE_CREATE_TABLE_ORGANIZATIONS, ()).unwrap();

    let ctx = RequestContext::new("test-user".to_string());
    let dao = OrganizationDaoImpl::new();

    // 插入两个组织
    let org1 = OrganizationPo::new(
        "test-org-1".to_string(),
        "组织一".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    let org2 = OrganizationPo::new(
        "test-org-2".to_string(),
        "组织二".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    dao.insert(ctx.clone(), &org1).unwrap();
    dao.insert(ctx.clone(), &org2).unwrap();

    let all = dao.find_all(ctx).unwrap();
    assert_eq!(all.len(), 2);
    // 按创建时间降序，所以第一个是 org2
    assert_eq!(all[0].id, "test-org-2");
    assert_eq!(all[1].id, "test-org-1");
}

/// 测试统计组织总数
#[test]
fn test_count_all() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute(sql::SQLITE_CREATE_TABLE_ORGANIZATIONS, ()).unwrap();

    let ctx = RequestContext::new("test-user".to_string());
    let dao = OrganizationDaoImpl::new();

    let count = dao.count_all(ctx.clone()).unwrap();
    assert_eq!(count, 0);

    let org = OrganizationPo::new(
        "test-org-1".to_string(),
        "我的组织".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    dao.insert(ctx.clone(), &org).unwrap();

    let count = dao.count_all(ctx).unwrap();
    assert_eq!(count, 1);
}

/// 测试更新组织
#[test]
fn test_update() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute(sql::SQLITE_CREATE_TABLE_ORGANIZATIONS, ()).unwrap();

    let ctx = RequestContext::new("test-user".to_string());
    let dao = OrganizationDaoImpl::new();

    let mut org = OrganizationPo::new(
        "test-org-1".to_string(),
        "旧名称".to_string(),
        "旧描述".to_string(),
        "test-user".to_string(),
    );
    dao.insert(ctx.clone(), &org).unwrap();

    // 修改名称和描述
    org.name = "新名称".to_string();
    org.description = "新描述".to_string();
    let result = dao.update(ctx.clone(), &org);
    assert!(result.is_ok());

    // 查询验证
    let found = dao.find_by_id(ctx, "test-org-1").unwrap().unwrap();
    assert_eq!(found.name, "新名称");
    assert_eq!(found.description, "新描述");
}

/// 测试软删除
#[test]
fn test_delete() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute(sql::SQLITE_CREATE_TABLE_ORGANIZATIONS, ()).unwrap();

    let ctx = RequestContext::new("test-user".to_string());
    let dao = OrganizationDaoImpl::new();

    let org = OrganizationPo::new(
        "test-org-1".to_string(),
        "我的组织".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    dao.insert(ctx.clone(), &org).unwrap();

    // 删除
    let result = dao.delete(ctx.clone(), "test-org-1");
    assert!(result.is_ok());

    // 查询应该找不到
    let found = dao.find_by_id(ctx, "test-org-1").unwrap();
    assert!(found.is_none());

    // 统计应该是 0
    let count = dao.count_all(ctx).unwrap();
    assert_eq!(count, 0);
}
