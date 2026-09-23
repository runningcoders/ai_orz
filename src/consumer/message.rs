//! Agent 唤醒消费者（业务层）
//!
//! 作为 AOP 事件中心的订阅者，消费两类事件：
//! - `message.created` → 按 `to_role` 分发：Agent → `RuntimeDomain.awaken()`、
//!   User → `MessageDomain.deliver_message()`、System → `RuntimeDomain.tool_execution()`
//! - `agent.settle.requested` → 睡眠沉淀（`settle_agent_exclusive`）
//!
//! 本模块只负责"订阅 + 调度"，业务逻辑通过调用 domain 层完成；与 AOP 框架解耦：
//! AOP 只负责事件流转，本模块负责业务编排。
//!
//! # 为什么沉淀要并到这个消费者里
//!
//! 「唤醒 Agent」和「让 Agent 去睡觉沉淀」是**同一把锁的两端**：两者都会改变同一个
//! Agent 的运行状态（Busy / Resting），因此必须串行。而 AOP 的 `order_key` 串行是
//! **按消费者隔离**的（`registry.queues` 是 `HashMap<consumer_name, EventQueue>`），
//! 拆成两个消费者时同一个 `agent_id` 会落在两条互不知晓的队列里——串行保证失效，
//! 只能靠运行期 `try_set_busy` / `try_set_resting` 抢占失败来兜底，而兜底的代价是
//! 失败重试：日志与失败指标被刷爆、worker 空转。
//!
//! 并成一个消费者后，两类事件共用一条 `order_key = agent_id` 的队列：沉淀在跑时，
//! 发给同一 Agent 的消息**压根不出队**（不用失败、不用重试、不用退避），沉淀 `ack`
//! 之后队列才推进下一条。串行点因此回到队列层——这正是
//! `MessageCreatedEvent::order_key` 注释里早就写明的设计意图。

use async_trait::async_trait;
use common::enums::{CallerType, MessageRole, MessageType};
use common::error::{Error, ErrorCode, Result};
use serde_json::Value;
use std::sync::Arc;

use super::message_route_policy::{
    AutoReplyRoute, MAX_AGENT_REPLY_CHAIN, RouteInput, judge_chain_reply_route,
    judge_static_reply_route,
};

use crate::handlers::hr::agent::settle_memory::{SettleAttempt, settle_agent_exclusive};
use crate::models::events::{AgentSettleEvent, MessageCreatedEvent};
use crate::models::message::{Message, ToolCallMessage};
use crate::pkg::RequestContext;
use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;
use crate::pkg::aop::{ConsumeMode, Consumer, Subscription};
use crate::service::dal::agent::AgentFetchOptions;
use crate::service::dal::message as message_dal;
use crate::service::domain::hr::{self as hr_domain, HrDomain};
use crate::service::domain::message::{
    self as message_domain, DeliverMessageCommand, DeliveryOptions, MessageDomain,
    SendToAgentCommand, SendToUserCommand, SendToolCallResultCommand, ToolCallExecutionOutcome,
};
use crate::service::domain::organization::{self as organization_domain, OrganizationDomain};
use crate::service::domain::project::{self as project_domain, ProjectDomain};
use crate::service::domain::runtime::{
    self as runtime_domain, RuntimeDomain, awakening::ThinkingOptions,
};
use common::enums::EventTopic;

// ==================== 消费者实现 ====================

