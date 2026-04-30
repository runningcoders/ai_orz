//! Message DAO SQLite 实现

use crate::error::Result;
use crate::models::message::MessagePo;
use crate::models::file::FileMeta;
use common::enums::{MessageRole, MessageType, MessageStatus, FileType};
use crate::pkg::RequestContext;
use crate::service::dao::message::{MessageDao, MessageQuery};
use sqlx::types::Json;
use std::sync::{Arc, OnceLock};
use chrono::Utc;
// ==================== 工厂方法 + 单例管理 ====================

static MESSAGE_DAO: OnceLock<Arc<dyn MessageDao>> = OnceLock::new();

/// 创建一个全新的 Message DAO 实例（用于测试）
pub fn new() -> Arc<dyn MessageDao> {
    Arc::new(MessageDaoSqliteImpl::new())
}

/// 获取 Message DAO 单例
pub fn dao() -> Arc<dyn MessageDao> {
    MESSAGE_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = MESSAGE_DAO.set(new());
}

// ==================== 实现 ====================

struct MessageDaoSqliteImpl;

impl MessageDaoSqliteImpl {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl MessageDao for MessageDaoSqliteImpl {
    async fn insert(&self, ctx: RequestContext, message: &MessagePo) -> Result<()> {
        let from_role = message.from_role as i32;
        let to_role = message.to_role as i32;
        let message_type = message.message_type as i32;
        let status = message.status as i32;
        let file_type = message.file_type.map(|ft| ft as i32);

        sqlx::query!(
            "INSERT INTO messages (id, project_id, task_id, from_id, to_id, from_role, to_role, message_type, file_type, status, content, file_meta, reply_to_id, created_by, modified_by, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            message.id,
            message.project_id,
            message.task_id,
            message.from_id,
            message.to_id,
            from_role,
            to_role,
            message_type,
            file_type,
            status,
            message.content,
            message.file_meta,
            message.reply_to_id,
            message.created_by,
            message.modified_by,
            message.created_at,
            message.updated_at
        )
            .execute(ctx.db_pool())
            .await?;

        Ok(())
    }

