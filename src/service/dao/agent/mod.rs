//! Agent DAO 模块

use crate::error::AppError;
use crate::models::agent::AgentPo;
use rusqlite::Connection;

/// Agent DAO 接口
pub trait AgentDaoTrait: Send + Sync {
    fn insert(&self, conn: &Connection, agent: &AgentPo) -> Result<(), AppError>;
    fn find_by_id(&self, conn: &Connection, id: &str) -> Result<Option<AgentPo>, AppError>;
    fn find_all(&self, conn: &Connection) -> Result<Vec<AgentPo>, AppError>;
    fn update(&self, conn: &Connection, agent: &AgentPo) -> Result<(), AppError>;
    fn delete(&self, conn: &Connection, id: &str, deleted_by: &str) -> Result<(), AppError>;
}

pub mod sqlite;
pub use sqlite::dao;
pub use sqlite::init;
