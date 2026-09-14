//! 邮件 DAL 实现（EmailDalImpl）
//!
//! 实现 [`EmailCredentialDal`] 与 [`EmailListenerDal`] 全部子 trait
//! （由 mod.rs 的总 trait [`EmailDal`](super::EmailDal) 组合），承载邮件渠道的
//! 数据访问、消息转换、以及 IMAP 受管轮询生命周期管理。
//!
//! - `adapt_email`：IMAP 入站邮件 → 内部 `AdaptedMessage` 转换（不含 Agent 路由）
//! - 监听生命周期：`sync_listener_for_channel` / `rebuild_listeners_for_credential`
//!
//! 与微信的差异：轮询单元 = 邮箱凭证（一个代理邮箱可能被 N 个渠道共用），
//! 入站路由靠 (credential_id, from == email_to_address) 二维匹配，
//! 停轮询前需确认凭证不再被其他启用渠道引用。
//!
//! 作为 `pkg/adapter` 注册中心的适配者，由 consumer 层获取并调用
//! `adapt_email`，转换结果交由 message domain 发送。

use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use common::enums::ChannelType;
use common::error::{Result, err};

use crate::models::events::EmailInboundEvent;
use crate::models::message_channel::MessageChannel;
use crate::models::user_credential::UserCredentialPo;
use crate::pkg::RequestContext;
use crate::pkg::adapter::AdaptedMessage;
use crate::pkg::adapter::message::{MessageAdapterCallback, MessageInboundAdapter};
use crate::service::dao::email::{EmailDao, EmailImapCredentials, resolve_imap_credentials};
use crate::service::dao::message::MessageDao;
use crate::service::dao::message_channel::MessageChannelQuery;

use super::EmailCredentialDal;
use super::EmailListenerDal;

// ==================== 实现 ====================

/// 邮件 DAL 实现
///
/// 组合基础 MessageChannelDal + EmailDao + UserCredentialDao + MessageDao，
/// 提供邮件接入所需的数据访问、消息转换、幂等去重与 IMAP 轮询生命周期管理。
pub struct EmailDalImpl {
    /// 基础消息渠道 DAL（渠道配置 + 消息分发）
    message_channel_dal: Arc<dyn crate::service::dal::message_channel::MessageChannelDal>,
    /// 邮件 DAO（SMTP 出站 + IMAP 受管轮询 registry）
    email_dao: Arc<dyn EmailDao>,
    /// 用户凭证 DAO（凭证引用解析：渠道 email_credential_id → user_credentials 行）
    credential_dao: Arc<dyn crate::service::dao::user_credential::UserCredentialDao>,
    /// 消息 DAO（入站幂等：外部键 `email:<Message-ID>` 查重）
    message_dao: Arc<dyn MessageDao>,
    /// 监听运行状态标记
    running: RwLock<bool>,
}

/// 判断渠道是否开启邮件入站监听（缺省视为开启）
fn listens_inbound(channel: &MessageChannel) -> bool {
    channel.config().email_listen_inbound.unwrap_or(true)
}

/// 渠道配置的凭证引用 ID（空串视为未配置）
fn credential_id_of(channel: &MessageChannel) -> Option<&str> {
    channel
        .config()
        .email_credential_id
        .as_deref()
        .filter(|s| !s.is_empty())
}

/// 规范化邮箱地址：去显示名包裹（"Name <addr>" → addr）+ trim + 小写
///
/// 二维路由两侧统一走此函数：DAO 侧 from 已规范化（幂等），渠道侧
/// `email_to_address` 为用户手填（大小写/显示名不可控）。
fn normalize_addr(addr: &str) -> String {
    let trimmed = addr.trim();
    // 取最后一个 '<' 之后、其后第一个 '>' 之前的内容：纯地址与
    // "Name <addr>" 两种形态都收敛为 addr；无包裹时原样返回
    let bare = trimmed
        .rsplit('<')
        .next()
        .and_then(|s| s.split('>').next())
        .unwrap_or(trimmed)
        .trim();
    bare.to_lowercase()
}

