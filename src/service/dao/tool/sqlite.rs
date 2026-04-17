//! SQLite implementation of ToolDao

use crate::models::tool::ToolPo;
use crate::pkg::request_context::RequestContext;
use anyhow::Result;
use async_trait::async_trait;
use sqlx::FromRow;
use uuid::Uuid;
use std::sync::OnceLock;

use super::ToolDao;

/// Global Tool DAO instance
static TOOL_DAO: OnceLock<Box<dyn ToolDao>> = OnceLock::new();

/// Get global Tool DAO
pub fn get() -> &'static Box<dyn ToolDao> {
    TOOL_DAO.get().unwrap()
}

/// SQLite Tool DAO implementation
#[derive(Clone, Default)]
pub struct SqliteToolDao {}

impl SqliteToolDao {
    pub fn new() -> Self {
        Self {}
    }
}

/// Initialize global Tool DAO
pub fn init() {
    // Create DAO instance and set global
    let dao = SqliteToolDao::new();
    TOOL_DAO.set(Box::new(dao)).ok();
}

#[async_trait]
impl ToolDao for SqliteToolDao {
    async fn create_tool(&self, ctx: &RequestContext, po: &ToolPo) -> Result<()> {
        let pool = ctx.db_pool();

        sqlx::query(
            r#"
            INSERT INTO tools (
                id, name, description, protocol, config, parameters_schema,
                status, created_at, updated_at, created_by, updated_by
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(po.id.to_string())
        .bind(&po.name)
        .bind(&po.description)
        .bind(po.protocol.to_string())
        .bind(&po.config)
        .bind(&po.parameters_schema)
        .bind(po.status.to_string())
        .bind(po.created_at)
        .bind(po.updated_at)
        .bind(&po.created_by)
        .bind(&po.updated_by)
        .execute(pool)
        .await?;

        Ok(())
    }

    async fn update_tool(&self, ctx: &RequestContext, po: &ToolPo) -> Result<()> {
        let pool = ctx.db_pool();

        sqlx::query(
            r#"
            UPDATE tools SET
                name = ?, description = ?, protocol = ?, config = ?,
                parameters_schema = ?, status = ?, updated_at = ?, updated_by = ?
            WHERE id = ?
            "#
        )
        .bind(&po.name)
        .bind(&po.description)
        .bind(po.protocol.to_string())
        .bind(&po.config)
        .bind(&po.parameters_schema)
        .bind(po.status.to_string())
        .bind(po.updated_at)
        .bind(&po.updated_by)
        .bind(po.id.to_string())
        .execute(pool)
        .await?;

        Ok(())
    }

    async fn get_by_id(&self, ctx: &RequestContext, id: Uuid) -> Result<Option<ToolPo>> {
        let pool = ctx.db_pool();

        let row = sqlx::query_as::<_, ToolPo>(
            r#"
            SELECT * FROM tools WHERE id = ?
            "#
        )
        .bind(id.to_string())
        .fetch_optional(pool)
        .await?;

        Ok(row)
    }

    async fn get_by_name(&self, ctx: &RequestContext, name: &str) -> Result<Option<ToolPo>> {
        let pool = ctx.db_pool();

        let row = sqlx::query_as::<_, ToolPo>(
            r#"
            SELECT * FROM tools WHERE name = ?
            "#
        )
        .bind(name)
        .fetch_optional(pool)
        .await?;

        Ok(row)
    }

    async fn list_enabled(&self, ctx: &RequestContext) -> Result<Vec<ToolPo>> {
        let pool = ctx.db_pool();

        let rows = sqlx::query_as::<_, ToolPo>(
            r#"
            SELECT * FROM tools WHERE status = 'enabled' ORDER BY created_at DESC
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(rows)
    }

    async fn add_tool_to_agent(
        &self,
        ctx: &RequestContext,
        agent_id: &str,
        tool_id: Uuid,
        created_by: Option<String>,
    ) -> Result<()> {
        let pool = ctx.db_pool();

        sqlx::query(
            r#"
            INSERT OR IGNORE INTO agent_tools (agent_id, tool_id, created_by)
            VALUES (?, ?, ?)
            "#
        )
        .bind(agent_id)
        .bind(tool_id.to_string())
        .bind(created_by)
        .execute(pool)
        .await?;

        Ok(())
    }

    async fn remove_tool_from_agent(
        &self,
        ctx: &RequestContext,
        agent_id: &str,
        tool_id: Uuid,
    ) -> Result<()> {
        let pool = ctx.db_pool();

        sqlx::query(
            r#"
            DELETE FROM agent_tools WHERE agent_id = ? AND tool_id = ?
            "#
        )
        .bind(agent_id)
        .bind(tool_id.to_string())
        .execute(pool)
        .await?;

        Ok(())
    }

    async fn list_tools_for_agent(&self, ctx: &RequestContext, agent_id: &str) -> Result<Vec<ToolPo>> {
        let pool = ctx.db_pool();

        let rows = sqlx::query_as::<_, ToolPo>(
            r#"
            SELECT t.* FROM tools t
            INNER JOIN agent_tools at ON t.id = at.tool_id
            WHERE at.agent_id = ? AND t.status = 'enabled'
            ORDER BY t.created_at DESC
            "#
        )
        .bind(agent_id)
        .fetch_all(pool)
        .await?;

        Ok(rows)
    }
}
