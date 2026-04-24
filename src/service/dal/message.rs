//! Message DAL 模块
//!
//! 基础消息数据访问层，提供消息保存和查询能力
//! 所有保存的消息都会自动入队事件队列

use crate::error::AppError;
use crate::models::event::Event;
use crate::models::message::Message;
use crate::pkg::RequestContext;
use crate::service::dao::message::{MessageDao, self};
use crate::service::dao::event_queue::{EventQueueDao, self};
use common::enums::MessageStatus;
use std::sync::{Arc, OnceLock};

// ==================== 单例管理 ====================

static MESSAGE_DAL: OnceLock<Arc<dyn MessageDal>> = OnceLock::new();

/// 获取 Message DAL 单例
pub fn dal() -> Arc<dyn MessageDal> {
    MESSAGE_DAL.get().cloned().unwrap()
}

/// 初始化 Message DAL（使用全局单例 DAO）
pub fn init() {
    let _ = MESSAGE_DAL.set(new(
        message::dao(),
        event_queue::message_dao(),
    ));
}

/// 创建 Message DAL（返回 trait 对象）
pub fn new(
    message_dao: Arc<dyn MessageDao + Send + Sync>,
    event_queue_dao: Arc<dyn EventQueueDao<Message> + Send + Sync>,
) -> Arc<dyn MessageDal> {
    Arc::new(MessageDalImpl {
        message_dao,
        event_queue_dao,
    })
}

// ==================== DAL 接口 ====================

/// Message DAL 接口
#[async_trait::async_trait]
pub trait MessageDal: Send + Sync {
    /// 保存消息
    ///
    /// 保存到数据库后自动入队事件队列，所有消息都入队不做过滤
    async fn save_message(&self, ctx: RequestContext, message: &Message) -> Result<(), AppError>;

