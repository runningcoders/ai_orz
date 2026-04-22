//! SQLite implementation of ToolDao

use common::enums::message::ControlMode;
use crate::models::tool::{FullTool, ToolEntity, ToolPo, Tool, RigToolAdapter};
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_registry::get_registry;
use crate::pkg::tool_tracing::ToolCallLoggingDecorator;
use anyhow::Result;
use async_trait::async_trait;
use rig::tool::ToolDyn;
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

    async fn get_tool_full(&self, ctx: &RequestContext, id: String) -> Result<Option<FullTool>> {
        let Some(po) = self.get_by_id(ctx, id).await? else {
            return Ok(None);
        };

        // Create tool instance from registry factory given po from DB
        let registry = get_registry();
        let Some(tool_raw) = registry.create_tool(po.clone()) else {
            return Ok(None);
        };

        // Based on control_mode, wrap with the appropriate logger decorator
        let tool = match po.control_mode {
            ControlMode::Auto => {
                // Auto mode: our core tool exists + wrapped with logging decorator
                let decorated_our = ToolCallLoggingDecorator::new(tool_raw);
                let decorated_our_box: Box<dyn Tool + Send + Sync> =
                    Box::new(decorated_our);
                
                // Also wrap as Rig tool for Rig to use - RigToolAdapter holds ctx
                let rig_adapter = RigToolAdapter::new(ctx.clone(), decorated_our_box.clone());
                let rig_adapter_box: Box<dyn ToolDyn + Send + Sync> = Box::new(rig_adapter);
                
                ToolEntity {
                    po,
                    control_mode: ControlMode::Auto,
                    rig_tool: Some(rig_adapter_box),
                    our_tool: decorated_our_box,
                }
            }
            ControlMode::Manual => {
                // Manual mode: only our core tool wrapped with logging decorator
                let decorated = ToolCallLoggingDecorator::new(tool_raw);
                let decorated_box: Box<dyn Tool + Send + Sync> = Box::new(decorated);
                ToolEntity {
                    po,
                    control_mode: ControlMode::Manual,
                    rig_tool: None,
                    our_tool: decorated_box,
                }
            }
        };

        Ok(Some(tool))
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

    async fn list_tools_for_agent_full(&self, ctx: &RequestContext, agent_id: &str) -> Result<Vec<FullTool>> {
        let pos = self.list_tools_for_agent(ctx, agent_id).await?;

        let mut tools = Vec::new();
        let registry = get_registry();
        for po in pos {
            if let Some(tool_raw) = registry.create_tool(po.clone()) {
                // Based on control_mode, wrap with the appropriate logger decorator
                let tool = match po.control_mode {
                    ControlMode::Auto => {
                        // Auto mode: our core tool exists + wrapped with logging decorator
                        let decorated_our = ToolCallLoggingDecorator::new(tool_raw);
                        let decorated_our_box: Box<dyn Tool + Send + Sync> =
                            Box::new(decorated_our);
                        
                        // Also wrap as Rig tool for Rig to use - RigToolAdapter holds ctx
                        let rig_adapter = RigToolAdapter::new(ctx.clone(), decorated_our_box.clone());
                        let rig_adapter_box: Box<dyn ToolDyn + Send + Sync> = Box::new(rig_adapter);
                        
                        ToolEntity {
                            po,
                            control_mode: ControlMode::Auto,
                            rig_tool: Some(rig_adapter_box),
                            our_tool: decorated_our_box,
                        }
                    }
                    ControlMode::Manual => {
                        // Manual mode: only our core tool wrapped with logging decorator
                        let decorated = ToolCallLoggingDecorator::new(tool_raw);
                        let decorated_box: Box<dyn Tool + Send + Sync> = Box::new(decorated);
                        ToolEntity {
                            po,
                            control_mode: ControlMode::Manual,
                            rig_tool: None,
                            our_tool: decorated_box,
                        }
                    }
                };
                tools.push(tool);
            }
            // Skip if not found in registry (automatic filtering)
        }

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

    async fn sync_builtin_tools_to_db(&self, ctx: &RequestContext) -> Result<usize> {
        let registry = crate::pkg::tool_registry::get_registry();
        let tool_ids = registry.list_builtin_ids();
        let mut inserted = 0;

        for tool_id in tool_ids {
            // Check if tool already exists in DB
            let exists: Option<ToolPo> = sqlx::query_as::<_, ToolPo>(
                r#"
                SELECT * FROM tools WHERE id = ?
                "#
            )
                .bind(&tool_id)
                .fetch_optional(ctx.db_pool())
                .await?;

            if exists.is_some() {
                // Skip if already exists - idempotent, prevents duplicate
                continue;
            }

            // Get the builtin factory from registry
            let Some(factory) = registry.get_builtin_factory(&tool_id) else {
                continue;
            };

            // Create ToolPo for DB from factory metadata
            let po = ToolPo::new_builtin(
                factory.id().to_string(),
                factory.name().to_string(),
                factory.description().to_string(),
            );

            // Insert into DB
            self.create_tool(ctx, &po).await?;
            inserted += 1;
        }

        Ok(inserted)
    }
}
