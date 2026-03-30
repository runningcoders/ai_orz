//! AgentDao SQLite 实现

use crate::error::AppError;
use crate::models::agent::AgentPo;
use crate::pkg::RequestContext;
use crate::pkg::{error as log_error, info};
use crate::service::dao::agent::AgentDaoTrait;
use rusqlite::Connection;
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
    fn insert(
        &self,
        ctx: RequestContext,
        conn: &Connection,
        agent: &AgentPo,
    ) -> Result<(), AppError> {
        conn.execute(
            "INSERT INTO agents (id, name, role, capabilities, soul, status, created_by, modified_by, created_at, updated_at) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, strftime('%s', 'now'), strftime('%s', 'now'))",
            rusqlite::params![
                agent.id,
                agent.name,
                agent.role,
                agent.capabilities,
                agent.soul,
                agent.status.to_i32(),
                ctx.uid(),
                ctx.uid(),
            ],
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    fn find_by_id(
        &self,
        _ctx: RequestContext,
        conn: &Connection,
        id: &str,
    ) -> Result<Option<AgentPo>, AppError> {
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

    fn find_all(&self, _ctx: RequestContext, conn: &Connection) -> Result<Vec<AgentPo>, AppError> {
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

    fn update(
        &self,
        ctx: RequestContext,
        conn: &Connection,
        agent: &AgentPo,
    ) -> Result<(), AppError> {
        conn.execute(
            "UPDATE agents SET name = ?1, role = ?2, capabilities = ?3, soul = ?4, modified_by = ?5, updated_at = strftime('%s', 'now') WHERE id = ?6",
            rusqlite::params![
                agent.name,
                agent.role,
                agent.capabilities,
                agent.soul,
                ctx.uid(),
                agent.id,
            ],
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    fn delete(&self, ctx: RequestContext, conn: &Connection, id: &str) -> Result<(), AppError> {
        conn.execute(
            "UPDATE agents SET status = 0, modified_by = ?1, updated_at = strftime('%s', 'now') WHERE id = ?2 AND status != 0",
            rusqlite::params![ctx.uid(), id],
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::agent::AgentPo;

    fn new_ctx(user_id: &str) -> RequestContext {
        RequestContext::new(Some(user_id.to_string()), None)
    }

    fn new_test_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
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

    #[test]
    fn test_insert_and_find_by_id() {
        init();
        let db = new_test_db();
        let dao = dao();
        let ctx = new_ctx("admin");
        let agent = AgentPo::new(
            "TestAgent".to_string(),
            "worker".to_string(),
            vec!["coding".to_string()],
            "A helpful agent".to_string(),
            "admin".to_string(),
        );
        dao.insert(ctx, &db, &agent).unwrap();
        let found = dao
            .find_by_id(new_ctx("user1"), &db, &agent.id)
            .unwrap()
            .unwrap();
        assert_eq!(found.name, "TestAgent");
    }

    #[test]
    fn test_find_all() {
        init();
        let db = new_test_db();
        let dao = dao();
        let ctx = new_ctx("admin");
        for i in 0..3 {
            let agent = AgentPo::new(
                format!("Agent{}", i),
                "worker".to_string(),
                vec![],
                "".to_string(),
                "admin".to_string(),
            );
            dao.insert(ctx.clone(), &db, &agent).unwrap();
        }
        let all = dao.find_all(ctx, &db).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_update() {
        init();
        let db = new_test_db();
        let dao = dao();
        let ctx = new_ctx("admin");
        let agent = AgentPo::new(
            "Original".to_string(),
            "worker".to_string(),
            vec![],
            "".to_string(),
            "admin".to_string(),
        );
        dao.insert(ctx.clone(), &db, &agent).unwrap();
        let mut updated = agent.clone();
        updated.name = "Updated".to_string();
        dao.update(ctx, &db, &updated).unwrap();
        let found = dao
            .find_by_id(new_ctx("user1"), &db, &agent.id)
            .unwrap()
            .unwrap();
        assert_eq!(found.name, "Updated");
    }

    #[test]
    fn test_soft_delete() {
        init();
        let db = new_test_db();
        let dao = dao();
        let ctx = new_ctx("admin");
        let agent = AgentPo::new(
            "ToDelete".to_string(),
            "worker".to_string(),
            vec![],
            "".to_string(),
            "admin".to_string(),
        );
        dao.insert(ctx.clone(), &db, &agent).unwrap();
        dao.delete(ctx, &db, &agent.id).unwrap();
        assert!(dao
            .find_by_id(new_ctx("user1"), &db, &agent.id)
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_find_all_excludes_deleted() {
        init();
        let db = new_test_db();
        let dao = dao();
        let ctx = new_ctx("admin");
        let agent1 = AgentPo::new(
            "Normal".to_string(),
            "w".to_string(),
            vec![],
            "".to_string(),
            "admin".to_string(),
        );
        let agent2 = AgentPo::new(
            "Deleted".to_string(),
            "w".to_string(),
            vec![],
            "".to_string(),
            "admin".to_string(),
        );
        dao.insert(ctx.clone(), &db, &agent1).unwrap();
        dao.insert(ctx.clone(), &db, &agent2).unwrap();
        dao.delete(ctx, &db, &agent2.id).unwrap();
        let all = dao.find_all(new_ctx("user1"), &db).unwrap();
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn test_delete_twice_is_idempotent() {
        init();
        let db = new_test_db();
        let dao = dao();
        let ctx = new_ctx("admin");
        let agent = AgentPo::new(
            "Test".to_string(),
            "w".to_string(),
            vec![],
            "".to_string(),
            "admin".to_string(),
        );
        dao.insert(ctx.clone(), &db, &agent).unwrap();
        dao.delete(ctx.clone(), &db, &agent.id).unwrap();
        dao.delete(ctx, &db, &agent.id).unwrap();
        assert!(dao
            .find_by_id(new_ctx("user1"), &db, &agent.id)
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_find_not_exists() {
        init();
        let db = new_test_db();
        let dao = dao();
        let ctx = new_ctx("user1");
        assert!(dao.find_by_id(ctx, &db, "not-exists").unwrap().is_none());
    }
}
