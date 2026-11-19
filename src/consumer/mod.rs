pub mod a2a_poll;
pub mod agent_loop_consumer;
pub mod aop_stats_hook;
pub mod email_inbound;
pub mod federation_directory;
pub mod federation_inbound_task;
pub mod federation_ws_outbound;
pub mod lark_inbound;
pub mod message;
pub mod message_route_policy;
pub mod ontology_certify_consumer;
pub mod scheduler;
pub mod task_completion_consumer;
pub mod task_dispatch_consumer;
pub mod task_event_consumer;
pub mod task_pending_review_consumer;
pub mod think_round_stats_consumer;
pub mod tool_exec_log_consumer;
pub mod tool_exec_stats_consumer;
pub mod wake_gate_policy;
pub mod wechat_inbound;

use common::error::Result;
use std::sync::Arc;

use crate::pkg::RequestContext;
use crate::pkg::aop;

/// 按「归属用户」补齐消费侧上下文（用户身份 + 组织）
///
/// 事件载体里 `organization_id` / `user_id` 都可能缺失（cron、系统触发、工具链路），
/// 而两者都是硬依赖：
/// - `user_id` 是工具凭据解析（`resolve_tool_credentials`）、用户画像、
///   `AI_ORZ_USER_ID` 的唯一来源，也是投递时落 `MessagePo::root_user_id` 的取值；
/// - `organization_id` 缺失会让下游按组织过滤的查询直接报「缺少组织上下文」。
///
/// ⚠️ **不要只补组织**：三身份模型下派发者是 Agent / System，`from_id` 不再
/// 兼职携带用户身份，归属用户只有这一个注入点。
///
/// ⚠️ 组织必须按**生效的 user_id** 查，不能直接按 `root_user_id`：ctx 已有
/// `user_id` 且与 `root_user_id` 不同时（入站链路带更准确的绑定），按后者查会得到
/// 「A 的凭据 + B 的组织」。ctx 已有组织 / 无归属用户 / 查询失败都原样返回，
/// 不阻塞主流程。
pub(crate) async fn enrich_org_from_project_user(
    ctx: &RequestContext,
    root_user_id: &str,
) -> RequestContext {
    let mut ctx = ctx.clone();
    let user_id = match ctx.user_id.as_deref() {
        Some(uid) if !uid.is_empty() => uid.to_string(),
        _ if !root_user_id.is_empty() => {
            ctx = ctx.to_builder().user_id(root_user_id.to_string()).build();
            root_user_id.to_string()
        }
        _ => String::new(),
    };
    if ctx.organization_id.is_some() || user_id.is_empty() {
        return ctx;
    }
    match crate::service::domain::organization::domain()
        .user_manage()
        .get_user_by_id(ctx.clone(), &user_id)
        .await
    {
        Ok(Some(user)) if !user.organization_id.is_empty() => ctx
            .to_builder()
            .organization_id(user.organization_id.clone())
            .build(),
        _ => ctx,
    }
}

/// 任务类消息的**派发者身份**（消息三身份之一）
///
/// 三身份正交：派发者（本函数）回答「谁派的」，接收者是 `to_id`，所属用户是
/// `MessagePo::root_user_id`。历史实现把所属用户塞进 `from_id`（用户身份中继），
/// 导致真实派发者不可见、Final 需要改道补丁 —— 本函数是取消中继后的唯一推导点。
///
/// 优先级：
/// 1. `initiator_agent_id`：任务被创建时被唤醒的 Agent，真正派活的人；
/// 2. `root_user_id`：管理面 / 用户直接建的任务，没有 Agent 参与派发，派发者就是用户；
/// 3. 两者皆空（A2A 等无归属场景）→ `system`，Final 无人可投递自然丢弃。
///
/// ⚠️ 不要用「归属用户」兜底有 initiator 的场景：那正是旧中继的行为，会让
/// 执行者的 Final 越过发起者直接到用户。
pub(crate) fn task_sender_identity<'a>(
    initiator_agent_id: &'a str,
    root_user_id: &'a str,
) -> (&'a str, common::enums::MessageRole) {
    use common::enums::MessageRole;
    if !initiator_agent_id.is_empty() {
        (initiator_agent_id, MessageRole::Agent)
    } else if !root_user_id.is_empty() {
        (root_user_id, MessageRole::User)
    } else {
        ("system", MessageRole::System)
    }
}

