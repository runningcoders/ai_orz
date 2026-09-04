//! 消息消费者（业务层）
//!
//! 作为 AOP 事件中心的订阅者，消费 MESSAGE_CREATED 事件。
//! 本模块只负责"订阅 + 调度"，业务逻辑通过调用 domain 层完成：
//! - Agent 消息 → RuntimeDomain.awaken()
//! - User 消息 → MessageDomain.deliver_message()
//! - System 消息 → RuntimeDomain.tool_execution()
//!
//! 与 AOP 框架解耦：AOP 只负责事件流转，本模块负责业务编排。

use async_trait::async_trait;
use common::enums::{CallerType, MessageRole, MessageStatus, MessageType};
use common::error::{Error, Result};
use serde_json::Value;
use std::sync::Arc;

use crate::models::events::MessageCreatedEvent;
use crate::models::message::{Message, ToolCallMessage};
use crate::pkg::RequestContext;
use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;
use crate::pkg::aop::{ConsumeMode, Consumer, EventKind};
use crate::service::dal::agent::AgentFetchOptions;
use crate::service::dal::message as message_dal;
use crate::service::domain::hr::{self as hr_domain, HrDomain};
use crate::service::domain::message::{
    self as message_domain, DeliverMessageCommand, MessageDomain, SendToAgentCommand,
    SendToUserCommand, SendToolCallResultCommand, ToolCallExecutionOutcome,
};
use crate::service::domain::organization::{self as organization_domain, OrganizationDomain};
use crate::service::domain::project::{self as project_domain, ProjectDomain};
use crate::service::domain::runtime::{
    self as runtime_domain, RuntimeDomain, awakening::ThinkingOptions,
};

// ==================== 消费者实现 ====================

/// Agent 唤醒消费者
///
/// 订阅 MESSAGE_CREATED 事件，按 to_role 分发到不同 domain 处理。
/// 作为 AOP 的 Async 消费者，由 Registry 调度器自动轮询拉取。
pub struct MessageConsumer {
    runtime_domain: Arc<dyn RuntimeDomain>,
    message_domain: Arc<dyn MessageDomain>,
    hr_domain: Arc<dyn HrDomain>,
    project_domain: Arc<dyn ProjectDomain>,
    organization_domain: Arc<dyn OrganizationDomain>,
}

impl Default for MessageConsumer {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageConsumer {
    pub fn new() -> Self {
        Self {
            runtime_domain: runtime_domain::domain(),
            message_domain: message_domain::domain(),
            hr_domain: hr_domain::domain(),
            project_domain: project_domain::domain(),
            organization_domain: organization_domain::domain(),
        }
    }
}

#[async_trait]
impl Consumer for MessageConsumer {
    fn name(&self) -> &str {
        "agent.awakening"
    }

    fn interested_events(&self) -> Vec<EventKind> {
        vec![EventKind::new("message.created")]
    }

    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Async
    }

    async fn on_event(&self, ctx: RequestContext, event: serde_json::Value) -> Result<()> {
        let msg_event: MessageCreatedEvent = serde_json::from_value(event)?;

        // 从 DB 加载完整 Message
        // ctx 已由 AOP 框架从事件顶层 context_carrier 还原，保留原始 log_id 等链路标识
        let message = message_dal::dal()
            .find_by_id(ctx.clone(), &msg_event.message_id)
            .await?
            .ok_or_else(|| {
                Error::not_found(format!("Message {} not found", msg_event.message_id))
            })?;

        sys_debug!(
            "received message: {:?} -> {:?}, type: {:?}",
            message.from_role(),
            message.to_role(),
            message.message_type()
        );

        // 根据 to_role 分发到对应 domain（携带框架还原的 ctx，下游可继续追加/修饰）
        match message.to_role() {
            MessageRole::Agent => {
                self.handle_agent_message(&ctx, &message).await?;
            }
            MessageRole::User => {
                self.handle_user_message(&ctx, &message).await?;
            }
            MessageRole::System => {
                self.handle_system_message(&ctx, &message).await?;
            }
        }

        Ok(())
    }

