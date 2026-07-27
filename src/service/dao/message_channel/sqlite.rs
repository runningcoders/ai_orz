//! MessageChannel DAO SQLite 实现

use crate::models::message_channel::MessageChannelPo;
use crate::pkg::RequestContext;
use crate::service::dao::message_channel::{MessageChannelDao, MessageChannelQuery};
use async_trait::async_trait;
use chrono::Utc;
use common::api::PagedResult;
use common::enums::ChannelStatus;
use common::error::Result;
use sqlx::QueryBuilder;
use std::sync::{Arc, OnceLock};

/// MessageChannel DAO SQLite 实现
#[derive(Debug, Clone, Default)]
pub struct MessageChannelDaoSqliteImpl;

#[async_trait]
impl MessageChannelDao for MessageChannelDaoSqliteImpl {
    async fn insert(&self, ctx: RequestContext, po: &MessageChannelPo) -> Result<()> {
        let channel_type_i32 = po.channel_type as i32;
        let status_i32 = po.status as i32;

        sqlx::query!(
            r#"
            INSERT INTO message_channels (
                id, org_id, user_id, agent_id, channel_type, channel_name,
                webhook_url, access_token, secret, config_json, status,
                last_pushed_at, last_error, created_by, modified_by, created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            po.id,
            po.org_id,
            po.user_id,
            po.agent_id,
            channel_type_i32,
            po.channel_name,
            po.webhook_url,
            po.access_token,
            po.secret,
            po.config_json as _,
            status_i32,
            po.last_pushed_at,
            po.last_error,
            po.created_by,
            po.modified_by,
            po.created_at,
            po.updated_at,
        )
        .execute(ctx.db_pool())
        .await?;

        Ok(())
    }

    async fn update(&self, ctx: RequestContext, po: &MessageChannelPo) -> Result<()> {
        let current_timestamp = Utc::now().timestamp();
        let channel_type_i32 = po.channel_type as i32;
        let status_i32 = po.status as i32;

        sqlx::query!(
            r#"
            UPDATE message_channels
            SET org_id = ?, user_id = ?, agent_id = ?, channel_type = ?, channel_name = ?,
                webhook_url = ?, access_token = ?, secret = ?, config_json = ?, status = ?,
                last_pushed_at = ?, last_error = ?, modified_by = ?, updated_at = ?
            WHERE id = ?
            "#,
            po.org_id,
            po.user_id,
            po.agent_id,
            channel_type_i32,
            po.channel_name,
            po.webhook_url,
            po.access_token,
            po.secret,
            po.config_json as _,
            status_i32,
            po.last_pushed_at,
            po.last_error,
            po.modified_by,
            current_timestamp,
            po.id,
        )
        .execute(ctx.db_pool())
        .await?;

        Ok(())
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: MessageChannelQuery,
    ) -> Result<PagedResult<MessageChannelPo>> {
        let pool = ctx.db_pool();

        let mut count_builder =
            QueryBuilder::new("SELECT COUNT(*) FROM message_channels WHERE 1=1");
        push_query_filters(&mut count_builder, &query);
        let total: i64 = count_builder.build_query_scalar().fetch_one(pool).await?;

        let mut list_builder = QueryBuilder::new("SELECT * FROM message_channels WHERE 1=1");
        push_query_filters(&mut list_builder, &query);

        // 排序
        if let Some(order_by) = &query.order_by {
            list_builder.push(" ORDER BY ").push(order_by.clone());
        } else {
            list_builder.push(" ORDER BY created_at DESC");
        }

        // 分页
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

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<MessageChannelPo>> {
        let page = self
            .query(
                ctx,
                MessageChannelQuery {
                    id: Some(id.to_string()),
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items.into_iter().next())
    }

    async fn list_by_user_id(
        &self,
        ctx: RequestContext,
        user_id: &str,
        only_enabled: bool,
    ) -> Result<Vec<MessageChannelPo>> {
        let page = self
            .query(
                ctx,
                MessageChannelQuery {
                    user_id: Some(user_id.to_string()),
                    only_enabled,
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    async fn list_by_user_and_agent_id(
        &self,
        ctx: RequestContext,
        user_id: &str,
        agent_id: &str,
        only_enabled: bool,
    ) -> Result<Vec<MessageChannelPo>> {
        // 查询用户+Agent 的所有渠道（Agent 专属 + 用户通用）
        let agent_page = self
            .query(
                ctx.clone(),
                MessageChannelQuery {
                    user_id: Some(user_id.to_string()),
                    agent_id: Some(agent_id.to_string()),
                    only_enabled,
                    ..Default::default()
                },
            )
            .await?;

        // 查询所有用户渠道
        let all_user_channels = self
            .list_by_user_id(ctx.clone(), user_id, only_enabled)
            .await?;

        // 过滤出用户通用渠道（未绑定 Agent 的）
        let user_channels: Vec<_> = all_user_channels
            .into_iter()
            .filter(|c| c.agent_id.is_none())
            .collect();

        let mut agent_channels = agent_page.items;
        agent_channels.extend(user_channels);

        Ok(agent_channels)
    }

    async fn set_status(&self, ctx: RequestContext, id: &str, status: ChannelStatus) -> Result<()> {
        let current_timestamp = Utc::now().timestamp();
        let status_i32 = status as i32;

        sqlx::query!(
            r#"
            UPDATE message_channels
            SET status = ?, updated_at = ?
            WHERE id = ?
            "#,
            status_i32,
            current_timestamp,
            id,
        )
        .execute(ctx.db_pool())
        .await?;

        Ok(())
    }

    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.set_status(ctx, id, ChannelStatus::Deleted).await
    }

    async fn mark_push_success(&self, ctx: RequestContext, id: &str) -> Result<()> {
        let current_timestamp = Utc::now().timestamp();

        sqlx::query!(
            r#"
            UPDATE message_channels
            SET last_pushed_at = ?, last_error = ?, updated_at = ?
            WHERE id = ?
            "#,
            current_timestamp,
            None::<String>,
            current_timestamp,
            id,
        )
        .execute(ctx.db_pool())
        .await?;

        Ok(())
    }

    async fn mark_push_failed(&self, ctx: RequestContext, id: &str, error: &str) -> Result<()> {
        let current_timestamp = Utc::now().timestamp();
        let error_str = error.to_string();
        let error_opt = Some(error_str);

        sqlx::query!(
            r#"
            UPDATE message_channels
            SET last_pushed_at = ?, last_error = ?, updated_at = ?
            WHERE id = ?
            "#,
            current_timestamp,
            error_opt,
            current_timestamp,
            id,
        )
        .execute(ctx.db_pool())
        .await?;

        Ok(())
    }
}

// ==================== 工厂方法 + 单例 ====================

/// Global MessageChannel DAO instance
static MESSAGE_CHANNEL_DAO: OnceLock<Arc<dyn MessageChannelDao + Send + Sync>> = OnceLock::new();

/// 创建一个全新的 MessageChannel DAO 实例（用于测试）
pub fn new() -> Arc<dyn MessageChannelDao + Send + Sync> {
    Arc::new(MessageChannelDaoSqliteImpl)
}

/// 获取全局 MessageChannel DAO 单例
pub fn dao() -> Arc<dyn MessageChannelDao + Send + Sync> {
    MESSAGE_CHANNEL_DAO.get().cloned().unwrap()
}

/// 初始化全局 MessageChannel DAO
pub fn init() {
    MESSAGE_CHANNEL_DAO.set(new()).ok();
}

/// 推送查询过滤条件到 QueryBuilder（COUNT 和 LIST 查询复用）
fn push_query_filters<'args>(
    builder: &mut QueryBuilder<'args, sqlx::Sqlite>,
    query: &MessageChannelQuery,
) {
    if let Some(id) = &query.id {
        builder.push(" AND id = ").push_bind(id.clone());
    }
    if let Some(org_id) = &query.org_id {
        builder.push(" AND org_id = ").push_bind(org_id.clone());
    }
    if let Some(user_id) = &query.user_id {
        builder.push(" AND user_id = ").push_bind(user_id.clone());
    }
    if let Some(agent_id) = &query.agent_id {
        builder.push(" AND agent_id = ").push_bind(agent_id.clone());
    }
    if let Some(channel_type) = query.channel_type {
        builder
            .push(" AND channel_type = ")
            .push_bind(channel_type as i32);
    }
    if query.only_enabled {
        builder.push(" AND status = 1");
    }
    if let Some(status_in) = &query.status_in
        && !status_in.is_empty() {
            builder.push(" AND status IN (");
            let mut separated = builder.separated(", ");
            for s in status_in {
                separated.push_bind(*s as i32);
            }
            drop(separated);
            builder.push(")");
        }
}
