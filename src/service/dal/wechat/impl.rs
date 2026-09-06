//! 微信 DAL 实现（WechatDalImpl）
//!
//! 实现 [`WechatCredentialDal`] 与 [`WechatListenerDal`] 全部子 trait
//! （由 mod.rs 的总 trait [`WechatDal`] 组合），承载微信 iLink 私信接入场景的
//! 数据访问、消息转换、以及入站长轮询生命周期管理。
//!
//! - `adapt_wechat`：iLink 消息 → 内部 `AdaptedMessage` 转换（不含 Agent 路由）
//! - 监听生命周期：`sync_listener_for_channel` / `rebuild_listeners_for_credential`
//!
//! 作为 `pkg/adapter` 注册中心的适配者，由 consumer 层获取并调用
//! `adapt_wechat`，转换结果交由 message domain 发送。

use std::sync::{Arc, RwLock};

use common::enums::ChannelType;
use common::error::{Result, err};

use crate::models::events::WechatInboundEvent;
use crate::models::message_channel::MessageChannel;
use crate::models::user_credential::UserCredentialPo;
use crate::pkg::RequestContext;
use crate::pkg::adapter::AdaptedMessage;
use crate::pkg::adapter::message::{MessageAdapterCallback, MessageInboundAdapter};
use crate::service::dao::message_channel::MessageChannelQuery;
use crate::service::dao::wechat::WechatDao;
use crate::service::dao::wechat::ilink::IlinkChannelCredentials;

use super::WechatCredentialDal;
use super::WechatListenerDal;

// ==================== 实现 ====================

/// 微信 DAL 实现
///
/// 组合基础 MessageChannelDal + WechatDao + UserCredentialDao，提供微信
/// 私信接入所需的数据访问、消息转换、以及入站长轮询生命周期管理。
pub struct WechatDalImpl {
    /// 基础消息渠道 DAL（渠道配置 + 消息分发）
    message_channel_dal: Arc<dyn crate::service::dal::message_channel::MessageChannelDal>,
    /// 微信 DAO（iLink 协议客户端 + 受管长轮询 registry）
    wechat_dao: Arc<dyn WechatDao>,
    /// 用户凭证 DAO（凭证引用解析：渠道 wechat_credential_id → user_credentials 行）
    credential_dao: Arc<dyn crate::service::dao::user_credential::UserCredentialDao>,
    /// 监听运行状态标记
    running: RwLock<bool>,
    /// 运行期回调句柄（start 时注入，供运行期新建轮询后 consumer 复用）
    callback: RwLock<Option<Arc<dyn MessageAdapterCallback>>>,
}

/// 判断渠道是否开启入站监听（缺省视为开启）
fn listens_inbound(channel: &MessageChannel) -> bool {
    channel.config().wechat_listen_inbound.unwrap_or(true)
}

impl WechatDalImpl {
    /// 创建实例（由 mod.rs `new_with_credential_dao` 包装为 Arc 注入依赖）
    pub fn new(
        message_channel_dal: Arc<dyn crate::service::dal::message_channel::MessageChannelDal>,
        wechat_dao: Arc<dyn WechatDao>,
        credential_dao: Arc<dyn crate::service::dao::user_credential::UserCredentialDao>,
    ) -> Self {
        Self {
            message_channel_dal,
            wechat_dao,
            credential_dao,
            running: RwLock::new(false),
            callback: RwLock::new(None),
        }
    }

    /// 已注册回调句柄（start 后由 registry 注入；未启动时为 None）
    pub(crate) fn callback_or_none(&self) -> Option<Arc<dyn MessageAdapterCallback>> {
        self.callback.read().map(|c| c.clone()).unwrap_or(None)
    }

    fn set_callback(&self, callback: Option<Arc<dyn MessageAdapterCallback>>) {
        if let Ok(mut guard) = self.callback.write() {
            *guard = callback;
        }
    }

    /// 按凭证 ID 加载凭证行（凭证不存在/已软删/查询失败返回 None）
    async fn load_credential_row(
        &self,
        ctx: RequestContext,
        credential_id: &str,
    ) -> Option<UserCredentialPo> {
        match self.credential_dao.find_by_id(ctx, credential_id).await {
            Ok(po) => po,
            Err(e) => {
                log_warn!("wechat credential {} 查询失败: {}", credential_id, e);
                None
            }
        }
    }

    /// 查询全部启用的微信渠道（系统上下文）
    async fn query_enabled_wechat_channels(&self) -> Result<Vec<MessageChannel>> {
        let ctx = RequestContext::new_system();
        let query = MessageChannelQuery {
            channel_type: Some(ChannelType::Wechat),
            only_enabled: true,
            ..Default::default()
        };
        let page = self.message_channel_dal.query_channels(ctx, query).await?;
        Ok(page.items)
    }

