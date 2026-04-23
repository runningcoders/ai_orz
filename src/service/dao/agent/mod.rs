//! Agent DAO 模块

use crate::error::AppError;
use crate::models::agent::AgentPo;
use crate::pkg::RequestContext;

/// Agent DAO 接口
#[async_trait::async_trait]
pub trait AgentDaoTrait: Send + Sync {
    async fn insert(&self, ctx: RequestContext, agent: &AgentPo) -> Result<(), AppError>;
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<AgentPo>, AppError>;
    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<AgentPo>, AppError>;
    async fn update(&self, ctx: RequestContext, agent: &AgentPo) -> Result<(), AppError>;
    async fn delete(&self, ctx: RequestContext, agent: &AgentPo) -> Result<(), AppError>;
}


pub mod sqlite;
pub use self::sqlite::{dao, init, new};

#[cfg(test)]
mod sqlite_test;