/// Agent 唤醒消费者
///
/// 订阅 `message.created` 与 `agent.settle.requested`，两者共用一条按 `agent_id`
/// 分片的队列（见模块文档）。作为 AOP 的 Async 消费者，由 Registry 调度器自动轮询拉取。
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

    /// `agent.awakening` 的订阅声明（静态知识，与实例状态无关）
    ///
    /// 两类事件共用一条 `order_key = agent_id` 的队列（见模块文档）—— 这是全项目
    /// **唯一** `concurrency() > 1` 的消费者，也是唯一能观测到 order_key 串行门闩的
    /// 地方，所以必须显式声明 ordered；其余消费者并发 1、天然串行，声明与否无差异。
    ///
    /// 单独抽成关联函数的理由：构造 `MessageConsumer` 会拉起整个 domain 全局单例
    /// （纯单测环境拿不到），而护栏单测只需断言订阅声明本身 → 声明与实例解耦。
    ///
    /// ⚠️ `message.created` 上的 `.notify_producer()` 是**必需**的：
    /// `messages.status` 的翻转（业务收尾）已不在本消费者里做，而由拥有该 topic
    /// 业务状态的对象（`impl Producer for MessageDalImpl`）在 `on_consumed` 里完成。
    /// 漏声明它 = 状态**永不翻转**且不报错。
    ///
    /// ⚠️ `agent.settle.requested` **不声明** `notify_producer`：它没有底层数据要收尾。
    /// 「失败重投到第几次就放弃」由本消费者的 `decide_retry`（默认策略）回答，
    /// 与生产者是否存在无关 —— 别为了次数兜底去造一个空生产者。
    pub fn declarations() -> Vec<Subscription> {
        vec![
            Subscription::new(EventTopic::MessageCreated)
                .ordered()
                .notify_producer(),
            Subscription::new(EventTopic::AgentSettleRequested).ordered(),
        ]
    }
}

#[async_trait]
impl Consumer for MessageConsumer {
    fn name(&self) -> &str {
        "agent.awakening"
    }

