//! 邮件入站消息消费者
//!
//! 订阅 `email.inbound.message` 事件（DAO 侧 IMAP 受管轮询收到新邮件后发布，
//! 轮询单元 = 邮箱凭证），组装两步：message domain 入站适配（(credential_id, From)
//! 二维路由定位渠道 / external_key 幂等去重）→ 中台投递回调（producer 路由档位链
//! → send_to_agent）。
//!
//! **Async 模式**：DAO 轮询循环里只 publish（入队即返回），路由 / 去重 /
//! 消息投递都在 AOP worker 线程执行，慢业务不阻塞 IMAP 收帧。

use async_trait::async_trait;
use common::error::{Error, Result};

use crate::models::events::EmailInboundEvent;
use crate::pkg::RequestContext;
use crate::pkg::aop::{ConsumeMode, Consumer, EventKind};
use crate::service::domain::message::{self as message_domain, InboundSource};

#[derive(Default)]
pub struct EmailInboundConsumer;

impl EmailInboundConsumer {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Consumer for EmailInboundConsumer {
    fn name(&self) -> &str {
        "email_inbound"
    }

    fn interested_events(&self) -> Vec<EventKind> {
        vec![EventKind::new("email.inbound.message")]
    }

    fn consume_mode(&self) -> ConsumeMode {
        // 读循环里不能做业务：渠道路由 + external_key 查重 + 消息投递都可能在 DB/HTTP 上耗时
        ConsumeMode::Async
    }

    async fn on_event(&self, ctx: RequestContext, event: serde_json::Value) -> Result<()> {
        let event: EmailInboundEvent = serde_json::from_value(event).map_err(|e| {
            Error::internal(format!("failed to deserialize EmailInboundEvent: {}", e))
        })?;
        let credential_id = event.credential_id.clone();
        let message_key = event.message_key.clone();

        // 1) 入站适配：domain 门面（枚举收敛各渠道转换）
        let adapted = match message_domain::domain()
            .inbound()
            .adapt_inbound(ctx, InboundSource::Email(Box::new(event)))
            .await
        {
            Ok(adapted) => adapted,
            Err(e) => {
                // 转换失败仅记录，不向事件管道传播（与 lark/wechat 行为一致，不 nack 重试）
                log_error!(
                    "email inbound adapt failed: credential_id={} message_key={} err={}",
                    credential_id,
                    message_key,
                    e
                );
                return Ok(());
            }
        };

        // 2) 投递：中台登记的回调（producer 路由档位链 → send_to_agent）
        if let Some(msg) = adapted {
            match crate::pkg::adapter::message::registry().current_callback() {
                Some(cb) => cb.on_message(msg).await?,
                None => log_warn!(
                    "email inbound consumer dropped message: no callback registered credential_id={}",
                    credential_id
                ),
            }
        }
        Ok(())
    }
}
