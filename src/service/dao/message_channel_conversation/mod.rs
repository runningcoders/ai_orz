//! 外部会话 ↔ 内部渠道绑定 DAO（入站路由）
//!
//! 渠道无关的会话路由表：`(channel_type, external_app_id, external_tenant_key,
//! external_chat_id)` 唯一映射到 `channel_id`。入站适配层首次成功解析渠道后
//! upsert，后续同会话消息直接按会话路由，不依赖发送者是否绑定渠道。

use crate::models::message_channel_conversation::MessageChannelConversationPo;
use crate::pkg::RequestContext;
use common::enums::ChannelType;
use common::error::Result;

/// 外部会话绑定 DAO 接口
#[async_trait::async_trait]
pub trait MessageChannelConversationDao: Send + Sync {
    /// 写入或刷新会话绑定（同唯一键覆盖 `channel_id`）
    async fn upsert(
        &self,
        ctx: RequestContext,
        binding: &MessageChannelConversationPo,
    ) -> Result<()>;

    /// 按会话查询绑定的渠道 ID
    async fn find_channel_id(
        &self,
        ctx: RequestContext,
        channel_type: ChannelType,
        app_id: &str,
        tenant_key: &str,
        chat_id: &str,
    ) -> Result<Option<String>>;

    /// 分页查询某渠道绑定的会话（按绑定时间倒序）
    async fn query_by_channel(
        &self,
        ctx: RequestContext,
        channel_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<MessageChannelConversationPo>>;

    /// 统计某渠道绑定的会话总数
    async fn count_by_channel(&self, ctx: RequestContext, channel_id: &str) -> Result<usize>;
}

// ==================== 实现 ====================

mod sqlite;
#[cfg(test)]
mod sqlite_test;

pub use self::sqlite::*;
