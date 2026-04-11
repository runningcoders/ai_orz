//! Message DAO SQLite 实现

use crate::error::AppError;
use crate::models::message::MessagePo;
use crate::pkg::storage;
use common::enums::MessageStatus;
use crate::pkg::RequestContext;
use crate::service::dao::message::MessageDaoTrait;
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
    async fn insert(&self, _ctx: RequestContext, message: &MessagePo) -> Result<(), AppError> {
        let role = message.role as i32;
        let message_type = message.message_type as i32;
        let status = message.status as i32;
        sqlx::query!(
            "INSERT INTO messages (id, task_id, from_id, to_id, role, message_type, status, content, meta_json, created_by, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            message.id,
            message.task_id,
            message.from_id,
            message.to_id,
            role,
            message_type,
            status,
            message.content,
            message.meta_json,
            message.created_by,
            message.created_at,
            message.updated_at
        )
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn find_by_id(&self, _ctx: RequestContext, id: &str) -> Result<Option<MessagePo>, AppError> {
        let message = sqlx::query_as!(
            MessagePo,
            r#"
SELECT id, task_id, from_id, to_id, role as 'role: MessageRole', message_type as 'message_type: MessageType', status as 'status: MessageStatus', content, meta_json, created_by, created_at, updated_at
FROM messages WHERE id = ?
            "#,
            id
        )
            .fetch_optional(&self.pool)
            .await?;

        Ok(message)
    }

    async fn list_by_task_id(&self, _ctx: RequestContext, task_id: &str, limit: Option<usize>) -> Result<Vec<MessagePo>, AppError> {
        let sql = if let Some(limit) = limit {
            format!(
                r#"
SELECT id, task_id, from_id, to_id, role as 'role: MessageRole', message_type as 'message_type: MessageType', status as 'status: MessageStatus', content, meta_json, created_by, created_at, updated_at
FROM messages WHERE task_id = ? ORDER BY created_at ASC
LIMIT {}
                "#,
                limit
            )
        } else {
            r#"
SELECT id, task_id, from_id, to_id, role as 'role: MessageRole', message_type as 'message_type: MessageType', status as 'status: MessageStatus', content, meta_json, created_by, created_at, updated_at
FROM messages WHERE task_id = ? ORDER BY created_at ASC
            "#.to_string()
        };

        let messages = sqlx::query_as::<_, MessagePo>(&sql)
            .bind(task_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(messages)
    }

    async fn list_by_from_id(&self, _ctx: RequestContext, from_id: &str, limit: Option<usize>) -> Result<Vec<MessagePo>, AppError> {
        let sql = if let Some(limit) = limit {
            format!(
                r#"
SELECT id, task_id, from_id, to_id, role as 'role: MessageRole', message_type as 'message_type: MessageType', status as 'status: MessageStatus', content, meta_json, created_by, created_at, updated_at
FROM messages WHERE from_id = ? ORDER BY created_at DESC
LIMIT {}
                "#,
                limit
            )
        } else {
            r#"
SELECT id, task_id, from_id, to_id, role as 'role: MessageRole', message_type as 'message_type: MessageType', status as 'status: MessageStatus', content, meta_json, created_by, created_at, updated_at
FROM messages WHERE from_id = ? ORDER BY created_at DESC
            "#.to_string()
        };

        let messages = sqlx::query_as::<_, MessagePo>(&sql)
            .bind(from_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(messages)
    }

    async fn list_by_to_id(&self, _ctx: RequestContext, to_id: &str, limit: Option<usize>) -> Result<Vec<MessagePo>, AppError> {
        let sql = if let Some(limit) = limit {
            format!(
                r#"
SELECT id, task_id, from_id, to_id, role as 'role: MessageRole', message_type as 'message_type: MessageType', status as 'status: MessageStatus', content, meta_json, created_by, created_at, updated_at
FROM messages WHERE to_id = ? ORDER BY created_at DESC
LIMIT {}
                "#,
                limit
            )
        } else {
            r#"
SELECT id, task_id, from_id, to_id, role as 'role: MessageRole', message_type as 'message_type: MessageType', status as 'status: MessageStatus', content, meta_json, created_by, created_at, updated_at
FROM messages WHERE to_id = ? ORDER BY created_at DESC
            "#.to_string()
        };

        let messages = sqlx::query_as::<_, MessagePo>(&sql)
            .bind(to_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(messages)
    }

    async fn delete(&self, _ctx: RequestContext, id: &str) -> Result<(), AppError> {
        sqlx::query!(
            "DELETE FROM messages WHERE id = ?",
            id
        )
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn count_by_task_id(&self, _ctx: RequestContext, task_id: &str) -> Result<u64, AppError> {
        let count = sqlx::query!(
            "SELECT COUNT(*) as count FROM messages WHERE task_id = ?",
            task_id
        )
            .fetch_one(&self.pool)
            .await?;

        Ok(count.count as u64)
    }

    async fn update_status(&self, _ctx: RequestContext, id: &str, status: MessageStatus) -> Result<(), AppError> {
        let now = Utc::now().timestamp();
        let status_i32 = status as i32;
        sqlx::query!(
            r#"
UPDATE messages SET status = ?, updated_at = ? WHERE id = ?
            "#,
            status_i32,
            now,
            id
        )
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn list_by_status(&self, _ctx: RequestContext, status: Vec<MessageStatus>, limit: Option<usize>) -> Result<Vec<MessagePo>, AppError> {
        let status_in: Vec<i32> = status.iter().map(|s| *s as i32).collect();

        // sqlx 不直接支持 IN 绑定，我们手动构建 SQL
        let mut sql = "SELECT id, task_id, from_id, to_id, role as 'role: MessageRole', message_type as 'message_type: MessageType', status as 'status: MessageStatus', content, meta_json, created_by, created_at, updated_at FROM messages WHERE status IN (".to_string();
        for (i, _) in status_in.iter().enumerate() {
            if i > 0 {
                sql.push_str(", ");
            }
            sql.push_str("?");
        }
        sql.push_str(") ORDER BY created_at ASC");

        if let Some(limit) = limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        let mut query = sqlx::query_as::<_, MessagePo>(&sql);
        for s in status_in {
            query = query.bind(s);
        }

        let messages = query.fetch_all(&self.pool).await?;
        Ok(messages)
    }
}