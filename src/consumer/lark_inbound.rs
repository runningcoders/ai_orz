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
use crate::pkg::aop::{ConsumeMode, Consumer, Subscription};
use crate::service::domain::message::{self as message_domain, InboundSource};
use common::enums::EventTopic;

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

    fn subscriptions(&self) -> Vec<Subscription> {
        // `.notify_producer()`：P6 —— 适配失败时（尤其是到次数上限被放弃时）需要回调
        // 飞书 DAL 的生产者：框架对 `Discard` 的处理是 ack + 照常回调 `on_consumed`。
        // ⚠️ 注意**不再是**让 DAL 回答「还要不要重投」—— 那是 `decide_retry` 的事（消费者）；
        // 生产者只在 `on_failed` 里接收结论。飞书侧无游标可推进，`on_consumed` 是空操作，
        // 保留它是为了给将来的告警 / 审计留一个明确的落点。
        vec![Subscription::new(EventTopic::LarkInboundMessage).notify_producer()]
    }

    fn consume_mode(&self) -> ConsumeMode {
        // 读循环里不能做业务：协议转换 + 渠道查找 + 消息投递都可能在 DB/HTTP 上耗时
        ConsumeMode::Async
    }

    async fn on_event(&self, ctx: RequestContext, event: serde_json::Value) -> Result<()> {
        let event: LarkInboundEvent = serde_json::from_value(event).map_err(|e| {
            Error::internal(format!("failed to deserialize LarkInboundEvent: {}", e))
        })?;
        // event 随后 move 进适配器；提前留档供过滤分支留痕
        let event_id = event.event.header.event_id.clone();

        // 1) 入站适配：domain 门面（枚举收敛各渠道转换）
        let adapted = match message_domain::domain()
            .inbound()
            .adapt_inbound(ctx, InboundSource::Lark(Box::new(event)))
            .await
        {
            Ok(adapted) => adapted,
            Err(e) => {
                // P6：适配失败**上报 Err**，不再当成功 ack。
                // 改造前这里 `log_error!` 后 `return Ok(())` —— 事件被 ack、不进失败指标、
                // 无任何审计痕迹（消息确定性丢失且不可观测）。
                // 现在由飞书 DAL 的 `on_failed` 判永久/瞬时：永久 → `Discard`
                // （框架记 `on_consume_discarded` 埋点 + error 日志），瞬时 → `Retry` 重投。
                log_error!("lark inbound adapt failed: err={}", e);
                return Err(e);
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
        } else {
            // 适配被过滤（非 P2P/非文本/空内容/重复/未绑定渠道）——此前零日志，
            // 「群里 @ 不理」在默认级别下查无此事，必须留一行 info（含 event_id）
            log_info!(
                "lark inbound adapted to nothing (filtered or deduped): event_id={}",
                event_id
            );
        }
        Ok(())
    }
}
