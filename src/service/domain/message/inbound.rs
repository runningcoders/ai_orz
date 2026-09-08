//! 入站消息适配门面——外部渠道事件 → 内部 `AdaptedMessage`
//!
//! 各外部渠道（飞书 WS / 微信 iLink 长轮询）的协议转换在渠道 DAL 内实现
//! （事件过滤 / 渠道定位 / 用户映射），本模块以 [`InboundSource`] 枚举收敛
//! 消费入口：组装层（AOP consumer）不再直捅渠道 DAL，统一经
//! `MessageDomain::inbound().adapt_inbound(...)` 取用。
//!
//! 适配**不做 Agent 路由**：返回的 `AdaptedMessage.to_agent_id` 由渠道绑定
//! 决定，未绑定时为 `None`，路由档位链（组装层职责）在投递回调中完成。

use crate::models::events::{LarkInboundEvent, WechatInboundEvent};
use crate::pkg::RequestContext;
use crate::pkg::adapter::AdaptedMessage;
use common::error::Result;

use super::MessageDomainImpl;

// ==================== 入站源枚举 ====================

/// 外部渠道入站消息源
///
/// 新增外部渠道时：加 variant + 在 [`MessageInboundAdapt`] 的 match 分支
/// 接入对应渠道 DAL 的 adapt 方法，组装层零改动。
#[derive(Debug, Clone)]
pub enum InboundSource {
    /// 飞书 WS 长连事件（按 app_id 定位渠道）
    Lark(Box<LarkInboundEvent>),
    /// 微信 iLink 长轮询事件
    Wechat(Box<WechatInboundEvent>),
}

// ==================== 门面 trait ====================

/// 入站消息适配能力
#[async_trait::async_trait]
pub trait MessageInboundAdapt: Send + Sync {
    /// 外部渠道事件 → 内部 `AdaptedMessage`
    ///
    /// 返回 `Ok(None)` 表示事件被过滤（非私信/非文本/未绑定渠道/内容为空）；
    /// 转换失败返回 `Err`，由调用方决定记录策略。
    async fn adapt_inbound(
        &self,
        ctx: RequestContext,
        source: InboundSource,
    ) -> Result<Option<AdaptedMessage>>;
}

// ==================== 实现 ====================

#[async_trait::async_trait]
impl MessageInboundAdapt for MessageDomainImpl {
    async fn adapt_inbound(
        &self,
        ctx: RequestContext,
        source: InboundSource,
    ) -> Result<Option<AdaptedMessage>> {
        match source {
            InboundSource::Lark(event) => {
                self.lark_dal
                    .adapt_lark(ctx, &event.app_id, &event.event)
                    .await
            }
            InboundSource::Wechat(event) => self.wechat_dal.adapt_wechat(ctx, &event).await,
        }
    }
}