pub async fn init() -> Result<()> {
    sys_info!("registering business consumers to AOP event center...");

    aop::registry().register_consumer(Arc::new(message::MessageConsumer::new()))?;

    // A2A 远端任务轮询执行：生产者只「认领」（每 30s emit 一次远端 Agent 列表），
    // 真正的远端拉取 / 消息投递在 worker 线程执行（Async + ordered，order_key = agent_id）
    aop::registry().register_consumer(Arc::new(a2a_poll::A2aPollConsumer::new()))?;

    // 飞书入站消息（iLink 之前的 WS 长连事件）：适配走 message domain 门面，
    // 投递回调经中台取用，consumer 不再持有渠道 DAL
    aop::registry().register_consumer(Arc::new(lark_inbound::LarkInboundConsumer::new()))?;

    // 微信入站消息（iLink 长轮询事件）：同上
    aop::registry().register_consumer(Arc::new(wechat_inbound::WechatInboundConsumer::new()))?;

    // 邮件入站消息（IMAP 受管轮询事件，轮询单元 = 邮箱凭证）：同上
    aop::registry().register_consumer(Arc::new(email_inbound::EmailInboundConsumer::new()))?;

    aop::registry().register_consumer(Arc::new(scheduler::CronTriggerConsumer::new()))?;

    aop::registry()
        .register_consumer(Arc::new(tool_exec_log_consumer::ToolExecLogConsumer::new()))?;
    aop::registry().register_consumer(Arc::new(
        tool_exec_stats_consumer::ToolExecStatsConsumer::new(),
    ))?;
    aop::registry().register_consumer(Arc::new(agent_loop_consumer::AgentLoopConsumer::new()))?;
    aop::registry().register_consumer(Arc::new(
        think_round_stats_consumer::ThinkRoundStatsConsumer::new(),
    ))?;
    aop::registry().register_consumer(Arc::new(task_event_consumer::TaskEventConsumer::new()))?;
    // 任务完成交付：Sync 内联落消息（详见 task_completion_consumer 文件头）。
    // ⚠️ 与上面的 Async 消费者订阅同一 topic，职责不重叠：那边只做 DAG 补发。
    aop::registry().register_consumer(Arc::new(
        task_completion_consumer::TaskCompletionConsumer::new(),
    ))?;
    // 任务验收链路：Sync 内联发「请验收」工作单 / 驳回通知。
    // ⚠️ 与上面两个消费者订阅同一 topic，靠 new_status 过滤，职责不重叠。
    aop::registry().register_consumer(Arc::new(
        task_pending_review_consumer::TaskPendingReviewConsumer::new(),
    ))?;
    // 任务派发：Sync 内联落消息，不进内存队列（详见 task_dispatch_consumer 文件头）
    aop::registry()
        .register_consumer(Arc::new(task_dispatch_consumer::TaskDispatchConsumer::new()))?;
    aop::registry().register_consumer(Arc::new(
        ontology_certify_consumer::OntologyCertifyConsumer::new(),
    ))?;
    aop::registry().register_consumer(Arc::new(
        federation_directory::FederationDirectoryConsumer::new(),
    ))?;
    aop::registry().register_consumer(Arc::new(
        federation_inbound_task::FederationInboundTaskConsumer::new(),
    ))?;
    aop::registry().register_consumer(Arc::new(
        federation_ws_outbound::FederationWsOutboundConsumer::new(),
    ))?;

    sys_info!("all business consumers registered");

    Ok(())
}

pub use aop_stats_hook::AopStatsHook;
