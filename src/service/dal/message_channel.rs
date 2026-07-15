//! 消息渠道 DAL 模块
//!
//! 统一整合：
//! 1. 渠道配置管理（CRUD）
//! 2. 消息分发推送（纯 match 分发到各渠道 DAO）
//!
//! 严格分层：所有 DAO 都是私有字段，不对外暴露。

use std::sync::{Arc, OnceLock};

use common::enums::{ChannelStatus, ChannelType};
use serde::Serialize;

use common::error::{err, bail_err, Result};
use crate::enrich_ctx;
use crate::models::message::Message;
use crate::models::message_channel::MessageChannel;
use crate::pkg::RequestContext;
use crate::service::dao::email::EmailDao;
use crate::service::dao::lark::LarkDao;
use crate::service::dao::message_channel::{MessageChannelDao, MessageChannelQuery};
use crate::service::dao::slack::SlackDao;
use crate::service::dao::webhook::WebhookDao;
use crate::service::dao::wechat::WechatDao;

// ==================== 单例管理 ====================

static MESSAGE_CHANNEL_DAL: OnceLock<Arc<dyn MessageChannelDal>> = OnceLock::new();

/// 获取 MessageChannel DAL 单例
pub fn dal() -> Arc<dyn MessageChannelDal> {
    MESSAGE_CHANNEL_DAL.get().cloned().unwrap()
}

/// 初始化 MessageChannel DAL
pub fn init() {
    use crate::service::dao::message_channel;

    let _ = MESSAGE_CHANNEL_DAL.set(new(message_channel::dao()));
}

/// 创建 MessageChannel DAL（返回 trait 对象，用于测试）
pub fn new(
    message_channel_dao: Arc<dyn MessageChannelDao + Send + Sync>,
) -> Arc<dyn MessageChannelDal> {
    use crate::service::dao::{email, lark, slack, webhook, wechat};

    Arc::new(MessageChannelDalImpl {
        message_channel_dao,
        lark_dao: lark::dao(),
        wechat_dao: wechat::dao(),
        slack_dao: slack::dao(),
        email_dao: email::dao(),
        webhook_dao: webhook::dao(),
    })
}

// ==================== DAL 接口 ====================

/// 消息渠道 DAL 接口
#[async_trait::async_trait]
pub trait MessageChannelDal: Send + Sync {
    // ---------- 配置管理 ----------

    /// 创建渠道
    async fn create_channel(&self, ctx: RequestContext, channel: &MessageChannel) -> Result<()>;

    /// 更新渠道
    async fn update_channel(&self, ctx: RequestContext, channel: &MessageChannel) -> Result<()>;

    /// 删除渠道（软删除）
    async fn delete_channel(&self, ctx: RequestContext, channel_id: &str) -> Result<()>;

    /// 获取单个渠道
    async fn get_channel(
        &self,
        ctx: RequestContext,
        channel_id: &str,
    ) -> Result<Option<MessageChannel>>;

    /// 列出用户的所有渠道
    async fn list_user_channels(
        &self,
        ctx: RequestContext,
        user_id: &str,
        only_enabled: bool,
    ) -> Result<Vec<MessageChannel>>;

    /// 通用查询渠道
    async fn query_channels(
        &self,
        ctx: RequestContext,
        query: MessageChannelQuery,
    ) -> Result<Vec<MessageChannel>>;

    /// 设置渠道状态
    async fn set_channel_status(
        &self,
        ctx: RequestContext,
        channel_id: &str,
        status: ChannelStatus,
    ) -> Result<()>;

    /// 测试渠道连接
    async fn test_channel(&self, ctx: RequestContext, channel_id: &str) -> Result<()>;

    // ---------- 消息分发 ----------

    /// 分发消息到用户所有可用渠道
    ///
    /// # 参数
    /// - `ctx`: 请求上下文
    /// - `message`: 消息实体
    /// - `user_id`: 用户 ID
    ///
    /// # 返回
    /// 分发结果详情，包含各渠道的推送状态
    async fn deliver_message(
        &self,
        ctx: RequestContext,
        message: &Message,
        user_id: &str,
    ) -> Result<DeliveryResult>;
}

