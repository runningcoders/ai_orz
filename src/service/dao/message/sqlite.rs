//! Message DAO SQLite 实现

use crate::error::Result;
use crate::models::message::MessagePo;
use crate::models::file::FileMeta;
use common::enums::{MessageRole, MessageType, MessageStatus, FileType};
use crate::pkg::RequestContext;
use crate::service::dao::message::MessageDaoTrait;
use sqlx::types::Json;
use std::sync::{Arc, OnceLock};
use chrono::Utc;
// ==================== 单例管理 ====================

static MESSAGE_DAO: OnceLock<Arc<dyn MessageDaoTrait>> = OnceLock::new();

/// 获取 Message DAO 单例
pub fn dao() -> Arc<dyn MessageDaoTrait> {
    MESSAGE_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = MESSAGE_DAO.set(Arc::new(MessageDaoImpl::new()));
}

// ==================== 实现 ====================

pub struct MessageDaoImpl;

impl MessageDaoImpl {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl MessageDaoTrait for MessageDaoImpl {
    async fn insert(&self, ctx: RequestContext, message: &MessagePo) -> Result<()> {
        let role = message.role as i32;
        let message_type = message.message_type as i32;
        let status = message.status as i32;
        let file_type = message.file_type.map(|ft| ft as i32);

        sqlx::query!(
            "INSERT INTO messages (id, project_id, task_id, from_id, to_id, role, message_type, file_type, status, content, file_meta, created_by, modified_by, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            message.id,
            message.project_id,
            message.task_id,
            message.from_id,
            message.to_id,
            role,
            message_type,
            file_type,
            status,
            message.content,
            message.file_meta,
            message.created_by,
            message.modified_by,
            message.created_at,
            message.updated_at
        )
            .execute(ctx.db_pool())
            .await?;

        Ok(())
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<MessagePo>> {
        let message = sqlx::query_as!(
            MessagePo,
            r#"
SELECT id, project_id, task_id, from_id, to_id, "role" as "role: MessageRole", "message_type" as "message_type: MessageType", file_type as "file_type: FileType", "status" as "status: MessageStatus", content, file_meta as "file_meta: Json<FileMeta>", created_by, modified_by, created_at, updated_at
FROM messages WHERE id = ? AND "status" != 0
            "#,
            id
        )
            .fetch_optional(ctx.db_pool())
            .await?;

        Ok(message)
    }

    async fn list_by_task_id(&self, ctx: RequestContext, task_id: &str, limit: Option<usize>) -> Result<Vec<MessagePo>> {
        let messages = if let Some(limit) = limit {
            let limit_i64 = limit as i64;
            sqlx::query_as!(
                MessagePo,
                r#"
SELECT id, project_id, task_id, from_id, to_id, "role" as "role: MessageRole", "message_type" as "message_type: MessageType", file_type as "file_type: FileType", "status" as "status: MessageStatus", content, file_meta as "file_meta: Json<FileMeta>", created_by, modified_by, created_at, updated_at
FROM messages WHERE task_id = ? AND "status" != 0 ORDER BY created_at ASC
LIMIT ?
                "#,
                task_id,
                limit_i64
            )
                .fetch_all(ctx.db_pool())
                .await?
        } else {
            sqlx::query_as!(
                MessagePo,
                r#"
SELECT id, project_id, task_id, from_id, to_id, "role" as "role: MessageRole", "message_type" as "message_type: MessageType", file_type as "file_type: FileType", "status" as "status: MessageStatus", content, file_meta as "file_meta: Json<FileMeta>", created_by, modified_by, created_at, updated_at
FROM messages WHERE task_id = ? AND "status" != 0 ORDER BY created_at ASC
                "#,
                task_id
            )
                .fetch_all(ctx.db_pool())
                .await?
        };
        Ok(messages)
    }

    async fn list_by_from_id(&self, ctx: RequestContext, from_id: &str, limit: Option<usize>) -> Result<Vec<MessagePo>> {
        let messages = if let Some(limit) = limit {
            let limit_i64 = limit as i64;
            sqlx::query_as!(
                MessagePo,
                r#"
SELECT id, project_id, task_id, from_id, to_id, "role" as "role: MessageRole", "message_type" as "message_type: MessageType", file_type as "file_type: FileType", "status" as "status: MessageStatus", content, file_meta as "file_meta: Json<FileMeta>", created_by, modified_by, created_at, updated_at
FROM messages WHERE from_id = ? AND "status" != 0 ORDER BY created_at ASC
LIMIT ?
                "#,
                from_id,
                limit_i64
            )
                .fetch_all(ctx.db_pool())
                .await?
        } else {
            sqlx::query_as!(
                MessagePo,
                r#"
SELECT id, project_id, task_id, from_id, to_id, "role" as "role: MessageRole", "message_type" as "message_type: MessageType", file_type as "file_type: FileType", "status" as "status: MessageStatus", content, file_meta as "file_meta: Json<FileMeta>", created_by, modified_by, created_at, updated_at
FROM messages WHERE from_id = ? AND "status" != 0 ORDER BY created_at ASC
                "#,
                from_id
            )
                .fetch_all(ctx.db_pool())
                .await?
        };
        Ok(messages)
    }

    async fn list_by_to_id(&self, ctx: RequestContext, to_id: &str, limit: Option<usize>) -> Result<Vec<MessagePo>> {
        let messages = if let Some(limit) = limit {
            let limit_i64 = limit as i64;
            sqlx::query_as!(
                MessagePo,
                r#"
SELECT id, project_id, task_id, from_id, to_id, "role" as "role: MessageRole", "message_type" as "message_type: MessageType", file_type as "file_type: FileType", "status" as "status: MessageStatus", content, file_meta as "file_meta: Json<FileMeta>", created_by, modified_by, created_at, updated_at
FROM messages WHERE to_id = ? AND "status" != 0 ORDER BY created_at ASC
LIMIT ?
                "#,
                to_id,
                limit_i64
            )
                .fetch_all(ctx.db_pool())
                .await?
        } else {
            sqlx::query_as!(
                MessagePo,
                r#"
SELECT id, project_id, task_id, from_id, to_id, "role" as "role: MessageRole", "message_type" as "message_type: MessageType", file_type as "file_type: FileType", "status" as "status: MessageStatus", content, file_meta as "file_meta: Json<FileMeta>", created_by, modified_by, created_at, updated_at
FROM messages WHERE to_id = ? AND "status" != 0 ORDER BY created_at ASC
                "#,
                to_id
            )
                .fetch_all(ctx.db_pool())
                .await?
        };
        Ok(messages)
    }

    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()> {
        // 软删除：更新状态为 Recalled (0)，保留数据用于审计
        let current_timestamp = Utc::now().timestamp();
        let uid = ctx.uid().to_string();
        sqlx::query!(
            r#"
UPDATE messages SET "status" = 0, updated_at = ?, modified_by = ? WHERE id = ?
            "#,
            current_timestamp,
            uid,
            id
        )
            .execute(ctx.db_pool())
            .await?;

        Ok(())
    }

    async fn count_by_task_id(&self, ctx: RequestContext, task_id: &str) -> Result<u64> {
        let count = sqlx::query!(
            r#"SELECT COUNT(*) as count FROM messages WHERE task_id = ? AND "status" != 0"#,
            task_id
        )
            .fetch_one(ctx.db_pool())
            .await?;

        Ok(count.count as u64)
    }

    async fn delete_by_task_id(&self, ctx: RequestContext, task_id: &str) -> Result<()> {
        // 软删除：批量更新任务下所有消息状态为 Recalled (0)，保留数据用于审计
        let current_timestamp = Utc::now().timestamp();
        let uid = ctx.uid().to_string();
        sqlx::query!(
            r#"
UPDATE messages SET "status" = 0, updated_at = ?, modified_by = ? WHERE task_id = ?
            "#,
            current_timestamp,
            uid,
            task_id
        )
            .execute(ctx.db_pool())
            .await?;

        Ok(())
    }

    async fn update_status(&self, ctx: RequestContext, id: &str, status: MessageStatus) -> Result<()> {
        let current_timestamp = Utc::now().timestamp();
        let uid = ctx.uid().to_string();
        let status_i32 = status as i32;
        sqlx::query!(
            r#"
UPDATE messages SET "status" = ?, updated_at = ?, modified_by = ? WHERE id = ?
            "#,
            status_i32,
            current_timestamp,
            uid,
            id
        )
            .execute(ctx.db_pool())
            .await?;

        Ok(())
    }

    async fn list_by_status(&self, ctx: RequestContext, status: Vec<MessageStatus>, limit: Option<usize>) -> Result<Vec<MessagePo>> {
        let pool = ctx.db_pool();
        let limit = limit.unwrap_or(1000);
        let limit_i64 = limit as i64;

        // Convert Vec to fixed optional bindings (max 4 statuses, enough for all common use cases)
        // Use OR to match ANY of the provided statuses: only non-null slots are checked
        let s: Vec<i32> = status.iter().map(|x| (*x) as i32).collect();
        let s1 = s.get(0).copied();
        let s2 = s.get(1).copied();
        let s3 = s.get(2).copied();
        let s4 = s.get(3).copied();

        let messages = sqlx::query_as!(
            MessagePo,
            r#"
SELECT id, project_id, task_id, from_id, to_id, "role" as "role: MessageRole", "message_type" as "message_type: MessageType", file_type as "file_type: FileType", "status" as "status: MessageStatus", content, file_meta as "file_meta: Json<FileMeta>", created_by, modified_by, created_at, updated_at
FROM messages WHERE "status" != 0 AND (
    (? IS NOT NULL AND "status" = ?) OR
    (? IS NOT NULL AND "status" = ?) OR
    (? IS NOT NULL AND "status" = ?) OR
    (? IS NOT NULL AND "status" = ?)
)
ORDER BY created_at ASC LIMIT ?
"#,
            s1, s1,
            s2, s2,
            s3, s3,
            s4, s4,
            limit_i64
        )
            .fetch_all(pool)
            .await?;

        Ok(messages)
    }
}
