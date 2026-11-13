//! 外部会话绑定 DAO SQLite 实现

use crate::models::message_channel_conversation::MessageChannelConversationPo;
use crate::pkg::RequestContext;
use crate::service::dao::message_channel_conversation::MessageChannelConversationDao;
use async_trait::async_trait;
use common::enums::ChannelType;
use common::error::Result;
use std::sync::{Arc, OnceLock};

/// 外部会话绑定 DAO SQLite 实现
#[derive(Debug, Clone, Default)]
pub struct MessageChannelConversationDaoSqliteImpl;

#[async_trait]
impl MessageChannelConversationDao for MessageChannelConversationDaoSqliteImpl {
    async fn upsert(
        &self,
        ctx: RequestContext,
        binding: &MessageChannelConversationPo,
    ) -> Result<()> {
        // query! 按引用绑定参数，先落变量避免临时值被提前释放
        let channel_type_i32 = binding.channel_type as i32;
        sqlx::query!(
            r#"
            INSERT INTO message_channel_conversations (
                id, channel_type, external_app_id, external_tenant_key,
                external_chat_id, channel_id, created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(channel_type, external_app_id, external_tenant_key, external_chat_id)
            DO UPDATE SET
                channel_id = excluded.channel_id,
                updated_at = excluded.updated_at
            "#,
            binding.id,
            channel_type_i32,
            binding.external_app_id,
            binding.external_tenant_key,
            binding.external_chat_id,
            binding.channel_id,
            binding.created_at,
            binding.updated_at,
        )
        .execute(ctx.db_pool())
        .await?;
        Ok(())
    }

    async fn find_channel_id(
        &self,
        ctx: RequestContext,
        channel_type: ChannelType,
        app_id: &str,
        tenant_key: &str,
        chat_id: &str,
    ) -> Result<Option<String>> {
        let channel_type_i32 = channel_type as i32;
        let row = sqlx::query!(
            r#"
            SELECT channel_id
            FROM message_channel_conversations
            WHERE channel_type = ?
              AND external_app_id = ?
              AND external_tenant_key = ?
              AND external_chat_id = ?
            "#,
            channel_type_i32,
            app_id,
            tenant_key,
            chat_id,
        )
        .fetch_optional(ctx.db_pool())
        .await?;
        Ok(row.map(|row| row.channel_id))
    }

    async fn query_by_channel(
        &self,
        ctx: RequestContext,
        channel_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<MessageChannelConversationPo>> {
        // 运行时 SQL（无编译期宏）：SELECT 列与 PO 字段一一对应，走 FromRow
        let items = sqlx::query_as::<_, MessageChannelConversationPo>(
            r#"
            SELECT id, channel_type, external_app_id, external_tenant_key,
                   external_chat_id, channel_id, created_at, updated_at
            FROM message_channel_conversations
            WHERE channel_id = ?
            ORDER BY created_at DESC, id DESC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(channel_id)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(ctx.db_pool())
        .await?;
        Ok(items)
    }

    async fn count_by_channel(&self, ctx: RequestContext, channel_id: &str) -> Result<usize> {
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM message_channel_conversations WHERE channel_id = ?",
        )
        .bind(channel_id)
        .fetch_one(ctx.db_pool())
        .await?;
        Ok(total as usize)
    }
}

// ==================== 工厂方法 + 单例 ====================

static MESSAGE_CHANNEL_CONVERSATION_DAO: OnceLock<
    Arc<dyn MessageChannelConversationDao + Send + Sync>,
> = OnceLock::new();

/// 创建一个全新的会话绑定 DAO 实例（用于测试）
pub fn new() -> Arc<dyn MessageChannelConversationDao + Send + Sync> {
    Arc::new(MessageChannelConversationDaoSqliteImpl)
}

/// 获取全局会话绑定 DAO 单例
pub fn dao() -> Arc<dyn MessageChannelConversationDao + Send + Sync> {
    MESSAGE_CHANNEL_CONVERSATION_DAO.get().cloned().unwrap()
}

/// 初始化全局会话绑定 DAO
pub fn init() {
    MESSAGE_CHANNEL_CONVERSATION_DAO.set(new()).ok();
}
