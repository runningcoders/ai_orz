//! SQLite implementation of ToolDao

use crate::models::tool::ToolPo;
use crate::pkg::request_context::RequestContext;
use anyhow::Result;
use async_trait::async_trait;
use sqlx::SqlitePool;
use std::sync::{Arc, OnceLock};

use super::{ToolDao, ToolQuery};

// ==================== 工厂方法 + 单例 ====================

/// Global Tool DAO instance
static TOOL_DAO: OnceLock<Arc<dyn ToolDao>> = OnceLock::new();

/// 创建一个全新的 Tool DAO 实例（用于测试）
pub fn new() -> Arc<dyn ToolDao> {
    Arc::new(ToolDaoSqliteImpl::new())
}

/// Get global Tool DAO (alias for get, consistent with other DAOs)
pub fn dao() -> Arc<dyn ToolDao> {
    TOOL_DAO.get().cloned().unwrap()
}

/// SQLite Tool DAO implementation
#[derive(Clone, Default)]
struct ToolDaoSqliteImpl {}

impl ToolDaoSqliteImpl {
    fn new() -> Self {
        Self {}
    }
}

/// Initialize global Tool DAO
pub fn init() {
    // Create DAO instance and set global
    TOOL_DAO.set(new()).ok();
}

#[async_trait]
impl ToolDao for ToolDaoSqliteImpl {
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

    async fn query(&self, ctx: &RequestContext, query: ToolQuery) -> Result<Vec<ToolPo>> {
        let pool = ctx.db_pool();
        let mut builder = sqlx::QueryBuilder::new(
            r#"SELECT t.* FROM tools t"#
        );

        // Agent 过滤（需要 JOIN）
        let has_agent_filter = query.agent_id.is_some();
        if has_agent_filter {
            builder.push(" INNER JOIN agent_tools at ON t.id = at.tool_id");
        }

        let mut has_where = false;

        // Agent 过滤
        if let Some(agent_id) = &query.agent_id {
            builder.push(" WHERE at.agent_id = ").push_bind(agent_id);
            has_where = true;
        }

        // Enabled 过滤
        if let Some(enabled_only) = query.enabled_only {
            if enabled_only {
                if has_where {
                    builder.push(" AND");
                } else {
                    builder.push(" WHERE");
                }
                builder.push(" t.status = 1");
            }
        }

        // 排序
        if has_agent_filter {
            builder.push(" ORDER BY at.created_at ASC");
        } else {
            builder.push(" ORDER BY t.created_at DESC");
        }

        // 限制数量
        if let Some(limit) = query.limit {
            builder.push(" LIMIT ").push_bind(limit as i64);
        }

        let rows = builder.build_query_as()
            .fetch_all(pool)
            .await?;

        Ok(rows)
    }

    async fn list_enabled(&self, ctx: &RequestContext) -> Result<Vec<ToolPo>> {
        // 语法糖：调用通用查询
        self.query(ctx, ToolQuery {
            enabled_only: Some(true),
            ..Default::default()
        }).await
    }

    async fn add_tool_to_agent(
        &self,
        ctx: &RequestContext,
        agent_id: &str,
        tool_id: &str,
        created_by: Option<String>,
    ) -> Result<()> {
        let pool = ctx.db_pool();
        let now = common::constants::utils::current_timestamp();

        sqlx::query(
            r#"
            INSERT INTO agent_tools (agent_id, tool_id, created_at, created_by)
            VALUES (?, ?, ?, ?)
            ON CONFLICT (agent_id, tool_id) DO NOTHING
            "#
        )
        .bind(agent_id)
        .bind(tool_id)
        .bind(now)
        .bind(&created_by)
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
        // 语法糖：调用通用查询
        self.query(ctx, ToolQuery {
            agent_id: Some(agent_id.to_string()),
            ..Default::default()
        }).await
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
