//! AgentDao SQLite 实现

use crate::error::AppError;
use crate::models::agent::AgentPo;
use common::enums::AgentStatus;
use crate::service::dao::agent::AgentDaoTrait;
use std::sync::{Arc, OnceLock};
use chrono::Utc;
use crate::pkg::RequestContext;
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

pub struct AgentDaoImpl;

impl AgentDaoImpl {
    pub fn new() -> Self {
        Self
    }
}
#[async_trait::async_trait]
impl AgentDaoTrait for AgentDaoImpl {
    async fn insert(&self, _ctx: RequestContext, agent: &AgentPo) -> Result<(), AppError> {
        let status = agent.status as i32;
        sqlx::query!(
            "INSERT INTO agents (id, name, role, description, soul, capabilities, model_provider_id, status, created_by, modified_by, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            agent.id,
            agent.name,
            agent.role,
            agent.description,
            agent.soul,
            agent.capabilities,
            agent.model_provider_id,
            status,
            agent.created_by,
            agent.modified_by,
            agent.created_at,
            agent.updated_at
        )
            .execute(_ctx.db_pool())
            .await?;

        Ok(())
    }

    async fn find_by_id(&self, _ctx: RequestContext, id: &str) -> Result<Option<AgentPo>, AppError> {
        let agent = sqlx::query_as!(
            AgentPo,
            r#"
SELECT id, name, role, description, soul, capabilities,
       model_provider_id, status as 'status: AgentStatus', created_by, modified_by, created_at, updated_at
FROM agents WHERE id = ? AND status <> 0
            "#,
            id
        )
            .fetch_optional(_ctx.db_pool())
            .await?;

        Ok(agent)
    }

    async fn find_all(&self, _ctx: RequestContext) -> Result<Vec<AgentPo>, AppError> {
        let agents = sqlx::query_as!(
            AgentPo,
            r#"
SELECT id, name, role, description, soul, capabilities,
       model_provider_id, status as 'status: AgentStatus', created_by, modified_by, created_at, updated_at
FROM agents WHERE status != 0
            "#
        )
            .fetch_all(_ctx.db_pool())
            .await?;

        Ok(agents)
    }

    async fn update(&self, _ctx: RequestContext, agent: &AgentPo) -> Result<(), AppError> {
        let current_timestamp = Utc::now().timestamp();
        let status = agent.status as i32;
        let uid = _ctx.uid();
        sqlx::query!(
            r#"
UPDATE agents
SET name = ?, role = ?, description = ?, soul = ?, capabilities = ?,
    model_provider_id = ?, status = ?, created_by = ?, modified_by = ?, created_at = ?, updated_at = ?
WHERE id = ?
            "#,
            agent.name,
            agent.role,
            agent.description,
            agent.soul,
            agent.capabilities,
            agent.model_provider_id,
            status,
            agent.created_by,
            uid,
            agent.created_at,
            current_timestamp,
            agent.id
        )
            .execute(_ctx.db_pool())
            .await?;

        Ok(())
    }

    async fn delete(&self, _ctx: RequestContext, agent: &AgentPo) -> Result<(), AppError> {
        let current_timestamp = Utc::now().timestamp();
        let uid = _ctx.uid().to_string();
        sqlx::query!(
            r#"
UPDATE agents SET status = 0, modified_by = ?, updated_at = ? WHERE id = ?
            "#,
            uid,
            current_timestamp,
            agent.id
        )
            .execute(_ctx.db_pool())
            .await?;

        Ok(())
    }
}