impl EmailDalImpl {
    /// 创建实例（由 mod.rs `new_with_dao` 包装为 Arc 注入依赖）
    pub fn new(
        message_channel_dal: Arc<dyn crate::service::dal::message_channel::MessageChannelDal>,
        email_dao: Arc<dyn EmailDao>,
        credential_dao: Arc<dyn crate::service::dao::user_credential::UserCredentialDao>,
        message_dao: Arc<dyn MessageDao>,
    ) -> Self {
        Self {
            message_channel_dal,
            email_dao,
            credential_dao,
            message_dao,
            running: RwLock::new(false),
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
                log_warn!("email credential {} 查询失败: {}", credential_id, e);
                None
            }
        }
    }

    /// 查询邮件渠道（系统上下文）
    ///
    /// - `only_enabled = true`：入站路由用（停用渠道不收信）
    /// - `only_enabled = false`：凭证引用统计用（调用方再过滤软删）
    async fn query_email_channels(&self, only_enabled: bool) -> Result<Vec<MessageChannel>> {
        let ctx = RequestContext::new_system();
        let query = MessageChannelQuery {
            channel_type: Some(ChannelType::Email),
            only_enabled,
            ..Default::default()
        };
        let page = self.message_channel_dal.query_channels(ctx, query).await?;
        Ok(page.items)
    }

    /// 停止凭证轮询前检查共享引用：仍有其他"启用+开监听"渠道引用 → 保持轮询
    ///
    /// 共享邮箱场景的安全阀：单个渠道的停用/删除不能误停其他渠道的入站。
    async fn stop_polling_if_unused(
        &self,
        credential_id: &str,
        exclude: &MessageChannel,
    ) -> Result<()> {
        let others = self.find_channels_by_credential_id(credential_id).await?;
        let still_used = others
            .iter()
            .any(|c| c.po.id != exclude.po.id && c.is_enabled() && listens_inbound(c));
        if still_used {
            log_info!(
                "email polling kept (shared by other channels): credential_id={}",
                credential_id
            );
            return Ok(());
        }
        self.email_dao.stop_polling(credential_id).await
    }

    /// 将 IMAP 入站邮件适配为内部 `AdaptedMessage`
    ///
    /// 适配流程：
    /// 1. 内容过滤：空正文邮件直接跳过（附件壳/纯 HTML 剥标签后为空）
    /// 2. 二维路由：`(credential_id, from == email_to_address)` 命中启用渠道，
    ///    未命中不自动建渠（对端地址必须预先配置）
    /// 3. 幂等去重：外部键 `email:<Message-ID>` 已落库则跳过（IMAP 重投防护）
    /// 4. 用户映射：渠道的 user_id 作为 from_id
    ///
    /// **不做 Agent 路由**：返回的 `AdaptedMessage.to_agent_id` 取渠道显式绑定，
    /// 未绑定时为 `None`，由 producer 层档位链路由。
    ///
    /// 返回 `None` 表示事件被过滤（空正文 / 渠道未命中 / 重复邮件）。
    pub async fn adapt_email(
        &self,
        ctx: RequestContext,
        event: &EmailInboundEvent,
    ) -> Result<Option<AdaptedMessage>> {
        // 1. 内容过滤：空正文不产生入站消息
        if event.content.trim().is_empty() {
            log_debug!(
                &ctx,
                "email_adapt",
                "skip empty-content mail: credential_id={} message_key={}",
                event.credential_id,
                event.message_key
            );
            return Ok(None);
        }

        // 2. 二维路由：(credential_id, from) → 渠道（不做自动建渠）
        let from = normalize_addr(&event.from);
        let matched: Vec<MessageChannel> = self
            .query_email_channels(true)
            .await?
            .into_iter()
            .filter(|c| {
                c.config().email_credential_id.as_deref() == Some(event.credential_id.as_str())
            })
            .filter(|c| {
                c.config()
                    .email_to_address
                    .as_deref()
                    .map(|to| normalize_addr(to) == from)
                    .unwrap_or(false)
            })
            .collect();
        if matched.len() > 1 {
            log_warn!(
                &ctx,
                "email_adapt",
                "二维路由命中多个渠道（取第一个）: credential_id={} from={} count={}",
                event.credential_id,
                from,
                matched.len()
            );
        }
        let Some(channel) = matched.into_iter().next() else {
            log_info!(
                &ctx,
                "email_adapt",
                "二维路由未命中（不自动建渠）: credential_id={} from={} message_key={}",
                event.credential_id,
                from,
                event.message_key
            );
            return Ok(None);
        };

        // 3. 幂等去重：外部键已落库（IMAP 重投 / 同键回显）→ 跳过
        let external_key = event.external_key();
        if let Some(existing_id) = self
            .message_dao
            .find_id_by_external_key(ctx.clone(), &external_key)
            .await?
        {
            log_info!(
                &ctx,
                "email_adapt",
                "duplicate mail skipped: message_key={} existing_message_id={}",
                event.message_key,
                existing_id
            );
            return Ok(None);
        }

        // 4. 用户映射 + 渠道绑定 Agent（可选）
        let from_id = channel.user_id().to_string();
        let to_agent_id = channel.agent_id().map(|s| s.to_string());

        log_info!(
            &ctx,
            "email_adapt",
            "adapted message_key={} channel_id={} credential_id={} from={} subject={} bound_agent={:?}",
            event.message_key,
            channel.po.id,
            event.credential_id,
            event.from,
            event.subject,
            to_agent_id
        );

        Ok(Some(AdaptedMessage {
            from_id,
            from_role: common::enums::MessageRole::User,
            to_agent_id,
            channel_type: ChannelType::Email,
            content: event.content.clone(),
            project_id: None,
            task_id: None,
            reply_to_id: None,
            // 外部键随消息落库（delivery 持久化），供 IMAP 重投去重与出站回写同键对齐
            external_key: Some(external_key),
        }))
    }
}