    fn subscriptions(&self) -> Vec<Subscription> {
        Self::declarations()
    }

    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Async
    }

    async fn on_event(&self, ctx: RequestContext, event: Value) -> Result<()> {
        // 封套的 `kind` 由框架在 publish 时注入（见 `Registry::publish`），是队列里
        // 唯一能在反序列化**之前**区分事件类型的依据。
        let raw_kind = event
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        match EventTopic::parse(&raw_kind) {
            Some(EventTopic::MessageCreated) => self.handle_message(ctx, event).await,
            Some(EventTopic::AgentSettleRequested) => self.handle_settle_request(ctx, event).await,
            _ => Err(Error::internal(format!(
                "agent.awakening 仅订阅 {} / {}，却收到 {}",
                EventTopic::MessageCreated,
                EventTopic::AgentSettleRequested,
                if raw_kind.is_empty() {
                    "<缺少 kind 字段>"
                } else {
                    raw_kind.as_str()
                }
            ))),
        }
    }

    // ℹ️ 原先的 `ack` / `nack` 已整体删除：
    // - `messages.status → Processed` 搬到了 `impl Producer for MessageDalImpl::on_consumed`；
    // - `messages.status → Pending`（启动恢复的依据）搬到同一个对象的 `on_failed`；
    // - 原先那个 `if source != "message.created" { return }` 的硬编码分流随之消失 ——
    //   归属现在由 `EventTopic::MessageCreated → producer` 索引直接决定，不再靠字符串比较。

    /// 并发 worker 数量
    ///
    /// 两类事件共用它：最多 4 个**不同 Agent** 并行被唤醒/沉淀
    /// （同一 Agent 被 `order_key` 串行挡住）。
    /// 沉淀是「一次完整 LLM 会话」，若模型侧出现限流，再给沉淀分支单独加信号量收口。
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
    /// `message.created`：按 `to_role` 分发到对应 domain
    async fn handle_message(&self, ctx: RequestContext, event: Value) -> Result<()> {
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

    /// `agent.settle.requested`：对指定 Agent 执行一次睡眠沉淀
    ///
    /// 由定时触发器派发（`consumer/scheduler.rs::handle_agent_rest` 只 `publish`），
    /// 因为一次沉淀是完整 LLM 往返（实测数分钟），绝不能在 cron `poll` 线程里同步跑。
    async fn handle_settle_request(&self, ctx: RequestContext, event: Value) -> Result<()> {
        let event: AgentSettleEvent = serde_json::from_value(event).map_err(|e| {
            Error::internal(format!("failed to deserialize agent settle event: {}", e))
        })?;

        log_info!(
            &ctx,
            "handle_settle_request",
            "agent_id={}, 开始沉淀（来源：{}）",
            event.agent_id,
            event.requested_by
        );

        match settle_agent_exclusive(ctx.clone(), &event.agent_id, event.settle_limit).await {
            Ok(SettleAttempt::Settled(count)) => {
                log_info!(
                    &ctx,
                    "handle_settle_request",
                    "agent_id={}, 沉淀完成，处理 {} 条短期记忆（来源：{}）",
                    event.agent_id,
                    count,
                    event.requested_by
                );
                Ok(())
            }
            Ok(SettleAttempt::Busy) => {
                // 本消费者是**唯一**会把 Agent 置为 Busy / Resting 的链路（`awaken` 的唯一
                // 生产调用方），且两类事件同队列同 order_key —— 所以正常路径下这里不可达。
                // 真出现说明有人绕过了消息队列把 Agent 置忙：不静默跳过，上抛 Conflict
                // 交给框架 nack 重投。
                //
                // 反面教训：旧实现在触发器里 `is_unavailable()` 判一下就 `Ok(0)` 跳过，
                // 而触发器已经把 `next_run_at` 推到下一个 cron 点（日触发 = 次日）→
                // 一次跳过等于丢一整天，且界面还显示「已执行」。
                Err(Error::conflict(format!(
                    "Agent {} 忙/休息中，沉淀请求排队等待（来源：{}）",
                    event.agent_id, event.requested_by
                )))
            }
            Err(e) if matches!(e.code_enum(), ErrorCode::ResourceNotFound) => {
                // Agent 已不存在 → 重试不可能成功，ack 丢弃避免空转
                log_warn!(
                    &ctx,
                    "handle_settle_request",
                    "agent_id={}, Agent 不存在，沉淀请求作废（不再重试）: {}",
                    event.agent_id,
                    e
                );
                Ok(())
            }
            Err(e) => {
                // 模型/DB 等临时错误：交给框架 nack 重投
                log_warn!(
                    &ctx,
                    "handle_settle_request",
                    "agent_id={}, 沉淀失败，等待重试: {}",
                    event.agent_id,
                    e
                );
                Err(e)
            }
        }
    }
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

    /// 把框架侧通知（非模型产出）回发给**触发本次唤醒的来源方**。
    ///
    /// 收件人通道必须由**来源方的角色**决定，不能一律按「人」发：
    /// `send_to_user` 落 `to_role=User`、`send_to_agent` 落 `to_role=Agent`，
    /// 而消费端是按 `to_role` 分流的。来源是 Agent 时若写
    /// `send_to_user(to_user_id = from_id)`，就落成 `to_role=User + to_id=<Agent>`
    /// 的死信：投递渠道全找不到人 → `All delivery channels failed` → 重试 8 次
    /// → `DISCARDED`（详见 `delivery.rs::ensure_recipient_role`）。
    ///
    /// Agent 分支用 `AgentNotify`（知会/无需回复）而非 `Text`：本通知是"我已停止"
    /// 的终态播报，对方若按普通消息自动回发，会重新唤醒本 Agent → 再次命中
    /// max_depth → 再次通知，形成 A↔B 无限乒乓。
    async fn notify_message_source(
        &self,
        ctx: &RequestContext,
        message: &Message,
        from_agent_id: &str,
        content: &str,
    ) -> Result<()> {
        let project_id = message.po.project_id.as_deref();
        let task_id = message.po.task_id.as_deref();

        match message.from_role() {
            MessageRole::User => self
                .message_domain
                .delivery()
                .send_to_user(
                    ctx.clone(),
                    SendToUserCommand {
                        from_agent_id,
                        to_user_id: &message.po.from_id,
                        content,
                        project_id,
                        task_id,
                        reply_to_id: None,
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
                        from_id: from_agent_id,
                        from_role: MessageRole::Agent,
                        to_agent_id: &message.po.from_id,
                        content,
                        project_id,
                        task_id,
                        reply_to_id: None,
                        external_key: None,
                        attachment_ids: None,
                        message_type: MessageType::AgentNotify,
                    },
                )
                .await
                .map(|_| ()),

            // 系统 / 自触发没有"对等来源方"，回给自己只会造自唤醒循环
            MessageRole::System => Ok(()),
        }
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

                let send_result = self
                    .notify_message_source(
                        &ctx,
                        message,
                        agent_id,
                        &format!(
                            "Agent has reached the maximum thinking depth ({} turns). The task has been stopped to prevent infinite loops.",
                            max_depth
                        ),
                    )
                    .await;

                // 通知失败仅记录警告，不阻塞 Agent 释放 busy / 返回 Ok
                // （thinking depth 是合法停止，通知失败不应触发消息重试）
                if let Err(notify_err) = send_result {
                    log_warn!(
                        &ctx,
                        "handle_agent_message",
                        "通知来源方 Agent 已达最大思考深度失败（不阻塞停止流程）: {}",
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
        // 注入【用户画像】（基础信息 + 自述偏好），构建 awaken 【用户画像】区块。
        // 取值规则见 resolve_profile_user_id；查询失败或用户不存在仅跳过，不阻塞唤醒。
        // 顺带兜底补齐组织上下文：历史遗留 / 部分系统触发链路落库的消息可能没有
        // organization_id，rebuild_context 还原的 ctx 缺组织绑定，Agent 工具调用
        // （如 list_messages 按组织过滤）会报「当前请求缺少组织上下文」——归属用户
        // 的组织是组织维度的唯一源头（仅 UserPo 持有 organization_id）。
        if let Some(profile_user_id) = resolve_profile_user_id(
            message.from_role(),
            &message.po.from_id,
            work_root_user_id.as_deref(),
        ) {
            match crate::service::domain::organization::domain()
                .user_manage()
                .get_user_by_id(ctx.clone(), &profile_user_id)
                .await
            {
                Ok(Some(user)) => {
                    if ctx.organization_id.is_none() && !user.organization_id.is_empty() {
                        ctx = ctx
                            .to_builder()
                            .organization_id(user.organization_id.clone())
                            .build();
                    }
                    thinking_options = thinking_options.with_user_profile(user);
                }
                Ok(None) => {
                    log_warn!(
                        &ctx,
                        "handle_agent_message",
                        "画像用户 {} 不存在，跳过画像注入",
                        profile_user_id
                    );
                }
                Err(e) => {
                    log_warn!(
                        &ctx,
                        "handle_agent_message",
                        "查询画像用户 {} 失败，跳过画像注入: {}",
                        profile_user_id,
                        e
                    );
                }
            }
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
        // 路由判定收敛在 resolve_auto_reply_route（单一扩展点），四层防线：
        //   ┌────────────────────────┬──────────────────────────────────────────────┐
        //   │ 判定                   │ 路由                                         │
        //   ├────────────────────────┼──────────────────────────────────────────────┤
        //   │ from_role=User         │ Peer → send_to_user(to = message.from_id)，  │
        //   │                        │   正常用户↔Agent 对话，99% 主流场景          │
        //   ├────────────────────────┼──────────────────────────────────────────────┤
        //   │ from_role=System       │ Discard → 系统触发 / 自触发（from==to）没有  │
        //   │ / Agent 自触发         │   "对等回复对象"，回给自己会无限自唤醒循环；  │
        //   │                        │   归属用户需要感知的通知由发送侧以用户身份    │
        //   │                        │   中继（走 User 分支），不依赖这里兜底        │
        //   ├────────────────────────┼──────────────────────────────────────────────┤
        //   │ Agent 知会消息         │ Discard → 发送方经 send_message_to_agent     │
        //   │ (AgentNotify)          │   notify_only=true 声明"无需回复"，处理结果  │
        //   │                        │   不再回发来源方（防乒乓第一道防线）         │
        //   ├────────────────────────┼──────────────────────────────────────────────┤
        //   │ 跨 Agent 协作          │ ① Final 全等 NO_REPLY 哨兵 → Discard         │
        //   │                        │   （Agent 自判"后续工作与来源方无关"）       │
        //   │                        │ ② reply_to 链连续 Agent 消息达上限           │
        //   │                        │   → EscalateToOwner 通知归属用户人工介入     │
        //   │                        │ ③ 否则 Peer → send_to_agent(from=本Agent,    │
        //   │                        │   to=message.from)，避免把 Agent ID 硬塞到   │
        //   │                        │   to_user_id 里导致投递失败                  │
        //   └────────────────────────┴──────────────────────────────────────────────┘
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

            let route = self
                .resolve_auto_reply_route(&ctx, message, agent_id, raw_output)
                .await;

            let send_result: Result<()> = match route {
                AutoReplyRoute::Peer => match message.from_role() {
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
                                external_key: None,
                                attachment_ids: None,
                                message_type: MessageType::Text,
                            },
                        )
                        .await
                        .map(|_| ()),

                    // resolve_auto_reply_route 对 System 来源恒返回 Discard，
                    // 此分支不可达，仅为 match 穷尽性保留
                    MessageRole::System => Ok(()),
                },

                AutoReplyRoute::Discard { reason } => {
                    log_debug!(
                        &ctx,
                        "handle_agent_message",
                        "skip auto-reply ({}), from_id={}, type={:?}, final len={}",
                        reason,
                        message.po.from_id,
                        message.po.message_type,
                        raw_output.len()
                    );
                    Ok(())
                }

                // 防乒乓机械兜底：reply_to 链上连续 Agent 消息已达上限，回发只会
                // 继续推高链深度形成 A↔B 无限乒乓。终止回发并通知归属用户人工介入；
                // 无归属用户（无 task/project root_user_id 的裸协作）时仅记日志。
                AutoReplyRoute::EscalateToOwner { reason } => {
                    log_warn!(
                        &ctx,
                        "handle_agent_message",
                        "auto-reply escalated to owner ({}), from_agent={}, final len={}",
                        reason,
                        message.po.from_id,
                        raw_output.len()
                    );
                    match work_root_user_id.as_deref() {
                        Some(owner_user_id) => self
                            .message_domain
                            .delivery()
                            .send_to_user(
                                ctx.clone(),
                                SendToUserCommand {
                                    from_agent_id: agent_id,
                                    to_user_id: owner_user_id,
                                    content: &format!(
                                        "⚠️ Agent 间自动回复已达链路深度上限（{} 轮），为防止两个 Agent 无限互发已自动中止协作。Agent {} 的最新处理结果未回发，请人工介入确认后续安排。",
                                        MAX_AGENT_REPLY_CHAIN,
                                        message.po.from_id
                                    ),
                                    project_id,
                                    task_id,
                                    reply_to_id,
                                },
                            )
                            .await
                            .map(|_| ()),
                        None => {
                            log_warn!(
                                &ctx,
                                "handle_agent_message",
                                "no owner user to escalate (no task/project root_user_id), agent reply chain stopped silently"
                            );
                            Ok(())
                        }
                    }
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

        // P4/P6：A2A project 按「做完才关」判定收口
        //
        // tasks/send 创建的 project（tags 含 "a2a"）对应对端 A2A task，生命周期映射：
        // - 无关联 task（一次性问答）→ 回复即完成：任务已做完，对端轮询 tasks/get
        //   拿到终态（同步路径依赖此收口）
        // - 有进行中 task（接待员已内部委派）→ 保持 working：project 不实时关闭，
        //   等任务完成/接待侧确认后再收口（P6 生命周期映射；对端持续看到 working
        //   是正确语义，不是缺口）
        // 本地项目会话不受影响（tags 不含 "a2a"）。
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
            let tasks = self
                .project_domain
                .task_manage()
                .list(ctx.clone(), Some(project_id), None, None, None, None)
                .await
                .unwrap_or_default();
            let has_in_flight = tasks.iter().any(|t| {
                matches!(
                    t.po.status,
                    common::enums::TaskStatus::Pending | common::enums::TaskStatus::InProgress
                )
            });
            if has_in_flight {
                log_debug!(
                    &ctx,
                    "handle_agent_message",
                    "A2A project {} has in-flight tasks, keep working (complete when tasks finish)",
                    project_id
                );
            } else {
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
                            "A2A project {} completed after agent reply (no in-flight tasks)",
                            project_id
                        );
                    }
                    Err(e) => {
                        // 收口失败不阻塞：对端轮询端仍能看到回复消息，只是状态停留 working
                        log_warn!(
                            &ctx,
                            "handle_agent_message",
                            "A2A project {} auto-complete failed: {}",
                            project_id,
                            e
                        );
                    }
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

    /// 计算 Final 自动回发的最终路由（静态判定 + 回复链深度机械判定）
    ///
    /// 判定顺序：静态规则（用户/系统/自触发/知会/哨兵）短路在前，
    /// 需要查库的链深度兜底只对「跨 Agent 对等回复候选」执行——
    /// User 消息与 Discard 场景零额外查询，主流路径不增加任何成本。
    async fn resolve_auto_reply_route(
        &self,
        ctx: &RequestContext,
        message: &Message,
        agent_id: &str,
        raw_output: &str,
    ) -> AutoReplyRoute {
        if let Some(route) = judge_static_reply_route(RouteInput {
            from_role: message.from_role(),
            from_id: &message.po.from_id,
            agent_id,
            message_type: message.message_type(),
            raw_output,
        }) {
            return route;
        }
        let chain_depth = self.agent_reply_chain_depth(ctx, message).await;
        judge_chain_reply_route(chain_depth)
    }

    /// 统计入口消息沿 `reply_to_id` 向上连续 Agent 来源消息的条数（含入口自身）
    ///
    /// 链的形态（A↔B 协作）：A→B(1) ← B回复(2) ← A回复(3) ← …，每条自动回复的
    /// reply_to_id 指向触发它的消息。遇到 User/System 来源消息或断链即停止——
    /// 用户介入是链的自然终止点。查询失败按断链处理（宁少勿错：放行 Peer 后
    /// 仍有模型侧哨兵与发送方知会声明继续兜底，不因存储抖动误伤正常协作）。
    async fn agent_reply_chain_depth(&self, ctx: &RequestContext, message: &Message) -> usize {
        let mut depth = usize::from(message.from_role() == MessageRole::Agent);
        let mut cursor = message.po.reply_to_id.clone();
        while depth < MAX_AGENT_REPLY_CHAIN {
            let Some(prev_id) = cursor.as_deref() else {
                break;
            };
            let prev = match self
                .message_domain
                .management()
                .get_by_id(ctx.clone(), prev_id)
                .await
            {
                Ok(Some(m)) => m,
                Ok(None) => break,
                Err(e) => {
                    log_warn!(
                        &ctx,
                        "handle_agent_message",
                        "reply chain traversal failed at message {} (treat as chain end): {}",
                        prev_id,
                        e
                    );
                    break;
                }
            };
            if prev.from_role() != MessageRole::Agent {
                break;
            }
            depth += 1;
            cursor = prev.po.reply_to_id.clone();
        }
        depth
    }

    /// User 消息处理：调用 MessageDomain 推送给用户
    async fn handle_user_message(&self, ctx: &RequestContext, message: &Message) -> Result<()> {
        let ctx = self.rebuild_context(message, ctx);
        let cmd = DeliverMessageCommand {
            message,
            user_id: &message.po.to_id,
            options: DeliveryOptions::default(),
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

/// 推导本次唤醒【用户画像】区块应注入哪个用户
///
/// 画像里带【用户 ID】/【显示名称】/【用户偏好】，直接决定 Agent「在跟谁打交道」的
/// 认知与回复风格；`send_message` 的目标 id 也从这里对齐。取值优先级：
///
/// 1. `from_role == User` → **消息发送者本人**：对话中"正在跟我说话的人"，语义最准确；
/// 2. 其他来源（`System` 后台唤醒 / `Agent` 间协作）→ **本次工作归属用户**
///    （任务 / 项目的 `root_user_id`）。
///
/// 第 2 条是兜底：这两类消息由系统或别的 Agent 构造，提示词里原本不含任何用户信息，
/// Agent 既不知道"这件事为谁负责"，也无从对齐偏好——而它们恰恰是 Agent 需要主动
/// `send_message` 汇报的主力场景（通知类消息的正文 / Final 送达路径见
/// `message_route_policy::judge_static_reply_route` 与 scheduler 的用户身份中继）。
///
/// 返回 `None` 表示不注入（例如 A2A 等无归属用户的项目，`root_user_id` 为空）。
fn resolve_profile_user_id(
    from_role: MessageRole,
    from_id: &str,
    work_root_user_id: Option<&str>,
) -> Option<String> {
    if from_role == MessageRole::User {
        return Some(from_id.to_string());
    }
    work_root_user_id
        .filter(|id| !id.is_empty())
        .map(|id| id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 护栏（§6.1-3）：`agent.awakening` 必须对两个 topic 都声明 `ordered`
    ///
    /// 它是全项目**唯一** `concurrency() > 1` 的消费者 —— 也是唯一能观测到
    /// order_key 串行门闩的地方。漏声明 ordered 会静默退回「两个 kind 在同一
    /// Agent 上并发执行」的形态（上一次事故的形状，且不报错、只有日志在飙）。
    #[test]
    fn awakening_declares_ordered_for_both_topics() {
        let subs = MessageConsumer::declarations();
        assert_eq!(
            subs.len(),
            2,
            "agent.awakening 应恰好订阅 message.created + agent.settle.requested"
        );
        assert!(
            subs.iter().any(|s| s.kind == EventTopic::MessageCreated),
            "必须订阅 message.created"
        );
        assert!(
            subs.iter()
                .any(|s| s.kind == EventTopic::AgentSettleRequested),
            "必须订阅 agent.settle.requested"
        );
        for sub in &subs {
            assert!(
                sub.ordered,
                "{} 必须声明 ordered —— 否则 order_key 串行门闩静默失效",
                sub.kind
            );
        }
    }

    /// 画像取值：User 消息恒用发送者本人
    #[test]
    fn profile_user_id_prefers_message_sender() {
        assert_eq!(
            resolve_profile_user_id(MessageRole::User, "user-sender", Some("user-owner"))
                .as_deref(),
            Some("user-sender")
        );
        // 发送者即归属用户时也走同一条路（不依赖 work_root_user_id）
        assert_eq!(
            resolve_profile_user_id(MessageRole::User, "user-1", None).as_deref(),
            Some("user-1")
        );
    }

    /// 画像取值：后台唤醒 / Agent 协作回退到工作归属用户，使主动通知有对象、有偏好可对齐
    #[test]
    fn profile_user_id_falls_back_to_work_owner() {
        assert_eq!(
            resolve_profile_user_id(MessageRole::System, "task-1", Some("user-owner")).as_deref(),
            Some("user-owner")
        );
        assert_eq!(
            resolve_profile_user_id(MessageRole::Agent, "agent-1", Some("user-owner")).as_deref(),
            Some("user-owner")
        );
    }

    /// 无归属用户（如 A2A 项目 root_user_id 为空）→ 不注入画像
    #[test]
    fn profile_user_id_absent_without_work_owner() {
        assert_eq!(
            resolve_profile_user_id(MessageRole::System, "task-1", None),
            None
        );
        assert_eq!(
            resolve_profile_user_id(MessageRole::System, "task-1", Some("")),
            None
        );
    }
}
