//! Message Channel 子模块实现
//!
//! 消息渠道配置管理：CRUD + 查询

use crate::models::message_channel::MessageChannel;
use crate::pkg::RequestContext;
use crate::service::domain::finance::FinanceDomainImpl;
use async_trait::async_trait;
use common::error::Result;

/// 为 FinanceDomainImpl 实现 MessageChannelManage
#[async_trait]
impl super::MessageChannelManage for FinanceDomainImpl {
    async fn create_message_channel(
        &self,
        ctx: RequestContext,
        channel: &MessageChannel,
    ) -> Result<()> {
        self.message_channel_dal.create_channel(ctx, channel).await
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
        self.message_channel_dal.update_channel(ctx, channel).await
    }

    async fn delete_message_channel(
        &self,
        ctx: RequestContext,
        channel: &MessageChannel,
    ) -> Result<()> {
        self.message_channel_dal
            .delete_channel(ctx, &channel.po.id)
            .await
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
}
