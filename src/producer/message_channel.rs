use common::api::AgentMatchCriteria;
use common::constants::agent_roles::{
    ROLE_FEISHU_RECEPTION, ROLE_RECEPTION, ROLE_WECHAT_RECEPTION,
};
use common::enums::{CallerType, ChannelType, MessageRole, MessageType};
use common::error::{Result, err};
use std::sync::Arc;

use crate::pkg::RequestContext;
use crate::pkg::adapter::AdaptedMessage;
use crate::pkg::adapter::message::MessageAdapterCallback;
use crate::service::domain::hr::HrDomain;
use crate::service::domain::message::{
    DeliverMessageCommand, DeliveryOptions, MessageDomain, SendToAgentCommand,
};

struct MessageChannelProducer {
    hr_domain: Arc<dyn HrDomain>,
    message_domain: Arc<dyn MessageDomain>,
}

impl MessageChannelProducer {
    pub fn new() -> Self {
        Self {
            hr_domain: crate::service::domain::hr::domain(),
            message_domain: crate::service::domain::message::domain(),
        }
    }
}

/// 渠道类型 → 渠道专属接待角色（档位链第 2 级）
fn reception_role_of(channel_type: ChannelType) -> &'static str {
    match channel_type {
        ChannelType::Lark => ROLE_FEISHU_RECEPTION,
        ChannelType::Wechat => ROLE_WECHAT_RECEPTION,
        _ => ROLE_RECEPTION,
    }
}

impl MessageChannelProducer {
    /// 入站消息的 Agent 路由档位链（有序，首个命中档位胜出，见设计文档 §4.3.4）：
    ///
    /// 1. 渠道显式绑定的 Agent（`by_id`，决定性命中）
    /// 2. 渠道专属接待角色（如 `feishu_reception` / `wechat_reception`）
    /// 3. 通用接待角色 `reception`（兜底，恒存在）
    async fn resolve_target_agent(
        &self,
        ctx: RequestContext,
        msg: &AdaptedMessage,
    ) -> Result<Option<String>> {
        let mut chain = Vec::new();
        if let Some(agent_id) = msg.to_agent_id.as_deref().filter(|s| !s.is_empty()) {
            chain.push(AgentMatchCriteria::by_id(agent_id));
        }
        chain.push(AgentMatchCriteria::by_role(reception_role_of(
            msg.channel_type,
        )));
        chain.push(AgentMatchCriteria::by_role(ROLE_RECEPTION));

        Ok(self
            .hr_domain
            .resolve_agent_multi(ctx, chain)
            .await?
            .map(|agent| agent.po.id))
    }

    /// 补组织上下文（渠道入站链路的必补项）
    ///
    /// 渠道入站的 ctx 只带 `caller_type` + `user_id`，没有组织绑定；而
    /// `send_to_agent` 直接取 `ctx.organization_id()` 落 `messages.organization_id`。
    /// 该列为 NULL 的行会被消息列表的 `AND organization_id = ?` 永久过滤掉
    /// （SQL 里 NULL 不等于任何值）→ 网页端**刷新也看不到**渠道入站消息。
    ///
    /// 组织维度的唯一源头是 UserPo（与唤醒侧 `handle_agent_message` 的兜底同口径）。
    async fn resolve_organization(&self, ctx: &RequestContext, user_id: &str) -> Option<String> {
        if user_id.is_empty() {
            return None;
        }
        match crate::service::domain::organization::domain()
            .user_manage()
            .get_user_by_id(ctx.clone(), user_id)
            .await
        {
            Ok(Some(user)) if !user.organization_id.is_empty() => Some(user.organization_id),
            Ok(_) => None,
            Err(e) => {
                log_warn!(
                    ctx,
                    "message_channel_producer",
                    "resolve organization failed: user_id={} err={}",
                    user_id,
                    e
                );
                None
            }
        }
    }
}

