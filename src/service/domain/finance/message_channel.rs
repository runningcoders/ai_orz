//! Message Channel 子模块实现
//!
//! 消息渠道配置管理：CRUD + 查询 + 飞书渠道生命周期联动

use crate::models::message_channel::MessageChannel;
use crate::pkg::RequestContext;
use crate::service::domain::finance::FinanceDomainImpl;
use async_trait::async_trait;
use common::enums::ChannelType;
use common::error::Result;

impl FinanceDomainImpl {
    /// 渠道落库成功后联动渠道监听（飞书 WS / 微信 iLink 长轮询）
    ///
    /// 建停规则与告警收敛在各渠道 DAL 的 `sync_listener_for_channel`，
    /// Domain 只负责类型判断与触发时机。
    async fn sync_channel_listener(&self, ctx: &RequestContext, channel: &MessageChannel) {
        match channel.channel_type() {
            ChannelType::Lark => {
                if let Some(dal) = &self.lark_channel_dal {
                    dal.sync_listener_for_channel(ctx.clone(), channel).await;
                }
            }
            ChannelType::Wechat => {
                if let Some(dal) = &self.wechat_channel_dal {
                    dal.sync_listener_for_channel(ctx.clone(), channel).await;
                }
            }
            _ => {}
        }
    }

    /// 渠道删除后释放渠道监听（飞书该 app 无其他引用时才真正停连；微信按 channel 停轮询）
    async fn release_channel_listener_after_delete(
        &self,
        ctx: &RequestContext,
        channel: &MessageChannel,
    ) {
        match channel.channel_type() {
            ChannelType::Lark => {
                if let Some(dal) = &self.lark_channel_dal {
                    dal.release_listener_for_channel(ctx.clone(), channel).await;
                }
            }
            ChannelType::Wechat => {
                if let Some(dal) = &self.wechat_channel_dal {
                    dal.release_listener_for_channel(ctx.clone(), channel).await;
                }
            }
            _ => {}
        }
    }
}

/// 为 FinanceDomainImpl 实现 MessageChannelManage
#[async_trait]
impl super::MessageChannelManage for FinanceDomainImpl {
    async fn create_message_channel(
        &self,
        ctx: RequestContext,
        channel: &MessageChannel,
    ) -> Result<()> {
        self.message_channel_dal
            .create_channel(ctx.clone(), channel)
            .await?;
        self.sync_channel_listener(&ctx, channel).await;
        Ok(())
    }

    async fn get_message_channel(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<MessageChannel>> {
        self.message_channel_dal.get_channel(ctx, id).await
    }

    async fn query_channels(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::message_channel::MessageChannelQuery,
    ) -> Result<common::api::PagedResult<MessageChannel>> {
        self.message_channel_dal.query_channels(ctx, query).await
    }

    async fn list_message_channels(&self, ctx: RequestContext) -> Result<Vec<MessageChannel>> {
        // 没有全局 list_all，用 query 替代
        let query = crate::service::dao::message_channel::MessageChannelQuery::default();
        let page = self.message_channel_dal.query_channels(ctx, query).await?;
        Ok(page.items)
    }

    async fn update_message_channel(
        &self,
        ctx: RequestContext,
        channel: &MessageChannel,
    ) -> Result<()> {
        self.message_channel_dal
            .update_channel(ctx.clone(), channel)
            .await?;
        self.sync_channel_listener(&ctx, channel).await;
        Ok(())
    }

    async fn delete_message_channel(
        &self,
        ctx: RequestContext,
        channel: &MessageChannel,
    ) -> Result<()> {
        self.message_channel_dal
            .delete_channel(ctx.clone(), &channel.po.id)
            .await?;
        self.release_channel_listener_after_delete(&ctx, channel)
            .await;
        Ok(())
    }

    async fn test_message_channel(
        &self,
        ctx: RequestContext,
        channel: &MessageChannel,
    ) -> Result<()> {
        self.message_channel_dal
            .test_channel(ctx, &channel.po.id)
            .await
    }

    /// 飞书信道 WS 连接监控快照（信道监控统一经 Domain 聚合；未接入时返回空快照）
    async fn lark_ws_metrics(&self) -> common::api::LarkWsMetrics {
        match &self.lark_channel_dal {
            Some(dal) => dal.listener_stats().await,
            None => common::api::LarkWsMetrics::default(),
        }
    }
}
