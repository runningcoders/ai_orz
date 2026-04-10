//! Message DAO 接口定义和实现

pub mod sqlite;

use crate::error::AppError;
use crate::models::message::MessagePo;
use common::enums::MessageStatus;
use common::constants::RequestContext;

// ==================== 接口 ====================

/// Message DAO trait
pub trait MessageDaoTrait: Send + Sync {
    /// 插入一条新消息
    fn insert(&self, ctx: RequestContext, message: &MessagePo) -> Result<(), AppError>;

    /// 根据 ID 查找消息
    fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<MessagePo>, AppError>;

    /// 根据任务 ID 查询所有消息，按创建时间升序排列
    /// 如果传入 limit 则限制返回数量
    fn list_by_task_id(&self, ctx: RequestContext, task_id: &str, limit: Option<usize>) -> Result<Vec<MessagePo>, AppError>;

    /// 根据来源 ID 查询所有消息
    fn list_by_from_id(&self, ctx: RequestContext, from_id: &str, limit: Option<usize>) -> Result<Vec<MessagePo>, AppError>;

    /// 根据目标 ID 查询所有消息
    fn list_by_to_id(&self, ctx: RequestContext, to_id: &str, limit: Option<usize>) -> Result<Vec<MessagePo>, AppError>;

    /// 删除消息（软删除可以用 status，但消息一般不删除，这里留作审计，所以接口只做物理删除保留）
    fn delete(&self, ctx: RequestContext, id: &str) -> Result<(), AppError>;

    /// 统计指定任务的消息数量
    fn count_by_task_id(&self, ctx: RequestContext, task_id: &str) -> Result<u64, AppError>;

    /// 更新消息处理状态
    fn update_status(&self, ctx: RequestContext, id: &str, status: MessageStatus) -> Result<(), AppError>;

    /// 根据多个状态查询消息（用于启动恢复未处理消息）
    fn list_by_status(&self, ctx: RequestContext, status: Vec<MessageStatus>, limit: Option<usize>) -> Result<Vec<MessagePo>, AppError>;
}

pub use sqlite::init;

#[cfg(test)]
mod sqlite_test;