#[async_trait::async_trait]
impl MessageAdapterCallback for MessageChannelProducer {
    async fn on_message(&self, msg: AdaptedMessage) -> Result<()> {
        // 根据 from_role 设置 caller_type 和 user_id
        // 外部渠道消息一般为 User，但保留根据 from_role 推断的灵活性
        let mut builder = RequestContext::builder().caller_type(match msg.from_role {
            MessageRole::User => CallerType::User,
            MessageRole::Agent => CallerType::Agent,
            MessageRole::System => CallerType::System,
        });
        if msg.from_role == MessageRole::User {
            builder = builder.user_id(msg.from_id.clone());
        }
        let ctx = builder.build();

        // 组织上下文兜底：入站链路没有 HTTP 头注入，org 只能按归属用户反查。
        // 缺失会让消息落库成 organization_id = NULL，被列表查询永久过滤。
        let ctx = match ctx.organization_id() {
            Some(_) => ctx,
            None => match self.resolve_organization(&ctx, &msg.from_id).await {
                Some(org_id) => ctx.to_builder().organization_id(org_id).build(),
                None => {
                    log_warn!(
                        &ctx,
                        "message_channel_producer",
                        "channel inbound without organization context: from_user={} channel={:?}",
                        msg.from_id,
                        msg.channel_type
                    );
                    ctx
                }
            },
        };

        let Some(to_agent_id) = self.resolve_target_agent(ctx.clone(), &msg).await? else {
            log_warn!(
                &ctx,
                "message_channel_producer",
                "no available onboarded agent for routing from_user={} channel={:?}",
                msg.from_id,
                msg.channel_type
            );
            return Ok(());
        };

        let cmd = SendToAgentCommand {
            from_id: &msg.from_id,
            from_role: msg.from_role,
            to_agent_id: &to_agent_id,
            content: &msg.content,
            project_id: msg.project_id.as_deref(),
            task_id: msg.task_id.as_deref(),
            reply_to_id: msg.reply_to_id.as_deref(),
            external_key: msg.external_key.as_deref(),
            attachment_ids: None,
            message_type: MessageType::Text,
        };

        let message = self
            .message_domain
            .delivery()
            .send_to_agent(ctx.clone(), cmd)
            .await
            .map_err(|e| {
                err!(
                    Internal,
                    "message channel producer send_to_agent failed from={} to_agent={}: {}",
                    msg.from_id,
                    to_agent_id,
                    e
                )
            })?;

        // 渠道入站消息实时上屏：网页端对它**没有本地乐观气泡**（只有网页自发的
        // 消息才有），不推就永远看不到。出口只开 SSE —— 走渠道投递会把这条消息
        // 回灌回它来的那个渠道，形成回声（用户收到自己刚发的消息）。
        let deliver_cmd = DeliverMessageCommand {
            message: &message,
            user_id: &msg.from_id,
            options: DeliveryOptions::sse_only(),
        };
        if let Err(e) = self
            .message_domain
            .delivery()
            .deliver_message(ctx.clone(), deliver_cmd)
            .await
        {
            // 消息已落库、唤醒已入队，推送失败只留痕不回滚（否则重投会重复唤醒）
            log_warn!(
                &ctx,
                "message_channel_producer",
                "inbound sse push failed: message_id={} err={}",
                message.id(),
                e
            );
        }

        log_info!(
            &ctx,
            "message_channel_producer",
            "message dispatched: from={} to_agent={} channel={:?}",
            msg.from_id,
            to_agent_id,
            msg.channel_type
        );
        Ok(())
    }
}

pub async fn init() -> Result<()> {
    let registry = crate::pkg::adapter::message::registry();

    if registry.is_empty() {
        sys_info!("no message channel adapters registered, skip init");
        return Ok(());
    }

    let producer = Arc::new(MessageChannelProducer::new());
    registry.start_all(producer).await?;

    sys_info!(
        "message channel producers started, total adapters: {}",
        registry.len()
    );
    Ok(())
}

pub async fn shutdown() -> Result<()> {
    let registry = crate::pkg::adapter::message::registry();
    registry.stop_all().await?;
    Ok(())
}