    /// 将 iLink 入站消息适配为内部 `AdaptedMessage`
    ///
    /// 适配流程：
    /// 1. 事件过滤：仅处理对端用户（USER）完整（FINISH）文本消息，过滤 BOT 回声
    /// 2. 渠道定位：信封自带 channel_id（轮询按 channel 隔离）直查
    /// 3. peer 校验：渠道 `wechat_peer_id` 未配置时自动回填（首次入站），
    ///    已配置但不一致则跳过（一个 channel = 一个对端）
    /// 4. 用户映射：渠道的 user_id 作为 from_id
    ///
    /// **不做 Agent 路由**：返回的 `AdaptedMessage.to_agent_id` 取渠道显式绑定，
    /// 未绑定时为 `None`，由 producer 层档位链路由。
    ///
    /// 返回 `None` 表示事件被过滤（非文本 / 渠道不可用 / peer 不匹配）。
    pub async fn adapt_wechat(
        &self,
        ctx: RequestContext,
        event: &WechatInboundEvent,
    ) -> Result<Option<AdaptedMessage>> {
        // 1. 事件过滤：仅处理对端发来的完整文本消息
        let message = &event.message;
        if !message.is_user() || !message.is_finished() {
            log_debug!(
                &ctx,
                "wechat_adapt",
                "skip non-user/unfinished message: channel_id={} msg_type={}",
                event.channel_id,
                message.message_type
            );
            return Ok(None);
        }
        let Some(content) = message.text() else {
            log_debug!(
                &ctx,
                "wechat_adapt",
                "skip non-text message: channel_id={} message_key={}",
                event.channel_id,
                event.message_key
            );
            return Ok(None);
        };

        // 2. 渠道定位（信封自带 channel_id，直查）
        let Some(channel) = self
            .message_channel_dal
            .get_channel(ctx.clone(), &event.channel_id)
            .await?
        else {
            log_info!(
                &ctx,
                "wechat_adapt",
                "channel not found: channel_id={} message_key={}",
                event.channel_id,
                event.message_key
            );
            return Ok(None);
        };
        if !channel.is_enabled() {
            log_info!(
                &ctx,
                "wechat_adapt",
                "channel not enabled, skip: channel_id={}",
                event.channel_id
            );
            return Ok(None);
        }

        // 3. peer 校验 + 首次入站自动回填
        let peer = message.from_user_id.clone();
        match channel
            .config()
            .wechat_peer_id
            .as_deref()
            .filter(|s| !s.is_empty())
        {
            Some(bound) if bound != peer => {
                log_warn!(
                    &ctx,
                    "wechat_adapt",
                    "peer mismatch, skip: channel_id={} bound={} peer={}",
                    event.channel_id,
                    bound,
                    peer
                );
                return Ok(None);
            }
            // 未配置：首次入站自动回填（写 config_json；仅首条消息触发，RMW 竞态窗口可接受）
            None => {
                let mut updated = channel.clone();
                updated.po.config_json.0.wechat_peer_id = Some(peer.clone());
                if let Err(e) = self
                    .message_channel_dal
                    .update_channel(ctx.clone(), &updated)
                    .await
                {
                    // 回填失败不阻断入站（下次入站重试）
                    log_warn!(
                        &ctx,
                        "wechat_adapt",
                        "wechat_peer_id 回填失败（忽略）: channel_id={} peer={} err={}",
                        event.channel_id,
                        peer,
                        e
                    );
                } else {
                    log_info!(
                        &ctx,
                        "wechat_adapt",
                        "wechat_peer_id 首次入站自动回填: channel_id={} peer={}",
                        event.channel_id,
                        peer
                    );
                }
            }
            _ => {}
        }

        // 4. 用户映射 + 渠道绑定 Agent（可选）
        let from_id = channel.user_id().to_string();
        let to_agent_id = channel.agent_id().map(|s| s.to_string());

        log_info!(
            &ctx,
            "wechat_adapt",
            "adapted message_key={} channel_id={} bot_id={} from_user={} bound_agent={:?}",
            event.message_key,
            event.channel_id,
            event.bot_id,
            from_id,
            to_agent_id
        );

        Ok(Some(AdaptedMessage {
            from_id,
            from_role: common::enums::MessageRole::User,
            to_agent_id,
            channel_type: ChannelType::Wechat,
            content,
            project_id: None,
            task_id: None,
            reply_to_id: None,
        }))
    }
}

// ==================== WechatCredentialDal 实现 ====================

#[async_trait::async_trait]
impl WechatCredentialDal for WechatDalImpl {
    async fn resolve_channel_credentials(
        &self,
        ctx: RequestContext,
        channel: &MessageChannel,
    ) -> Option<IlinkChannelCredentials> {
        let credential_id = channel
            .config()
            .wechat_credential_id
            .as_deref()
            .filter(|s| !s.is_empty())?;
        let row = self.load_credential_row(ctx, credential_id).await?;
        match crate::service::dao::wechat::ilink::resolve_ilink_credentials(&row, channel) {
            Ok(c) => Some(c),
            Err(e) => {
                log_warn!("wechat channel {} 凭证引用解析失败: {}", channel.po.id, e);
                None
            }
        }
    }

