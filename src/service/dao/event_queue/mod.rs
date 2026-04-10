//! EventQueue DAO 层
//!
//! 通用事件队列 DAO 接口定义

use crate::error::AppError;
use common::constants::RequestContext;
use crate::models::event::Event;

/// 事件队列 DAO trait
///
/// 通用事件队列接口，支持不同实现替换
pub trait EventQueueDaoTrait: Send + Sync + std::fmt::Debug {
    /// 入队一个事件
    fn enqueue(&self, ctx: &RequestContext, event: Box<dyn Event>) -> Result<(), AppError>;

    /// 批量入队多个事件
    fn enqueue_batch(&self, ctx: &RequestContext, events: Vec<Box<dyn Event>>) -> Result<(), AppError>;

    /// 获取下一个待处理事件
    /// 返回 None 表示队列为空
    /// 获取后事件进入 "处理中" 状态，需要调用 ack 确认完成
    fn dequeue_next(&self, ctx: &RequestContext) -> Result<Option<Box<dyn Event>>, AppError>;

    /// 确认事件处理完成，从队列中移除
    fn ack(&self, ctx: &RequestContext, event_id: &str) -> Result<(), AppError>;

    /// 标记事件处理失败，重新放回队列等待重试
    fn nack(&self, ctx: &RequestContext, event_id: &str) -> Result<(), AppError>;

    /// 获取当前队列总长度（包含待处理 + 处理中）
    fn len(&self) -> usize;

    /// 判断队列是否为空
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 获取处理中事件数量
    fn in_progress_count(&self) -> usize;

    /// 恢复启动：从持久化层恢复未完成事件重新入队
    /// 返回恢复的事件数量
    fn recover(&self, ctx: &RequestContext) -> Result<usize, AppError>;
}

mod in_memory;
pub use self::in_memory::init;

#[cfg(test)]
mod in_memory_test;
