//! 微信入站消息消费者
//!
//! 订阅 `wechat.inbound.message` 事件（DAO 侧 iLink 长轮询 adapter 收到消息后
//! 发布），组装两步：message domain 入站适配（协议转换 / 渠道定位 / 用户映射）
//! → 中台投递回调（producer 路由档位链 → send_to_agent）。
//!
//! **Async 模式**：DAO 读循环里只 publish（入队即返回），协议转换 / 渠道查找 /
//! 消息投递都在 AOP worker 线程执行，慢业务不阻塞长轮询收帧。

use async_trait::async_trait;
use common::error::{Result, err};

use crate::models::events::WechatInboundEvent;
use crate::pkg::RequestContext;
use crate::pkg::aop::{ConsumeMode, Consumer, Subscription};
use crate::service::domain::message::{self as message_domain, InboundSource};
use common::enums::EventTopic;

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

    fn subscriptions(&self) -> Vec<Subscription> {
        // `.notify_producer()`：消费成功后由微信 DAL（`WechatDalImpl` 的 Producer impl）
        // **推进 opaque 游标**（修 P2）——
        // 不声明它就永不回调，游标只能靠轮询循环自己推进，等于回退到"消费失败也丢消息"。
        vec![Subscription::new(EventTopic::WechatInboundMessage).notify_producer()]
    }

    fn consume_mode(&self) -> ConsumeMode {
        // 读循环里不能做业务：协议转换 + 渠道查找 + 消息投递都可能在 DB/HTTP 上耗时
        ConsumeMode::Async
    }

    async fn on_event(&self, ctx: RequestContext, event: serde_json::Value) -> Result<()> {
        // 封套由本仓自己的序列化产生，反序列化失败即**永久形态**（非瞬时故障）：
        // 报 InvalidRequest 走首败即弃，避免确定性错误空转 8 次重试（指数退避累计约 4 分钟）
        let event: WechatInboundEvent = serde_json::from_value(event).map_err(|e| {
            err!(
                InvalidRequest,
                "failed to deserialize WechatInboundEvent: {}",
                e
            )
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
                // P6：适配失败**上报 Err**，不再当成功 ack。
                // 改造前这里 `log_error!` 后 `return Ok(())` —— 事件被 ack、不进失败指标、
                // 无任何审计痕迹；叠加当时"游标已推进"（P2）= 消息确定性丢失。
                // 重投判定走 Consumer 默认 `decide_retry`（永久错误码表首败即弃，
                // 其余重试至上限）：永久 → `Discard`（框架记 `on_consume_discarded`
                // 埋点），瞬时 → `Retry` 重投（游标不动，AOP 队列兜底）。
                log_error!(
                    "wechat inbound adapt failed: channel_id={} message_key={} err={}",
                    channel_id,
                    message_key,
                    e
                );
                return Err(e);
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
        } else {
            // 适配层**有意跳过**（非文本 / 非本渠道 peer / 渠道已停用 / 渠道不存在等）。
            // 事件按成功 ack、游标照常推进——这是预期行为（非文本消息不该重投），
            // 但**必须留痕**：否则「收到帧却无下文」在日志与监控上完全静默，
            // 无法与「消息根本没到 iLink」区分。与 poll_loop 的 batch 日志构成闭环。
            log_info!(
                "wechat inbound adapted to nothing (intentionally skipped): channel_id={} message_key={}",
                channel_id,
                message_key
            );
        }
        Ok(())
    }
}
