//! Agent DAO 接口定义

use crate::error::AppError;
use crate::models::agent::AgentPo;
use rusqlite::Connection;

/// Agent DAO 接口（dal 只感知 trait）
pub trait AgentDaoTrait: Send + Sync {
    fn insert(&self, conn: &Connection, agent: &AgentPo) -> Result<(), AppError>;
    fn find_by_id(&self, conn: &Connection, id: &str) -> Result<Option<AgentPo>, AppError>;
    fn find_all(&self, conn: &Connection) -> Result<Vec<AgentPo>, AppError>;
    fn update(&self, conn: &Connection, agent: &AgentPo) -> Result<(), AppError>;
    fn delete(&self, conn: &Connection, id: &str, deleted_by: &str) -> Result<(), AppError>;
}
