//! 外部消息适配层（consumer/adapter）
//!
//! 作为 consumer 层的子模块，通过 `pkg/aop/message_adapter` 中台
//! 统一管理所有外部渠道的入站消息监听。
//!
//! 本模块实现 `MessageAdapterCallback` trait，负责：
//! 1. Agent 路由（渠道未绑定 agent_id 时，调 hr domain 的 resolve_agent 兜底）
//! 2. 调用 MessageDomain 完成内部消息投递
//!
//! 本模块不直接依赖任何具体渠道 DAL，
//! 新增渠道只需 DAL 层注册适配器，consumer 零改动。

use common::error::{err, Result};
use std::sync::Arc;

use crate::pkg::adapter::AdaptedMessage;
use crate::pkg::aop::message_adapter::MessageAdapterCallback;
use crate::pkg::RequestContext;
use crate::service::domain::hr::HrDomain;
use crate::service::domain::message::{MessageDomain, SendToAgentCommand};

/// 消息适配回调实现（consumer 层编排）
///
/// 收到适配后的 `AdaptedMessage` 后：
/// 1. 若 `to_agent_id` 为 `None`，通过 HrDomain.resolve_agent 兜底路由
/// 2. 调用 MessageDomain.send_to_agent 投递消息
struct ConsumerMessageCallback {
    hr_domain: Arc<dyn HrDomain>,
    message_domain: Arc<dyn MessageDomain>,
}

#[async_trait::async_trait]
impl MessageAdapterCallback for ConsumerMessageCallback {
    async fn on_message(&self, msg: AdaptedMessage) -> Result<()> {
        let ctx = RequestContext::new(None, None);

        let to_agent_id = match msg.to_agent_id {
            Some(id) => id,
            None => match self.hr_domain.resolve_agent(ctx.clone()).await? {
                Some(agent) => agent.po.id,
                None => {
                    log_warn!(
                        &ctx,
                        "adapter_dispatch",
                        "no available onboarded agent for routing from_user={}",
                        msg.from_id
                    );
                    return Ok(());
                }
            },
        };

        let cmd = SendToAgentCommand {
            from_id: &msg.from_id,
            from_role: msg.from_role,
            to_agent_id: &to_agent_id,
            content: &msg.content,
            project_id: msg.project_id.as_deref(),
            task_id: msg.task_id.as_deref(),
            reply_to_id: msg.reply_to_id.as_deref(),
            attachment_ids: None,
        };

        self.message_domain
            .delivery()
            .send_to_agent(ctx.clone(), cmd)
            .await
            .map_err(|e| {
                err!(
                    Internal,
                    "adapter dispatch send_to_agent failed from={} to_agent={}: {}",
                    msg.from_id,
                    to_agent_id,
                    e
                )
            })?;

        log_info!(
            &ctx,
            "adapter_dispatch",
            "message dispatched: from={} to_agent={}",
            msg.from_id,
            to_agent_id
        );
        Ok(())
    }
}

/// 初始化外部消息适配层
///
/// 通过 `pkg/aop/message_adapter` 中台统一启动所有已注册的渠道适配器。
/// 各渠道 DAL 在 init 阶段已按自身配置决定是否注册，这里只负责统一启动。
pub async fn init() -> Result<()> {
    let registry = crate::pkg::aop::message_adapter::registry();

    if registry.is_empty() {
        sys_info!("no message adapters registered, skip init");
        return Ok(());
    }

    let hr_domain = crate::service::domain::hr::domain();
    let message_domain = crate::service::domain::message::domain();

    let callback = Arc::new(ConsumerMessageCallback {
        hr_domain,
        message_domain,
    });

    registry.start_all(callback).await?;

    sys_info!(
        "external message adapters started, total: {}",
        registry.len()
    );
    Ok(())
}

/// 关闭外部消息适配层（停止所有事件监听）
pub async fn shutdown() -> Result<()> {
    let registry = crate::pkg::aop::message_adapter::registry();
    registry.stop_all().await?;
    Ok(())
}
