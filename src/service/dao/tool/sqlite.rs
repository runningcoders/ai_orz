//! SQLite implementation of ToolDao

use crate::models::tool::{Tool, ToolPo};
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_registry::GLOBAL_TOOL_REGISTRY;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::OnceLock;

use super::ToolDao;

/// Global Tool DAO instance
static TOOL_DAO: OnceLock<Box<dyn ToolDao>> = OnceLock::new();

/// Get global Tool DAO (alias for get, consistent with other DAOs)
pub fn dao() -> &'static Box<dyn ToolDao> {
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
        .bind(po.protocol as i32)
        .bind(&po.config)
        .bind(&po.parameters_schema)
        .bind(po.status as i32)
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
        .bind(po.protocol as i32)
        .bind(&po.config)
        .bind(&po.parameters_schema)
        .bind(po.status as i32)
        .bind(po.updated_at)
        .bind(&po.updated_by)
        .bind(po.id.to_string())
        .execute(pool)
        .await?;

        Ok(())
    }

    async fn get_by_id(&self, ctx: &RequestContext, id: String) -> Result<Option<ToolPo>> {
        let pool = ctx.db_pool();

        let row = sqlx::query_as::<_, ToolPo>(
            r#"
            SELECT * FROM tools WHERE id = ?
            "#
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;

        Ok(row)
    }

    async fn get_tool_full(&self, ctx: &RequestContext, id: String) -> Result<Option<Tool>> {
        let Some(po) = self.get_by_id(ctx, id).await? else {
            return Ok(None);
        };

        // Get built tool from global registry
        let Some(tool) = GLOBAL_TOOL_REGISTRY.get().unwrap().get(&po.id) else {
            return Ok(None);
        };

        Ok(Some(Tool { po, tool }))
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
            SELECT * FROM tools WHERE status = 1 ORDER BY created_at DESC
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(rows)
    }

    async fn list_tools_for_agent_full(&self, ctx: &RequestContext, agent_id: &str) -> Result<Vec<Tool>> {
        let pos = self.list_tools_for_agent(ctx, agent_id).await?;

        let mut tools = Vec::new();
        if let Some(registry) = GLOBAL_TOOL_REGISTRY.get() {
            for po in pos {
                if let Some(tool) = registry.get(&po.id) {
                    tools.push(Tool { po, tool });
                }
                // Skip if not found in registry (automatic filtering)
            }
        }
        // If registry not initialized, return empty list

        Ok(tools)
    }

    async fn add_tool_to_agent(
        &self,
        ctx: &RequestContext,
        agent_id: &str,
        tool_id: &str,
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
        .bind(tool_id)
        .bind(created_by)
        .execute(pool)
        .await?;

        Ok(())
    }

    async fn remove_tool_from_agent(
        &self,
        ctx: &RequestContext,
        agent_id: &str,
        tool_id: &str,
    ) -> Result<()> {
        let pool = ctx.db_pool();

        sqlx::query(
            r#"
            DELETE FROM agent_tools WHERE agent_id = ? AND tool_id = ?
            "#
        )
        .bind(agent_id)
        .bind(tool_id)
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
            WHERE at.agent_id = ? AND t.status = 1
            ORDER BY t.created_at DESC
            "#
        )
        .bind(agent_id)
        .fetch_all(pool)
        .await?;

        Ok(rows)
    }
}
