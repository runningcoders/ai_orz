//! SQLite implementation of ToolDao

use crate::models::tool::ToolPo;
use crate::pkg::request_context::RequestContext;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use common::enums::{ToolProtocol, ToolStatus};
use common::constants::utils::current_timestamp;
use sqlx::FromRow;
use uuid::Uuid;
use serde::{Deserialize, Serialize};

use super::{ToolDao, TOOL_DAO};
use crate::service::dao::tool::providers::{
    self,
    init_global,
    GLOBAL_BUILTIN_REGISTRY,
    builtin::{self, BuiltinTool}
};

/// SQLite Tool DAO implementation
#[derive(Clone, Default)]
pub struct SqliteToolDao {}

impl SqliteToolDao {
    pub fn new() -> Self {
        Self {}
    }

    /// Register a built-in tool: sync metadata to DB and register to memory registry.
    ///
    /// The tool must implement `BuiltinTool` to provide constant TOOL_ID and DESCRIPTION.
    pub async fn register_builtin_tool<T>(&self, ctx: &RequestContext, tool: T) -> Result<()>
    where
        T: BuiltinTool + Clone + Send + Sync + 'static,
        T::Args: for<'de> Deserialize<'de>,
        T::Output: Serialize,
    {
        let id = tool.tool_id();
        let name = tool.tool_name().to_string();
        let description = tool.tool_description().to_string();

        // 1. Upsert metadata to DB
        self.upsert_builtin_tool(ctx, id, &name, &description).await?;

        // 2. Register to memory registry for runtime lookup
        let registry = GLOBAL_BUILTIN_REGISTRY.get()
            .ok_or_else(|| anyhow!("Builtin registry not initialized"))?;

        let wrapped = builtin::RigToolWrapper::new(tool);
        let erased: builtin::DynTool = Box::new(wrapped);
        let name_str = erased.name();
        registry.register_raw(&name_str, erased);

        Ok(())
    }

    /// Upsert a built-in tool into database
    async fn upsert_builtin_tool(&self, ctx: &RequestContext, id: Uuid, name: &str, description: &str) -> Result<()> {
        let pool = ctx.db_pool();
        let now = current_timestamp();

        // Check if exists
        let existing = self.get_by_id(ctx, id).await?;

        match existing {
            Some(_) => {
                // Update - keep existing config and other fields, only update name and description
                sqlx::query(
                    r#"
                    UPDATE tools SET
                        name = ?, description = ?, updated_at = ?
                    WHERE id = ?
                    "#
                )
                .bind(name)
                .bind(description)
                .bind(now)
                .bind(id.to_string())
                .execute(pool)
                .await?;
            }
            None => {
                // Insert new - builtin tools have default config
                let po = ToolPo {
                    id,
                    name: name.to_string(),
                    description: description.to_string(),
                    protocol: ToolProtocol::Builtin,
                    config: serde_json::Value::Object(Default::default()), // empty config for builtin
                    parameters_schema: Some(serde_json::Value::Object(Default::default())), // will be filled at runtime from definition
                    status: ToolStatus::Enabled,
                    created_at: now,
                    updated_at: now,
                    created_by: Some("system".to_string()),
                    updated_by: Some("system".to_string()),
                };
                self.create_tool(ctx, &po).await?;
            }
        }

        Ok(())
    }
}

/// Initialize global Tool DAO
pub fn init() {
    // Initialize global registries (cache + builtin registry)
    init_global();
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
