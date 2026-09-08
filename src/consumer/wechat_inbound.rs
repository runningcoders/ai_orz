//! 微信入站消息消费者
//!
//! 订阅 `wechat.inbound.message` 事件（DAO 侧 iLink 长轮询 adapter 收到消息后
//! 发布），组装两步：message domain 入站适配（协议转换 / 渠道定位 / 用户映射）
//! → 中台投递回调（producer 路由档位链 → send_to_agent）。
//!
//! **Async 模式**：DAO 读循环里只 publish（入队即返回），协议转换 / 渠道查找 /
//! 消息投递都在 AOP worker 线程执行，慢业务不阻塞长轮询收帧。

use async_trait::async_trait;
use common::error::{Error, Result};

use crate::models::events::WechatInboundEvent;
use crate::pkg::RequestContext;
use crate::pkg::aop::{ConsumeMode, Consumer, EventKind};
use crate::service::domain::message::{self as message_domain, InboundSource};

#[derive(Default)]
pub struct WechatInboundConsumer;

impl WechatInboundConsumer {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Consumer for WechatInboundConsumer {
    fn name(&self) -> &str {
        "wechat_inbound"
    }

    fn interested_events(&self) -> Vec<EventKind> {
        vec![EventKind::new("wechat.inbound.message")]
    }

    fn consume_mode(&self) -> ConsumeMode {
        // 读循环里不能做业务：协议转换 + 渠道查找 + 消息投递都可能在 DB/HTTP 上耗时
        ConsumeMode::Async
    }

    async fn on_event(&self, ctx: RequestContext, event: serde_json::Value) -> Result<()> {
        let event: WechatInboundEvent = serde_json::from_value(event).map_err(|e| {
            Error::internal(format!("failed to deserialize WechatInboundEvent: {}", e))
        })?;
        let channel_id = event.channel_id.clone();
        let message_key = event.message_key.clone();

        // 1) 入站适配：domain 门面（枚举收敛各渠道转换）
        let adapted = match message_domain::domain()
            .inbound()
            .adapt_inbound(ctx, InboundSource::Wechat(Box::new(event)))
            .await
        {
            Ok(adapted) => adapted,
            Err(e) => {
                // 转换失败仅记录，不向事件管道传播（与 lark 行为一致，不 nack 重试）
                log_error!(
                    "wechat inbound adapt failed: channel_id={} message_key={} err={}",
                    channel_id,
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
                    "wechat inbound consumer dropped message: no callback registered channel_id={}",
                    channel_id
                ),
            }
        }
        Ok(())
    }
}
