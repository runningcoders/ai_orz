//! Agent DAO SQLite 单元测试

use super::*;
use crate::models::agent::AgentPo;
use crate::pkg::storage;
use crate::pkg::storage::sql;
use common::constants::RequestContext;
use common::enums::AgentStatus;

fn new_ctx(user_id: &str) -> RequestContext {
    RequestContext::new(Some(user_id.to_string()), None)
}

fn insert_agent(conn: &rusqlite::Connection, agent: &AgentPo, creator: &str) {
    let now = current_timestamp();
    conn.execute(
        "INSERT INTO agents (id, name, description, soul, capability, model_provider_id, status, created_by, modified_by, created_at, updated_at) 
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![
            agent.id,
            agent.name,
            agent.description,
            agent.soul,
            agent.capabilities, // capabilities (struct) → capability (DB)
            agent.model_provider_id,
            agent.status.to_i32(),
            creator,
            creator,
            now,
            now,
        ],
    )
    .unwrap();
}

fn find_agent(conn: &rusqlite::Connection, id: &str) -> Option<AgentPo> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, description, soul, capability, model_provider_id, status, created_by, modified_by, created_at, updated_at 
             FROM agents WHERE id = ?1 AND status != 0",
        )
        .unwrap();

    match stmt.query_row([id], |row| {
        Ok(AgentPo {
            id: row.get(0)?,
            name: row.get(1)?,
            role: "".to_string(), // role is not in SQL table, keep default empty
            description: row.get(2)?,
            capabilities: row.get::<_, String>(4)?, // capability (DB) → capabilities (struct)
            soul: row.get(3)?,
            model_provider_id: row.get(5)?,
            status: AgentStatus::from_i32(row.get(6)?),
            created_by: row.get(7)?,
            modified_by: row.get(8)?,
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
        })
    }) {
        Ok(a) => Some(a),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(_) => None,
    }
}

#[test]
fn test_insert_and_find_by_id() {
    let conn = storage::test_conn();
    conn.execute_batch(sql::SQLITE_CREATE_TABLE_AGENTS)
        .unwrap();

    sqlite::init();
    let dao = sqlite::dao();
    let ctx = new_ctx("admin");

    let agent_po = AgentPo::new(
        "TestAgent".to_string(),
        Some("worker".to_string()),
        "A helpful agent".to_string(),
        vec!["coding".to_string()],
        "A helpful agent that can code".to_string(),
        "provider-id-1".to_string(),
        "admin".to_string(),
    );

    dao.insert(ctx, &agent_po).unwrap();
    let found = find_agent(&conn, &agent_po.id).unwrap();

    assert_eq!(found.name, "TestAgent");
    assert_eq!(found.created_by, "admin");
}

#[test]
fn test_find_all() {
    let conn = storage::test_conn();
    conn.execute_batch(sql::SQLITE_CREATE_TABLE_AGENTS)
        .unwrap();
    sqlite::init();
    let dao = sqlite::dao();
    let ctx = new_ctx("admin");

    for i in 0..3 {
        let agent_po = AgentPo::new(
            format!("Agent{}", i),
            Some("worker".to_string()),
            "".to_string(),
            vec![],
            "".to_string(),
            format!("provider-{}", i),
            "admin".to_string(),
        );
        insert_agent(&conn, &agent_po, "admin");
    }

    let result = dao.find_all(ctx).unwrap();
    assert_eq!(result.len(), 3);
}

#[test]
fn test_update() {
    let conn = storage::test_conn();
    conn.execute_batch(sql::SQLITE_CREATE_TABLE_AGENTS)
        .unwrap();
    sqlite::init();
    let dao = sqlite::dao();
    let agent_po = AgentPo::new(
        "Original".to_string(),
        Some("worker".to_string()),
        "".to_string(),
        vec![],
        "".to_string(),
        "provider-id-1".to_string(),
        "admin".to_string(),
    );
    insert_agent(&conn, &agent_po, "admin");

    let mut updated = agent_po.clone();
    updated.name = "Updated".to_string();
    dao.update(new_ctx("editor"), &updated).unwrap();

    let found = find_agent(&conn, &updated.id).unwrap();
    assert_eq!(found.name, "Updated");
    assert_eq!(found.modified_by, "editor");
}

#[test]
fn test_soft_delete() {
    let conn = storage::test_conn();
    conn.execute_batch(sql::SQLITE_CREATE_TABLE_AGENTS)
        .unwrap();
    sqlite::init();
    let dao = sqlite::dao();
    let agent_po = AgentPo::new(
        "ToDelete".to_string(),
        Some("worker".to_string()),
        "".to_string(),
        vec![],
        "".to_string(),
        "provider-id-1".to_string(),
        "admin".to_string(),
    );
    insert_agent(&conn, &agent_po, "admin");

    dao.delete(new_ctx("admin"), &agent_po).unwrap();
    assert!(find_agent(&conn, &agent_po.id).is_none());
}

#[test]
fn test_find_all_excludes_deleted() {
    let conn = storage::test_conn();
    conn.execute_batch(sql::SQLITE_CREATE_TABLE_AGENTS)
        .unwrap();
    sqlite::init();
    let dao = sqlite::dao();

    let agent1 = AgentPo::new(
        "Normal".to_string(),
        Some("w".to_string()),
        "".to_string(),
        vec![],
        "".to_string(),
        "provider-id-1".to_string(),
        "admin".to_string(),
    );
    let agent2 = AgentPo::new(
        "Deleted".to_string(),
        Some("w".to_string()),
        "".to_string(),
        vec![],
        "".to_string(),
        "provider-id-2".to_string(),
        "admin".to_string(),
    );

    insert_agent(&conn, &agent1, "admin");
    insert_agent(&conn, &agent2, "admin");
    dao.delete(new_ctx("admin"), &agent2).unwrap();

    let result = dao.find_all(new_ctx("admin")).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].name, "Normal");
}

#[test]
fn test_delete_twice_is_idempotent() {
    let conn = storage::test_conn();
    conn.execute_batch(sql::SQLITE_CREATE_TABLE_AGENTS)
        .unwrap();
    sqlite::init();
    let dao = sqlite::dao();

    let agent = AgentPo::new(
        "Test".to_string(),
        Some("w".to_string()),
        "".to_string(),
        vec![],
        "".to_string(),
        "provider-id-1".to_string(),
        "admin".to_string(),
    );
    insert_agent(&conn, &agent, "admin");
    let now = current_timestamp();
    conn.execute(
        "UPDATE agents SET status = 0, modified_by = ?1, updated_at = ?2 WHERE id = ?3 AND status != 0",
        rusqlite::params!["admin", now, agent.id],
    )
    .unwrap();
    conn.execute(
        "UPDATE agents SET status = 0, modified_by = ?1, updated_at = ?2 WHERE id = ?3 AND status != 0",
        rusqlite::params!["admin", now, agent.id],
    )
    .unwrap();
    assert!(find_agent(&conn, &agent.id).is_none());
}

#[test]
fn test_find_not_exists() {
    let conn = storage::test_conn();
    conn.execute_batch(sql::SQLITE_CREATE_TABLE_AGENTS)
        .unwrap();

    assert!(find_agent(&conn, "not-exists").is_none());
}

fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
