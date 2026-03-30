//! AgentDao 单元测试

use crate::service::dao::agent_dao::{
    agent_dao, AgentDaoImpl, AgentDaoTrait, AGENT_DAO,
};
use crate::models::agent::AgentPo;
use crate::pkg::constants::AgentPoStatus;
use std::sync::Arc;

fn init_dao_for_test() {
    let _ = AGENT_DAO.set(Arc::new(AgentDaoImpl::new()));
}

fn new_test_db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS agents (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            role TEXT NOT NULL DEFAULT '',
            capabilities TEXT NOT NULL DEFAULT '[]',
            soul TEXT NOT NULL DEFAULT '',
            status INTEGER NOT NULL DEFAULT 1,
            created_by TEXT NOT NULL DEFAULT '',
            modified_by TEXT NOT NULL DEFAULT '',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        "#,
    )
    .unwrap();
    conn
}

#[test]
fn test_insert_and_find_by_id() {
    init_dao_for_test();
    let db = new_test_db();
    let dao = agent_dao();
    let agent = AgentPo::new("TestAgent".to_string(), "worker".to_string(), vec!["coding".to_string()], "A helpful agent".to_string(), "admin".to_string());
    dao.insert(&db, &agent).unwrap();
    let found = dao.find_by_id(&db, &agent.id).unwrap().unwrap();
    assert_eq!(found.name, "TestAgent");
    assert_eq!(found.status, AgentPoStatus::Normal);
}

#[test]
fn test_find_all() {
    init_dao_for_test();
    let db = new_test_db();
    let dao = agent_dao();
    for i in 0..3 {
        let agent = AgentPo::new(format!("Agent{}", i), "worker".to_string(), vec![], "".to_string(), "admin".to_string());
        dao.insert(&db, &agent).unwrap();
    }
    let all = dao.find_all(&db).unwrap();
    assert_eq!(all.len(), 3);
}

#[test]
fn test_update() {
    init_dao_for_test();
    let db = new_test_db();
    let dao = agent_dao();
    let agent = AgentPo::new("Original".to_string(), "worker".to_string(), vec![], "".to_string(), "admin".to_string());
    dao.insert(&db, &agent).unwrap();
    let mut updated = agent.clone();
    updated.name = "Updated".to_string();
    updated.modified_by = "other".to_string();
    dao.update(&db, &updated).unwrap();
    let found = dao.find_by_id(&db, &agent.id).unwrap().unwrap();
    assert_eq!(found.name, "Updated");
}

#[test]
fn test_soft_delete() {
    init_dao_for_test();
    let db = new_test_db();
    let dao = agent_dao();
    let agent = AgentPo::new("ToDelete".to_string(), "worker".to_string(), vec![], "".to_string(), "admin".to_string());
    dao.insert(&db, &agent).unwrap();
    dao.delete(&db, &agent.id, "admin").unwrap();
    assert!(dao.find_by_id(&db, &agent.id).unwrap().is_none());
}

#[test]
fn test_find_all_excludes_deleted() {
    init_dao_for_test();
    let db = new_test_db();
    let dao = agent_dao();
    let agent1 = AgentPo::new("Normal".to_string(), "w".to_string(), vec![], "".to_string(), "admin".to_string());
    let agent2 = AgentPo::new("Deleted".to_string(), "w".to_string(), vec![], "".to_string(), "admin".to_string());
    dao.insert(&db, &agent1).unwrap();
    dao.insert(&db, &agent2).unwrap();
    dao.delete(&db, &agent2.id, "admin").unwrap();
    let all = dao.find_all(&db).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name, "Normal");
}

#[test]
fn test_delete_twice_is_idempotent() {
    init_dao_for_test();
    let db = new_test_db();
    let dao = agent_dao();
    let agent = AgentPo::new("Test".to_string(), "w".to_string(), vec![], "".to_string(), "admin".to_string());
    dao.insert(&db, &agent).unwrap();
    dao.delete(&db, &agent.id, "admin").unwrap();
    dao.delete(&db, &agent.id, "other").unwrap();
    assert!(dao.find_by_id(&db, &agent.id).unwrap().is_none());
}

#[test]
fn test_find_not_exists() {
    init_dao_for_test();
    let db = new_test_db();
    let dao = agent_dao();
    let found = dao.find_by_id(&db, "not-exists").unwrap();
    assert!(found.is_none());
}
