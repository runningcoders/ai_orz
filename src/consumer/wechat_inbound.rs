//! 微信入站消息消费者
//!
//! 订阅 `wechat.inbound.message` 事件（DAO 侧 iLink 长轮询 adapter 收到消息后
//! 发布），异步执行原 adapter 桥接链路：
//! `adapt_wechat` 协议转换 → `MessageAdapterCallback` 投递上层（producer 路由）。
//!
//! **Async 模式**：DAO 读循环里只 publish（入队即返回），协议转换 / 渠道查找 /
//! 消息投递都在 AOP worker 线程执行，慢业务不阻塞长轮询收帧。

use std::sync::Weak;

use async_trait::async_trait;
use common::error::{Error, Result};

use crate::models::events::WechatInboundEvent;
use crate::pkg::RequestContext;
use crate::pkg::aop::{ConsumeMode, Consumer, EventKind};
use crate::service::dal::wechat::WechatDalImpl;

pub struct WechatInboundConsumer {
    /// 微信 DAL 实例弱引用（init 时从单例注入，运行期升级）
    wechat_dal: Weak<WechatDalImpl>,
}

impl WechatInboundConsumer {
    pub fn new(wechat_dal: Weak<WechatDalImpl>) -> Self {
        Self { wechat_dal }
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

        let Some(wechat_dal) = self.wechat_dal.upgrade() else {
            log_warn!(
                "wechat inbound consumer dropped message: dal instance gone channel_id={}",
                event.channel_id
            );
            return Ok(());
        };

        // 协议转换（事件过滤 / 渠道定位 / 用户映射都在 DAL 内）
        let adapted = match wechat_dal.adapt_wechat(ctx, &event).await {
            Ok(adapted) => adapted,
            Err(e) => {
                // 转换失败仅记录，不向事件管道传播（与 lark 直调行为一致，不 nack 重试）
                log_error!(
                    "wechat inbound adapt failed: channel_id={} message_key={} err={}",
                    event.channel_id,
                    event.message_key,
                    e
                );
                return Ok(());
            }
        };

        // 投递上层 producer 路由（优先用消费时回调句柄，回落 DAL 注册的句柄）
        if let Some(msg) = adapted {
            match wechat_dal.callback_or_none() {
                Some(cb) => cb.on_message(msg).await?,
                None => log_warn!(
                    "wechat inbound consumer dropped message: no callback registered channel_id={}",
                    event.channel_id
                ),
            }
        }
        Ok(())
    }
}
