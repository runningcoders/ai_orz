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
use common::enums::{CallerType, MessageRole, MessageStatus, MessageType, TaskStatus};
use common::error::{Error, ErrorCode, Result};
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;

use super::message_route_policy::{
    AutoReplyRoute, ReplyObligation, RouteInput, cerebellum_close_route, classify_reply_decision,
    judge_chain_reply_route, judge_static_reply_route,
};
use super::wake_gate_policy::{
    STALE_DEPENDENCY_POLICY_ID, WAKEUP_BUDGET_POLICY_ID, WakeGateInput, judge_wake_gate,
};

use crate::handlers::hr::agent::settle_memory::{SettleAttempt, settle_agent_exclusive};
use crate::models::events::{AgentSettleEvent, MessageCreatedEvent};
use crate::models::message::{Message, ToolCallMessage};
use crate::pkg::RequestContext;
use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;
use crate::pkg::aop::{ConsumeMode, Consumer, Subscription};
use crate::pkg::collaboration_pressure::{ChainHop, ChainPressure, Novelty, PressureConfig};
use crate::service::dal::agent::AgentFetchOptions;
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

/// 协作链回溯的查库硬上限（异常长链 / 环的保护；正常 A↔B 链远小于该值）
const MAX_CHAIN_TRAVERSAL: usize = 32;

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
        let message = match self
            .message_domain
            .management()
            .get_by_id(ctx.clone(), &msg_event.message_id)
            .await?
        {
            Some(m) => m,
            None => {
                // `find_by_id` 带 `status != 0` 的软删除过滤 ⇒ 查不到有两种可能：
                // ① 消息已被**撤回**（`Recalled = 0`，逻辑作废）；② 行确实不存在。
                // 撤回是一个**正常结局**且事件仍排在 AOP 队列里，出队即走到此处 ——
                // 必须 ack 跳过（返回 `Ok`），**不能**当「消息不存在」上抛：上抛会让
                // 框架 nack 重投最多 8 次，把该 Agent 的队列堵死（与撤回的初衷完全相反）。
                if self
                    .message_domain
                    .management()
                    .get_by_id_with_recalled(ctx.clone(), &msg_event.message_id)
                    .await?
                    .is_some()
                {
                    log_info!(
                        &ctx,
                        "message_recalled",
                        message_id = %msg_event.message_id,
                        "消息已撤回，出队即跳过（不唤醒 Agent）"
                    );
                    return Ok(());
                }

                return Err(Error::not_found(format!(
                    "Message {} not found",
                    msg_event.message_id
                )));
            }
        };

        // 兜底守卫：撤回态本会被上面的软删除过滤挡掉，此处再判一次，防止将来
        // `find_by_id` 的过滤口径变化，让已撤回的消息悄悄唤醒 Agent。
        if message.po.status == MessageStatus::Recalled {
            log_info!(
                &ctx,
                "message_recalled",
                message_id = %message.po.id,
                "消息已撤回（status=Recalled），跳过唤醒"
            );
            return Ok(());
        }

        // 幂等守卫：已处理完成的消息不再唤醒 Agent。
        //
        // 存在的必要：消息状态是**权威的待办记录**，同一 message_id 的事件可能被
        // 投递多次 —— ① AOP nack 重投；② `scheduler::handle_pending_notification_redelivery`
        // 对「滞留 Pending」的跟进类通知做定时补投（老事件可能仍在队列里等门闩）。
        // 没有这道守卫，补投会变成「同一通知唤醒 Owner 两次」，正文里的
        // 「请先核对是否已处理」根本来不及起作用。
        if message.po.status == MessageStatus::Processed {
            log_debug!(
                &ctx,
                "message_idempotent",
                message_id = %message.po.id,
                agent_id = %message.po.to_id,
                "消息已处理完成（status=Processed），重复投递跳过唤醒"
            );
            return Ok(());
        }

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
                        thread_id: None,
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

        // 加载关联任务实体：① 唤醒门闩的判定输入（任务状态 / 前置依赖）
        // ② 后续 ThinkingOptions 注入 prompt 上下文复用（不重复查询）
        let mut cached_task: Option<crate::models::task::Task> = None;
        if let Some(task_id) = &message.po.task_id {
            match self
                .project_domain
                .task_manage()
                .get(ctx.clone(), task_id)
                .await
            {
                Ok(Some(task)) => cached_task = Some(task),
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

        // ══════ 唤醒前置门闩（策略引擎驱动，判据集中在 wake_gate_policy）══════
        //
        // 三条判据在这里一次性裁决：「任务是否还活着」「前置依赖是否满足」
        // 「单任务唤醒预算是否耗尽」。新增判据只在 `WAKE_GATE_DEFS` 加一条规则，
        // 本分支不需要改动 —— 策略只回答「跳不跳 + 为什么」，副作用留在这里处理。
        let gate_input = self
            .build_wake_gate_input(&ctx, message, &agent, cached_task.as_ref())
            .await;
        if let Some(skip) = judge_wake_gate(gate_input) {
            // 唤醒预算耗尽是「合法停止」，来源方（派发者 Agent 或用户）需要知道
            // 为什么停；其余门闩是「还没轮到 / 已终结」，通知只会制造噪音
            if skip.policy_id == WAKEUP_BUDGET_POLICY_ID {
                let send_result = self
                    .notify_message_source(
                        &ctx,
                        message,
                        agent_id,
                        &format!(
                            "Agent has reached the maximum wakeup count for this task ({}). The task has been stopped to prevent infinite loops.",
                            skip.reason
                        ),
                    )
                    .await;

                // 通知失败仅记录警告，不阻塞 Agent 释放 busy / 返回 Ok
                if let Err(notify_err) = send_result {
                    log_warn!(
                        &ctx,
                        "handle_agent_message",
                        "通知来源方 Agent 已达单任务唤醒次数上限失败（不阻塞停止流程）: {}",
                        notify_err
                    );
                }
            } else if skip.policy_id == STALE_DEPENDENCY_POLICY_ID {
                // 前置被撤销 / 归档 ⇒ 这次等待永远不会结束，属于需要人工介入的
                // 死锁信号，不能混在普通「还没轮到」里被静默吞掉
                log_warn!(
                    &ctx,
                    "handle_agent_message",
                    "Agent {} wake skipped ({}): {}",
                    agent_id,
                    skip.policy_id,
                    skip.reason
                );
            } else {
                log_info!(
                    &ctx,
                    "handle_agent_message",
                    "Agent {} wake skipped ({}): {}",
                    agent_id,
                    skip.policy_id,
                    skip.reason
                );
            }

            // 释放 Busy 状态（awaken 不会被调用，BusyGuard 不会创建）
            AgentRuntimeStateManager::global().set_idle(agent_id);
            return Ok(());
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
        // 项目上下文：缺失/失败都只是降级（不阻断唤醒），但原来的 `if let ... && let Ok(Some(..))`
        // 把 Err 与 None 一起静默吞掉 —— 「置忙后零进展」现场因此少一段关键时间线。
        // 拆开显式留痕：这三次 DB 读正好落在 brain woken 与 awaken 之间。
        if let Some(project_id) = &message.po.project_id {
            log_info!(
                &ctx,
                "handle_agent_message",
                "loading project context, agent={}, project_id={}",
                agent_id,
                project_id
            );
            match self
                .project_domain
                .project_manage()
                .get(ctx.clone(), project_id)
                .await
            {
                Ok(Some(project)) => {
                    if work_root_user_id.is_none() && !project.po.root_user_id.is_empty() {
                        work_root_user_id = Some(project.po.root_user_id.clone());
                    }
                    thinking_options = thinking_options.with_project(project);
                    log_info!(
                        &ctx,
                        "handle_agent_message",
                        "project context loaded, agent={}, project_id={}",
                        agent_id,
                        project_id
                    );
                }
                Ok(None) => {
                    log_warn!(
                        &ctx,
                        "handle_agent_message",
                        "project not found, agent={}, project_id={}, skip project context",
                        agent_id,
                        project_id
                    );
                }
                Err(e) => {
                    log_warn!(
                        &ctx,
                        "handle_agent_message",
                        "load project failed, agent={}, project_id={}, skip project context: {}",
                        agent_id,
                        project_id,
                        e
                    );
                }
            }
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
            log_info!(
                &ctx,
                "handle_agent_message",
                "loading user profile, agent={}, user_id={}",
                agent_id,
                profile_user_id
            );
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

        // ══════ 回复仲裁 + 协作压力（只对 Agent 来源消息）══════
        //
        // 是否回复由 Agent 在思考中自决，Final 是唯一权威：正文即答复，
        // `NO_REPLY` 即放弃。框架**不反查「本轮是否已调过发送工具」**——
        // 那是副作用推断，既分不清「发给了谁」，也会把「已经交代过」和
        // 「还欠一句答复」混为一谈。见 `message_route_policy::ReplyObligation`。
        //
        // 预筛与静态路由同源：用户/系统来源、自触发不注入本区块，主流对话
        // 路径零额外 DB 读。两组行都必须在 awaken **之前**注入，本轮正在思考
        // 的 Agent 才能在决策点看到「要不要回」与「已经来回多少轮」。
        let reply_obligation = ReplyObligation::of(
            message.from_role(),
            &message.po.from_id,
            agent_id,
            message.message_type(),
        );
        // 观测条件按来源类型分两种用途，都只对 Agent 来源生效：
        // - `Expected`：施压/硬墙裁决本身需要链事实（本轮回发要据此路由）；
        // - `NotifyOnly`：不施压（本来就不回），但要识别「本条是不是上游触顶后
        //   收口出来的知会」——那是站到接收方一侧的同一份证据，接收方要靠它
        //   知道「链在此终止、要继续请自己发起新消息」。
        // 知会消息因此也会有一次回溯查库；换来的是收口机制闭环（否则接收方
        // 只看到一条普通知会，不知道链已经到底）。
        let chain_pressure = match reply_obligation {
            Some(_) => {
                let (pressure, chain_messages) = self.collect_chain_pressure(&ctx, message).await;
                let decision = pressure.judge();
                log_info!(
                    &ctx,
                    "handle_agent_message",
                    "collaboration pressure observed: {}",
                    decision.log_line()
                );
                Some((pressure, decision, chain_messages))
            }
            None => None,
        };

        // 顺序固定：先「本条要不要回」（跨轮稳定，利于前缀复用），再「已经
        // 回了多少轮」（每轮变化），最后一行的结论紧贴「请输出本轮结果」。
        let mut turn_guidance = reply_obligation
            .map(ReplyObligation::prompt_lines)
            .unwrap_or_default();
        if let Some((_, decision, _)) = &chain_pressure {
            match reply_obligation {
                // 对端在等答复：压力事实 + 本轮指引（施压 / 硬墙收口提示）。
                Some(ReplyObligation::Expected) => turn_guidance.extend(decision.prompt_lines()),
                // 知会是链的终点：只有「上游触顶收口」这一种情况需要额外说明，
                // 普通知会（对方主动 notify_only）不加噪音。
                Some(ReplyObligation::NotifyOnly) if decision.closes_chain_as_notify() => {
                    turn_guidance.extend(decision.chain_closed_prompt_lines());
                }
                _ => {}
            }
        }
        if !turn_guidance.is_empty() {
            thinking_options = thinking_options.with_turn_guidance_lines(turn_guidance);
        }
        // 压力事实与链内正文一起带到回发判定：正文只在小脑判内容时用，
        // 未触发/无小脑时它一直是未读的 `Option`，零额外成本。
        let (chain_pressure, chain_messages) = match chain_pressure {
            Some((pressure, _, messages)) => (Some(pressure), Some(messages)),
            None => (None, None),
        };

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

        // 本轮唤醒吸收的其他消息：成功后才提交 `Processed`。
        // 收集阶段只读，失败时它们仍是 `Pending`，会各自作为独立唤醒处理；
        // 未提交成功也不会丢消息，只是多跑一次唤醒（幂等守卫兜底）。
        if !awaken_result.injected_ids.is_empty() {
            if awaken_result.cancelled {
                // 用户主动停止：本轮吸收的补充消息没被真正处理完，
                // 保持 Pending 让它们各自触发后续唤醒，不能静默吞掉。
                log_info!(
                    &ctx,
                    "handle_agent_message",
                    "wake cancelled, absorbed messages stay pending: count={}",
                    awaken_result.injected_ids.len()
                );
            } else {
                match self
                    .message_domain
                    .management()
                    .update_status_batch(
                        ctx.clone(),
                        &awaken_result.injected_ids,
                        MessageStatus::Processed,
                    )
                    .await
                {
                    Ok(updated) => log_info!(
                        &ctx,
                        "handle_agent_message",
                        "absorbed messages committed, updated={}, total={}",
                        updated,
                        awaken_result.injected_ids.len()
                    ),
                    // 整批失败时全部保持 Pending：后续会各自作为独立唤醒处理，
                    // 未提交成功也不会丢消息，幂等守卫会跳过已处理的消息。
                    Err(e) => log_warn!(
                        &ctx,
                        "handle_agent_message",
                        "吸收消息批量置 Processed 失败（保持 Pending 交后续唤醒）: count={}, error={}",
                        awaken_result.injected_ids.len(),
                        e
                    ),
                }
            }
        }

        // =============== 关键：Framework 主动把 Final 文本转为"对等回复" ===============
        //
        // 设计动机（参见前一版注释）：awaken 返回的 raw_output 过去被直接丢弃，
        // Agent 被迫在 think_loop 里额外调用 send_message 工具才能交付回复，
        // 造成 to_user_id 推导失败 + "必须走工具才算完成任务"的 365 轮死循环。
        // 本段改为 Framework 层按入口消息的来源角色，路由到对应的落库通道。
        //
        // 路由判定收敛在 resolve_auto_reply_route（单一扩展点），三段判定：
        // 前置（优先级最高，先于下表所有来源规则）：Final 归一化后全等哨兵
        // 词表 → Discard。词表是封闭集合：英文主哨兵 NO_REPLY + 模型自造的
        // 中文等价写法（2026-10-18 实证：前台把哨兵写成「未回复」，英文全等
        // 不命中 → 当真实回复投给财务总监 + 两轮无意义往返）。保留字任何
        // 来源均不投递——任务派发经用户身份中继后 from_role=User，若让 User
        // 分支先命中会把哨兵原文发给用户（2026-10-16 实证：全库 29 条泄漏）。
        //
        // ⚠️ 判定**只看 Final**，不查「本轮是否已经用工具发过消息」：是否回复
        // 是 Agent 在思考中自决的结果，提示词已把义务（`ReplyObligation`）
        // 与证据（压力指标）摆到决策点；用副作用反推会把「发了进度同步但还
        // 欠完整答复」误判成已交付，也会让 Agent 的显式选择失去意义。
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
        //   │ 跨 Agent 协作          │ ① Final 命中 NO_REPLY 哨兵词表 → Discard     │
        //   │                        │   （Agent 自决「本轮不回复」，含中文等价写法）│
        //   │                        │ ② 协作压力 Exhausted（疲劳度/窗口触顶）      │
        //   │                        │   → CloseChainAsNotify：Final 作为知会投给  │
        //   │                        │   对方（信息照常送达）+ 通知归属用户收口    │
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

            // ══════ 小脑判内容（P2-A：链深收敛判定，scene = chain_reply）══════
            //
            // 只有「静态规则未命中的跨 Agent 对等候选」才进这一步；静态命中
            // （哨兵 / 用户 / 系统 / 自触发 / 知会）时小脑零调用、不掺和。
            // Q2 改判（AMan 2026-11-10）：**每次都问**（原门槛已取消）——
            // 「有可能从第二句开始就是废话」，挂门槛会漏掉短链。
            // Q1 绝对护栏：`continue` 只返回 `None`（不覆盖）⇒ 落回下面的静态
            // 判定，`hard_window_depth` 撞满时的 `Escalate` 照旧生效。
            //   close    → 直接收口（复用 CloseChainAsNotify，消息必达语义不变）
            //   continue → 落到原静态判定
            //   降级 / 空输入 / 未触发 → 落到原静态判定（不抛错、不短路）
            //
            // `None` = 不覆盖 ⇒ 下面 `resolve_auto_reply_route` 逐字节保持原样。
            let cerebellum_route = self
                .judge_chain_reply_by_cerebellum(
                    &ctx,
                    message,
                    agent_id,
                    raw_output,
                    chain_pressure.as_ref(),
                    chain_messages.as_deref(),
                    agent.brain.as_ref(),
                    awaken_result.trace_ids.first().map(String::as_str),
                )
                .await;
            let route = match cerebellum_route {
                Some(route) => route,
                None => self.resolve_auto_reply_route(
                    message,
                    agent_id,
                    raw_output,
                    chain_pressure.as_ref(),
                ),
            };
            // 决策留痕：Final 形态 + 义务 + 路由，三件一起打印。后续接小脑时
            // 这份日志就是「证据 → 决策」的天然训练/回放样本。
            log_info!(
                &ctx,
                "handle_agent_message",
                "auto-reply decision: {:?}, obligation={:?}, route={:?}, from_id={}, type={:?}",
                classify_reply_decision(raw_output),
                reply_obligation,
                route,
                message.po.from_id,
                message.po.message_type
            );

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
                                thread_id: None,
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

                // 硬墙收口：把本轮 Final 作为**知会**送达来源方，再通知归属用户。
                //
                // 为什么不是「直接停」：直接丢弃会让最后一轮结论凭空消失，对方
                // 只知道自己发了消息、永远等不到结果。为什么不是「照常回发」：
                // 那会把乒乓推高一跳，正是硬墙要拦的东西。
                //
                // 知会语义（`AgentNotify`）是这里的关键：它让对端的
                // `ReplyObligation` 变成 `NotifyOnly`，静态路由直接 Discard，
                // 链条必然终止；同时对端能在提示词里看到「本链因疲劳度触顶收口」，
                // 确需继续时会**自己主动发新消息**（新链从第 1 跳重新计数），
                // 而不是靠回发本条续命。信息传到、链必断，两者同时成立。
                AutoReplyRoute::CloseChainAsNotify { reason } => {
                    log_warn!(
                        &ctx,
                        "handle_agent_message",
                        "chain closed as notify ({}), from_agent={}, final len={}",
                        reason,
                        message.po.from_id,
                        raw_output.len()
                    );
                    // 收口只对跨 Agent 对等链产出（`judge_chain_reply_route` 的
                    // 前置静态规则已拦下用户/系统/自触发/知会来源），这里仍显式
                    // 校验来源角色：万一路由被误用，宁可不发也不能把 Agent 的
                    // 答复塞进用户通道。
                    let notify_result: Result<()> = if message.from_role() == MessageRole::Agent {
                        self.message_domain
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
                                    thread_id: None,
                                    attachment_ids: None,
                                    // 唯一区别就是这里：Text → AgentNotify
                                    message_type: MessageType::AgentNotify,
                                },
                            )
                            .await
                            .map(|_| ())
                    } else {
                        log_warn!(
                            &ctx,
                            "handle_agent_message",
                            "chain-close notify skipped: source role is {:?}, not Agent",
                            message.from_role()
                        );
                        Ok(())
                    };
                    if let Err(e) = &notify_result {
                        log_warn!(
                            &ctx,
                            "handle_agent_message",
                            "chain-close notify to {} failed: {:?}",
                            message.po.from_id,
                            e
                        );
                    }
                    // 归属用户只做知会，不承载「结果」——结果已经给到对端了。
                    let owner_result = match work_root_user_id.as_deref() {
                        Some(owner_user_id) => self
                            .message_domain
                            .delivery()
                            .send_to_user(
                                ctx.clone(),
                                SendToUserCommand {
                                    from_agent_id: agent_id,
                                    to_user_id: owner_user_id,
                                    content: &format!(
                                        "ℹ️ Agent 间协作连续多轮未引入有效新信息、疲劳度触顶，已把 Agent {} 的最后一轮结果作为**知会**送达对方，该协作链到此终止（对方不会再自动回发）。\n\n如需继续这段讨论，回复本条并补充新信息、或直接让相关 Agent 重新发起新消息即可（新链重新计数）；若无需继续，忽略本条即可。",
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
                                "no owner user to notify chain close (no task/project root_user_id)"
                            );
                            Ok(())
                        }
                    };
                    notify_result.and(owner_result)
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

    /// 组装唤醒门闩的判定输入（事实收敛点：策略侧零查库）
    ///
    /// 依赖判据**只对 TaskAssignment 生效**：普通文本消息（用户针对某任务的追问、
    /// Agent 之间的协作汇报）即使在 DAG 未就绪时也必须送达 —— 门闩管的是
    /// 「要不要因为这条派发开始干活」，不是「能不能说话」。
    async fn build_wake_gate_input(
        &self,
        ctx: &RequestContext,
        message: &Message,
        agent: &crate::models::agent::Agent,
        cached_task: Option<&crate::models::task::Task>,
    ) -> WakeGateInput {
        let mut input = WakeGateInput {
            task_status: None,
            pending_dependencies: Vec::new(),
            stale_dependencies: Vec::new(),
            wakeup_count: None,
            max_wakeups: agent.po.get_runtime_config().max_thinking_depth as u64,
        };

        let Some(task) = cached_task else {
            return input;
        };

        // 任务级判据（终态 / 依赖 / 唤醒预算）**只对执行类消息（TaskAssignment）生效**。
        //
        // 通知类消息天然绑定「刚发生变化的那个任务」：`TaskDispatchNotification` 的
        // task_id 正是刚刚变成 Completed 的任务 ⇒ 若照样喂给 `task_terminal`，等于
        // 100% 自我否决 —— 消息落库后被 ack 成 Processed，Owner 永不唤醒且只有 INFO
        // 日志（见 `consumer/task_event_consumer.rs` 顶部说明）。而这类消息的语义是
        // 「去审视项目全局」，不是「去这个任务上干活」，终态拦它没有意义。
        //
        // 普通对话（用户/Agent 对某任务的追问、汇报）同理：门闩管的是「要不要因为
        // 这条派发开始干活」，不是「能不能说话」。
        if applies_task_scope_gates(message.message_type()) {
            input.task_status = Some(task.po.status);

            let (pending, stale) = self.classify_dependencies(ctx, task).await;
            input.pending_dependencies = pending;
            input.stale_dependencies = stale;

            // 唤醒预算的口径是「该 Agent 在该任务上被唤醒的累计次数」，
            // 只在「这条消息是派给它的活」时成立；通知类的接收方是 Owner，
            // 这不是它的累计口径，用它会误伤
            input.wakeup_count = agent
                .stats
                .as_ref()
                .and_then(|s| s.call_summary.as_ref())
                .map(|summary| summary.total_calls);
        }

        input
    }

    /// 把任务的前置依赖分箱为「尚未完成」与「已失效」
    ///
    /// 口径（与 `task.po.dependencies` 的 DAG 语义一致）：
    /// - `pending`：还没跑到 Completed，这次唤醒应该等 —— 前置完成时会由
    ///   `TaskEventConsumer` 重发 TaskAssignment，不会漏执行
    /// - `stale`：Cancelled / Archived，永远不会 Completed —— 单独分箱是为了在日志里
    ///   显式暴露，而不是混进「还没轮到」里被当成正常排队忽略
    ///
    /// 查不到的依赖 id（已删除 / 脏数据 / 查询失败）一律按 `pending` 处理：
    /// 宁可多等一轮，也不要因为一次查询异常就把 DAG 误判为就绪
    async fn classify_dependencies(
        &self,
        ctx: &RequestContext,
        task: &crate::models::task::Task,
    ) -> (Vec<String>, Vec<String>) {
        let deps = task.po.get_dependencies();
        if deps.is_empty() {
            return (Vec::new(), Vec::new());
        }

        let mut pending = Vec::new();
        let mut stale = Vec::new();
        for dep_id in deps {
            match self
                .project_domain
                .task_manage()
                .get(ctx.clone(), &dep_id)
                .await
            {
                Ok(Some(dep)) => match dep.po.status {
                    TaskStatus::Completed => {}
                    TaskStatus::Cancelled | TaskStatus::Archived => stale.push(dep_id),
                    TaskStatus::Pending | TaskStatus::InProgress => pending.push(dep_id),
                },
                Ok(None) => pending.push(dep_id),
                Err(e) => {
                    log_warn!(
                        &ctx,
                        "classify_dependencies",
                        "前置任务 {} 状态查询失败，按未完成处理（保守）：{}",
                        dep_id,
                        e
                    );
                    pending.push(dep_id);
                }
            }
        }
        (pending, stale)
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

    /// 小脑判「这条新消息还有没有信息量、本轮还需不需要自动回发」
    ///
    /// 返回 `Some(route)` 表示**覆盖**静态路由，两种形态：
    /// - `Close` ⇒ `CloseChainAsNotify`（内容判空，收口）；
    /// - `Continue` ⇒ `Peer`，**跳过静态判定** —— 硬墙降软墙的落点（AMan
    ///   2026-11-10 定谳：判据是内容，跳数只作辅助）。
    ///
    /// `None` = 不覆盖，调用方继续走 `resolve_auto_reply_route`（该函数逐字节
    /// 保持原样，含 `window_depth >= hard_window_depth` 的静态 `Escalate`）。
    ///
    /// 降级边界：静态规则命中 / 压力缺失 / 正文缺失 / 未装配小脑 / 熔断 /
    /// 达到绝对天花板 / 配额耗尽 / 失败 / 超时 / 低置信 / 幻觉选项 ——
    /// 除天花板（结论是「必收口」，走 `Close`）外**全部返回 `None`**，
    /// 一次都不抛错、不短路主流程。绝对天花板在 `chain_reply` 内部已转成
    /// `Close` 结论，因此这里只处理两个有结论的分支。
    #[allow(clippy::too_many_arguments)]
    async fn judge_chain_reply_by_cerebellum(
        &self,
        ctx: &RequestContext,
        message: &Message,
        agent_id: &str,
        raw_output: &str,
        chain_pressure: Option<&ChainPressure>,
        chain_messages: Option<&[runtime_domain::chain_reply::ChainMessage]>,
        brain: Option<&crate::models::brain::Brain>,
        trace_id: Option<&str>,
    ) -> Option<AutoReplyRoute> {
        // 静态规则命中 ⇒ 小脑不掺和（零调用）。这里是**纯函数**再算一次，
        // 只为「要不要进小脑」这一个开关，不改任何既有结论。
        if judge_static_reply_route(RouteInput {
            from_role: message.from_role(),
            from_id: &message.po.from_id,
            agent_id,
            message_type: message.message_type(),
            raw_output,
        })
        .is_some()
        {
            return None;
        }
        let (pressure, messages) = (chain_pressure?, chain_messages?);
        let facts = runtime_domain::chain_reply::ChainPressureFacts {
            chain_depth: pressure.depth(),
            window_depth: pressure.window_depth(),
            fatigue: pressure.fatigue(),
            soft_fatigue: pressure.config().soft_fatigue,
            hard_fatigue: pressure.config().hard_fatigue,
            hard_window_depth: pressure.config().hard_window_depth,
        };
        // 轨迹键：本 run 的第一条 trace（小脑留痕挂在它名下）。缺失时传空串
        // —— `record_cerebellum` 对空键同样只做丢弃，不 panic。
        let verdict = runtime_domain::chain_reply::judge_chain_reply_with_default_dao(
            ctx,
            brain,
            messages,
            &facts,
            trace_id.unwrap_or(""),
        )
        .await?;
        Some(match verdict {
            // 收口：复用现成路径（Final 作为知会送达 + 通知归属用户）
            runtime_domain::chain_reply::ChainReplyVerdict::Close { .. } => {
                cerebellum_close_route(true)?
            }
            // 软墙放行：不落回静态判定，硬墙此时不生效
            runtime_domain::chain_reply::ChainReplyVerdict::Continue { .. } => AutoReplyRoute::Peer,
        })
    }

    /// 计算 Final 自动回发的最终路由（静态判定 + 协作压力机械判定）
    ///
    /// 判定顺序：静态规则（哨兵/用户/系统/自触发/知会）短路在前，协作压力
    /// 裁决只对「跨 Agent 对等回复候选」执行——压力观测已在 awaken 前完成并
    /// 缓存，这里只做纯函数映射，零查库。是否回复是 Agent 的 Final 自决结果，
    /// 路由只负责按它执行，不做副作用推断。
    fn resolve_auto_reply_route(
        &self,
        message: &Message,
        agent_id: &str,
        raw_output: &str,
        chain_pressure: Option<&ChainPressure>,
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
        // 静态未命中 = 跨 Agent 对等候选。压力数据缺失属异常（预筛与静态规则
        // 同源），按「宁少勿错」放行 Peer，不因存储/装配抖动误伤协作。
        chain_pressure
            .map(judge_chain_reply_route)
            .unwrap_or(AutoReplyRoute::Peer)
    }

    /// 沿 `reply_to_id` 回溯观测协作链压力（Agent 来源消息专用）
    ///
    /// 保留旧链深度遍历的「宁少勿错」语义：非 Agent 来源、断链、父消息不存在
    /// 或查询失败即停止；额外加 MAX_CHAIN_TRAVERSAL 硬上限，防异常长链 / 环
    /// 把一次回发判定放大成几十次 DB 读。
    ///
    /// ⚠️ **知会是链的边界**（见 [`Self::is_chain_boundary`]）：回溯到知会父消息
    /// 即停止，不把它与更早的跳计入本次窗口。否则「硬墙收口 → 对端重新主动
    /// 发起新消息」时，新消息的 reply_to_id 仍指向那条知会，回溯会穿过边界把
    /// 旧链的疲劳度整段继承过来，新链第一跳就又撞墙——「要推进请自己开新链」
    /// 的承诺会当场失效，两个 Agent 永远无法重新开始协作。
    async fn collect_chain_pressure(
        &self,
        ctx: &RequestContext,
        message: &Message,
    ) -> (
        ChainPressure,
        Vec<runtime_domain::chain_reply::ChainMessage>,
    ) {
        let mut pressure = ChainPressure::new(PressureConfig::from_system());
        let mut seen_participants: HashSet<String> = HashSet::new();
        let mut hops: Vec<ChainHop> = Vec::new(); // 回溯顺序：新 → 旧
        // 小脑判据的输入正文：与 `hops` **同一趟**收集 —— 这一趟本来就要读每条
        // 消息做 ChainHop 分类，正文已在手里 ⇒ 新增判据零额外 DB 读。
        let mut chain_messages: Vec<runtime_domain::chain_reply::ChainMessage> = Vec::new();
        let mut current = Some(message.clone());

        for _ in 0..MAX_CHAIN_TRAVERSAL {
            let Some(current_message) = current else {
                break;
            };
            // 链断点不入 hops：归一化反转后它会落在链头，observe 返回 false
            // 会让整条链被误截断。User/System 来源是协作链的自然终止点。
            if current_message.from_role() != MessageRole::Agent {
                break;
            }
            hops.push(ChainHop {
                from_agent: true,
                novelty: Self::classify_chain_novelty(&current_message, &mut seen_participants),
            });
            // 与 `hops` 同源同序（新 → 旧），末尾一起归一化为时间正序。
            chain_messages.push(runtime_domain::chain_reply::ChainMessage {
                from_id: current_message.po.from_id.clone(),
                type_label: chain_type_label(current_message.message_type()),
                content: current_message.po.content.clone(),
                created_at: current_message.po.created_at,
            });
            let Some(prev_id) = current_message.po.reply_to_id.as_deref() else {
                break;
            };
            match self
                .message_domain
                .management()
                .get_by_id(ctx.clone(), prev_id)
                .await
            {
                Ok(Some(prev)) => {
                    // 知会是链边界：父消息是知会 ⇒ 当前这条是「对方主动重新
                    // 发起的新话头」，从第 1 跳重新计数，不继承旧链疲劳度。
                    if Self::is_chain_boundary(&prev) {
                        break;
                    }
                    current = Some(prev);
                }
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
            }
        }
        // ⚠️ 回血会清零连续窗口，是有方向的运算。回溯得到的新→旧序列必须
        // 归一化为时间正序再观测，否则「最近回血之后的连续窗口」会算成
        // 「回血之前的旧跳」：硬墙可能永不触发，或误对刚续接的链施压。
        let traversed = hops.len();
        pressure.observe_backward_chain(hops);
        // `hops` 由组件自己归一化方向；正文列表由本处归一化（链头在前）。
        chain_messages.reverse();
        if traversed >= MAX_CHAIN_TRAVERSAL {
            log_warn!(
                &ctx,
                "handle_agent_message",
                "reply chain traversal reached hard cap {} (possible cycle), pressure observation truncated",
                MAX_CHAIN_TRAVERSAL
            );
        }
        (pressure, chain_messages)
    }

    /// 领域对象 → 组件语义标志：结构化新信息锚点的唯一翻译处
    ///
    /// 不做内容相似度 / embedding：工具与任务结果、附件、新参与方已经能
    /// 表达「是否引入新信息」，成本与不确定性都远低于落库后的向量比对。
    ///
    /// `seen_participants` 刻意按**回溯顺序**（新 → 旧）填充：最先见到的两个
    /// 参与方就是当前协作的 A↔B，更早出现的第三方才算「新鲜血液」。若改成
    /// 按时间正序分类，A↔B 里后加入的那一方会被误判成新参与方而多一次回血。
    /// 观测顺序与分类顺序解耦是允许的：`observe_backward_chain` 只负责把
    /// `hops` 反转为时间正序，标志本身在构造时已定。
    fn classify_chain_novelty(
        message: &Message,
        seen_participants: &mut HashSet<String>,
    ) -> Novelty {
        let is_new = seen_participants.insert(message.po.from_id.clone());
        match message.message_type() {
            MessageType::Text | MessageType::AgentNotify => {
                // 前两个参与方就是 A↔B 本身，第三个起才算「新鲜血液」
                if is_new && seen_participants.len() > 2 {
                    Novelty::NewParticipant
                } else {
                    Novelty::None
                }
            }
            _ => Novelty::ToolResult,
        }
    }

    /// 回溯链时的边界判定：遇到它就不再往更早的跳走
    ///
    /// 知会（`AgentNotify`）语义上是**发送方宣告「这一轮到此为止」**，因此它是
    /// 一段对话的终点、也是下一段对话的起点：父消息是知会 ⇒ 当前这条是「对方
    /// 主动重新发起的新话头」，协作窗口与疲劳度都从第 1 跳重新计数。
    ///
    /// 这正是硬墙收口机制能闭环的另一半：没有这条边界，收口后对方按提示词
    /// 「自己主动发新消息」发出的第一跳，会顺着 `reply_to_id` 穿过那条知会、
    /// 把旧链疲劳度整段继承，立刻再次撞墙——「换个新链继续」变成一句空话。
    ///
    /// 只按消息类型判定，不看来源：知会本身不会由用户/系统发出，多一个角色
    /// 判断只会让两处口径有机会漂移。
    fn is_chain_boundary(message: &Message) -> bool {
        message.message_type() == MessageType::AgentNotify
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

/// 消息类型 → 链内判据的中文短标签
///
/// 只给模型看，不参与任何分支判定；未知/新增类型统一落「其它消息」，
/// 避免 common 侧枚举扩充时这里编译不过（判据不需要精确到每一种）。
fn chain_type_label(message_type: MessageType) -> &'static str {
    match message_type {
        MessageType::Text => "文本消息",
        MessageType::ToolCallRequest => "工具调用请求",
        MessageType::ToolCallResult => "工具执行结果",
        MessageType::ConfirmRequest => "确认请求",
        MessageType::ConfirmResponse => "确认回复",
        MessageType::TaskAssignment => "任务分配通知",
        MessageType::TaskDispatchNotification => "任务调度通知",
        MessageType::ProjectFollowupNotification => "项目跟进通知",
        MessageType::AgentNotify => "知会",
        MessageType::Image => "图片消息",
        MessageType::File => "文件消息",
        MessageType::Audio => "音频消息",
        MessageType::Video => "视频消息",
        _ => "其它消息",
    }
}

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

/// 任务级唤醒门闩（任务终态 / 前置依赖 / 唤醒预算）是否适用于这条消息
///
/// 只有「派给某个 Agent 去执行的活」才受这些判据约束。其余类型必须能落地：
/// - `TaskDispatchNotification`：绑定的就是刚刚变成 `Completed` 的那个任务，
///   若也走 `task_terminal`，等于通知 100% 自我否决（消息落库 → ack 成 Processed
///   → Owner 永不唤醒，全程只有 INFO 日志）；
/// - `ProjectFollowupNotification` / 普通对话：语义是「审视全局」或「说话」，
///   不是「去这个任务上干活」，终态与依赖都不该拦。
fn applies_task_scope_gates(message_type: MessageType) -> bool {
    message_type == MessageType::TaskAssignment
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

    /// 护栏：任务级门闩只能拦「派活」，不能拦「通知」与「说话」
    ///
    /// `TaskDispatchNotification` 必然绑定刚完成的任务（`task_id` 指向 Completed），
    /// 一旦被算进 `task_terminal`，Owner 通知链路就是 100% 死的 —— 而失败形态是
    /// 「消息已置 Processed + 一行 INFO 日志」，肉眼看不出来。这条断言把边界钉在
    /// 白名单上：新增类型默认**不受**任务级约束。
    #[test]
    fn task_scope_gates_only_apply_to_task_assignments() {
        assert!(
            applies_task_scope_gates(MessageType::TaskAssignment),
            "TaskAssignment 是执行类消息，必须走任务终态 / 依赖 / 预算门闩"
        );
        for message_type in [
            MessageType::TaskDispatchNotification,
            MessageType::ProjectFollowupNotification,
            MessageType::Text,
            MessageType::AgentNotify,
        ] {
            assert!(
                !applies_task_scope_gates(message_type),
                "{message_type:?} 不该受任务级门闩约束（通知/对话被任务终态拦住 = 静默丢唤醒）"
            );
        }
    }

    /// 知会是链边界：硬墙收口后「自己主动发新消息」必须从第 1 跳重新计数
    ///
    /// 收口机制承诺「想继续就自己开新链」。若回溯不停在知会，新消息的
    /// `reply_to_id` 指向那条知会，疲劳度会整段继承旧链 ⇒ 新链第一跳立刻
    /// 再次撞墙，两个 Agent 永远无法重新开始协作。这条护栏把边界钉在类型上。
    #[test]
    fn notify_is_a_chain_boundary_so_reopened_chain_starts_fresh() {
        let with_type = |message_type: MessageType| {
            Message::from_po(crate::models::message::MessagePo {
                message_type,
                ..Default::default()
            })
        };

        assert!(
            MessageConsumer::is_chain_boundary(&with_type(MessageType::AgentNotify)),
            "知会宣告「本轮到此为止」，必须是边界"
        );
        // 其余类型都不是边界：工具结果 / 任务派发自带回血，纯文本是正常往返
        for message_type in [
            MessageType::Text,
            MessageType::Mixed,
            MessageType::TaskAssignment,
            MessageType::ToolCallResult,
        ] {
            assert!(
                !MessageConsumer::is_chain_boundary(&with_type(message_type)),
                "{message_type:?} 不该切断协作链（会凭空给重复往返洗白疲劳度）"
            );
        }
    }
}
