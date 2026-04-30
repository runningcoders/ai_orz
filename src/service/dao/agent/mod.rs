//! Agent DAO 模块

use crate::error::AppError;
use crate::models::agent::AgentPo;
use crate::pkg::RequestContext;
use common::enums::AgentStatus;

/// Agent 查询参数
#[derive(Debug, Clone, Default)]
pub struct AgentQuery {
    pub name: Option<String>,
    pub status: Option<AgentStatus>,
    pub exclude_status: Option<AgentStatus>,
    pub created_by: Option<String>,
    pub model_provider_id: Option<String>,
    pub limit: Option<usize>,
}

/// Agent DAO 接口
#[async_trait::async_trait]
pub trait AgentDao: Send + Sync {
    async fn insert(&self, ctx: RequestContext, agent: &AgentPo) -> Result<(), AppError>;
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<AgentPo>, AppError>;
    /// 通用查询
    async fn query(&self, ctx: RequestContext, query: AgentQuery) -> Result<Vec<AgentPo>, AppError>;
    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<AgentPo>, AppError>;
    async fn update(&self, ctx: RequestContext, agent: &AgentPo) -> Result<(), AppError>;
    async fn delete(&self, ctx: RequestContext, agent: &AgentPo) -> Result<(), AppError>;
}


pub mod sqlite;
pub use self::sqlite::{dao, init, new};

#[cfg(test)]
mod sqlite_test;
