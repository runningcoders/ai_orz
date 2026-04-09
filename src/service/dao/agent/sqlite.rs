//! AgentDao SQLite 实现

use crate::error::AppError;
use crate::models::agent::AgentPo;
use crate::pkg::storage;
use common::constants::RequestContext;
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
    fn insert(&self, _ctx: RequestContext, agent: &AgentPo) -> Result<(), AppError> {
        let conn = storage::get().conn();
        let now = current_timestamp();

        conn.execute(
            "INSERT INTO agents (id, name, description, capabilities, soul, model_provider_id, status, created_by, modified_by, created_at, updated_at) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                agent.id,
                agent.name,
                agent.description,
                agent.capabilities,
                agent.soul,
                agent.model_provider_id,
                agent.status.to_i32(),
                agent.created_by,
                agent.modified_by,
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
                "SELECT id, name, description, capabilities, soul, model_provider_id, status, created_by, modified_by, created_at, updated_at 
                 FROM agents WHERE id = ?1 AND status != 0",
            )
            .map_err(|e| AppError::Internal(e.to_string()))?;

        match stmt.query_row([id], |row| {
            Ok(AgentPo {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                capabilities: row.get(3)?,
                soul: row.get(4)?,
                model_provider_id: row.get(5)?,
                status: common::constants::AgentPoStatus::from_i32(row.get(6)?),
                created_by: row.get(7)?,
                modified_by: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
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
                "SELECT id, name, description, capabilities, soul, model_provider_id, status, created_by, modified_by, created_at, updated_at 
                 FROM agents WHERE status != 0 ORDER BY id DESC",
            )
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let agents = stmt
            .query_map([], |row| {
                Ok(AgentPo {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    capabilities: row.get(3)?,
                    soul: row.get(4)?,
                    model_provider_id: row.get(5)?,
                    status: common::constants::AgentPoStatus::from_i32(row.get(6)?),
                    created_by: row.get(7)?,
                    modified_by: row.get(8)?,
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
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
            "UPDATE agents SET name = ?1, description = ?2, capabilities = ?3, soul = ?4, model_provider_id = ?5, modified_by = ?6, updated_at = ?7 WHERE id = ?8",
            rusqlite::params![
                agent.name,
                agent.description,
                agent.capabilities,
                agent.soul,
                agent.model_provider_id,
                ctx.uid(),
                current_timestamp(),
                agent.id,
            ],
        )
            .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    fn delete(&self, ctx: RequestContext, agent: &AgentPo) -> Result<(), AppError> {
        let conn = storage::get().conn();

        conn.execute(
            "UPDATE agents SET status = 0, modified_by = ?1, updated_at = ?2 WHERE id = ?3 AND status != 0",
            rusqlite::params![ctx.uid(), current_timestamp(), agent.id],
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
