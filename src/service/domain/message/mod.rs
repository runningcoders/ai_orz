//! Message Domain 模块
//!
//! 消息领域，管理：
//! - delivery - 消息投递（发送/消费/投递到渠道）
//! - management - 消息管理（查询/更新/删除）
//!
//! 注意：渠道配置管理属于 Finance Domain，此处只保留实际投递能力

pub mod builder;
pub mod delivery;
pub mod management;

#[cfg(test)]
mod delivery_test;
#[cfg(test)]
mod management_test;

use crate::models::file::FileMeta;
use crate::models::message::Message;
pub use crate::models::tool::ToolCallTraceRef;
use crate::pkg::RequestContext;
use crate::service::dal::attachment::AttachmentDal;
use crate::service::dal::message::MessageDal;
pub use crate::service::dal::message_channel::{DeliveryResult, MessageChannelDal};
use crate::service::dal::message_push::MessagePushDal;
use crate::service::dao::message::{MessageQuery, MessageSearch};
use common::enums::{MessageRole, MessageStatus};
use common::error::Result;
use serde_json::Value;
use std::sync::{Arc, OnceLock};

// ==================== 单例 ====================

static MESSAGE_DOMAIN: OnceLock<Arc<dyn MessageDomain>> = OnceLock::new();

/// 获取 Message Domain 单例
pub fn domain() -> Arc<dyn MessageDomain> {
    MESSAGE_DOMAIN.get().cloned().unwrap()
}

/// 创建新的 Message Domain 实例（用于测试，每次测试创建独立实例保证隔离）
pub fn new(
    message_dal: Arc<dyn MessageDal>,
    message_channel_dal: Arc<dyn MessageChannelDal>,
    message_push_dal: Arc<dyn MessagePushDal>,
    attachment_dal: Arc<dyn AttachmentDal>,
) -> Arc<dyn MessageDomain> {
    let domain = MessageDomainImpl::new(
        message_dal,
        message_channel_dal,
        message_push_dal,
        attachment_dal,
    );
    Arc::new(domain)
}

/// 初始化 Message Domain（使用全局单例 DAO）
pub fn init() {
    let message_domain = MessageDomainImpl::new(
        crate::service::dal::message::dal(),
        crate::service::dal::message_channel::dal(),
        crate::service::dal::message_push::dal(),
        crate::service::dal::attachment::dal(),
    );
    let _ = MESSAGE_DOMAIN.set(Arc::new(message_domain));
}

// ==================== 实现 ====================

/// Message Domain 实现
///
/// 聚合所有消息子功能实现
struct MessageDomainImpl {
    message_dal: Arc<dyn MessageDal>,
    message_channel_dal: Arc<dyn MessageChannelDal>, // 仅用于投递，不用于配置管理
    message_push_dal: Arc<dyn MessagePushDal>,
    /// 用于在发送消息时按 ID 查找附件
    attachment_dal: Arc<dyn AttachmentDal>,
}

impl MessageDomainImpl {
    /// 创建 Domain 实例
    fn new(
        message_dal: Arc<dyn MessageDal>,
        message_channel_dal: Arc<dyn MessageChannelDal>,
        message_push_dal: Arc<dyn MessagePushDal>,
        attachment_dal: Arc<dyn AttachmentDal>,
    ) -> Self {
        Self {
            message_dal,
            message_channel_dal,
            message_push_dal,
            attachment_dal,
        }
    }
}

#[async_trait::async_trait]
impl MessageDomain for MessageDomainImpl {
    fn delivery(&self) -> &dyn MessageDelivery {
        self
    }
    fn management(&self) -> &dyn MessageManagement {
        self
    }

    async fn has_pending_message_for_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        message_type: common::enums::message::MessageType,
    ) -> Result<bool> {
        self.message_dal
            .has_pending_message_for_agent(ctx, agent_id, message_type)
            .await
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
    /// 附件 ID 列表（可选）
    /// 如果提供，会为每个附件创建一条附件消息，按顺序排列在文本消息之前
    pub attachment_ids: Option<&'a [String]>,
    /// 消息类型（默认 Text，系统通知可指定 TaskDispatchNotification 等）
    pub message_type: common::enums::message::MessageType,
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

/// 发送工具调用请求消息的命令参数
#[derive(Debug, Clone)]
pub struct SendToolCallRequestCommand<'a> {
    /// 工具调用请求 ID（用于请求/结果关联）
    pub request_id: &'a str,
    /// 工具 ID
    pub tool_id: &'a str,
    /// 工具名称（便于日志和 Prompt 展示）
    pub tool_name: &'a str,
    /// 发起调用的 Agent ID
    pub from_agent_id: &'a str,
    /// 工具执行方 ID（通常是系统工具执行器）
    pub to_executor_id: &'a str,
    /// 关联项目 ID（可选）
    pub project_id: Option<&'a str>,
    /// 关联任务 ID（可选）
    pub task_id: Option<&'a str>,
    /// 引用的父消息 ID（可选，支持消息链）
    pub reply_to_id: Option<&'a str>,
    /// 工具调用参数
    pub args: Value,
}

/// 工具调用执行结果
#[derive(Debug, Clone)]
pub enum ToolCallExecutionOutcome {
    /// 工具执行成功
    Success {
        /// 工具返回结果
        result: Value,
        /// 大结果附件元数据（可选）
        result_file_meta: Option<FileMeta>,
        /// 可选轻量追踪引用，指向 tool-specific call_trace。
        trace_ref: Option<ToolCallTraceRef>,
    },
    /// 工具执行失败
    Failure {
        /// 已脱敏的错误信息
        error_message: String,
        /// 可选轻量追踪引用。
        ///
        /// 仅当 Runtime 已经实际开始工具执行并产生真实 ToolCallEntry.call_id 时填写；
        /// 执行前授权/校验/路由失败不得伪造 trace_ref。
        trace_ref: Option<ToolCallTraceRef>,
    },
}