    async fn query(&self, ctx: RequestContext, query: MessageQuery) -> Result<Vec<MessagePo>> {
        // 使用 sqlx::QueryBuilder 动态构建查询
        let mut builder = sqlx::QueryBuilder::new("SELECT * FROM messages WHERE 1=1");

        // 默认软删除过滤：排除 Recalled (0) 状态的消息
        // 如果用户显式指定了 status_in，则使用用户指定的状态（可能包含 0）
        if query.status_in.is_none() {
            builder.push(" AND \"status\" != 0");
        }

        // 逐个添加查询条件
        if let Some(id) = &query.id {
            builder.push(" AND id = ").push_bind(id);
        }
        if let Some(task_id) = &query.task_id {
            builder.push(" AND task_id = ").push_bind(task_id);
        }
        if let Some(project_id) = &query.project_id {
            builder.push(" AND project_id = ").push_bind(project_id);
        }
        if let Some(from_id) = &query.from_id {
            builder.push(" AND from_id = ").push_bind(from_id);
        }
        if let Some(to_id) = &query.to_id {
            builder.push(" AND to_id = ").push_bind(to_id);
        }
        if let Some(status_in) = &query.status_in {
            if !status_in.is_empty() {
                builder.push(" AND status IN (");
                let mut separated = builder.separated(", ");
                for s in status_in {
                    separated.push_bind(*s as i32);
                }
                separated.push_unseparated(")");
            }
        }

        // 添加排序
        if let Some(order_by) = &query.order_by {
            builder.push(format!(" ORDER BY {}", order_by));
        } else {
            // 默认按创建时间升序
            builder.push(" ORDER BY created_at ASC");
        }

        // 添加分页
        if let Some(limit) = query.limit {
            builder.push(format!(" LIMIT {}", limit));
            if let Some(offset) = query.offset {
                builder.push(format!(" OFFSET {}", offset));
            }
        }

        // 执行查询
        let rows = builder
            .build_query_as()
            .fetch_all(ctx.db_pool())
            .await?;

        Ok(rows)
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<MessagePo>> {
        let message = sqlx::query_as!(
            MessagePo,
            r#"
SELECT id, project_id, task_id, from_id, to_id, from_role as "from_role: MessageRole", to_role as "to_role: MessageRole", message_type as "message_type: MessageType", file_type as "file_type: FileType", "status" as "status: MessageStatus", content, file_meta as "file_meta: Json<FileMeta>", reply_to_id, created_by, modified_by, created_at, updated_at
FROM messages WHERE id = ? AND "status" != 0
            "#,
            id
        )
            .fetch_optional(ctx.db_pool())
            .await?;

        Ok(message)
    }

    async fn list_by_task_id(&self, ctx: RequestContext, task_id: &str, limit: Option<usize>) -> Result<Vec<MessagePo>> {
        // 语法糖：调用通用查询
        self.query(ctx, MessageQuery {
            task_id: Some(task_id.to_string()),
            limit,
            ..Default::default()
        }).await
    }

    async fn list_by_project_id(&self, ctx: RequestContext, project_id: &str, limit: Option<usize>) -> Result<Vec<MessagePo>> {
        // 语法糖：调用通用查询
        self.query(ctx, MessageQuery {
            project_id: Some(project_id.to_string()),
            limit,
            ..Default::default()
        }).await
    }

    async fn list_by_from_id(&self, ctx: RequestContext, from_id: &str, limit: Option<usize>) -> Result<Vec<MessagePo>> {
        // 语法糖：调用通用查询
        self.query(ctx, MessageQuery {
            from_id: Some(from_id.to_string()),
            limit,
            ..Default::default()
        }).await
    }

    async fn list_by_to_id(&self, ctx: RequestContext, to_id: &str, limit: Option<usize>) -> Result<Vec<MessagePo>> {
        // 语法糖：调用通用查询
        self.query(ctx, MessageQuery {
            to_id: Some(to_id.to_string()),
            limit,
            ..Default::default()
        }).await
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
        // 语法糖：调用通用查询
        // 注意：显式传入 status_in 会覆盖默认的 "status != 0" 过滤
        self.query(ctx, MessageQuery {
            status_in: Some(status),
            limit,
            ..Default::default()
        }).await
    }

    async fn create_tool_call_request(
        &self,
        ctx: RequestContext,
        req: crate::models::message::ToolCallMessage,
    ) -> Result<MessagePo> {
        use common::enums::{MessageRole, MessageType};
        use rand::Rng;

        /// 生成随机 ID（和项目风格一致）
        fn generate_id() -> String {
            const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
            const ID_LEN: usize = 16;
            let mut rng = rand::thread_rng();
            (0..ID_LEN)
                .map(|_| {
                    let idx = rng.gen_range(0..CHARSET.len());
                    CHARSET[idx] as char
                })
                .collect()
        }

        let message_id = generate_id();
        // 序列化整个 ToolCallMessage 为 JSON 存储在 content
        let content = serde_json::to_string(&req)?;

        // 如果有大附件，取出来用 message.file_meta 存储
        let file_meta = req.result_file_meta.unwrap_or_default();

        // 创建 MessagePo
        let message = MessagePo::new(
            message_id,
            req.project_id,
            req.task_id,
            req.from_id,
            req.to_id,
            MessageRole::Agent,   // from_role
            MessageRole::Agent,   // to_role (工具调用是 Agent → Agent)
            MessageType::ToolCallRequest,
            content,
            None, // file_type 保持 None，这是结构化消息不是文件附件
            file_meta,
            req.reply_to_id.clone(),
            ctx.uid().to_string(),
        );

        // 插入数据库
        self.insert(ctx, &message).await?;

        Ok(message)
    }

    async fn create_tool_call_result(
        &self,
        ctx: RequestContext,
        res: crate::models::message::ToolCallMessage,
    ) -> Result<MessagePo> {
        use common::enums::{MessageRole, MessageType};
        use rand::Rng;

        /// 生成随机 ID（和项目风格一致）
        fn generate_id() -> String {
            const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
            const ID_LEN: usize = 16;
            let mut rng = rand::thread_rng();
            (0..ID_LEN)
                .map(|_| {
                    let idx = rng.gen_range(0..CHARSET.len());
                    CHARSET[idx] as char
                })
                .collect()
        }

        let message_id = generate_id();
        // 序列化整个 ToolCallMessage 为 JSON 存储在 content
        let content = serde_json::to_string(&res)?;

        // 如果有大结果附件，取出来用 message.file_meta 存储
        let file_meta = res.result_file_meta.unwrap_or_default();

        // 创建 MessagePo
        let message = MessagePo::new(
            message_id,
            res.project_id,
            res.task_id,
            res.from_id,
            res.to_id,
            MessageRole::System,   // from_role (结果来自系统工具执行)
            MessageRole::Agent,    // to_role (结果返回给 Agent)
            MessageType::ToolCallResult,
            content,
            None, // file_type 保持 None，大结果通过 file_meta 附件机制存储
            file_meta,
            res.reply_to_id.clone(),
            ctx.uid().to_string(),
        );

        // 插入数据库
        self.insert(ctx, &message).await?;

        Ok(message)
    }
}