    /// 按任务 ID 查询消息列表
    ///
    /// 默认按 created_at 升序排序，保持对话顺序
    /// 支持限制返回条数，用于分页加载
    async fn list_by_task_id(
        &self,
        ctx: RequestContext,
        task_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Message>, AppError>;

    /// 按发送方 ID 查询消息列表
    async fn list_by_from_id(
        &self,
        ctx: RequestContext,
        from_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Message>, AppError>;

    /// 按接收方 ID 查询消息列表
    async fn list_by_to_id(
        &self,
        ctx: RequestContext,
        to_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Message>, AppError>;

    /// 按状态查询消息列表
    async fn list_by_status(
        &self,
        ctx: RequestContext,
        status: Vec<MessageStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<Message>, AppError>;

    /// 更新消息状态
    async fn update_status(
        &self,
        ctx: RequestContext,
        message_id: &str,
        status: MessageStatus,
    ) -> Result<(), AppError>;

    /// 统计任务消息数量
    async fn count_by_task_id(
        &self,
        ctx: RequestContext,
        task_id: &str,
    ) -> Result<u64, AppError>;

    /// 根据 ID 查询消息
    async fn find_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<Message>, AppError>;

    /// 删除消息
    async fn delete_message(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<(), AppError>;

    /// 删除任务下所有消息
    async fn delete_by_task_id(
        &self,
        ctx: RequestContext,
        task_id: &str,
    ) -> Result<(), AppError>;

    /// 从事件队列中取出下一个待处理的消息事件
    ///
    /// 返回 None 表示队列为空
    /// 获取后消息事件进入 "处理中" 状态，需要调用 ack_message 确认完成
    async fn dequeue_next_message(
        &self,
        ctx: RequestContext,
    ) -> Result<Option<Message>, AppError>;

    /// 确认消息处理完成，从事件队列中移除
    async fn ack_message(
        &self,
        ctx: RequestContext,
        message_id: &str,
    ) -> Result<(), AppError>;

    /// 标记消息处理失败，重新放回队列等待重试
    async fn nack_message(
        &self,
        ctx: RequestContext,
        message_id: &str,
    ) -> Result<(), AppError>;
}

// ==================== DAL 实现 ====================

/// Message DAL 实现
struct MessageDalImpl {
    message_dao: Arc<dyn MessageDao>,
    event_queue_dao: Arc<dyn EventQueueDao<Message>>,
}

impl MessageDalImpl {
    /// 创建 DAL 实例
    fn new(
        message_dao: Arc<dyn MessageDao>,
        event_queue_dao: Arc<dyn EventQueueDao<Message>>,
    ) -> Self {
        Self {
            message_dao,
            event_queue_dao,
        }
    }
}

#[async_trait::async_trait]
impl MessageDal for MessageDalImpl {
    async fn save_message(&self, ctx: RequestContext, message: &Message) -> Result<(), AppError> {
        // 1. 保存消息到数据库
        self.message_dao.insert(ctx.clone(), &message.po).await?;

        // 2. 消息本身就是事件，直接入队，所有消息都入队不做过滤
        // Message 已经实现了 Event trait
        let event: Box<Message> = Box::new(message.clone());
        self.event_queue_dao.enqueue(&ctx, event)?;

        Ok(())
    }

    async fn list_by_task_id(
        &self,
        ctx: RequestContext,
        task_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Message>, AppError> {
        let pos = self.message_dao.list_by_task_id(ctx, task_id, limit).await?;
        Ok(pos.into_iter().map(Message::from_po).collect())
    }

    async fn list_by_from_id(
        &self,
        ctx: RequestContext,
        from_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Message>, AppError> {
        let pos = self.message_dao.list_by_from_id(ctx, from_id, limit).await?;
        Ok(pos.into_iter().map(Message::from_po).collect())
    }

    async fn list_by_to_id(
        &self,
        ctx: RequestContext,
        to_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Message>, AppError> {
        let pos = self.message_dao.list_by_to_id(ctx, to_id, limit).await?;
        Ok(pos.into_iter().map(Message::from_po).collect())
    }

    async fn list_by_status(
        &self,
        ctx: RequestContext,
        status: Vec<MessageStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<Message>, AppError> {
        let pos = self.message_dao.list_by_status(ctx, status, limit).await?;
        Ok(pos.into_iter().map(Message::from_po).collect())
    }

    async fn update_status(
        &self,
        ctx: RequestContext,
        message_id: &str,
        status: MessageStatus,
    ) -> Result<(), AppError> {
        self.message_dao.update_status(ctx, message_id, status).await
    }

    async fn count_by_task_id(
        &self,
        ctx: RequestContext,
        task_id: &str,
    ) -> Result<u64, AppError> {
        self.message_dao.count_by_task_id(ctx, task_id).await
    }

    async fn find_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<Message>, AppError> {
        let opt = self.message_dao.find_by_id(ctx, id).await?;
        Ok(opt.map(Message::from_po))
    }

    async fn delete_message(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<(), AppError> {
        self.message_dao.delete(ctx, id).await
    }

    async fn delete_by_task_id(
        &self,
        ctx: RequestContext,
        task_id: &str,
    ) -> Result<(), AppError> {
        self.message_dao.delete_by_task_id(ctx, task_id).await
    }

    async fn dequeue_next_message(
        &self,
        ctx: RequestContext,
    ) -> Result<Option<Message>, AppError> {
        // 1. 优先从内存队列取出
        let opt_msg = self.event_queue_dao.dequeue_next(&ctx)?;
        match opt_msg {
            Some(msg) => {
                // 出队成功后更新状态为 Processing，避免回源重复入队
                let msg = *msg;
                self.update_status(ctx.clone(), msg.id(), MessageStatus::Processing).await?;
                Ok(Some(msg))
            }
            None => {
                // 2. 队列为空，回源 DB 查询 pending 状态的消息
                // 查询 Pending 状态，最多取 5 条，按创建时间升序
                let pending_messages = self.list_by_status(
                    ctx.clone(),
                    vec![MessageStatus::Pending],
                    Some(5),
                ).await?;

                if pending_messages.is_empty() {
                    // DB 也没有，真的空了
                    return Ok(None);
                }

                // 3. 将 DB 查到的消息全部入队到内存队列
                for msg in pending_messages {
                    let event: Box<Message> = Box::new(msg.clone());
                    self.event_queue_dao.enqueue(&ctx, event)?;
                }

                // 4. 再次尝试出队（肯定能取到了）
                Ok(self.event_queue_dao.dequeue_next(&ctx)?.map(|msg| *msg))
            }
        }
    }

    async fn ack_message(
        &self,
        _ctx: RequestContext,
        message_id: &str,
    ) -> Result<(), AppError> {
        self.event_queue_dao.ack(&_ctx, message_id)
    }

    async fn nack_message(
        &self,
        _ctx: RequestContext,
        message_id: &str,
    ) -> Result<(), AppError> {
        self.event_queue_dao.nack(&_ctx, message_id)
    }
}