/// 发送工具调用结果回调消息的命令参数
#[derive(Debug, Clone)]
pub struct SendToolCallResultCommand<'a> {
    /// 原始工具调用请求消息
    pub request_message: &'a Message,
    /// 执行结果
    pub outcome: ToolCallExecutionOutcome,
}

/// 发送任务分配消息的命令参数
#[derive(Debug, Clone)]
pub struct SendTaskAssignmentCommand<'a> {
    /// 任务 ID
    pub task_id: &'a str,
    /// 任务标题
    pub task_title: &'a str,
    /// 任务描述（可选）
    pub task_description: Option<&'a str>,
    /// 分配者 ID
    pub from_id: &'a str,
    /// 分配者角色
    pub from_role: MessageRole,
    /// 接收 Agent ID
    pub to_agent_id: &'a str,
    /// 关联项目 ID（可选）
    pub project_id: Option<&'a str>,
}

/// 分发消息到用户所有可用渠道的命令参数
#[derive(Debug, Clone)]
pub struct DeliverMessageCommand<'a> {
    /// 待分发的消息
    pub message: &'a Message,
    /// 目标用户 ID
    pub user_id: &'a str,
}

use tokio::sync::broadcast;

/// 订阅结果
#[derive(Debug)]
pub struct SubscribeResult {
    pub connection_id: String,
    pub receiver: broadcast::Receiver<String>,
}

/// Message Domain 总 trait
///
/// 聚合消息领域所有子功能 trait
#[async_trait::async_trait]
pub trait MessageDomain: Send + Sync {
    /// 消息投递能力
    fn delivery(&self) -> &dyn MessageDelivery;
    /// 消息管理能力
    fn management(&self) -> &dyn MessageManagement;

    /// 检查指定 Agent 是否有 Pending 状态的指定类型消息
    ///
    /// 用于 TaskEventConsumer 发送通知前去重，避免对同一 Agent 重复投递
    /// TaskDispatchNotification 等系统通知。
    async fn has_pending_message_for_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        message_type: common::enums::message::MessageType,
    ) -> Result<bool>;
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
    ) -> Result<Message>;

    /// 发送消息给用户
    async fn send_to_user(
        &self,
        ctx: RequestContext,
        cmd: SendToUserCommand<'_>,
    ) -> Result<Message>;

    /// 发送工具调用请求消息
    async fn send_tool_call_request(
        &self,
        ctx: RequestContext,
        cmd: SendToolCallRequestCommand<'_>,
    ) -> Result<Message>;

    /// 发送工具调用结果回调消息
    async fn send_tool_call_result(
        &self,
        ctx: RequestContext,
        cmd: SendToolCallResultCommand<'_>,
    ) -> Result<Message>;

    /// 发送任务分配消息
    async fn send_task_assignment(
        &self,
        ctx: RequestContext,
        cmd: SendTaskAssignmentCommand<'_>,
    ) -> Result<Message>;

    /// 分发消息到用户所有可用渠道
    ///
    /// 自动查询用户配置的所有活跃渠道，将消息推送到每个渠道
    async fn deliver_message(
        &self,
        ctx: RequestContext,
        cmd: DeliverMessageCommand<'_>,
    ) -> Result<DeliveryResult>;

    /// 订阅 SSE 消息推送
    async fn subscribe_sse(&self, ctx: RequestContext, user_id: &str) -> Result<SubscribeResult>;

    /// 取消订阅 SSE 消息推送
    async fn unsubscribe_sse(&self, ctx: RequestContext, connection_id: &str) -> Result<()>;
}

/// 消息管理 trait
///
/// 定义消息查询、更新、删除等管理接口
#[async_trait::async_trait]
pub trait MessageManagement: Send + Sync {
    /// 通用综合查询
    ///
    /// 支持组合查询条件，所有字段都是 Option
    async fn query(&self, ctx: RequestContext, query: MessageQuery) -> Result<Vec<Message>>;

    /// 按任务 ID 查询消息列表
    async fn list_by_task_id(&self, ctx: RequestContext, task_id: &str) -> Result<Vec<Message>>;

    /// 按项目 ID 查询消息列表
    async fn list_by_project_id(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<Vec<Message>>;

    /// 根据消息 ID 获取消息
    async fn get_by_id(&self, ctx: RequestContext, message_id: &str) -> Result<Option<Message>>;

    /// 更新消息状态
    async fn update_status(
        &self,
        ctx: RequestContext,
        message_id: &str,
        status: MessageStatus,
    ) -> Result<()>;

    /// 删除单条消息
    async fn delete_by_id(&self, ctx: RequestContext, message_id: &str) -> Result<()>;

    /// 清理对话（删除任务下所有消息）
    async fn cleanup_conversation(&self, ctx: RequestContext, task_id: &str) -> Result<()>;

    /// 🔍 消息混合搜索（关键词 + 向量语义）
    ///
    /// 自动选择搜索策略：
    /// - keyword 存在 → FTS5 全文检索
    /// - query_vector 存在 → 向量语义搜索
    /// - 两者都有 → 混合搜索，合并结果
    async fn search(&self, ctx: RequestContext, search: MessageSearch) -> Result<Vec<Message>>;
}
