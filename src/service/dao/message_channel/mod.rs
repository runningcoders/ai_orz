//! MessageChannel DAO 模块

//! MessageChannel DAO 模块

use common::error::Result;
use crate::models::message_channel::MessageChannelPo;
use crate::pkg::RequestContext;
use common::enums::{ChannelStatus, ChannelType};

/// 消息渠道通用查询条件
///
/// 支持组合查询，所有字段都是 Option：
/// - None 表示不限制该条件
/// - Some(value) 表示必须匹配该值
#[derive(Debug, Clone, Default)]
pub struct MessageChannelQuery {
    /// 按渠道 ID 查询（通常返回单条）
    pub id: Option<String>,
    /// 按组织 ID 查询
    pub org_id: Option<String>,
    /// 按用户 ID 查询
    pub user_id: Option<String>,
    /// 按 Agent ID 查询（用于 Agent 专属渠道）
    pub agent_id: Option<String>,
    /// 按渠道类型查询
    pub channel_type: Option<ChannelType>,
    /// 只查询启用的渠道
    pub only_enabled: bool,
    /// 按状态 IN 查询（支持多选）
    pub status_in: Option<Vec<ChannelStatus>>,
    /// 限制返回条数（分页）
    pub limit: Option<usize>,
    /// 跳过条数（分页）
    pub offset: Option<usize>,
    /// 排序规则，如 "created_at ASC", "created_at DESC"
    pub order_by: Option<String>,
}

// ==================== 接口 ====================

/// MessageChannel DAO 接口
#[async_trait::async_trait]
pub trait MessageChannelDao: Send + Sync {
    /// 插入一条新渠道配置
    async fn insert(&self, ctx: RequestContext, channel: &MessageChannelPo) -> Result<()>;

    /// 更新渠道配置
    async fn update(&self, ctx: RequestContext, channel: &MessageChannelPo) -> Result<()>;

    /// 通用查询方法
    ///
    /// 支持组合查询条件，所有字段都是 Option
    async fn query(
        &self,
        ctx: RequestContext,
        query: MessageChannelQuery,
    ) -> Result<Vec<MessageChannelPo>>;

    /// 统计查询结果数量
    async fn query_count(&self, ctx: RequestContext, query: MessageChannelQuery) -> Result<u64>;

    /// 根据 ID 查找渠道
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<MessageChannelPo>>;

    /// 查询用户的所有渠道
    async fn list_by_user_id(
        &self,
        ctx: RequestContext,
        user_id: &str,
        only_enabled: bool,
    ) -> Result<Vec<MessageChannelPo>>;

    /// 查询用户 + Agent 的渠道（优先 Agent 专属，否则返回用户通用渠道）
    async fn list_by_user_and_agent_id(
        &self,
        ctx: RequestContext,
        user_id: &str,
        agent_id: &str,
        only_enabled: bool,
    ) -> Result<Vec<MessageChannelPo>>;

    /// 设置渠道状态
    async fn set_status(&self, ctx: RequestContext, id: &str, status: ChannelStatus) -> Result<()>;

    /// 删除渠道（软删除，设置状态为 Deleted）
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 标记推送成功（更新 last_error, last_push_at）
    async fn mark_push_success(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 标记推送失败（更新 last_error, last_push_at）
    async fn mark_push_failed(&self, ctx: RequestContext, id: &str, error: &str) -> Result<()>;
}

// ==================== 实现 ====================

mod sqlite;
#[cfg(test)]
mod sqlite_test;

pub use self::sqlite::*;
