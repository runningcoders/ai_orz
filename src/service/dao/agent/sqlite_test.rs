//! Agent DAO SQLite 单元测试

use crate::models::agent::AgentPo;
use crate::pkg::storage;
use crate::pkg::storage::sql;
use common::constants::RequestContext;
use crate::service::dao::agent::{AgentDaoTrait, sqlite::AgentDaoImpl};
use uuid::Uuid;

fn new_ctx(user_id: &str) -> RequestContext {
    RequestContext::new(Some(user_id.to_string()), None)
}

#[test]
fn test_insert_and_find_by_id() {
    // 使用随机文件名，避免冲突
    let random_name = format!("/tmp/ai_orz_test_agent_dao_{}.db", Uuid::now_v7());
    let _ = std::fs::remove_file(&random_name);
    let _ = storage::init(&random_name);

    // 创建表
    let _ = storage::get().conn().execute(sql::SQLITE_CREATE_TABLE_AGENTS, ());

    let ctx = new_ctx("admin");
    let agent_dao = AgentDaoImpl::new();

    // ========== 测试: 插入并查询
    let agent_po = AgentPo::new(
        "TestAgent".to_string(),
        Some("worker".to_string()),
        "A helpful agent".to_string(),
        vec!["coding".to_string()],
        "A helpful agent that can code".to_string(),
        "provider-id-1".to_string(),
        "admin".to_string(),
    );
    let result = agent_dao.insert(ctx.clone(), &agent_po);
    assert!(result.is_ok());

    // 验证插入成功（使用 DAO 接口查询，不是直接 SQL）
    let found = agent_dao.find_by_id(ctx, &agent_po.id).unwrap();
    assert!(found.is_some());
    let found_agent = found.unwrap();
    assert_eq!(found_agent.name, "TestAgent");
    assert_eq!(found_agent.created_by, "admin");
}

#[test]
fn test_find_all() {
    let random_name = format!("/tmp/ai_orz_test_agent_dao_{}.db", Uuid::now_v7());
    let _ = std::fs::remove_file(&random_name);
    let _ = storage::init(&random_name);

    // 创建表
    let _ = storage::get().conn().execute(sql::SQLITE_CREATE_TABLE_AGENTS, ());

    let ctx = new_ctx("admin");
    let agent_dao = AgentDaoImpl::new();

    // 插入另外两个 Agent（全部通过 DAO 接口插入）
    for i in 0..2 {
        let agent_po2 = AgentPo::new(
            format!("Agent{}", i),
            Some("worker".to_string()),
            "".to_string(),
            vec![],
            "".to_string(),
            format!("provider-{}", i),
            "admin".to_string(),
        );
        let _ = agent_dao.insert(ctx.clone(), &agent_po2);
    }

    let all = agent_dao.find_all(ctx).unwrap();
    assert_eq!(all.len(), 3); // 1 (from first test) + 2 = 3
}

#[test]
fn test_update() {
    let random_name = format!("/tmp/ai_orz_test_agent_dao_{}.db", Uuid::now_v7());
    let _ = std::fs::remove_file(&random_name);
    let _ = storage::init(&random_name);

    // 创建表
    let _ = storage::get().conn().execute(sql::SQLITE_CREATE_TABLE_AGENTS, ());

    let ctx = new_ctx("admin");
    let agent_dao = AgentDaoImpl::new();

    let agent_po = AgentPo::new(
        "Original".to_string(),
        Some("worker".to_string()),
        "".to_string(),
        vec![],
        "".to_string(),
        "provider-id-1".to_string(),
        "admin".to_string(),
    );
    let _ = agent_dao.insert(ctx.clone(), &agent_po);

    let found = agent_dao.find_by_id(ctx.clone(), &agent_po.id).unwrap().unwrap();
    let mut updated = found;
    updated.name = "UpdatedAgent".to_string();
    updated.modified_by = "editor".to_string();
    let result = agent_dao.update(new_ctx("editor"), &updated);
    assert!(result.is_ok());

    let found_after_update = agent_dao.find_by_id(ctx, &updated.id).unwrap().unwrap();
    assert_eq!(found_after_update.name, "UpdatedAgent");
    assert_eq!(found_after_update.modified_by, "editor");
}

#[test]
fn test_soft_delete() {
    let random_name = format!("/tmp/ai_orz_test_agent_dao_{}.db", Uuid::now_v7());
    let _ = std::fs::remove_file(&random_name);
    let _ = storage::init(&random_name);

    // 创建表
    let _ = storage::get().conn().execute(sql::SQLITE_CREATE_TABLE_AGENTS, ());

    let ctx = new_ctx("admin");
    let agent_dao = AgentDaoImpl::new();

    let agent_po = AgentPo::new(
        "ToDelete".to_string(),
        Some("worker".to_string()),
        "".to_string(),
        vec![],
        "".to_string(),
        "provider-id-1".to_string(),
        "admin".to_string(),
    );
    let _ = agent_dao.insert(ctx.clone(), &agent_po);

    assert!(agent_dao.delete(new_ctx("admin"), &agent_po).is_ok());
    let found_after_delete = agent_dao.find_by_id(ctx, &agent_po.id).unwrap();
    assert!(found_after_delete.is_none());
}

#[test]
fn test_find_all_excludes_deleted() {
    let random_name = format!("/tmp/ai_orz_test_agent_dao_{}.db", Uuid::now_v7());
    let _ = std::fs::remove_file(&random_name);
    let _ = storage::init(&random_name);

    // 创建表
    let _ = storage::get().conn().execute(sql::SQLITE_CREATE_TABLE_AGENTS, ());

    let ctx = new_ctx("admin");
    let agent_dao = AgentDaoImpl::new();

    // 插入两个 Agent，删除一个
    let agent_po1 = AgentPo::new(
        "Normal".to_string(),
        Some("worker".to_string()),
        "".to_string(),
        vec![],
        "".to_string(),
        "provider-id-1".to_string(),
        "admin".to_string(),
    );
    let agent_po2 = AgentPo::new(
        "Deleted".to_string(),
        Some("worker".to_string()),
        "".to_string(),
        vec![],
        "".to_string(),
        "provider-id-2".to_string(),
        "admin".to_string(),
    );

    let _ = agent_dao.insert(ctx.clone(), &agent_po1);
    let _ = agent_dao.insert(ctx.clone(), &agent_po2);
    let _ = agent_dao.delete(ctx.clone(), &agent_po2);

    let result = agent_dao.find_all(ctx).unwrap();
    assert_eq!(result.len(), 1);
    let names: Vec<String> = result.iter().map(|a| a.name.clone()).collect();
    assert!(names.contains(&"Normal".to_string()));
    assert!(!names.contains(&"Deleted".to_string()));
}

#[test]
fn test_find_not_exists() {
    let random_name = format!("/tmp/ai_orz_test_agent_dao_{}.db", Uuid::now_v7());
    let _ = std::fs::remove_file(&random_name);
    let _ = storage::init(&random_name);

    // 创建表
    let _ = storage::get().conn().execute(sql::SQLITE_CREATE_TABLE_AGENTS, ());

    let ctx = new_ctx("admin");
    let agent_dao = AgentDaoImpl::new();

    let found_none = agent_dao.find_by_id(ctx, "not-exist-id").unwrap();
    assert!(found_none.is_none());
}
