//! Message Domain 模块
//!
//! 消息领域，管理：
//! - delivery - 消息投递（发送/消费）
//! - management - 消息管理（查询/更新/删除）

pub mod delivery;
pub mod management;

#[cfg(test)]
mod delivery_test;
#[cfg(test)]
mod management_test;

use crate::error::AppError;
use crate::models::message::Message;
use crate::pkg::RequestContext;
use crate::service::dao::message::MessageQuery;
use crate::service::dal::message::MessageDal;
use async_trait::async_trait;
use common::enums::{MessageRole, MessageStatus};
use std::sync::{Arc, OnceLock};

// ==================== 单例 ====================

static MESSAGE_DOMAIN: OnceLock<Arc<dyn MessageDomain>> = OnceLock::new();

/// 获取 Message Domain 单例
pub fn domain() -> Arc<dyn MessageDomain> {
    MESSAGE_DOMAIN.get().cloned().unwrap()
}

/// 创建新的 Message Domain 实例（用于测试，每次测试创建独立实例保证隔离）
pub fn new(message_dal: Arc<dyn MessageDal>) -> Arc<dyn MessageDomain> {
    let domain = MessageDomainImpl::new(message_dal);
    Arc::new(domain)
}

/// 初始化 Message Domain（使用全局单例 DAO）
pub fn init() {
    let message_domain = MessageDomainImpl::new(
        crate::service::dal::message::dal(),
    );
    let _ = MESSAGE_DOMAIN.set(Arc::new(message_domain));
}

// ==================== 实现 ====================

/// Message Domain 实现
///
/// 聚合所有消息子功能实现
struct MessageDomainImpl {
    message_dal: Arc<dyn MessageDal>,
}

impl MessageDomainImpl {
    /// 创建 Domain 实例
    fn new(message_dal: Arc<dyn MessageDal>) -> Self {
        Self { message_dal }
    }
}

impl MessageDomain for MessageDomainImpl {
    fn delivery(&self) -> &dyn MessageDelivery {
        self
    }
    fn management(&self) -> &dyn MessageManagement {
        self
    }
}

// ==================== traits 定义 ====================

/// 发送消息给 Agent 的命令参数
#[derive(Debug, Clone)]
pub struct SendToAgentCommand<'a> {
    /// 发送者 ID
    pub from_id: &'a str,
    /// 发送者角色（用户/Agent）
    pub from_role: MessageRole,
    /// 目标 Agent ID
    pub to_agent_id: &'a str,
    /// 消息内容
    pub content: &'a str,
    /// 关联项目 ID（可选）
    pub project_id: Option<&'a str>,
    /// 关联任务 ID（可选）
    pub task_id: Option<&'a str>,
    /// 引用的父消息 ID（可选，支持消息链）
    pub reply_to_id: Option<&'a str>,
}

/// 发送消息给用户的命令参数
#[derive(Debug, Clone)]
pub struct SendToUserCommand<'a> {
    /// 发送者 Agent ID
    pub from_agent_id: &'a str,
    /// 目标用户 ID
    pub to_user_id: &'a str,
    /// 消息内容
    pub content: &'a str,
    /// 关联项目 ID（可选）
    pub project_id: Option<&'a str>,
    /// 关联任务 ID（可选）
    pub task_id: Option<&'a str>,
    /// 引用的父消息 ID（可选，支持消息链）
    pub reply_to_id: Option<&'a str>,
}

/// Message Domain 总 trait
///
/// 聚合消息领域所有子功能 trait
pub trait MessageDomain: Send + Sync {
    /// 消息投递能力
    fn delivery(&self) -> &dyn MessageDelivery;
    /// 消息管理能力
    fn management(&self) -> &dyn MessageManagement;
}

/// 消息投递 trait
///
/// 定义消息投递相关的核心业务接口
#[async_trait::async_trait]
pub trait MessageDelivery: Send + Sync {
    /// 发送消息给 Agent
    async fn send_to_agent(
        &self,
        ctx: RequestContext,
        cmd: SendToAgentCommand<'_>,
    ) -> Result<Message, AppError>;

    /// 发送消息给用户
    async fn send_to_user(
        &self,
        ctx: RequestContext,
        cmd: SendToUserCommand<'_>,
    ) -> Result<Message, AppError>;

    /// 获取下一个待消费的消息
    ///
    /// 返回 None 表示队列为空
    /// 获取后消息状态变为 Processing，需要调用 ack 确认完成
    async fn dequeue_next(
        &self,
        ctx: RequestContext,
    ) -> Result<Option<Message>, AppError>;

    /// 确认消息消费完成
    ///
    /// 更新消息状态为 Processed，并从事件队列移除
    async fn ack(
        &self,
        ctx: RequestContext,
        message_id: &str,
    ) -> Result<(), AppError>;

    /// 消息消费失败，重新入队等待重试
    ///
    /// 更新消息状态为 Pending，放回队列
    async fn nack(
        &self,
        ctx: RequestContext,
        message_id: &str,
    ) -> Result<(), AppError>;
}

/// 消息管理 trait
///
/// 定义消息查询、更新、删除等管理接口
#[async_trait::async_trait]
pub trait MessageManagement: Send + Sync {
    /// 通用综合查询
    ///
    /// 支持组合查询条件，所有字段都是 Option
    async fn query(
        &self,
        ctx: RequestContext,
        query: MessageQuery,
    ) -> Result<Vec<Message>, AppError>;

    /// 按任务 ID 查询消息列表
    async fn list_by_task_id(
        &self,
        ctx: RequestContext,
        task_id: &str,
    ) -> Result<Vec<Message>, AppError>;

    /// 按项目 ID 查询消息列表
    async fn list_by_project_id(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<Vec<Message>, AppError>;

    /// 根据消息 ID 获取消息
    async fn get_by_id(
        &self,
        ctx: RequestContext,
        message_id: &str,
    ) -> Result<Option<Message>, AppError>;

    /// 更新消息状态
    async fn update_status(
        &self,
        ctx: RequestContext,
        message_id: &str,
        status: MessageStatus,
    ) -> Result<(), AppError>;

    /// 删除单条消息
    async fn delete_by_id(
        &self,
        ctx: RequestContext,
        message_id: &str,
    ) -> Result<(), AppError>;

    /// 清理对话（删除任务下所有消息）
    async fn cleanup_conversation(
        &self,
        ctx: RequestContext,
        task_id: &str,
    ) -> Result<(), AppError>;
}
