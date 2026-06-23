//! SQLite implementation of McpServerDao

use crate::error::Result;
use crate::models::mcp_server::{McpServerPo, McpServerStatus};
use crate::pkg::RequestContext;
use async_trait::async_trait;
use common::api::PagedResult;
use sqlx::QueryBuilder;
use std::sync::{Arc, OnceLock};

use super::McpServerDao;
use crate::models::mcp_server::McpServerQuery;

static MCP_SERVER_DAO: OnceLock<Arc<dyn McpServerDao + Send + Sync>> = OnceLock::new();

pub fn new() -> Arc<dyn McpServerDao + Send + Sync> {
    Arc::new(McpServerDaoSqliteImpl::new())
}

pub fn dao() -> Arc<dyn McpServerDao + Send + Sync> {
    MCP_SERVER_DAO.get().cloned().unwrap()
}

pub fn init() {
    MCP_SERVER_DAO.set(new()).ok();
}

#[derive(Clone, Default)]
struct McpServerDaoSqliteImpl;

impl McpServerDaoSqliteImpl {
    fn new() -> Self {
        Self
    }
}

#[async_trait]
impl McpServerDao for McpServerDaoSqliteImpl {
    async fn insert(&self, ctx: RequestContext, server: &McpServerPo) -> Result<()> {
        let pool = ctx.db_pool();
        sqlx::query(
            r#"
            INSERT INTO mcp_servers (
                id, name, transport, config, status,
                created_at, updated_at, created_by, updated_by
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&server.id)
        .bind(&server.name)
        .bind(server.transport as i32)
        .bind(&server.config)
        .bind(server.status as i32)
        .bind(server.created_at)
        .bind(server.updated_at)
        .bind(&server.created_by)
        .bind(&server.updated_by)
        .execute(pool)
        .await?;

        Ok(())
    }

    async fn update(&self, ctx: RequestContext, server: &McpServerPo) -> Result<()> {
        let pool = ctx.db_pool();
        sqlx::query(
            r#"
            UPDATE mcp_servers SET
                name = ?, transport = ?, config = ?, status = ?,
                updated_at = ?, updated_by = ?
            WHERE id = ? AND status != ?
            "#,
        )
        .bind(&server.name)
        .bind(server.transport as i32)
        .bind(&server.config)
        .bind(server.status as i32)
        .bind(server.updated_at)
        .bind(&server.updated_by)
        .bind(&server.id)
        .bind(McpServerStatus::Deleted as i32)
        .execute(pool)
        .await?;

        Ok(())
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<McpServerPo>> {
        let page = self
            .query(
                ctx,
                McpServerQuery {
                    id: Some(id.to_string()),
                    exclude_status: Some(McpServerStatus::Deleted),
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items.into_iter().next())
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: McpServerQuery,
    ) -> Result<PagedResult<McpServerPo>> {
        let pool = ctx.db_pool();

        let mut count_builder = QueryBuilder::new("SELECT COUNT(*) FROM mcp_servers WHERE 1=1");
        push_query_filters(&mut count_builder, &query);
        let total: i64 = count_builder.build_query_scalar().fetch_one(pool).await?;

        let mut list_builder = QueryBuilder::new(
            r#"
            SELECT id, name, transport, config, status,
                   created_at, updated_at, created_by, updated_by
            FROM mcp_servers WHERE 1=1
            "#,
        );
        push_query_filters(&mut list_builder, &query);
        list_builder.push(" ORDER BY created_at DESC");

        if let Some(limit) = query.pagination.limit {
            list_builder.push(" LIMIT ").push_bind(limit as i64);
        } else if query.pagination.offset.is_some() {
            list_builder.push(" LIMIT -1");
        }

        if let Some(offset) = query.pagination.offset {
            list_builder.push(" OFFSET ").push_bind(offset as i64);
        }

        let items = list_builder.build_query_as().fetch_all(pool).await?;
        Ok(PagedResult {
            items,
            total: total as usize,
        })
    }

    async fn set_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: McpServerStatus,
    ) -> Result<()> {
        let pool = ctx.db_pool();
        let now = common::constants::utils::current_timestamp();
        let uid = ctx.uid().to_string();
        sqlx::query(
            r#"
            UPDATE mcp_servers
            SET status = ?, updated_at = ?, updated_by = ?
            WHERE id = ? AND status != ?
            "#,
        )
        .bind(status as i32)
        .bind(now)
        .bind(uid)
        .bind(id)
        .bind(McpServerStatus::Deleted as i32)
        .execute(pool)
        .await?;

        Ok(())
    }

    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.set_status(ctx, id, McpServerStatus::Deleted).await
    }
}

fn push_query_filters<'args>(
    builder: &mut QueryBuilder<'args, sqlx::Sqlite>,
    query: &McpServerQuery,
) {
    if let Some(id) = &query.id {
        builder.push(" AND id = ").push_bind(id.clone());
    }

    if let Some(name) = &query.name {
        builder.push(" AND name = ").push_bind(name.clone());
    }

    if let Some(transport) = query.transport {
        builder
            .push(" AND transport = ")
            .push_bind(transport as i32);
    }

    if let Some(status) = query.status {
        builder.push(" AND status = ").push_bind(status as i32);
    }

    if let Some(exclude_status) = query.exclude_status {
        builder
            .push(" AND status != ")
            .push_bind(exclude_status as i32);
    } else if query.status.is_none() {
        builder
            .push(" AND status != ")
            .push_bind(McpServerStatus::Deleted as i32);
    }
}
