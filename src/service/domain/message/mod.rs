//! Message Domain 模块
//!
//! 消息领域，管理：
//! - delivery - 消息投递（发送/消费/投递到渠道）
//! - management - 消息管理（查询/更新/删除）
//! - inbound - 入站适配门面（外部渠道事件 → AdaptedMessage，枚举收敛）
//!
//! 注意：渠道配置管理属于 Finance Domain，此处只保留实际投递能力

pub mod builder;
pub mod delivery;
pub mod inbound;
pub mod management;
pub mod recall;
pub mod resource_resolver;

#[cfg(test)]
mod delivery_test;
#[cfg(test)]
mod management_test;
#[cfg(test)]
mod recall_test;

use crate::models::file::FileMeta;
use crate::models::message::Message;
pub use crate::models::tool::ToolCallTraceRef;
use crate::pkg::RequestContext;
use crate::service::dal::agent::AgentDal;
use crate::service::dal::artifact::ArtifactDal;
use crate::service::dal::attachment::AttachmentDal;
use crate::service::dal::message::MessageDal;
pub use crate::service::dal::message_channel::{DeliveryResult, MessageChannelDal, ReplyTarget};
use crate::service::dal::message_push::MessagePushDal;
use crate::service::dal::project::ProjectDal;
use crate::service::dal::user::UserDal;
use crate::service::dao::message::{MessageQuery, MessageSearch};
use common::enums::{MessageRole, MessageStatus};
use common::error::Result;
pub use inbound::{InboundSource, MessageInboundAdapt};
pub use recall::RecallOutcome;
use serde_json::Value;
use std::sync::{Arc, OnceLock};

// ==================== 单例 ====================

static MESSAGE_DOMAIN: OnceLock<Arc<dyn MessageDomain>> = OnceLock::new();

/// 获取 Message Domain 单例
pub fn domain() -> Arc<dyn MessageDomain> {
    MESSAGE_DOMAIN.get().cloned().unwrap()
}