// ==================== EmailCredentialDal 实现 ====================

#[async_trait::async_trait]
impl EmailCredentialDal for EmailDalImpl {
    async fn find_channels_by_credential_id(
        &self,
        credential_id: &str,
    ) -> Result<Vec<MessageChannel>> {
        let ctx = RequestContext::new_system();
        let query = MessageChannelQuery {
            channel_type: Some(ChannelType::Email),
            ..Default::default()
        };
        let page = self.message_channel_dal.query_channels(ctx, query).await?;
        Ok(page
            .items
            .into_iter()
            // 已删除渠道不再计入引用（软删除：status=Deleted）
            .filter(|c| !matches!(c.po.status, common::enums::ChannelStatus::Deleted))
            .filter(|c| c.config().email_credential_id.as_deref() == Some(credential_id))
            .collect())
    }

    async fn resolve_imap_credentials_by_id(
        &self,
        ctx: RequestContext,
        credential_id: &str,
    ) -> Option<EmailImapCredentials> {
        let row = self.load_credential_row(ctx, credential_id).await?;
        match resolve_imap_credentials(&row) {
            Ok(c) => Some(c),
            Err(e) => {
                log_warn!("email credential {} IMAP 解析失败: {}", credential_id, e);
                None
            }
        }
    }
}

// ==================== EmailListenerDal 实现 ====================

#[async_trait::async_trait]
impl EmailListenerDal for EmailDalImpl {
    async fn sync_listener_for_channel(&self, ctx: RequestContext, channel: &MessageChannel) {
        let result: Result<()> = async {
            if channel.is_enabled() && listens_inbound(channel) {
                let Some(credential_id) = credential_id_of(channel) else {
                    return Err(err!(
                        InvalidRequest,
                        "email channel {} 未配置 email_credential_id",
                        channel.po.id
                    ));
                };
                let credentials = self
                    .resolve_imap_credentials_by_id(ctx.clone(), credential_id)
                    .await
                    .ok_or_else(|| {
                        err!(
                            InvalidRequest,
                            "email channel {} credential unresolved",
                            channel.po.id
                        )
                    })?;
                self.email_dao.start_polling(&credentials).await
            } else {
                // 停用/关监听 → 共享引用检查后按需停止（同凭证其他渠道不受影响）
                match credential_id_of(channel) {
                    Some(credential_id) => {
                        self.stop_polling_if_unused(credential_id, channel).await
                    }
                    None => Ok(()),
                }
            }
        }
        .await;
        if let Err(e) = result {
            log_warn!(
                "email listener sync failed (ignored): channel_id={} err={}",
                channel.po.id,
                e
            );
        }
    }