    async fn ack(&self, event_id: &str) -> Result<()> {
        let ctx = RequestContext::new_system();
        message_dal::dal()
            .update_status(ctx, event_id, MessageStatus::Processed)
            .await?;
        Ok(())
    }

    async fn nack(&self, event_id: &str) -> Result<()> {
        let ctx = RequestContext::new_system();
        message_dal::dal()
            .update_status(ctx, event_id, MessageStatus::Pending)
            .await?;
        Ok(())
    }

    fn concurrency(&self) -> usize {
        4
    }

    fn empty_queue_sleep_ms(&self) -> u64 {
        100
    }

    fn error_retry_sleep_ms(&self) -> u64 {
        1000
    }
}

// ==================== 业务编排（调用 domain 层）====================

impl MessageConsumer {
    /// 跨组织提及直连路由（P4）
    ///
    /// 仅用户消息触发（Agent 回复中的 @ 对端提及不外呼，防调用环）：
    /// - 可路由（对端 Active 连接 + a2a_task 能力）→ 联邦委派，对端回复以对端
    ///   Agent 名义发回原发送用户，返回 true（跳过本端 Agent 唤醒）
    /// - 不可路由（无 Active 连接 / 未开放能力）→ 返回 false，走既有流程
    ///   （提及降级为普通上下文注入，既有原则不变）
    /// - 已建联但调用失败 → 以目标本端 Agent 名义回错误说明，返回 true
    async fn try_federated_delegation(
        &self,
        ctx: &RequestContext,
        message: &Message,
    ) -> Result<bool> {
        use common::mention::{MentionKind, extract_mentions};

        if message.from_role() != MessageRole::User {
            return Ok(false);
        }
        let federated: Vec<common::mention::MentionRef> = extract_mentions(message.content())
            .into_iter()
            .filter(|m| m.kind == MentionKind::Agent && m.org.is_some())
            .collect();
        if federated.is_empty() {
            return Ok(false);
        }

        let caller_user = message.from_id().to_string();
        // 组织上下文优先取消息自带 organization_id（消费链路 ctx 可能无组织绑定，
        // 且多 Local 组织共存时回退查询有歧义——测试环境即多节点逻辑隔离场景）
        let delegation_ctx = match &message.po.organization_id {
            Some(org) => ctx.to_builder().organization_id(org).build(),
            None => ctx.clone(),
        };
        let mut routed = false;
        for m in federated {
            let peer_org = m.org.clone().unwrap_or_default();
            match self
                .organization_domain
                .organization_manage()
                .delegate_agent_task(
                    delegation_ctx.clone(),
                    &peer_org,
                    &m.id,
                    message.content(),
                    Some(caller_user.clone()),
                )
                .await
            {
                Ok(Some(reply)) => {
                    let cmd = SendToUserCommand {
                        from_agent_id: &m.id,
                        to_user_id: message.from_id(),
                        content: &reply,
                        project_id: message.project_id(),
                        task_id: message.task_id(),
                        reply_to_id: Some(message.po.id.as_str()),
                    };
                    self.message_domain
                        .delivery()
                        .send_to_user(ctx.clone(), cmd)
                        .await?;
                    routed = true;
                }
                Ok(None) => {
                    // 不可路由：降级为普通提及（仅上下文注入），继续既有流程
                    log_info!(
                        ctx,
                        "federated_delegation",
                        "对端不可路由，降级为普通提及 peer_org={}",
                        peer_org
                    );
                }
                Err(e) => {
                    log_warn!(
                        ctx,
                        "federated_delegation",
                        "跨组织委派失败 peer_org={} peer_agent={} error={}",
                        peer_org,
                        m.id,
                        e
                    );
                    let err_text = format!("跨组织委派失败（对端组织 {}）：{}", peer_org, e);
                    let cmd = SendToUserCommand {
                        from_agent_id: message.to_id(),
                        to_user_id: message.from_id(),
                        content: &err_text,
                        project_id: message.project_id(),
                        task_id: message.task_id(),
                        reply_to_id: Some(message.po.id.as_str()),
                    };
                    self.message_domain
                        .delivery()
                        .send_to_user(ctx.clone(), cmd)
                        .await?;
                    routed = true;
                }
            }
        }
        Ok(routed)
    }