/// 创建新的 Message Domain 实例（用于测试，每次测试创建独立实例保证隔离）
#[allow(clippy::too_many_arguments)]
pub fn new(
    message_dal: Arc<dyn MessageDal>,
    message_channel_dal: Arc<dyn MessageChannelDal>,
    message_push_dal: Arc<dyn MessagePushDal>,
    attachment_dal: Arc<dyn AttachmentDal>,
    artifact_dal: Arc<dyn ArtifactDal>,
    lark_dal: Arc<crate::service::dal::lark::LarkDalImpl>,
    wechat_dal: Arc<crate::service::dal::wechat::WechatDalImpl>,
    email_dal: Arc<crate::service::dal::email::EmailDalImpl>,
    user_dal: Arc<dyn UserDal + Send + Sync>,
    agent_dal: Arc<dyn AgentDal>,
    project_dal: Arc<dyn ProjectDal + Send + Sync>,
) -> Arc<dyn MessageDomain> {
    let domain = MessageDomainImpl::new(
        message_dal,
        message_channel_dal,
        message_push_dal,
        attachment_dal,
        artifact_dal,
        lark_dal,
        wechat_dal,
        email_dal,
        user_dal,
        agent_dal,
        project_dal,
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
        crate::service::dal::artifact::dal(),
        crate::service::dal::lark::dal(),
        crate::service::dal::wechat::dal(),
        crate::service::dal::email::dal(),
        crate::service::dal::user::dal(),
        crate::service::dal::agent::dal(),
        crate::service::dal::project::dal(),
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
    /// 用于消息资源引用（artifact:）预解析与出站降级
    artifact_dal: Arc<dyn ArtifactDal>,
    /// 飞书渠道 DAL（入站适配：WS 事件 → AdaptedMessage）
    lark_dal: Arc<crate::service::dal::lark::LarkDalImpl>,
    /// 微信渠道 DAL（入站适配：iLink 轮询事件 → AdaptedMessage）
    wechat_dal: Arc<crate::service::dal::wechat::WechatDalImpl>,
    /// 邮件渠道 DAL（入站适配：IMAP 轮询事件 → AdaptedMessage）
    email_dal: Arc<crate::service::dal::email::EmailDalImpl>,
    /// 用户 DAL：收件人「角色 ⟷ ID」一致性门闩用（`to_user_id` 必须不是 Agent）
    user_dal: Arc<dyn UserDal + Send + Sync>,
    /// Agent DAL：收件人「角色 ⟷ ID」一致性门闩用（`to_agent_id` 必须不是用户）
    agent_dal: Arc<dyn AgentDal>,
    /// 项目 DAL：撤回权限 gate 用（判据「归属用户 / Owner Agent」需读项目）
    project_dal: Arc<dyn ProjectDal + Send + Sync>,
}

impl MessageDomainImpl {
    /// 创建 Domain 实例
    #[allow(clippy::too_many_arguments)]
    fn new(
        message_dal: Arc<dyn MessageDal>,
        message_channel_dal: Arc<dyn MessageChannelDal>,
        message_push_dal: Arc<dyn MessagePushDal>,
        attachment_dal: Arc<dyn AttachmentDal>,
        artifact_dal: Arc<dyn ArtifactDal>,
        lark_dal: Arc<crate::service::dal::lark::LarkDalImpl>,
        wechat_dal: Arc<crate::service::dal::wechat::WechatDalImpl>,
        email_dal: Arc<crate::service::dal::email::EmailDalImpl>,
        user_dal: Arc<dyn UserDal + Send + Sync>,
        agent_dal: Arc<dyn AgentDal>,
        project_dal: Arc<dyn ProjectDal + Send + Sync>,
    ) -> Self {
        Self {
            message_dal,
            message_channel_dal,
            message_push_dal,
            attachment_dal,
            artifact_dal,
            lark_dal,
            wechat_dal,
            email_dal,
            user_dal,
            agent_dal,
            project_dal,
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
    fn inbound(&self) -> &dyn MessageInboundAdapt {
        self
    }

    async fn has_unfinished_message_for_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        message_type: common::enums::message::MessageType,
    ) -> Result<bool> {
        self.message_dal
            .has_unfinished_message_for_agent(ctx, agent_id, message_type)
            .await
    }

    async fn has_pending_followup_for_project(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<bool> {
        // 「跟进类」两类统一去重，口径与理由见 trait 文档
        //
        // 作用域是**项目**而不是接收方 Agent：Owner 同时带多个项目时，按 Agent 去重
        // 会让先挂上 Pending 的那个项目一直占着门，其余项目的完成通知被永久饿死
        // （「重复」变「饿死」，更隐蔽）。与 `scheduler::handle_project_followup`
        // 的批量去重口径一致。
        //
        // ⚠️ 宽口径 `unfinished`：已派发（`Processing`）的跟进通知仍在途，必须算「已有」，
        // 否则同一项目会被重复投跟进通知（见 `MessageStatus::is_unfinished` 的双用途说明）。
        for message_type in [
            common::enums::message::MessageType::TaskDispatchNotification,
            common::enums::message::MessageType::ProjectFollowupNotification,
        ] {
            let pending = self
                .message_dal
                .count(
                    ctx.clone(),
                    MessageQuery {
                        project_id: Some(project_id.to_string()),
                        message_type: Some(message_type),
                        status_in: Some(vec![MessageStatus::Pending, MessageStatus::Processing]),
                        ..Default::default()
                    },
                )
                .await?;
            if pending > 0 {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn list_stale_pending_notifications(
        &self,
        ctx: RequestContext,
        older_than_ms: i64,
        limit: usize,
    ) -> Result<Vec<Message>> {
        // 两种跟进类型各查一次：`MessageQuery::message_type` 是单值，且不能不带它 ——
        // 否则普通对话会把 `limit` 占满，真正要救的通知永远排不进来。
        //
        // `created_before` 取「创建时间早于 now - older_than_ms」：这个时间窗是
        // 幂等前提 —— 只有「早就该处理却还停在 Pending」的才算滞留；刚创建的消息
        // 可能正在被消费（状态要等 `on_consumed` 才翻 `Processed`），提前重投会
        // 变成同一通知唤醒两次。
        let cutoff = common::constants::utils::current_timestamp_ms() - older_than_ms;
        let mut stale = Vec::new();
        for message_type in [
            common::enums::message::MessageType::TaskDispatchNotification,
            common::enums::message::MessageType::ProjectFollowupNotification,
        ] {
            let rows = self
                .message_dal
                .query(
                    ctx.clone(),
                    MessageQuery {
                        to_role: Some(MessageRole::Agent),
                        message_type: Some(message_type),
                        status_in: Some(vec![MessageStatus::Pending]),
                        created_before: Some(cutoff),
                        order_by: Some("created_at ASC".to_string()),
                        limit: Some(limit),
                        ..Default::default()
                    },
                )
                .await?;
            stale.extend(rows);
        }
        Ok(stale)
    }

    async fn recall_message(
        &self,
        ctx: RequestContext,
        message_id: &str,
        reason: Option<&str>,
    ) -> Result<RecallOutcome> {
        self.recall_message_impl(ctx, message_id, reason).await
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
    /// 外部渠道消息键（可选，形如 `"lark:om_xxx"`）
    ///
    /// 渠道入站消息落库时随消息写入 `messages.external_key`，供后续
    /// 入站回复按渠道 parent_id/root_id 反查父消息、贯通跨渠道消息链。
    pub external_key: Option<&'a str>,
    /// 外部渠道话题键（可选，形如 `"omt_xxx"`，飞书「话题」唯一标识）
    ///
    /// 渠道入站话题消息落库时随消息写入 `messages.thread_id`，同一话题内
    /// 所有消息共享，供出站时按话题回复。
    pub thread_id: Option<&'a str>,
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

/// 投递出口开关
///
/// 决定一条消息落到哪些出口。默认双出口全开（`deliver_message` 的历史行为）。
///
/// ⚠️ **渠道入站消息必须用 [`DeliveryOptions::sse_only()`]**：它由对端渠道送达，
/// 再走渠道投递会把消息回灌回原渠道形成回声（用户在微信里收到自己刚发的消息）；
/// 但网页端没有本地气泡可兜底，SSE 这一路必须保留。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeliveryOptions {
    /// 是否投递到外部渠道（飞书/微信/邮件…）
    pub channels: bool,
    /// 是否推送到 SSE 长连接（网页端）
    pub sse: bool,
}

impl Default for DeliveryOptions {
    /// 默认双出口全开
    fn default() -> Self {
        Self {
            channels: true,
            sse: true,
        }
    }
}

impl DeliveryOptions {
    /// 只推前端（SSE），不做渠道投递
    pub fn sse_only() -> Self {
        Self {
            channels: false,
            sse: true,
        }
    }
}

/// 分发消息到用户所有可用渠道的命令参数
#[derive(Debug, Clone)]
pub struct DeliverMessageCommand<'a> {
    /// 待分发的消息
    pub message: &'a Message,
    /// 目标用户 ID
    pub user_id: &'a str,
    /// 出口开关（默认渠道 + SSE 双投递）
    pub options: DeliveryOptions,
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
    /// 入站适配能力（外部渠道事件 → AdaptedMessage）
    fn inbound(&self) -> &dyn MessageInboundAdapt;

    /// 检查指定 Agent 是否已有「未完成」的指定类型消息
    ///
    /// 「未完成」= `Pending` 或 `Processing`（见 [`MessageStatus::is_unfinished`]）。
    ///
    /// ⚠️⚠️ **必须是宽口径**：消息被派发给消费者的那一刻就会写成 `Processing`，
    /// 若只看 `Pending`，正在处理的那条会「隐形」 ⇒ `dispatch_ready_successors`
    /// 会把同一个任务重复派发给同一个执行 Agent（重复干活，且更难被发现）。
    ///
    /// 用于 TaskEventConsumer 发送通知前去重，避免对同一 Agent 重复投递
    /// TaskDispatchNotification 等系统通知。
    async fn has_unfinished_message_for_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        message_type: common::enums::message::MessageType,
    ) -> Result<bool>;

    /// 检查指定**项目**是否已有「跟进类」的**在途**（未完成）通知
    ///
    /// 「跟进类」= 语义同源、都会让 Owner 重跑一遍全局编排的这两类：
    /// `TaskDispatchNotification`（任务完成驱动 Owner 审视）与
    /// `ProjectFollowupNotification`（定时巡检驱动 Owner 审视）。
    ///
    /// 二者由不同链路投递同一 Owner，但去重口径必须统一：若只在各自类型内去重，
    /// 同一项目会同时收到两条跟进通知（一条变更驱动、一条定时驱动）。消费侧不得
    /// 自行拼类型清单，故收敛到本 domain 单点，避免清单在各消费者间漂移。
    ///
    /// ⚠️ 作用域必须是项目，不能是接收方 Agent：Owner 带多个项目时，按 Agent 去重
    /// 会让先挂上 Pending 的项目一直占着门，其余项目的通知永远排不上。
    ///
    /// ⚠️ 判据是**未完成**（`Pending | Processing`）而非 Pending：正在处理的跟进
    /// 通知仍会让 Owner 重跑一轮编排，它没「在途」只是我们看不见（详见
    /// [`MessageStatus::is_unfinished`] 的双用途对照表）。
    async fn has_pending_followup_for_project(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<bool>;

    /// 列出「滞留 Pending」的跟进类通知（定时兜底重投用）
    ///
    /// 「滞留」判据只有两条：状态仍是 `Pending`、创建时间早于 `now - older_than_ms`。
    ///
    /// ⚠️⚠️ **状态必须是窄口径 `Pending`，不能用 `is_unfinished()`** —— 这是本
    /// 方法与其他去重口径**唯一相反**的地方：`Processing` 表示「消费侧已领走、正在跑」，
    /// 捞它重投会造成同一条消息被并发消费两次。去重要宽、重投要窄，理由见
    /// [`MessageStatus::is_unfinished`] 的对照表。
    ///
    /// 时间窗同样是必需的：消息在消费完成前也是 `Pending`，只有过了窗口还停着，
    /// 才能断定「这条的唤醒被吃掉了」。
    ///
    /// 为什么要这个兜底：Owner 通知一旦投递时恰逢接收方不可用（沉淀 Resting /
    /// 被别的链路占用），AOP 的重投会在退避若干次后 `Discard`，消息就永久停在
    /// `Pending` 再无人问津。定时扫描 + `MessageDal::republish_created` 是这条链路
    /// 最后的确定性兜底 —— 消息表是权威待办，队列只是加速器。
    async fn list_stale_pending_notifications(
        &self,
        ctx: RequestContext,
        older_than_ms: i64,
        limit: usize,
    ) -> Result<Vec<Message>>;

    /// 撤回一条消息（权限 gate + 状态 gate + 在飞转取消）
    ///
    /// 单点入口：REST handler 与神经工具 `recall_message` 共用同一实现
    /// （权限判据 / 状态判据只写一遍）。
    ///
    /// - 未处理（`Pending`）→ 标 `Recalled`；其事件仍在 AOP 队列，出队时被消费者守卫跳过；
    /// - 正在处理（in-flight，以 `AgentRuntimeStateManager` 为准）→ 发取消信号 + 标 `Recalled`；
    /// - 已撤回 / 已处理 / 已失败 → 幂等返回，**不报 error**（见 [`RecallOutcome`]）。
    ///
    /// `reason` 仅用于审计日志，不落库。
    async fn recall_message(
        &self,
        ctx: RequestContext,
        message_id: &str,
        reason: Option<&str>,
    ) -> Result<RecallOutcome>;
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