    async fn release_listener_for_channel(&self, _ctx: RequestContext, channel: &MessageChannel) {
        let result: Result<()> = async {
            match credential_id_of(channel) {
                Some(credential_id) => self.stop_polling_if_unused(credential_id, channel).await,
                None => Ok(()),
            }
        }
        .await;
        if let Err(e) = result {
            log_warn!(
                "email listener release failed (ignored): channel_id={} err={}",
                channel.po.id,
                e
            );
        }
    }

    async fn rebuild_listeners_for_credential(&self, ctx: RequestContext, credential_id: &str) {
        // 凭证键控轮询：无需逐渠道 sync——仍有启用+开监听引用 → 整体重建
        //（ensure 凭证指纹幂等，参数变化时停旧重建）；已无引用 → 停止
        let result: Result<()> = async {
            let channels = self.find_channels_by_credential_id(credential_id).await?;
            let has_listener = channels
                .iter()
                .any(|c| c.is_enabled() && listens_inbound(c));
            if has_listener {
                let credentials = self
                    .resolve_imap_credentials_by_id(ctx, credential_id)
                    .await
                    .ok_or_else(|| {
                        err!(
                            InvalidRequest,
                            "email credential {} IMAP 解析失败",
                            credential_id
                        )
                    })?;
                self.email_dao.start_polling(&credentials).await
            } else {
                self.email_dao.stop_polling(credential_id).await
            }
        }
        .await;
        if let Err(e) = result {
            log_warn!(
                "email credential rebuild skipped (ignored): credential_id={} err={}",
                credential_id,
                e
            );
        }
    }
}

// ==================== 总 trait 空实现 ====================

impl super::EmailDal for EmailDalImpl {}

// ==================== MessageInboundAdapter 实现 ====================
//
// 实现消息入站适配器 trait，向中台注册后由 consumer 统一启停。
// 入站事件链路：DAO IMAP 轮询 publish AOP 事件 → consumer/email_inbound 异步消费
// → adapt_email 转换 → MessageAdapterCallback 投递上层。

#[async_trait::async_trait]
impl MessageInboundAdapter for EmailDalImpl {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Email
    }

    async fn start(&self, _callback: Arc<dyn MessageAdapterCallback>) -> Result<()> {
        // 投递回调统一由中台 start_all 登记持有（消费侧经中台取用），
        // 本实现不再自存回调句柄
        {
            let mut running = self
                .running
                .write()
                .map_err(|e| err!(Internal, "email adapter running lock poisoned: {}", e))?;
            if *running {
                return Err(err!(Conflict, "email message adapter already running"));
            }
            *running = true;
        }

        // 渠道数据驱动：查询启用且开监听的邮件渠道，按凭证去重后逐凭证建立
        // IMAP 轮询（轮询单元 = 代理邮箱凭证，同凭证多渠道共享一次登录）
        let channels: Vec<MessageChannel> = self
            .query_email_channels(true)
            .await?
            .into_iter()
            .filter(listens_inbound)
            .collect();
        let sys_ctx = RequestContext::new_system();
        let mut seen: HashSet<&str> = HashSet::new();
        for channel in &channels {
            let Some(credential_id) = credential_id_of(channel) else {
                log_warn!(
                    "email adapter start skipped channel {}: email_credential_id 未配置",
                    channel.po.id
                );
                continue;
            };
            if !seen.insert(credential_id) {
                continue;
            }
            let Some(credentials) = self
                .resolve_imap_credentials_by_id(sys_ctx.clone(), credential_id)
                .await
            else {
                log_warn!(
                    "email adapter start skipped credential {}: credential reference unresolved",
                    credential_id
                );
                continue;
            };
            if let Err(e) = self.email_dao.start_polling(&credentials).await {
                // 单凭证建轮询失败不阻塞其他凭证
                log_error!(
                    "email adapter start failed for credential {}: {}",
                    credential_id,
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
                .map_err(|e| err!(Internal, "email adapter running lock poisoned: {}", e))?;
            if !*running {
                return Ok(());
            }
            *running = false;
        }

        self.email_dao.stop_all_polling().await?;
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.read().map(|r| *r).unwrap_or(false)
    }
}
