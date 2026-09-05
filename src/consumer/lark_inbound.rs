//! 飞书入站消息消费者
//!
//! 订阅 `lark.inbound.message` 事件（DAO 侧飞书 WS 长连接 adapter 收到
//! `im.message.receive_v1` 后发布），异步执行原 `LarkAdapterHandler` 桥接链路：
//! `adapt_lark` 协议转换 → `MessageAdapterCallback` 投递上层（producer 路由）。
//!
//! **Async 模式**：DAO 读循环里只 publish（入队即返回），协议转换 / 渠道查找 /
//! 消息投递都在 AOP worker 线程执行，慢业务不阻塞 WS 收帧。
//! 事件链与原直调行为等价，仅多一层 AOP 解耦。

use std::sync::Weak;

use async_trait::async_trait;
use common::error::{Error, Result};

use crate::models::events::LarkInboundEvent;
use crate::pkg::RequestContext;
use crate::pkg::aop::{ConsumeMode, Consumer, EventKind};
use crate::service::dal::lark::LarkDalImpl;

pub struct LarkInboundConsumer {
    /// 飞书 DAL 实例弱引用（init 时从单例注入，运行期升级）
    lark_dal: Weak<LarkDalImpl>,
}

impl LarkInboundConsumer {
    pub fn new(lark_dal: Weak<LarkDalImpl>) -> Self {
        Self { lark_dal }
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

        let Some(lark_dal) = self.lark_dal.upgrade() else {
            log_warn!(
                "lark inbound consumer dropped message: dal instance gone app_id={}",
                event.app_id
            );
            return Ok(());
        };

        // 协议转换（事件过滤 / 渠道定位 / 用户映射都在 DAL 内）
        let adapted = match lark_dal.adapt_lark(ctx, &event.app_id, &event.event).await {
            Ok(adapted) => adapted,
            Err(e) => {
                // 转换失败仅记录，不向事件管道传播（与原直调行为一致，不 nack 重试）
                log_error!(
                    "lark inbound adapt failed: event_id={} err={}",
                    event.event.header.event_id,
                    e
                );
                return Ok(());
            }
        };

        // 投递上层 producer 路由（优先用消费时回调句柄，回落 DAL 注册的句柄）
        if let Some(msg) = adapted {
            match lark_dal.callback_or_none() {
                Some(cb) => cb.on_message(msg).await?,
                None => log_warn!(
                    "lark inbound consumer dropped message: no callback registered app_id={}",
                    event.app_id
                ),
            }
        }
        Ok(())
    }
}
