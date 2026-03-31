//! AgentDao SQLite 实现

use crate::error::AppError;
use crate::models::agent::AgentPo;
use crate::pkg::RequestContext;
use crate::pkg::storage;
use crate::service::dao::agent::AgentDaoTrait;
use std::sync::{Arc, OnceLock};

// ==================== 单例 ====================

static AGENT_DAO: OnceLock<Arc<dyn AgentDaoTrait>> = OnceLock::new();

/// 获取 AgentDao 单例
pub fn dao() -> Arc<dyn AgentDaoTrait> {
    AGENT_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = AGENT_DAO.set(Arc::new(AgentDaoImpl::new()));
}

// ==================== 实现 ====================

struct AgentDaoImpl;

impl AgentDaoImpl {
    fn new() -> Self {
        Self
    }
}

impl AgentDaoTrait for AgentDaoImpl {
    fn insert(&self, ctx: RequestContext, agent: &AgentPo) -> Result<(), AppError> {
        let conn = storage::get().conn();
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
                ctx.uid(),
                ctx.uid(),
                now,
                now,
            ],
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    fn find_by_id(&self, _ctx: RequestContext, id: &str) -> Result<Option<AgentPo>, AppError> {
        let conn = storage::get().conn();

        let mut stmt = conn
            .prepare(
                "SELECT id, name, role, capabilities, soul, status, created_by, modified_by, created_at, updated_at 
                 FROM agents WHERE id = ?1 AND status != 0",
            )
            .map_err(|e| AppError::Internal(e.to_string()))?;

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
            Ok(a) => Ok(Some(a)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Internal(e.to_string())),
        }
    }

    fn find_all(&self, _ctx: RequestContext) -> Result<Vec<AgentPo>, AppError> {
        let conn = storage::get().conn();

        let mut stmt = conn
            .prepare(
                "SELECT id, name, role, capabilities, soul, status, created_by, modified_by, created_at, updated_at 
                 FROM agents WHERE status != 0 ORDER BY id DESC",
            )
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let agents = stmt
            .query_map([], |row| {
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
            })
            .map_err(|e| AppError::Internal(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(agents)
    }

    fn update(&self, ctx: RequestContext, agent: &AgentPo) -> Result<(), AppError> {
        let conn = storage::get().conn();

        conn.execute(
            "UPDATE agents SET name = ?1, role = ?2, capabilities = ?3, soul = ?4, modified_by = ?5, updated_at = ?6 WHERE id = ?7",
            rusqlite::params![
                agent.name,
                agent.role,
                agent.capabilities,
                agent.soul,
                ctx.uid(),
                current_timestamp(),
                agent.id,
            ],
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    fn delete(&self, ctx: RequestContext, id: &str) -> Result<(), AppError> {
        let conn = storage::get().conn();

        conn.execute(
            "UPDATE agents SET status = 0, modified_by = ?1, updated_at = ?2 WHERE id = ?3 AND status != 0",
            rusqlite::params![ctx.uid(), current_timestamp(), id],
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }
}

fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::agent::AgentPo;

    fn new_ctx(user_id: &str) -> RequestContext {
        RequestContext::new(Some(user_id.to_string()), None)
    }

    fn setup_test_db() -> rusqlite::Connection {
        let conn = storage::test_conn();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS agents (
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
            );",
        )
        .unwrap();
        conn
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
        let conn = setup_test_db();
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
        let conn = setup_test_db();
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
        let conn = setup_test_db();
        let agent = AgentPo::new(
            "Original".to_string(),
            "worker".to_string(),
            vec![],
            "".to_string(),
            "admin".to_string(),
        );
        insert_agent(&conn, &agent, "admin");
        let mut updated = agent.clone();
        updated.name = "Updated".to_string();
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
        let conn = setup_test_db();
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
        let conn = setup_test_db();
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
        let conn = setup_test_db();
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
        let conn = setup_test_db();
        assert!(find_agent(&conn, "not-exists").is_none());
    }
}
