pub mod a2a_poll;
pub mod agent_loop_consumer;
pub mod aop_stats_collector;
pub mod aop_stats_hook;
pub mod email_inbound;
pub mod federation_directory;
pub mod federation_inbound_task;
pub mod federation_ws_outbound;
pub mod lark_inbound;
pub mod message;
pub mod message_route_policy;
pub mod scheduler;
pub mod task_event_consumer;
pub mod think_round_stats_consumer;
pub mod tool_exec_log_consumer;
pub mod tool_exec_stats_consumer;
pub mod wechat_inbound;

use common::error::Result;
use std::sync::Arc;

use crate::pkg::RequestContext;
use crate::pkg::aop;

/// 从项目归属用户补齐组织上下文（组织维度的唯一源头是 `UserPo.organization_id`）
///
/// 系统触发链路（Cron 触发器 / 事件消费者）的 ctx 无组织绑定，发出的消息若不落
/// organization_id，消费侧 `rebuild_context` 还原的 ctx 同样缺组织，Agent 后续的
/// 工具调用（如 `list_messages` 按组织过滤）会报「当前请求缺少组织上下文」。
/// 这里以项目 `root_user_id` 的组织回退补齐；ctx 已有组织、项目无归属用户或查询
/// 失败时原样返回，不阻塞主流程。
pub(crate) async fn enrich_org_from_project_user(
    ctx: &RequestContext,
    root_user_id: &str,
) -> RequestContext {
    if ctx.organization_id.is_some() || root_user_id.is_empty() {
        return ctx.clone();
    }
    match crate::service::domain::organization::domain()
        .user_manage()
        .get_user_by_id(ctx.clone(), root_user_id)
        .await
    {
        Ok(Some(user)) if !user.organization_id.is_empty() => ctx
            .to_builder()
            .organization_id(user.organization_id.clone())
            .build(),
        _ => ctx.clone(),
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

pub use aop_stats_collector::{
    AopDistributionItem, AopOverview, AopStatsCollector, AopTimeSeriesPoint,
};
pub use aop_stats_hook::AopStatsHook;