// ==================== DAL 实现 ====================

/// 消息渠道 DAL 实现
struct MessageChannelDalImpl {
    /// 渠道配置 DAO（私有）
    message_channel_dao: Arc<dyn MessageChannelDao + Send + Sync>,

    /// 各渠道推送 DAO（私有，不对外暴露）
    lark_dao: Arc<dyn LarkDao + Send + Sync>,
    wechat_dao: Arc<dyn WechatDao + Send + Sync>,
    slack_dao: Arc<dyn SlackDao + Send + Sync>,
    email_dao: Arc<dyn EmailDao + Send + Sync>,
    webhook_dao: Arc<dyn WebhookDao + Send + Sync>,
}

#[async_trait::async_trait]
impl MessageChannelDal for MessageChannelDalImpl {
    // ---------- 配置管理 ----------

    async fn create_channel(&self, ctx: RequestContext, channel: &MessageChannel) -> Result<()> {
        self.message_channel_dao.insert(ctx, &channel.po).await
    }

    async fn update_channel(&self, ctx: RequestContext, channel: &MessageChannel) -> Result<()> {
        self.message_channel_dao.update(ctx, &channel.po).await
    }

    async fn delete_channel(&self, ctx: RequestContext, channel_id: &str) -> Result<()> {
        self.message_channel_dao.delete(ctx, channel_id).await
    }

    async fn get_channel(
        &self,
        ctx: RequestContext,
        channel_id: &str,
    ) -> Result<Option<MessageChannel>> {
        let po = self.message_channel_dao.find_by_id(ctx, channel_id).await?;
        Ok(po.map(MessageChannel::from_po))
    }

    async fn list_user_channels(
        &self,
        ctx: RequestContext,
        user_id: &str,
        only_enabled: bool,
    ) -> Result<Vec<MessageChannel>> {
        let pos = self
            .message_channel_dao
            .list_by_user_id(ctx, user_id, only_enabled)
            .await?;
        Ok(pos.into_iter().map(MessageChannel::from_po).collect())
    }

    async fn query_channels(
        &self,
        ctx: RequestContext,
        query: MessageChannelQuery,
    ) -> Result<Vec<MessageChannel>> {
        let pos = self.message_channel_dao.query(ctx, query).await?;
        Ok(pos.into_iter().map(MessageChannel::from_po).collect())
    }

    async fn set_channel_status(
        &self,
        ctx: RequestContext,
        channel_id: &str,
        status: ChannelStatus,
    ) -> Result<()> {
        self.message_channel_dao
            .set_status(ctx, channel_id, status)
            .await
    }

    async fn test_channel(&self, ctx: RequestContext, channel_id: &str) -> Result<()> {
        let channel = self
            .get_channel(ctx.clone(), channel_id)
            .await?
            .ok_or_else(|| err!(ResourceNotFound, "渠道不存在: {}", channel_id))?;

        // 🎯 核心：纯 match 分发！无 trait！无工厂！
        match channel.channel_type() {
            ChannelType::Lark => self.lark_dao.test_connection(ctx, &channel).await,
            ChannelType::Wechat => self.wechat_dao.test_connection(ctx, &channel).await,
            ChannelType::Slack => self.slack_dao.test_connection(ctx, &channel).await,
            ChannelType::Email => self.email_dao.test_connection(ctx, &channel).await,
            ChannelType::Webhook => self.webhook_dao.test_connection(ctx, &channel).await,
        }
        .map_err(|e| err!(ChannelPushFailed, "push failed: {e}"))
    }

    // ---------- 消息分发 ----------

