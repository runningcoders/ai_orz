//! Message DAO 模块

use crate::error::Result;
use crate::models::message::{MessagePo, ToolCallMessage};
use common::enums::MessageStatus;
use crate::pkg::RequestContext;

// ==================== 查询参数 ====================

/// 消息通用查询条件
///
/// 支持组合查询，所有字段都是 Option：
/// - None 表示不限制该条件
/// - Some(value) 表示必须匹配该值
#[derive(Debug, Clone, Default)]
pub struct MessageQuery {
    /// 按消息 ID 查询（通常返回单条）
    pub id: Option<String>,
    /// 按任务 ID 查询
    pub task_id: Option<String>,
    /// 按项目 ID 查询
    pub project_id: Option<String>,
    /// 按发送方 ID 查询
    pub from_id: Option<String>,
    /// 按接收方 ID 查询
    pub to_id: Option<String>,
    /// 按状态 IN 查询（支持多选）
    pub status_in: Option<Vec<MessageStatus>>,
    /// 限制返回条数（分页）
    pub limit: Option<usize>,
    /// 跳过条数（分页）
    pub offset: Option<usize>,
    /// 排序规则，如 "created_at ASC", "created_at DESC"
    pub order_by: Option<String>,
}

// ==================== 接口 ====================

/// Message DAO 接口
#[async_trait::async_trait]
pub trait MessageDao: Send + Sync {
    /// 插入一条新消息
    async fn insert(&self, ctx: RequestContext, message: &MessagePo) -> Result<()>;

    /// 通用查询方法
    ///
    /// 支持组合查询条件，所有字段都是 Option
    /// 示例：
    /// ```
    /// let messages = dao.query(ctx, MessageQuery {
    ///     task_id: Some("task-123".to_string()),
    ///     status_in: Some(vec![MessageStatus::Pending, MessageStatus::Processing]),
    ///     limit: Some(10),
    ///     ..Default::default()
    /// }).await?;
    /// ```
    async fn query(&self, ctx: RequestContext, query: MessageQuery) -> Result<Vec<MessagePo>>;

    /// 根据 ID 查找消息
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<MessagePo>>;

    /// 根据任务 ID 查询所有消息，按创建时间升序排列
    /// 如果传入 limit 则限制返回数量
    async fn list_by_task_id(&self, ctx: RequestContext, task_id: &str, limit: Option<usize>) -> Result<Vec<MessagePo>>;

    /// 根据项目 ID 查询所有消息，按创建时间升序排列
    /// 如果传入 limit 则限制返回数量
    async fn list_by_project_id(&self, ctx: RequestContext, project_id: &str, limit: Option<usize>) -> Result<Vec<MessagePo>>;

    /// 根据来源 ID 查询所有消息
    async fn list_by_from_id(&self, ctx: RequestContext, from_id: &str, limit: Option<usize>) -> Result<Vec<MessagePo>>;

    /// 根据目标 ID 查询所有消息
    async fn list_by_to_id(&self, ctx: RequestContext, to_id: &str, limit: Option<usize>) -> Result<Vec<MessagePo>>;

    /// 删除消息（软删除可以用 status，但消息一般不删除，这里留作审计，所以接口只做物理删除保留）
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 统计指定任务的消息数量
    async fn count_by_task_id(&self, ctx: RequestContext, task_id: &str) -> Result<u64>;

    /// 删除任务下所有消息（清空任务对话）
    async fn delete_by_task_id(&self, ctx: RequestContext, task_id: &str) -> Result<()>;

    /// 更新消息处理状态
    async fn update_status(&self, ctx: RequestContext, id: &str, status: MessageStatus) -> Result<()>;

    /// 根据多个状态查询消息（用于启动恢复未处理消息）
    async fn list_by_status(&self, ctx: RequestContext, status: Vec<MessageStatus>, limit: Option<usize>) -> Result<Vec<MessagePo>>;

    /// 创建工具调用请求消息（便捷方法）
    /// 工具调用请求由 Agent 发起，请求执行某个工具
    async fn create_tool_call_request(
        &self,
        ctx: RequestContext,
        req: ToolCallMessage,
    ) -> Result<MessagePo>;

    /// 创建工具调用结果消息（便捷方法）
    /// 工具调用结果由执行器返回，包含执行结果
    async fn create_tool_call_result(
        &self,
        ctx: RequestContext,
        res: ToolCallMessage,
    ) -> Result<MessagePo>;
}



pub mod sqlite;
pub use self::sqlite::{dao, init, new};

#[cfg(test)]
mod sqlite_test;