//! Agent DAO 模块

use crate::error::AppError;
use crate::models::agent::AgentPo;
use crate::pkg::RequestContext;
use rusqlite::Connection;

/// Agent DAO 接口
pub trait AgentDaoTrait: Send + Sync {
    fn insert(
        &self,
        ctx: RequestContext,
        conn: &Connection,
        agent: &AgentPo,
    ) -> Result<(), AppError>;
    fn find_by_id(
        &self,
        ctx: RequestContext,
        conn: &Connection,
        id: &str,
    ) -> Result<Option<AgentPo>, AppError>;
    fn find_all(&self, ctx: RequestContext, conn: &Connection) -> Result<Vec<AgentPo>, AppError>;
    fn update(
        &self,
        ctx: RequestContext,
        conn: &Connection,
        agent: &AgentPo,
    ) -> Result<(), AppError>;
    fn delete(&self, ctx: RequestContext, conn: &Connection, id: &str) -> Result<(), AppError>;
}

pub mod sqlite;
pub use sqlite::dao;
pub use sqlite::init;