    /// Agent 消息处理：调用 RuntimeDomain 唤醒 Agent
    async fn handle_agent_message(&self, ctx: &RequestContext, message: &Message) -> Result<()> {
        // P4：跨组织提及直连路由（agent:<id>@<org_id>），命中即不再唤醒本端 Agent
        if self.try_federated_delegation(ctx, message).await? {
            return Ok(());
        }

        let agent_id = &message.po.to_id;

        // 原子地占用 Agent（修复 TOCTOU 竞态）
        // 之前 is_unavailable + 后续 awaken 的 set_busy 之间存在窗口，4 个 worker 并发时
        // 同一 agent 收不同 project 消息会被两个 worker 同时通过检查
        let acquired = AgentRuntimeStateManager::global().try_set_busy(
            agent_id,
            &message.po.id,
            message.po.task_id.as_deref(),
            message.po.project_id.as_deref(),
        );
        if !acquired {
            return Err(Error::conflict(format!(
                "Agent {} is busy or resting, message will be retried",
                agent_id
            )));
        }
        // 注意：此时已 set_busy，后续失败路径必须 set_idle
        // awaken 内部会创建 BusyGuard 确保清理
        // 但 awaken 之前的失败（如 get_agent）需要显式清理

        let mut ctx = self.rebuild_context(message, ctx);

        // 加载 Agent 实体（包含工具 + 技能 + 统计信息，供唤醒流程使用）
        let fetch_options = AgentFetchOptions {
            with_tools: Some(true),
            with_skills: Some(true),
            with_stats: Some(message.po.task_id.is_some()),
            stats_task_id: message.po.task_id.clone(),
            ..Default::default()
        };
        let agent_result = self
            .hr_domain
            .agent_manage()
            .get_agent(ctx.clone(), agent_id, fetch_options)
            .await;

        let mut agent = match agent_result {
            Ok(Some(a)) => a,
            Ok(None) => {
                // Agent 不存在：永久错误，不应无限重试
                // 释放 Busy 状态并返回非重试错误
                AgentRuntimeStateManager::global().set_idle(agent_id);
                return Err(Error::not_found(format!(
                    "Agent {} not found, message will not be retried",
                    agent_id
                )));
            }
            Err(e) => {
                // 查询失败：临时错误，释放 Busy 允许重试
                AgentRuntimeStateManager::global().set_idle(agent_id);
                return Err(e);
            }
        };

        // 检查任务完成状态（优先于 thinking_depth 检查）
        // 顺序说明：若任务已 Completed/Cancelled，应直接跳过唤醒，避免向已结束的任务
        // 发送误导性的"达到最大思考深度"消息
        // 同时缓存 task 实体，供后续 ThinkingOptions 注入 prompt 上下文复用
        let mut cached_task: Option<crate::models::task::Task> = None;
        if let Some(task_id) = &message.po.task_id {
            match self
                .project_domain
                .task_manage()
                .get(ctx.clone(), task_id)
                .await
            {
                Ok(Some(task)) => {
                    if matches!(
                        task.po.status,
                        common::enums::TaskStatus::Completed
                            | common::enums::TaskStatus::Cancelled
                            | common::enums::TaskStatus::Archived
                    ) {
                        log_info!(
                            &ctx,
                            "handle_agent_message",
                            "Task {} is in {:?} state, skipping agent wake",
                            task_id,
                            task.po.status
                        );
                        // 释放 Busy 状态（awaken 不会被调用）
                        AgentRuntimeStateManager::global().set_idle(agent_id);
                        return Ok(());
                    }
                    cached_task = Some(task);
                }
                Ok(None) => {
                    log_warn!(
                        &ctx,
                        "handle_agent_message",
                        "task {} not found, skip status check",
                        task_id
                    );
                }
                Err(e) => {
                    // 查询失败：临时错误，释放 Busy 允许重试
                    AgentRuntimeStateManager::global().set_idle(agent_id);
                    return Err(e);
                }
            }
        }

        // 检查轮次限制
        if let (Some(_task_id), Some(stats)) = (&message.po.task_id, &agent.stats)
            && let Some(call_summary) = &stats.call_summary
        {
            let runtime_config = agent.po.get_runtime_config();
            let max_depth = runtime_config.max_thinking_depth as u64;
            if call_summary.total_calls >= max_depth {
                log_warn!(
                    &ctx,
                    "handle_agent_message",
                    "Agent {} reached max thinking depth ({}), stopping loop",
                    agent_id,
                    max_depth
                );

                let send_result = self.message_domain
                        .delivery()
                        .send_to_user(
                            ctx.clone(),
                            crate::service::domain::message::SendToUserCommand {
                                from_agent_id: agent_id,
                                to_user_id: &message.po.from_id,
                                content: &format!(
                                    "Agent has reached the maximum thinking depth ({} turns). The task has been stopped to prevent infinite loops.",
                                    max_depth
                                ),
                                project_id: message.po.project_id.as_deref(),
                                task_id: message.po.task_id.as_deref(),
                                reply_to_id: None,
                            },
                        )
                        .await;

                // 通知失败仅记录警告，不阻塞 Agent 释放 busy / 返回 Ok
                // （thinking depth 是合法停止，通知失败不应触发消息重试）
                if let Err(notify_err) = send_result {
                    log_warn!(
                        &ctx,
                        "handle_agent_message",
                        "通知用户 Agent 已达最大思考深度失败（不阻塞停止流程）: {}",
                        notify_err
                    );
                }

                // 释放 Busy 状态（awaken 不会被调用，BusyGuard 不会创建）
                AgentRuntimeStateManager::global().set_idle(agent_id);
                return Ok(());
            }
        }

        // 确保 Agent 有 Brain
        // wake_agent_brain 内部会查询 ModelProvider 并 enrich ctx
        // （补充 model_provider_id / model_name 字段），返回的新 ctx 用于后续 awaken
        if agent.brain.is_none() {
            log_info!(
                &ctx,
                "handle_agent_message",
                "Agent {} brain not initialized, auto waking brain",
                agent_id
            );
            let enriched_ctx = self
                .runtime_domain
                .awakening()
                .wake_agent_brain(ctx, &mut agent)
                .await
                .inspect_err(|_e| {
                    // wake_agent_brain 失败：释放 Busy 允许重试
                    // （awaken 未被调用，BusyGuard 未创建）
                    AgentRuntimeStateManager::global().set_idle(agent_id);
                })?;
            ctx = enriched_ctx;
        }

        // 构造 ThinkingOptions：注入消息关联的 project/task 实体作为业务上下文
        // task 实体复用上方状态检查的查询结果（不重复查询）；project 按需查询
        // 遵循 Context 补充原则：仅当下游 awaken 需要 project 上下文时才查询
        //
        // max_thinking_rounds: 0 = 使用系统配置 [agent].max_thinking_rounds
        // 非 0 = Agent 级覆盖值
        let runtime_config = agent.po.get_runtime_config();
        let mut thinking_options = ThinkingOptions::new();
        if runtime_config.max_thinking_rounds > 0 {
            thinking_options =
                thinking_options.with_max_thinking_rounds(runtime_config.max_thinking_rounds);
        }
        // 捕获"当前工作关联的用户 id"（任务/项目的 root_user_id）：
        // Agent 间协作消息（from_role=Agent）的 from_id 是 Agent ID，消息本身不携带用户上下文，
        // rebuild_context 重建出的 ctx 缺 user_id，会在凭据解析（resolve_tool_credentials 依赖
        // ctx.user_id）处断链。这里复用上方已加载的任务实体（不重复查询），在真正唤醒前推导并注入。
        let mut work_root_user_id = cached_task
            .as_ref()
            .map(|task| task.po.root_user_id.clone())
            .filter(|s| !s.is_empty());
        if let Some(project_id) = &message.po.project_id
            && let Ok(Some(project)) = self
                .project_domain
                .project_manage()
                .get(ctx.clone(), project_id)
                .await
        {
            if work_root_user_id.is_none() && !project.po.root_user_id.is_empty() {
                work_root_user_id = Some(project.po.root_user_id.clone());
            }
            thinking_options = thinking_options.with_project(project);
        }
        if let Some(task) = cached_task {
            thinking_options = thinking_options.with_task(task);
        }
        // 注入消息发送者的用户画像（基础信息 + 自述偏好），构建 awaken 【用户画像】区块
        // 仅 User 发送的消息才查询；查询失败仅记日志不阻塞唤醒
        if message.from_role() == MessageRole::User
            && let Ok(Some(sender)) = crate::service::domain::organization::domain()
                .user_manage()
                .get_user_by_id(ctx.clone(), &message.po.from_id)
                .await
        {
            thinking_options = thinking_options.with_user_profile(sender);
        }

        // 注入推导出的用户上下文（任务/项目 root_user_id），保证凭据链路按归属用户解析
        if ctx.user_id().is_none()
            && let Some(root_user_id) = work_root_user_id.clone()
        {
            ctx = ctx.to_builder().user_id(root_user_id).build();
        }

        // 调用 RuntimeDomain 唤醒 Agent
        let awaken_result = match self
            .runtime_domain
            .awakening()
            .awaken(ctx.clone(), &agent, message, &thinking_options)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                // 模型调用类错误（429 限流 / 5xx / 鉴权 / 内容过滤等）：
                // 属于不可靠重试的故障，直接记录并通知用户，ack 不再重投，
                // 避免 AOP worker 无限 nack 重试造成"重试雪崩"。
                // 是否重试由业务层（本消费者）决定——此处选择不重试、及时告知用户。
                if e.is_model_error() {
                    AgentRuntimeStateManager::global().set_idle(agent_id);
                    log_error!(
                        &ctx,
                        "handle_agent_message",
                        "Agent {} awaken failed (model error, will notify user & ack): {}",
                        agent_id,
                        e
                    );
                    if let Err(notify_err) = self
                        .notify_agent_failure(
                            &ctx,
                            message,
                            agent_id,
                            &e,
                            work_root_user_id.clone(),
                        )
                        .await
                    {
                        log_warn!(
                            &ctx,
                            "handle_agent_message",
                            "通知用户 Agent 唤醒失败信息失败（不阻塞）: {}",
                            notify_err
                        );
                    }
                    return Ok(());
                }
                // 其他错误保持原有重试语义（nack 重投）
                return Err(e);
            }
        };

        log_info!(
            &ctx,
            "handle_agent_message",
            "Agent {} awakened successfully, trace_ids: {:?}",
            agent_id,
            awaken_result.trace_ids
        );

        // =============== 关键：Framework 主动把 Final 文本转为"对等回复" ===============
        //
        // 设计动机（参见前一版注释）：awaken 返回的 raw_output 过去被直接丢弃，
        // Agent 被迫在 think_loop 里额外调用 send_message 工具才能交付回复，
        // 造成 to_user_id 推导失败 + "必须走工具才算完成任务"的 365 轮死循环。
        // 本段改为 Framework 层按入口消息的来源角色，路由到对应的落库通道。
        //
        // 三路分发规则：
        //   ┌──────────────┬───────────────────────────────────────────────┐
        //   │ from_role=   │ 回复行为                                      │
        //   ├──────────────┼───────────────────────────────────────────────┤
        //   │ User         │ send_to_user(to = message.from_id)            │
        //   │              │ → 正常用户↔Agent 对话，99% 主流场景           │
        //   ├──────────────┼───────────────────────────────────────────────┤
        //   │ Agent        │ send_to_agent(from=本Agent, to=message.from)  │
        //   │              │ → 跨 Agent 协作消息的回复链路，避免把 Agent   │
        //   │              │   ID 硬塞到 to_user_id 里导致投递失败         │
        //   ├──────────────┼───────────────────────────────────────────────┤
        //   │ System       │ 跳过 auto-reply（只记 debug 日志）            │
        //   │              │ → 系统消息通常来自 cron / 取消 / 内部控制，   │
        //   │              │   没有"对等回复对象"；通知用户/Agent 由 Agent │
        //   │              │   在 think_loop 内部按业务语义通过 send_message│
        //   │              │   工具主动选择目标对象                        │
        //   └──────────────┴───────────────────────────────────────────────┘
        //
        // 边界（同上一版）：
        //   - raw_output 为空（Cancel / 纯工具执行任务）不发消息；
        //   - 发送失败仅记 warn，不返回 Err，避免 awaken 侧 nack 重复执行
        //     造成工具副作用 / token 重复消耗；
        //   - send_message 工具仍保留给"不在当前对话中的对象"的异步通知
        //     （跨用户通知、任务完成后台推送、跨项目广播等）。
        let raw_output = awaken_result.raw_output.trim();
        if !raw_output.is_empty() {
            let project_id = message.po.project_id.as_deref();
            let task_id = message.po.task_id.as_deref();
            let reply_to_id = Some(message.po.id.as_str());

            let send_result: Result<()> = match message.from_role() {
                MessageRole::User => self
                    .message_domain
                    .delivery()
                    .send_to_user(
                        ctx.clone(),
                        SendToUserCommand {
                            from_agent_id: agent_id,
                            to_user_id: &message.po.from_id,
                            content: raw_output,
                            project_id,
                            task_id,
                            reply_to_id,
                        },
                    )
                    .await
                    .map(|_| ()),

                MessageRole::Agent => self
                    .message_domain
                    .delivery()
                    .send_to_agent(
                        ctx.clone(),
                        SendToAgentCommand {
                            from_id: agent_id,
                            from_role: MessageRole::Agent,
                            to_agent_id: &message.po.from_id,
                            content: raw_output,
                            project_id,
                            task_id,
                            reply_to_id,
                            attachment_ids: None,
                            message_type: MessageType::Text,
                        },
                    )
                    .await
                    .map(|_| ()),

                MessageRole::System => {
                    // 系统触发（cron/取消/内部控制命令等）：
                    // 没有对等回复目标，Final 文本如需落地请走 send_message 工具
                    // 在 think_loop 里按业务语义指定明确的 to_user_id / to_agent_id。
                    log_debug!(
                        &ctx,
                        "handle_agent_message",
                        "skip auto-reply for system-originated message (from_id={}), final len={}",
                        message.po.from_id,
                        raw_output.len()
                    );
                    Ok(())
                }
            };

            if let Err(e) = send_result {
                log_warn!(
                    &ctx,
                    "handle_agent_message",
                    "auto-reply (role={:?} → to={}) failed (non-fatal, will not retry awaken): {:?}",
                    message.from_role(),
                    message.po.from_id,
                    e
                );
            }
        }

        // P4：A2A 一次性会话回复后自动 complete
        //
        // tasks/send 创建的 project（tags 含 "a2a"、无 task 关联）是一次性联邦请求，
        // 没有看板生命周期；若不收口，对端/外部客户端轮询 tasks/get 永远 working，
        // 永远拿不到终态。Agent 回复产生后（无论发送成败，回复内容已在 messages），
        // 将该 project 流转到 Completed。本地项目会话不受影响（tags 不含 "a2a"）。
        if message.from_role() == MessageRole::User
            && message.po.task_id.is_none()
            && let Some(project_id) = &message.po.project_id
            && let Ok(Some(project)) = self
                .project_domain
                .project_manage()
                .get(ctx.clone(), project_id)
                .await
            && serde_json::from_str::<Vec<String>>(&project.po.tags)
                .map(|tags| tags.iter().any(|t| t == "a2a"))
                .unwrap_or(false)
        {
            match self
                .project_domain
                .project_manage()
                .complete(ctx.clone(), project_id, agent_id.clone())
                .await
            {
                Ok(()) => {
                    log_debug!(
                        &ctx,
                        "handle_agent_message",
                        "A2A one-shot project {} auto-completed after agent reply",
                        project_id
                    );
                }
                Err(e) => {
                    // 收口失败不阻塞：对端轮询端仍能看到回复消息，只是状态停留 working
                    log_warn!(
                        &ctx,
                        "handle_agent_message",
                        "A2A one-shot project {} auto-complete failed: {}",
                        project_id,
                        e
                    );
                }
            }
        }

        Ok(())
    }

    /// 向用户推送 Agent 执行失败通知（如模型调用错误）。
    ///
    /// 仅负责"记录 + 通知"，不阻塞主流程；通知失败仅记日志。
    /// - 用户来源消息：直接通知 `message.from_id`
    /// - Agent/System 来源消息：回退到任务/项目归属用户（root_user_id）
    async fn notify_agent_failure(
        &self,
        ctx: &RequestContext,
        message: &Message,
        agent_id: &str,
        err: &Error,
        fallback_user_id: Option<String>,
    ) -> Result<()> {
        let user_id = match message.from_role() {
            MessageRole::User => message.po.from_id.clone(),
            _ => fallback_user_id.unwrap_or_default(),
        };
        if user_id.is_empty() {
            log_warn!(
                &ctx,
                "notify_agent_failure",
                "无法确定通知对象（agent={}, msg={}），跳过用户通知",
                agent_id,
                message.po.id
            );
            return Ok(());
        }

        // 用户可见文案统一由错误系统（ErrorCode.message）维护：
        // 模型类错误返回人话提示，其它错误回退到错误详情。
        let mut content = format!("⚠️ Agent 执行失败：{}", err.user_message());

        // 中断兜底进度概览：domain 层异常收尾（abort_summary）时挂在错误 field 上，
        // 告知用户「做到哪一步、已记入记忆」，避免失败通知显得工作全部白费。
        if let Some(notice) = err
            .field()
            .and_then(|f| {
                f.extra
                    .get(crate::service::domain::runtime::abort_summary::ABORT_NOTICE_FIELD)
            })
            .and_then(|v| v.as_str())
        {
            content.push_str("\n\n");
            content.push_str(notice);
        }

        self.message_domain
            .delivery()
            .send_to_user(
                ctx.clone(),
                SendToUserCommand {
                    from_agent_id: agent_id,
                    to_user_id: &user_id,
                    content: &content,
                    project_id: message.po.project_id.as_deref(),
                    task_id: message.po.task_id.as_deref(),
                    reply_to_id: None,
                },
            )
            .await?;
        Ok(())
    }

    /// User 消息处理：调用 MessageDomain 推送给用户
    async fn handle_user_message(&self, ctx: &RequestContext, message: &Message) -> Result<()> {
        let ctx = self.rebuild_context(message, ctx);
        let cmd = DeliverMessageCommand {
            message,
            user_id: &message.po.to_id,
        };
        let result = self
            .message_domain
            .delivery()
            .deliver_message(ctx, cmd)
            .await?;

        sys_debug!(
            "user message delivered: sse={}, channels={}/{}",
            result.sse_delivered,
            result.success,
            result.total
        );

        // 修复：所有渠道投递失败时返回错误，触发 nack 重试
        // 之前即使 success=0 也返回 Ok(())，消息被 ack 标记为 Processed，永远不会重试
        if result.success == 0 && result.sse_delivered == 0 {
            return Err(Error::internal(format!(
                "All delivery channels failed for message {}, will retry",
                message.po.id
            )));
        }

        Ok(())
    }

    /// System 消息处理：按类型分发
    async fn handle_system_message(&self, ctx: &RequestContext, message: &Message) -> Result<()> {
        match message.message_type() {
            MessageType::ToolCallRequest => self.handle_tool_call_request(ctx, message).await,
            _ => {
                sys_debug!("system message processed by system module");
                Ok(())
            }
        }
    }

    /// ToolCallRequest 处理：调用 RuntimeDomain 执行工具，MessageDomain 回写结果
    async fn handle_tool_call_request(
        &self,
        ctx: &RequestContext,
        message: &Message,
    ) -> Result<()> {
        let tool_call = parse_tool_call_request(message)?;
        let args = tool_call.args.unwrap_or(Value::Null);

        // 以框架还原的 ctx 为基底（保留 log_id 等链路标识），再叠加 ToolCallMessage 显式携带的字段
        let mut builder = ctx.to_builder();
        builder = builder.agent_id(tool_call.from_id.clone());
        // ToolCallRequest 一定由 Agent 发起（to_role=System）
        builder = builder.caller_type(CallerType::Agent);
        if let Some(project_id) = &tool_call.project_id {
            builder = builder.project_id(project_id.clone());
        }
        if let Some(task_id) = &tool_call.task_id {
            builder = builder.task_id(task_id.clone());
        }
        if let Some(org_id) = &message.po.organization_id {
            builder = builder.organization_id(org_id.clone());
        }
        // 修复：从 ToolCallMessage 回填 ctx 字段，与同步路径保持一致
        // 之前 from_role=Agent 时 user_id 永远不会被设置，
        // log_id 重新生成与触发轮次断链，model_provider_id / model_name 全部丢失
        if let Some(log_id) = &tool_call.from_log_id {
            builder = builder.log_id(log_id.clone());
        }
        if let Some(user_id) = &tool_call.from_user_id {
            builder = builder.user_id(user_id.clone());
        } else if message.from_role() == MessageRole::User {
            builder = builder.user_id(message.po.from_id.clone());
        }
        if let Some(model_provider_id) = &tool_call.from_model_provider_id {
            builder = builder.model_provider_id(model_provider_id.clone());
        }
        if let Some(model_name) = &tool_call.from_model_name {
            builder = builder.model_name(model_name.clone());
        }
        let ctx = builder.build();

        let execution = self
            .runtime_domain
            .tool_execution()
            .call_manual_tool_for_agent(
                ctx.clone(),
                tool_call.from_id.clone(),
                tool_call.tool_id.clone(),
                args,
            )
            .await;

        let outcome = match execution {
            Ok(execution_result) => ToolCallExecutionOutcome::Success {
                result: execution_result.result,
                result_file_meta: None,
                trace_ref: Some(execution_result.trace_ref),
            },
            Err(err) => {
                let trace_ref = err.field().and_then(|f| f.trace_ref.clone());
                ToolCallExecutionOutcome::Failure {
                    error_message: tool_error_message(&err),
                    trace_ref,
                }
            }
        };

        self.message_domain
            .delivery()
            .send_tool_call_result(
                ctx,
                SendToolCallResultCommand {
                    request_message: message,
                    outcome,
                },
            )
            .await?;

        Ok(())
    }

    /// 从 MessagePo + 传入的基础 ctx 重建 RequestContext
    ///
    /// 以框架从事件还原的 `base` ctx 为基底（保留 log_id 等链路标识），
    /// 叠加 message 派生的业务字段：
    /// - organization_id（消息归属组织）
    /// - caller_type 根据 message.from_role() 推断（User/Agent/System）
    /// - user_id（User 消息时取 from_id）
    /// - project_id / task_id / agent_id（to_id）
    ///
    /// 关键：保留 base 中的 log_id，从而把“用户发消息 → Agent 回复”整条链路
    /// 串联到同一个 log_id，解决消费侧丢失调度 ID 的问题。
    fn rebuild_context(&self, message: &Message, base: &RequestContext) -> RequestContext {
        let mut builder = base.to_builder();

        if let Some(org_id) = &message.po.organization_id {
            builder = builder.organization_id(org_id.clone());
        }

        // 根据 from_role 设置 caller_type 和 user_id
        let from_role = message.from_role();
        builder = builder.caller_type(match from_role {
            MessageRole::User => CallerType::User,
            MessageRole::Agent => CallerType::Agent,
            MessageRole::System => CallerType::System,
        });
        if from_role == MessageRole::User {
            builder = builder.user_id(message.po.from_id.clone());
        }

        if let Some(project_id) = &message.po.project_id {
            builder = builder.project_id(project_id.clone());
        }

        if let Some(task_id) = &message.po.task_id {
            builder = builder.task_id(task_id.clone());
        }

        builder = builder.agent_id(message.po.to_id.clone());

        builder.build()
    }
}

// ==================== 辅助函数 ====================

fn parse_tool_call_request(message: &Message) -> Result<ToolCallMessage> {
    if message.message_type() != MessageType::ToolCallRequest {
        return Err(Error::bad_request(format!(
            "expected ToolCallRequest message, got {:?}",
            message.message_type()
        )));
    }

    serde_json::from_str(&message.po.content)
        .map_err(|err| Error::bad_request(format!("invalid ToolCallRequest content: {}", err)))
}

fn tool_error_message(err: &Error) -> String {
    err.msg.clone()
}
