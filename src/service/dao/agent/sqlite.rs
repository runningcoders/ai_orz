//! AgentDao SQLite 实现

use crate::error::AppError;
use crate::models::agent::AgentPo;
use common::enums::AgentStatus;
use crate::service::dao::agent::{AgentDao, AgentQuery};
use std::sync::{Arc, OnceLock};
use chrono::Utc;
use crate::pkg::RequestContext;
// ==================== 工厂方法 + 单例 ====================

static AGENT_DAO: OnceLock<Arc<dyn AgentDao>> = OnceLock::new();

/// 创建一个全新的 Agent DAO 实例（用于测试）
pub fn new() -> Arc<dyn AgentDao> {
    Arc::new(AgentDaoSqliteImpl::new())
}

/// 获取 AgentDao 单例
pub fn dao() -> Arc<dyn AgentDao> {
    AGENT_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = AGENT_DAO.set(new());
}

// ==================== 实现 ====================

struct AgentDaoSqliteImpl;

impl AgentDaoSqliteImpl {
    fn new() -> Self {
        Self
    }
}
#[async_trait::async_trait]
impl AgentDao for AgentDaoSqliteImpl {
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

    async fn query(&self, _ctx: RequestContext, query: AgentQuery) -> Result<Vec<AgentPo>, AppError> {
        let mut builder = sqlx::QueryBuilder::new(
            r#"SELECT id, name, role, description, soul, capabilities, model_provider_id, status, created_by, modified_by, created_at, updated_at FROM agents WHERE 1=1"#
        );

        // 名称模糊匹配
        if let Some(name) = &query.name {
            let like_pattern = format!("%{}%", name);
            builder.push(" AND name LIKE ").push_bind(like_pattern);
        }

        // 状态过滤
        if let Some(status) = &query.status {
            builder.push(" AND status = ").push_bind(*status as i32);
        }

        // 排除状态过滤
        if let Some(exclude_status) = &query.exclude_status {
            builder.push(" AND status != ").push_bind(*exclude_status as i32);
        }

        // 创建者过滤
        if let Some(created_by) = &query.created_by {
            builder.push(" AND created_by = ").push_bind(created_by);
        }

        // 模型提供商过滤
        if let Some(model_provider_id) = &query.model_provider_id {
            builder.push(" AND model_provider_id = ").push_bind(model_provider_id);
        }

        // 排序
        builder.push(" ORDER BY created_at DESC");

        // 限制数量
        if let Some(limit) = query.limit {
            builder.push(" LIMIT ").push_bind(limit as i64);
        }

        // 执行查询
        let rows = builder.build_query_as()
            .fetch_all(_ctx.db_pool())
            .await?;

        Ok(rows)
    }

    async fn find_all(&self, _ctx: RequestContext) -> Result<Vec<AgentPo>, AppError> {
        // 语法糖：调用通用查询，排除已删除状态
        self.query(_ctx, AgentQuery {
            exclude_status: Some(AgentStatus::Deleted),
            ..Default::default()
        }).await
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