    async fn find_channels_by_credential_id(
        &self,
        credential_id: &str,
    ) -> Result<Vec<MessageChannel>> {
        let ctx = RequestContext::new_system();
        let query = MessageChannelQuery {
            channel_type: Some(ChannelType::Wechat),
            ..Default::default()
        };
        let page = self.message_channel_dal.query_channels(ctx, query).await?;
        Ok(page
            .items
            .into_iter()
            // 已删除渠道不再计入引用（软删除：status=Deleted）
            .filter(|c| !matches!(c.po.status, common::enums::ChannelStatus::Deleted))
            .filter(|c| c.config().wechat_credential_id.as_deref() == Some(credential_id))
            .collect())
    }
}

// ==================== WechatListenerDal 实现 ====================

#[async_trait::async_trait]
impl WechatListenerDal for WechatDalImpl {
    async fn sync_listener_for_channel(&self, ctx: RequestContext, channel: &MessageChannel) {
        let result: Result<()> = async {
            if channel.is_enabled() && listens_inbound(channel) {
                let credentials = self
                    .resolve_channel_credentials(ctx.clone(), channel)
                    .await
                    .ok_or_else(|| {
                        err!(
                            InvalidRequest,
                            "wechat channel {} credential unresolved",
                            channel.po.id
                        )
                    })?;
                self.wechat_dao.start_polling(channel, &credentials).await
            } else {
                self.wechat_dao.stop_polling(channel.id()).await
            }
        }
        .await;
        if let Err(e) = result {
            log_warn!(
                "wechat listener sync failed (ignored): channel_id={} err={}",
                channel.po.id,
                e
            );
        }
    }

    async fn release_listener_for_channel(&self, _ctx: RequestContext, channel: &MessageChannel) {
        if let Err(e) = self.wechat_dao.stop_polling(channel.id()).await {
            log_warn!(
                "wechat listener release failed (ignored): channel_id={} err={}",
                channel.po.id,
                e
            );
        }
    }

    async fn rebuild_listeners_for_credential(&self, ctx: RequestContext, credential_id: &str) {
        let channels = match self.find_channels_by_credential_id(credential_id).await {
            Ok(channels) => channels,
            Err(e) => {
                log_warn!(
                    "wechat credential rebuild skipped (ignored): credential_id={} err={}",
                    credential_id,
                    e
                );
                return;
            }
        };
        for channel in channels {
            if channel.is_enabled() && listens_inbound(&channel) {
                self.sync_listener_for_channel(ctx.clone(), &channel).await;
            }
        }
    }
}

// ==================== 总 trait 空实现 ====================

impl super::WechatDal for WechatDalImpl {}

// ==================== MessageInboundAdapter 实现 ====================
//
// 实现消息入站适配器 trait，向中台注册后由 consumer 统一启停。
// 入站事件链路：DAO 长轮询 publish AOP 事件 → consumer/wechat_inbound 异步消费
// → adapt_wechat 转换 → MessageAdapterCallback 投递上层。

#[async_trait::async_trait]
impl MessageInboundAdapter for WechatDalImpl {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Wechat
    }

    async fn start(&self, callback: Arc<dyn MessageAdapterCallback>) -> Result<()> {
        {
            let mut running = self
                .running
                .write()
                .map_err(|e| err!(Internal, "wechat adapter running lock poisoned: {}", e))?;
            if *running {
                return Err(err!(Conflict, "wechat message adapter already running"));
            }
            *running = true;
        }
        self.set_callback(Some(callback.clone()));

        // 渠道数据驱动：查询全部启用且开启入站监听的微信渠道，逐个建立长轮询
        let channels = self.query_enabled_wechat_channels().await?;
        let sys_ctx = RequestContext::new_system();
        for channel in channels {
            if !listens_inbound(&channel) {
                continue;
            }
            let Some(credentials) = self
                .resolve_channel_credentials(sys_ctx.clone(), &channel)
                .await
            else {
                log_warn!(
                    "wechat adapter start skipped channel {}: credential reference unresolved",
                    channel.po.id
                );
                continue;
            };
            if let Err(e) = self.wechat_dao.start_polling(&channel, &credentials).await {
                // 单渠道建轮询失败不阻塞其他渠道
                log_error!(
                    "wechat adapter start failed for channel {}: {}",
                    channel.po.id,
                    e
                );
            }
        }
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        {
            let mut running = self
                .running
                .write()
                .map_err(|e| err!(Internal, "wechat adapter running lock poisoned: {}", e))?;
            if !*running {
                return Ok(());
            }
            *running = false;
        }
        self.set_callback(None);

        self.wechat_dao.stop_all_polling().await?;
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.read().map(|r| *r).unwrap_or(false)
    }
}
