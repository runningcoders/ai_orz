//! Message DAO SQLite 实现

use crate::error::AppError;
use crate::models::message::MessagePo;
use crate::pkg::storage;
use common::enums::MessageStatus;
use common::constants::RequestContext;
use crate::service::dao::message::MessageDaoTrait;
use std::sync::{Arc, OnceLock};

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

impl MessageDaoTrait for MessageDaoImpl {
    fn insert(&self, _ctx: RequestContext, message: &MessagePo) -> Result<(), AppError> {
        let conn = storage::get().conn();

        conn.execute(
            "INSERT INTO messages (id, task_id, from_id, to_id, role, message_type, status, content, meta_json, created_by, created_at, updated_at) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                message.id,
                message.task_id,
                message.from_id,
                message.to_id,
                message.role,
                message.message_type,
                message.status,
                message.content,
                message.meta_json,
                message.created_by,
                message.created_at,
                message.updated_at,
            ],
        )
            .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    fn find_by_id(&self, _ctx: RequestContext, id: &str) -> Result<Option<MessagePo>, AppError> {
        let conn = storage::get().conn();

        let mut stmt = conn
            .prepare(
                "SELECT id, task_id, from_id, to_id, role, message_type, status, content, meta_json, created_by, created_at, updated_at 
                 FROM messages WHERE id = ?1",
            )
            .map_err(|e| AppError::Internal(e.to_string()))?;

        match stmt.query_row([id], |row| {
            Ok(MessagePo {
                id: row.get(0)?,
                task_id: row.get(1)?,
                from_id: row.get(2)?,
                to_id: row.get(3)?,
                role: row.get(4)?,
                message_type: row.get(5)?,
                status: row.get(6)?,
                content: row.get(7)?,
                meta_json: row.get(8)?,
                created_by: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        }) {
            Ok(msg) => Ok(Some(msg)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Internal(e.to_string())),
        }
    }

    fn list_by_task_id(&self, _ctx: RequestContext, task_id: &str, limit: Option<usize>) -> Result<Vec<MessagePo>, AppError> {
        let conn = storage::get().conn();

        let mut sql = "SELECT id, task_id, from_id, to_id, role, message_type, status, content, meta_json, created_by, created_at, updated_at 
                     FROM messages WHERE task_id = ?1 ORDER BY created_at ASC".to_string();
        if let Some(limit_val) = limit {
            sql.push_str(&format!(" LIMIT {}", limit_val));
        }

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let messages = stmt
            .query_map([task_id], |row| {
                Ok(MessagePo {
                    id: row.get(0)?,
                    task_id: row.get(1)?,
                    from_id: row.get(2)?,
                    to_id: row.get(3)?,
                    role: row.get(4)?,
                    message_type: row.get(5)?,
                    status: row.get(6)?,
                    content: row.get(7)?,
                    meta_json: row.get(8)?,
                    created_by: row.get(9)?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                })
            })
            .map_err(|e| AppError::Internal(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(messages)
    }

    fn list_by_from_id(&self, _ctx: RequestContext, from_id: &str, limit: Option<usize>) -> Result<Vec<MessagePo>, AppError> {
        let conn = storage::get().conn();

        let mut sql = "SELECT id, task_id, from_id, to_id, role, message_type, status, content, meta_json, created_by, created_at, updated_at 
                     FROM messages WHERE from_id = ?1 ORDER BY created_at DESC".to_string();
        if let Some(limit_val) = limit {
            sql.push_str(&format!(" LIMIT {}", limit_val));
        }

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let messages = stmt
            .query_map([from_id], |row| {
                Ok(MessagePo {
                    id: row.get(0)?,
                    task_id: row.get(1)?,
                    from_id: row.get(2)?,
                    to_id: row.get(3)?,
                    role: row.get(4)?,
                    message_type: row.get(5)?,
                    status: row.get(6)?,
                    content: row.get(6 + 1)?,
                    meta_json: row.get(6 + 2)?,
                    created_by: row.get(6 + 3)?,
                    created_at: row.get(6 + 4)?,
                    updated_at: row.get(6 + 5)?,
                })
            })
            .map_err(|e| AppError::Internal(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(messages)
    }

    fn list_by_to_id(&self, _ctx: RequestContext, to_id: &str, limit: Option<usize>) -> Result<Vec<MessagePo>, AppError> {
        let conn = storage::get().conn();

        let mut sql = "SELECT id, task_id, from_id, to_id, role, message_type, status, content, meta_json, created_by, created_at, updated_at 
                     FROM messages WHERE to_id = ?1 ORDER BY created_at DESC".to_string();
        if let Some(limit_val) = limit {
            sql.push_str(&format!(" LIMIT {}", limit_val));
        }

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let messages = stmt
            .query_map([to_id], |row| {
                Ok(MessagePo {
                    id: row.get(0)?,
                    task_id: row.get(1)?,
                    from_id: row.get(2)?,
                    to_id: row.get(3)?,
                    role: row.get(4)?,
                    message_type: row.get(5)?,
                    status: row.get(6)?,
                    content: row.get(7)?,
                    meta_json: row.get(8)?,
                    created_by: row.get(9)?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                })
            })
            .map_err(|e| AppError::Internal(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(messages)
    }

    fn delete(&self, _ctx: RequestContext, id: &str) -> Result<(), AppError> {
        let conn = storage::get().conn();

        conn.execute(
            "DELETE FROM messages WHERE id = ?1",
            rusqlite::params![id],
        )
            .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    fn count_by_task_id(&self, _ctx: RequestContext, task_id: &str) -> Result<u64, AppError> {
        let conn = storage::get().conn();

        let mut stmt = conn
            .prepare("SELECT COUNT(*) FROM messages WHERE task_id = ?1")
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let count: i64 = stmt
            .query_row([task_id], |row| row.get(0))
            .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(count as u64)
    }

    fn update_status(&self, _ctx: RequestContext, id: &str, status: MessageStatus) -> Result<(), AppError> {
        let conn = storage::get().conn();
        let now = common::constants::utils::current_timestamp();

        conn.execute(
            "UPDATE messages SET status = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![status, now, id],
        )
            .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    fn list_by_status(&self, _ctx: RequestContext, status: Vec<MessageStatus>, limit: Option<usize>) -> Result<Vec<MessagePo>, AppError> {
        let conn = storage::get().conn();

        let status_in: Vec<i32> = status.iter().map(|s: &MessageStatus| s.to_i32()).collect();
        let placeholders: Vec<String> = status_in.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect();
        let placeholders_str = placeholders.join(", ");

        let mut sql = format!(
            "SELECT id, task_id, from_id, to_id, role, message_type, status, content, meta_json, created_by, created_at, updated_at 
             FROM messages WHERE status IN ({}) ORDER BY created_at ASC",
            placeholders_str
        );

        if let Some(limit_val) = limit {
            sql.push_str(&format!(" LIMIT {}", limit_val));
        }

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| AppError::Internal(e.to_string()))?;

        // 构建参数
        let params: Vec<&dyn rusqlite::ToSql> = status_in.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

        let messages = stmt
            .query_map(params.as_slice(), |row| {
                Ok(MessagePo {
                    id: row.get(0)?,
                    task_id: row.get(1)?,
                    from_id: row.get(2)?,
                    to_id: row.get(3)?,
                    role: row.get(4)?,
                    message_type: row.get(5)?,
                    status: row.get(6)?,
                    content: row.get(7)?,
                    meta_json: row.get(8)?,
                    created_by: row.get(9)?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                })
            })
            .map_err(|e| AppError::Internal(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(messages)
    }
}
