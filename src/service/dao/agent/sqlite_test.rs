//! AgentDao SQLite 单元测试

use super::*;
use crate::models::agent::AgentPo;
use crate::pkg::storage;
use crate::pkg::RequestContext;

fn new_ctx(user_id: &str) -> RequestContext {
    RequestContext::new(Some(user_id.to_string()), None)
}

fn insert_agent(conn: &rusqlite::Connection, agent: &AgentPo, creator: &str) {
    let now = current_timestamp();
    conn.execute(
        "INSERT INTO agents (id, name, role, capabilities, soul, status, created_by, modified_by, created_at, updated_at) 
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            agent.id,
            agent.name,
            agent.role,
            agent.capabilities,
            agent.soul,
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
            "SELECT id, name, role, capabilities, soul, status, created_by, modified_by, created_at, updated_at 
             FROM agents WHERE id = ?1 AND status != 0",
        )
        .unwrap();

    match stmt.query_row([id], |row| {
        Ok(AgentPo {
            id: row.get(0)?,
            name: row.get(1)?,
            role: row.get(2)?,
            capabilities: row.get(3)?,
            soul: row.get(4)?,
            status: crate::pkg::constants::AgentPoStatus::from_i32(row.get(5)?),
            created_by: row.get(6)?,
            modified_by: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
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
    conn.execute_batch(crate::pkg::sql::SQLITE_CREATE_TABLE_AGENTS)
        .unwrap();
    let agent = AgentPo::new(
        "TestAgent".to_string(),
        "worker".to_string(),
        vec!["coding".to_string()],
        "A helpful agent".to_string(),
        "admin".to_string(),
    );
    insert_agent(&conn, &agent, "admin");
    let found = find_agent(&conn, &agent.id).unwrap();
    assert_eq!(found.name, "TestAgent");
    assert_eq!(found.created_by, "admin");
}

#[test]
fn test_find_all() {
    let conn = storage::test_conn();
    conn.execute_batch(crate::pkg::sql::SQLITE_CREATE_TABLE_AGENTS)
        .unwrap();
    for i in 0..3 {
        let agent = AgentPo::new(
            format!("Agent{}", i),
            "worker".to_string(),
            vec![],
            "".to_string(),
            "admin".to_string(),
        );
        insert_agent(&conn, &agent, "admin");
    }
    let mut stmt = conn
        .prepare("SELECT COUNT(*) FROM agents WHERE status != 0")
        .unwrap();
    let count: i64 = stmt.query_row([], |row| row.get(0)).unwrap();
    assert_eq!(count, 3);
}

#[test]
fn test_update() {
    let conn = storage::test_conn();
    conn.execute_batch(crate::pkg::sql::SQLITE_CREATE_TABLE_AGENTS)
        .unwrap();
    let agent = AgentPo::new(
        "Original".to_string(),
        "worker".to_string(),
        vec![],
        "".to_string(),
        "admin".to_string(),
    );
    insert_agent(&conn, &agent, "admin");
    let now = current_timestamp();
    conn.execute(
        "UPDATE agents SET name = ?1, modified_by = ?2, updated_at = ?3 WHERE id = ?4",
        rusqlite::params!["Updated", "editor", now, agent.id],
    )
    .unwrap();
    let found = find_agent(&conn, &agent.id).unwrap();
    assert_eq!(found.name, "Updated");
    assert_eq!(found.modified_by, "editor");
}

#[test]
fn test_soft_delete() {
    let conn = storage::test_conn();
    conn.execute_batch(crate::pkg::sql::SQLITE_CREATE_TABLE_AGENTS)
        .unwrap();
    let agent = AgentPo::new(
        "ToDelete".to_string(),
        "worker".to_string(),
        vec![],
        "".to_string(),
        "admin".to_string(),
    );
    insert_agent(&conn, &agent, "admin");
    let now = current_timestamp();
    conn.execute(
        "UPDATE agents SET status = 0, modified_by = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params!["admin", now, agent.id],
    )
    .unwrap();
    assert!(find_agent(&conn, &agent.id).is_none());
}

#[test]
fn test_find_all_excludes_deleted() {
    let conn = storage::test_conn();
    conn.execute_batch(crate::pkg::sql::SQLITE_CREATE_TABLE_AGENTS)
        .unwrap();
    let agent1 = AgentPo::new("Normal".to_string(), "w".to_string(), vec![], "".to_string(), "admin".to_string());
    let agent2 = AgentPo::new("Deleted".to_string(), "w".to_string(), vec![], "".to_string(), "admin".to_string());
    insert_agent(&conn, &agent1, "admin");
    insert_agent(&conn, &agent2, "admin");
    let now = current_timestamp();
    conn.execute(
        "UPDATE agents SET status = 0, modified_by = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params!["admin", now, agent2.id],
    )
    .unwrap();
    let mut stmt = conn
        .prepare("SELECT COUNT(*) FROM agents WHERE status != 0")
        .unwrap();
    let count: i64 = stmt.query_row([], |row| row.get(0)).unwrap();
    assert_eq!(count, 1);
}

#[test]
fn test_delete_twice_is_idempotent() {
    let conn = storage::test_conn();
    conn.execute_batch(crate::pkg::sql::SQLITE_CREATE_TABLE_AGENTS)
        .unwrap();
    let agent = AgentPo::new("Test".to_string(), "w".to_string(), vec![], "".to_string(), "admin".to_string());
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
    conn.execute_batch(crate::pkg::sql::SQLITE_CREATE_TABLE_AGENTS)
        .unwrap();
    assert!(find_agent(&conn, "not-exists").is_none());
}