    async fn deliver_message(
        &self,
        ctx: RequestContext,
        message: &Message,
        user_id: &str,
    ) -> Result<DeliveryResult> {
        let ctx = enrich_ctx!(&ctx, message);
        // 1. 查询用户的所有活跃渠道
        let channels = self
            .message_channel_dao
            .list_by_user_id(ctx.clone(), user_id, true)
            .await?;

        if channels.is_empty() {
            return Ok(DeliveryResult::empty());
        }

        // 2. 逐个渠道推送
        let mut details = Vec::with_capacity(channels.len());

        for po in channels {
            let channel = MessageChannel::from_po(po);
            let result = self.push_to_channel(ctx.clone(), message, &channel).await;

            // 3. 更新渠道推送状态
            let _ = self
                .update_channel_push_status(ctx.clone(), &channel, &result)
                .await;

            details.push(ChannelDeliveryDetail {
                channel_id: channel.id().to_string(),
                channel_type: channel.channel_type(),
                channel_name: channel.po.channel_name.clone(),
                success: result.is_ok(),
                error: result.err().map(|e| e.to_string()),
            });
        }

        Ok(DeliveryResult::from_details(details))
    }
}

// ==================== 私有内部方法 ====================

impl MessageChannelDalImpl {
    /// 🎯 核心分发逻辑（内部私有，不对外暴露）
    ///
    /// 纯 match 分发到各渠道 DAO，无 trait，无工厂，无注册表。
    /// 新增渠道只需要：
    /// 1. 创建新的 DAO 文件
    /// 2. 在 MessageChannelDalImpl 结构体中添加字段
    /// 3. 在这个 match 中加一行
    /// 漏加了？编译直接报错！✅
    async fn push_to_channel(
        &self,
        ctx: RequestContext,
        message: &Message,
        channel: &MessageChannel,
    ) -> std::result::Result<(), common::error::Error> {
        match channel.channel_type() {
            ChannelType::Lark => self.lark_dao.push(ctx, message, channel).await,
            ChannelType::Wechat => self.wechat_dao.push(ctx, message, channel).await,
            ChannelType::Slack => self.slack_dao.push(ctx, message, channel).await,
            ChannelType::Email => self.email_dao.push(ctx, message, channel).await,
            ChannelType::Webhook => self.webhook_dao.push(ctx, message, channel).await,
        }
    }

    /// 更新渠道推送状态（内部私有）
    async fn update_channel_push_status(
        &self,
        ctx: RequestContext,
        channel: &MessageChannel,
        result: &std::result::Result<(), common::error::Error>,
    ) -> Result<()> {
        match result {
            Ok(_) => {
                self.message_channel_dao
                    .mark_push_success(ctx.clone(), channel.id())
                    .await
            }
            Err(e) => {
                let err_msg = e.to_string();
                self.message_channel_dao
                    .mark_push_failed(ctx.clone(), channel.id(), &err_msg)
                    .await
            }
        }
    }
}

// ==================== 分发结果结构体 ====================

/// 消息分发结果
#[derive(Debug, Serialize, Clone)]
pub struct DeliveryResult {
    /// 总渠道数
    pub total: usize,
    /// 成功推送的渠道数
    pub success: usize,
    /// 推送失败的渠道数
    pub failed: usize,
    /// 各渠道的推送详情
    pub details: Vec<ChannelDeliveryDetail>,
    /// SSE 成功推送的连接数
    pub sse_delivered: usize,
}

impl DeliveryResult {
    /// 创建空的分发结果
    pub fn empty() -> Self {
        Self {
            total: 0,
            success: 0,
            failed: 0,
            details: vec![],
            sse_delivered: 0,
        }
    }

    /// 从详情列表创建分发结果
    pub fn from_details(details: Vec<ChannelDeliveryDetail>) -> Self {
        let total = details.len();
        let success = details.iter().filter(|d| d.success).count();
        let failed = total - success;

        Self {
            total,
            success,
            failed,
            details,
            sse_delivered: 0,
        }
    }

    /// 是否全部成功
    pub fn all_success(&self) -> bool {
        self.failed == 0
    }

    /// 是否全部失败
    pub fn all_failed(&self) -> bool {
        self.success == 0
    }
}

/// 单个渠道的推送详情
#[derive(Debug, Serialize, Clone)]
pub struct ChannelDeliveryDetail {
    /// 渠道 ID
    pub channel_id: String,
    /// 渠道类型
    pub channel_type: ChannelType,
    /// 渠道名称
    pub channel_name: String,
    /// 是否推送成功
    pub success: bool,
    /// 错误信息（如果失败）
    pub error: Option<String>,
}
