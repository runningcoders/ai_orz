//! SQLite implementation of ToolDao

use crate::models::tool::ToolPo;
use crate::pkg::request_context::RequestContext;
use crate::pkg::storage::escape_fts5_keyword;
use common::error::Result;
use async_trait::async_trait;
use sqlx::FromRow;
use std::sync::{Arc, OnceLock};

use super::{ToolDao, ToolQuery, ToolSearch};

// ==================== FTS5 辅助 ====================

/// 工具搜索行（PO + fts_rank）
///
/// 用于 FTS5 MATCH 查询结果，包含 ToolPo 所有字段 + BM25 rank。
#[derive(FromRow)]
struct ToolSearchRow {
    id: String,
    name: String,
    description: String,
    protocol: common::enums::ToolProtocol,
    control_mode: common::enums::ControlMode,
    config: serde_json::Value,
    parameters_schema: Option<serde_json::Value>,
    tags: String,
    status: common::enums::ToolStatus,
    created_at: i64,
    updated_at: i64,
    created_by: Option<String>,
    updated_by: Option<String>,
    fts_rank: Option<f32>,
}

impl ToolSearchRow {
    /// 将搜索行转换为 (ToolPo, fts_rank) 元组
    fn into_po_with_rank(self) -> (ToolPo, Option<f32>) {
        let po = ToolPo {
            id: self.id,
            name: self.name,
            description: self.description,
            protocol: self.protocol,
            control_mode: self.control_mode,
            config: self.config,
            parameters_schema: self.parameters_schema,
            tags: self.tags,
            status: self.status,
            created_at: self.created_at,
            updated_at: self.updated_at,
            created_by: self.created_by,
            updated_by: self.updated_by,
        };
        (po, self.fts_rank)
    }
}

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
    async fn create_tool(&self, ctx: RequestContext, po: &ToolPo) -> Result<()> {
        let pool = ctx.db_pool();

        sqlx::query(
            r#"
            INSERT INTO tools (
                id, name, description, protocol, control_mode, config, parameters_schema,
                tags, status, created_at, updated_at, created_by, updated_by
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(po.id.to_string())
        .bind(&po.name)
        .bind(&po.description)
        .bind(po.protocol as i32)
        .bind(po.control_mode as i32)
        .bind(&po.config)
        .bind(&po.parameters_schema)
        .bind(&po.tags)
        .bind(po.status as i32)
        .bind(po.created_at)
        .bind(po.updated_at)
        .bind(&po.created_by)
        .bind(&po.updated_by)
        .execute(pool)
        .await?;

        Ok(())
    }

    async fn update_tool(&self, ctx: RequestContext, po: &ToolPo) -> Result<()> {
        // Check if this is a built-in tool
        if let Some(existing) = self.get_by_id(ctx.clone(), po.id.clone()).await? {
            if matches!(existing.protocol, common::enums::ToolProtocol::Builtin) {
                return Err(anyhow::anyhow!("Built-in tools cannot be modified").into());
            }
        }

        let pool = ctx.db_pool();

        sqlx::query(
            r#"
            UPDATE tools SET
                name = ?, description = ?, protocol = ?, control_mode = ?, config = ?,
                parameters_schema = ?, tags = ?, status = ?, updated_at = ?, updated_by = ?
            WHERE id = ?
            "#,
        )
        .bind(&po.name)
        .bind(&po.description)
        .bind(po.protocol as i32)
        .bind(po.control_mode as i32)
        .bind(&po.config)
        .bind(&po.parameters_schema)
        .bind(&po.tags)
        .bind(po.status as i32)
        .bind(po.updated_at)
        .bind(&po.updated_by)
        .bind(po.id.to_string())
        .execute(pool)
        .await?;

        Ok(())
    }

    async fn delete_tool(&self, ctx: RequestContext, id: &str) -> Result<()> {
        // Check if this is a built-in tool
        if let Some(existing) = self.get_by_id(ctx.clone(), id.to_string()).await? {
            if matches!(existing.protocol, common::enums::ToolProtocol::Builtin) {
                return Err(anyhow::anyhow!("Built-in tools cannot be deleted").into());
            }
        }

        let pool = ctx.db_pool();

        sqlx::query(
            r#"
            DELETE FROM tools WHERE id = ?
            "#,
        )
        .bind(id)
        .execute(pool)
        .await?;

        Ok(())
    }

    async fn get_by_id(&self, ctx: RequestContext, id: String) -> Result<Option<ToolPo>> {
        let pool = ctx.db_pool();

        let row = sqlx::query_as::<_, ToolPo>(
            r#"
            SELECT * FROM tools WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;

        Ok(row)
    }

    async fn get_by_name(&self, ctx: RequestContext, name: &str) -> Result<Option<ToolPo>> {
        let pool = ctx.db_pool();

        let row = sqlx::query_as::<_, ToolPo>(
            r#"
            SELECT * FROM tools WHERE name = ?
            "#,
        )
        .bind(name)
        .fetch_optional(pool)
        .await?;

        Ok(row)
    }

    async fn query(&self, ctx: RequestContext, query: ToolQuery) -> Result<Vec<ToolPo>> {
        let pool = ctx.db_pool();
        let mut builder = sqlx::QueryBuilder::new(r#"SELECT t.* FROM tools t"#);

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

        // IDs 批量查询
        if let Some(ids) = &query.ids {
            if !ids.is_empty() {
                if has_where {
                    builder.push(" AND");
                } else {
                    builder.push(" WHERE");
                }
                builder.push(" t.id IN (");
                let mut separated = builder.separated(", ");
                for id in ids {
                    separated.push_bind(id.clone());
                }
                separated.push_unseparated(")");
                has_where = true;
            }
        }

        // 关键词搜索：已废弃 LIKE 分支，改为忽略并 warn
        // 关键词全文搜索请使用 search_tools 方法（FTS5 MATCH + BM25）
        if let Some(keyword) = &query.keyword {
            if !keyword.is_empty() {
                log_warn!(
                    "keyword in ToolDao::query is deprecated, use search_tools for FTS5 full-text search; keyword ignored"
                );
            }
        }

        // tag 过滤（OR 语义：包含任一 tag 即可命中）
        if let Some(tags) = &query.tags {
            if !tags.is_empty() {
                if has_where {
                    builder.push(" AND");
                } else {
                    builder.push(" WHERE");
                }
                builder.push(" EXISTS (SELECT 1 FROM json_each(t.tags) WHERE json_each.value IN (");
                let mut separated = builder.separated(", ");
                for tag in tags {
                    separated.push_bind(tag);
                }
                separated.push_unseparated("))");
                has_where = true;
            }
        }

        // Protocol 过滤
        if let Some(protocol) = query.protocol {
            if has_where {
                builder.push(" AND");
            } else {
                builder.push(" WHERE");
            }
            builder.push(" t.protocol = ").push_bind(protocol as i32);
            has_where = true;
        }

        // Status 过滤：DAO 只按显式查询条件持久化过滤；默认不注入业务状态语义
        if let Some(status) = query.status {
            if has_where {
                builder.push(" AND");
            } else {
                builder.push(" WHERE");
            }
            builder.push(" t.status = ").push_bind(status as i32);
            has_where = true;
        }

        // 排除状态过滤：由 DAL/Domain 显式传入业务过滤语义
        if let Some(exclude_status) = query.exclude_status {
            if has_where {
                builder.push(" AND");
            } else {
                builder.push(" WHERE");
            }
            builder
                .push(" t.status != ")
                .push_bind(exclude_status as i32);
            has_where = true;
        }

        // MCP Server 过滤
        if let Some(server_id) = &query.mcp_server_id {
            if has_where {
                builder.push(" AND");
            } else {
                builder.push(" WHERE");
            }
            builder
                .push(" json_extract(t.config, '$.server_id') = ")
                .push_bind(server_id.clone());
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

        // 分页
        match (query.limit, query.offset) {
            (Some(limit), Some(offset)) => {
                builder.push(" LIMIT ").push_bind(limit as i64);
                builder.push(" OFFSET ").push_bind(offset as i64);
            }
            (Some(limit), None) => {
                builder.push(" LIMIT ").push_bind(limit as i64);
            }
            (None, Some(offset)) => {
                // SQLite requires OFFSET to be paired with LIMIT.
                // LIMIT -1 means "no upper bound" while preserving offset-only pagination semantics.
                builder.push(" LIMIT -1 OFFSET ").push_bind(offset as i64);
            }
            (None, None) => {}
        }

        let rows = builder.build_query_as().fetch_all(pool).await?;

        Ok(rows)
    }

    async fn list_enabled(&self, ctx: RequestContext) -> Result<Vec<ToolPo>> {
        // 语法糖：调用通用查询
        self.query(
            ctx.clone(),
            ToolQuery {
                enabled_only: Some(true),
                ..Default::default()
            },
        )
        .await
    }

    async fn add_tool_to_agent(
        &self,
        ctx: RequestContext,
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
            "#,
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
        ctx: RequestContext,
        agent_id: &str,
        tool_id: &str,
    ) -> Result<()> {
        let pool = ctx.db_pool();

        sqlx::query(
            r#"
            DELETE FROM agent_tools WHERE agent_id = ? AND tool_id = ?
            "#,
        )
        .bind(agent_id)
        .bind(tool_id)
        .execute(pool)
        .await?;

        Ok(())
    }

    async fn list_tools_for_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<ToolPo>> {
        // 语法糖：调用通用查询
        self.query(
            ctx.clone(),
            ToolQuery {
                agent_id: Some(agent_id.to_string()),
                ..Default::default()
            },
        )
        .await
    }

    async fn sync_builtin_tools_to_db(&self, ctx: RequestContext) -> Result<usize> {
        let registry = crate::pkg::tool_registry::get_registry();
        let tool_ids = registry.list_builtin_ids();
        let mut inserted = 0;

        for tool_id in tool_ids {
            // Check if tool already exists in DB
            let exists: Option<ToolPo> = sqlx::query_as::<_, ToolPo>(
                r#"
                SELECT * FROM tools WHERE id = ?
                "#,
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

            // Create ToolPo for DB from factory
            let mut po = factory.create_po();
            // Fill default values for builtin tools
            po.fill_defaults_for_builtin();

            // Insert into DB
            self.create_tool(ctx.clone(), &po).await?;
            inserted += 1;
        }

        Ok(inserted)
    }

    async fn search_tools(
        &self,
        ctx: RequestContext,
        params: ToolSearch,
    ) -> Result<Vec<(ToolPo, Option<f32>)>> {
        let pool = ctx.db_pool();
        let keyword = params.keyword.unwrap_or_default();
        let limit_i64 = params.limit as i64;

        // 空关键词直接返回空结果（FTS5 MATCH 空字符串会报错）
        if keyword.trim().is_empty() {
            return Ok(Vec::new());
        }

        let escaped_keyword = escape_fts5_keyword(&keyword);

        // FTS5 MATCH + JOIN 主表 + BM25 排序
        // 注意：MATCH 左侧必须使用完整表名（非别名），否则 SQLite 会将别名解释为列名
        let mut builder = sqlx::QueryBuilder::new(
            r#"SELECT t.id, t.name, t.description, t.protocol, t.control_mode, t.config,
                      t.parameters_schema, t.tags, t.status, t.created_at, t.updated_at,
                      t.created_by, t.updated_by, tools_fts.rank as fts_rank
               FROM tools_fts
               JOIN tools t ON tools_fts.rowid = t.rowid
               WHERE tools_fts MATCH "#,
        );
        builder.push_bind(escaped_keyword);

        // 始终排除 Stale 状态（远端已消失的工具不应出现在搜索结果中）
        builder.push(" AND t.status != 2");

        // enabled_only 过滤：只返回 Enabled (1) 状态的工具
        if params.enabled_only {
            builder.push(" AND t.status = 1");
        }

        // Agent 过滤：通过 EXISTS 子查询检查 agent_tools 关联表
        if let Some(agent_id) = &params.agent_id {
            builder.push(
                " AND EXISTS (SELECT 1 FROM agent_tools at WHERE at.tool_id = t.id AND at.agent_id = ",
            );
            builder.push_bind(agent_id);
            builder.push(")");
        }

        // BM25 排序（rank 越小越相关）+ 分页
        builder.push(" ORDER BY tools_fts.rank LIMIT ");
        builder.push_bind(limit_i64);

        let rows: Vec<ToolSearchRow> = builder
            .build_query_as::<ToolSearchRow>()
            .fetch_all(pool)
            .await?;

        let results = rows
            .into_iter()
            .map(|row| row.into_po_with_rank())
            .collect();

        Ok(results)
    }
}
