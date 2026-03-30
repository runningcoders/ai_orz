use crate::error::AppError;
use crate::models::agent::AgentPo;
use rusqlite::Connection;

/// Agent DAO 接口
pub trait AgentDaoTrait: Send + Sync {
    fn insert(&self, conn: &Connection, agent: &AgentPo) -> Result<(), AppError>;
    fn find_by_id(&self, conn: &Connection, id: &str) -> Result<Option<AgentPo>, AppError>;
    fn find_all(&self, conn: &Connection) -> Result<Vec<AgentPo>, AppError>;
    fn update(&self, conn: &Connection, agent: &AgentPo) -> Result<(), AppError>;
    fn delete(&self, conn: &Connection, id: &str) -> Result<(), AppError>;
}

/// Agent DAO 实现
pub struct AgentDao;

impl AgentDao {
    pub fn new() -> Self {
        Self
    }
}

impl AgentDaoTrait for AgentDao {
    fn insert(&self, conn: &Connection, agent: &AgentPo) -> Result<(), AppError> {
        conn.execute(
            "INSERT INTO agents (id, name, role, capabilities, status, created_at, updated_at) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                agent.id,
                agent.name,
                agent.role,
                agent.capabilities,
                agent.status,
                agent.created_at,
                agent.updated_at,
            ],
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    fn find_by_id(&self, conn: &Connection, id: &str) -> Result<Option<AgentPo>, AppError> {
        let mut stmt = conn
            .prepare("SELECT id, name, role, capabilities, status, created_at, updated_at FROM agents WHERE id = ?1")
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let result = stmt.query_row([id], |row| {
            Ok(AgentPo {
                id: row.get(0)?,
                name: row.get(1)?,
                role: row.get(2)?,
                capabilities: row.get(3)?,
                status: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        });

        match result {
            Ok(agent) => Ok(Some(agent)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Internal(e.to_string())),
        }
    }

    fn find_all(&self, conn: &Connection) -> Result<Vec<AgentPo>, AppError> {
        let mut stmt = conn
            .prepare("SELECT id, name, role, capabilities, status, created_at, updated_at FROM agents ORDER BY created_at DESC")
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let agents = stmt
            .query_map([], |row| {
                Ok(AgentPo {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    role: row.get(2)?,
                    capabilities: row.get(3)?,
                    status: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })
            .map_err(|e| AppError::Internal(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(agents)
    }

    fn update(&self, conn: &Connection, agent: &AgentPo) -> Result<(), AppError> {
        conn.execute(
            "UPDATE agents SET name = ?1, role = ?2, capabilities = ?3, status = ?4, updated_at = ?5 WHERE id = ?6",
            rusqlite::params![
                agent.name,
                agent.role,
                agent.capabilities,
                agent.status,
                current_timestamp(),
                agent.id,
            ],
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    fn delete(&self, conn: &Connection, id: &str) -> Result<(), AppError> {
        conn.execute("DELETE FROM agents WHERE id = ?1", [id])
            .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }
}

fn current_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
