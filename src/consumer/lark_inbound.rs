//! 飞书入站消息消费者
//!
//! 订阅 `lark.inbound.message` 事件（DAO 侧飞书 WS 长连接 adapter 收到
//! `im.message.receive_v1` 后发布），组装两步：message domain 入站适配
//! （协议转换 / 渠道定位 / 用户映射）→ 中台投递回调（producer 路由档位链
//! → send_to_agent）。
//!
//! **Async 模式**：DAO 读循环里只 publish（入队即返回），协议转换 / 渠道查找 /
//! 消息投递都在 AOP worker 线程执行，慢业务不阻塞 WS 收帧。

use async_trait::async_trait;
use common::error::{Error, Result};

use crate::models::events::LarkInboundEvent;
use crate::pkg::RequestContext;
use crate::pkg::aop::{ConsumeMode, Consumer, EventKind};
use crate::service::domain::message::{self as message_domain, InboundSource};

#[derive(Default)]
pub struct LarkInboundConsumer;

impl LarkInboundConsumer {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Consumer for LarkInboundConsumer {
    fn name(&self) -> &str {
        "lark_inbound"
    }

    fn interested_events(&self) -> Vec<EventKind> {
        vec![EventKind::new("lark.inbound.message")]
    }

    fn consume_mode(&self) -> ConsumeMode {
        // 读循环里不能做业务：协议转换 + 渠道查找 + 消息投递都可能在 DB/HTTP 上耗时
        ConsumeMode::Async
    }

    async fn on_event(&self, ctx: RequestContext, event: serde_json::Value) -> Result<()> {
        let event: LarkInboundEvent = serde_json::from_value(event).map_err(|e| {
            Error::internal(format!("failed to deserialize LarkInboundEvent: {}", e))
        })?;

        // 1) 入站适配：domain 门面（枚举收敛各渠道转换）
        let adapted = match message_domain::domain()
            .inbound()
            .adapt_inbound(ctx, InboundSource::Lark(Box::new(event)))
            .await
        {
            Ok(adapted) => adapted,
            Err(e) => {
                // 转换失败仅记录，不向事件管道传播（不 nack 重试）
                log_error!("lark inbound adapt failed: err={}", e);
                return Ok(());
            }
        };

        // 2) 投递：中台登记的回调（producer 路由档位链 → send_to_agent）
        if let Some(msg) = adapted {
            match crate::pkg::adapter::message::registry().current_callback() {
                Some(cb) => cb.on_message(msg).await?,
                None => {
                    log_warn!("lark inbound consumer dropped message: no callback registered")
                }
            }
        }
        Ok(())
    }
}
