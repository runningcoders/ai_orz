//! 飞书消息渠道 DAL（LarkMessageChannelDal）
//!
//! 继承基础 MessageChannelDal 的能力，承载飞书私信接入场景的特有数据访问：
//! - `find_channel_by_lark_identity`：按 app_id + open_id 二维定位归属渠道
//! - `adapt_lark`：飞书事件 → 内部 `AdaptedMessage` 转换（不含 Agent 路由）
//! - 监听生命周期：`ensure_listener_for` / `release_listener_if_unused`
//!
//! 作为 `pkg/adapter` 注册中心的适配者，由 consumer 层获取并调用
//! `adapt_lark`，转换结果交由 message domain 发送。
//!
//! # 分层职责
//!
//! - **LarkMessageChannelDal（本模块）**：纯数据访问 + 事件转换 + 监听编排
//!   - 仅依赖 `MessageChannelDao`（渠道查询）与 `LarkDao`（外部调用）
//!   - `adapt_lark` 只做"事件 → AdaptedMessage"转换，**不负责 Agent 路由**
//!   - 转换结果中的 `to_agent_id` 为 `None`，由 consumer 层填充
//!
//! - **consumer/adapter 层**：业务编排
//!   - 调用 `adapt_lark` 获取适配结果
//!   - 通过 `HrDomain::AgentManage::query` 查询 Agent 路由
//!   - 通过 `MessageDomain::send_to_agent` 发送消息
//!
//! 这样设计遵循 DAL 层不跨 DAL 依赖的约束，业务编排由上层 consumer 完成。
//!
//! # 多应用模型
//!
//! 启停由渠道数据驱动：`start()` 查询全部启用且开启入站监听的飞书渠道，
//! 按 app_id 去重后逐个建立 WebSocket 长连接；渠道增删改时由 Domain 层
//! 调用 `ensure_listener_for` / `release_listener_if_unused` 联动。

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use common::enums::{ChannelStatus, ChannelType};
use common::error::{Result, err};
use common::models::CredentialKind;

use crate::models::message_channel::MessageChannel;
use crate::models::user_credential::UserCredentialPo;
use crate::pkg::RequestContext;
use crate::pkg::adapter::AdaptedMessage;
use crate::pkg::adapter::message::{MessageAdapterCallback, MessageInboundAdapter};
use crate::service::dal::message_channel::MessageChannelDal;
use crate::service::dao::lark::{LarkAppCredentials, LarkDao, LarkEventHandler, LarkMessageEvent};
use crate::service::dao::message_channel::MessageChannelQuery;
use crate::service::dao::user_credential::UserCredentialDao;

// ==================== lark_cli 凭证解析器 ====================

/// lark_cli 工具的凭证解析器实现
///
/// 按 `ctx.user_id` 查该用户启用的 Lark 渠道，经凭证引用解析取解密后凭证 + 身份模式；
/// 在 `service::init` 阶段注册到 pkg 层全局注入口。
pub struct LarkDalCredentialResolver;

#[async_trait::async_trait]
impl crate::pkg::tool_registry::lark_cli::LarkCredentialResolver for LarkDalCredentialResolver {
    async fn resolve(&self, ctx: &RequestContext) -> Result<Option<(String, String, String)>> {
        Ok(dal()
            .resolve_credentials_for_user(ctx)
            .await?
            .map(|(c, mode)| (c.app_id, c.app_secret, mode)))
    }
}

// ==================== 单例管理 ====================

static LARK_DAL: OnceLock<Arc<LarkMessageChannelDal>> = OnceLock::new();

/// 获取 LarkMessageChannelDal 单例
pub fn dal() -> Arc<LarkMessageChannelDal> {
    LARK_DAL
        .get()
        .cloned()
        .expect("LarkMessageChannelDal not initialized, call init() first")
}

/// 初始化 LarkMessageChannelDal 并注册到消息适配中台
///
/// 无条件注册：飞书启停由渠道数据驱动（无渠道时 `start()` 不建任何连接）。
pub fn init() {
    let instance = new_with_credential_dao(
        crate::service::dal::message_channel::dal(),
        crate::service::dao::lark::dao(),
        crate::service::dao::user_credential::dao(),
    );
    // 注册到消息入站适配中台
    if let Err(e) = crate::pkg::adapter::message::registry().register(instance.clone()) {
        log_warn!("lark message adapter register skipped: {}", e);
    }
    let _ = LARK_DAL.set(instance);
    sys_info!("lark message adapter registered to adapter registry");
}

/// 创建 LarkMessageChannelDal 实例（测试可注入隔离依赖）
pub fn new_with_credential_dao(
    message_channel_dal: Arc<dyn MessageChannelDal>,
    lark_dao: Arc<dyn LarkDao>,
    credential_dao: Arc<dyn UserCredentialDao>,
) -> Arc<LarkMessageChannelDal> {
    Arc::new(LarkMessageChannelDal {
        message_channel_dal,
        lark_dao,
        credential_dao,
        running: RwLock::new(false),
        callback: RwLock::new(None),
    })
}

// ==================== 实现 ====================

/// 飞书消息渠道 DAL
///
/// 组合基础 MessageChannelDal + LarkDao，提供飞书私信接入所需的
/// 数据访问、消息转换、以及入站监听生命周期管理。
///
/// 实现 `MessageInboundAdapter` trait，向中台注册后由 consumer 统一启停。
pub struct LarkMessageChannelDal {
    /// 基础消息渠道 DAL（渠道配置 + 消息分发）
    message_channel_dal: Arc<dyn MessageChannelDal>,
    /// 飞书 DAO（HTTP API + WebSocket 长连接池）
    lark_dao: Arc<dyn LarkDao>,
    /// 用户凭证 DAO（凭证引用解析：渠道 lark_credential_id → user_credentials 行）
    credential_dao: Arc<dyn UserCredentialDao>,
    /// 监听运行状态标记
    running: RwLock<bool>,
    /// 运行期回调句柄（start 时注入，供运行期新建连接复用）
    callback: RwLock<Option<Arc<dyn MessageAdapterCallback>>>,
}

/// 判断渠道是否开启入站监听（缺省视为开启）
fn listens_inbound(channel: &MessageChannel) -> bool {
    channel.config().lark_listen_inbound.unwrap_or(true)
}

/// 渠道身份模式归一化（auto/bot/user，缺省 auto）
fn identity_mode_of(channel: &MessageChannel) -> String {
    channel
        .config()
        .lark_identity_mode
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("auto")
        .to_string()
}

impl LarkMessageChannelDal {
    /// 按凭证 ID 加载凭证行（凭证不存在/已软删/查询失败返回 None）
    async fn load_credential_row(
        &self,
        ctx: RequestContext,
        credential_id: &str,
    ) -> Option<UserCredentialPo> {
        match self.credential_dao.find_by_id(ctx, credential_id).await {
            Ok(po) => po,
            Err(e) => {
                log_warn!("lark credential {} 查询失败: {}", credential_id, e);
                None
            }
        }
    }

    /// 带缓存的凭证行加载（同一批渠道扫描中同凭证只查一次，消除 N+1）
    async fn cached_credential(
        &self,
        cache: &mut HashMap<String, Option<UserCredentialPo>>,
        ctx: RequestContext,
        credential_id: &str,
    ) -> Option<UserCredentialPo> {
        if let Some(po) = cache.get(credential_id) {
            return po.clone();
        }
        let loaded = self.load_credential_row(ctx, credential_id).await;
        cache.insert(credential_id.to_string(), loaded.clone());
        loaded
    }

    /// 基于已加载凭证行解析渠道凭证（纯查找，不查库）
    fn resolve_credentials_from_row(
        row: &UserCredentialPo,
        channel: &MessageChannel,
    ) -> Option<LarkAppCredentials> {
        match crate::service::dao::lark::resolve_lark_credentials(row, channel) {
            Ok(c) => Some(c),
            Err(e) => {
                log_warn!("lark channel {} 凭证引用解析失败: {}", channel.po.id, e);
                None
            }
        }
    }

    /// 解析渠道引用的飞书应用凭证（引用 ID → 凭证行，缺失/失败返回 None）
    ///
    /// 凭证查询复用调用方上下文的连接池（测试隔离友好）。
    async fn resolve_channel_credentials(
        &self,
        ctx: RequestContext,
        channel: &MessageChannel,
    ) -> Option<LarkAppCredentials> {
        let credential_id = channel
            .config()
            .lark_credential_id
            .as_deref()
            .filter(|s| !s.is_empty())?;
        let row = self.load_credential_row(ctx, credential_id).await?;
        Self::resolve_credentials_from_row(&row, channel)
    }

    /// 解析渠道引用凭证的 app_id（供按应用分组/过滤与 Domain 生命周期联动）
    pub async fn resolve_channel_app_id(
        &self,
        ctx: RequestContext,
        channel: &MessageChannel,
    ) -> Option<String> {
        self.resolve_channel_credentials(ctx, channel)
            .await
            .map(|c| c.app_id)
    }

    /// 查询全部启用的飞书渠道（系统上下文）
    async fn query_enabled_lark_channels(&self) -> Result<Vec<MessageChannel>> {
        let ctx = RequestContext::new_system();
        let query = MessageChannelQuery {
            channel_type: Some(ChannelType::Lark),
            only_enabled: true,
            ..Default::default()
        };
        let page = self.message_channel_dal.query_channels(ctx, query).await?;
        Ok(page.items)
    }

    /// 解析指定用户的飞书应用凭证（lark_cli 工具身份来源）
    ///
    /// 按 `ctx.user_id` 查询该用户启用的 Lark 渠道，经凭证引用解析取可用凭证（已解密），
    /// 附带渠道身份模式（auto/bot/user，缺省 auto）。
    /// 优先取引用**用户默认凭证**（`find_default` 解析链：个人默认 > 个人其他 >
    /// 组织默认 > 组织其他 public）的渠道；默认凭证未被渠道引用时回退第一条可用渠道。
    /// 未绑定或凭证不完整返回 `None`，由调用方给出引导性错误。
    pub async fn resolve_credentials_for_user(
        &self,
        ctx: &RequestContext,
    ) -> Result<Option<(LarkAppCredentials, String)>> {
        let Some(user_id) = ctx.user_id.clone() else {
            return Ok(None);
        };
        // 复用调用方上下文的 storage，避免依赖全局单例（测试友好）
        let query_ctx = ctx.to_builder().build();
        // 用户默认 Lark 凭证（可能为 None：未设默认且无活跃凭证）
        let default_id = self
            .credential_dao
            .find_default(query_ctx.clone(), &user_id, CredentialKind::LarkApp, None)
            .await?
            .map(|po| po.id);
        let query = MessageChannelQuery {
            user_id: Some(user_id),
            channel_type: Some(ChannelType::Lark),
            only_enabled: true,
            ..Default::default()
        };
        let page = self
            .message_channel_dal
            .query_channels(query_ctx.clone(), query)
            .await?;
        // 凭证行按 ID 缓存（同凭证被多渠道引用时免重复查库）
        let mut cache: HashMap<String, Option<UserCredentialPo>> = HashMap::new();
        let mut fallback: Option<(LarkAppCredentials, String)> = None;
        for channel in &page.items {
            let Some(credential_id) = channel
                .config()
                .lark_credential_id
                .as_deref()
                .filter(|s| !s.is_empty())
            else {
                continue;
            };
            let Some(row) = self
                .cached_credential(&mut cache, query_ctx.clone(), credential_id)
                .await
            else {
                continue;
            };
            let Some(credentials) = Self::resolve_credentials_from_row(&row, channel) else {
                continue;
            };
            let mode = identity_mode_of(channel);
            let is_default = default_id.as_deref() == Some(credential_id);
            if is_default {
                return Ok(Some((credentials, mode)));
            }
            if fallback.is_none() {
                fallback = Some((credentials, mode));
            }
        }
        Ok(fallback)
    }

    /// 查找引用指定凭证的飞书渠道（供 Domain 凭证变更联动编排）
    ///
    /// 内存过滤渠道 config_json 的 `lark_credential_id`（渠道数量有限，可接受）。
    pub async fn find_channels_by_credential_id(
        &self,
        credential_id: &str,
    ) -> Result<Vec<MessageChannel>> {
        let ctx = RequestContext::new_system();
        let query = MessageChannelQuery {
            channel_type: Some(ChannelType::Lark),
            ..Default::default()
        };
        let page = self.message_channel_dal.query_channels(ctx, query).await?;
        Ok(page
            .items
            .into_iter()
            // 已删除渠道不再计入引用（软删除：status=Deleted）
            .filter(|c| c.po.status != ChannelStatus::Deleted)
            .filter(|c| c.config().lark_credential_id.as_deref() == Some(credential_id))
            .collect())
    }

    /// 按 app_id + open_id 二维定位启用的飞书渠道
    ///
    /// 多应用模型下同一 open_id 可能存在于不同应用中，
    /// 先按渠道引用凭证解析出的 app_id 过滤再匹配 open_id。
    /// 渠道数量有限（每个用户最多几条），内存过滤可接受。
    pub async fn find_channel_by_lark_identity(
        &self,
        ctx: RequestContext,
        app_id: &str,
        open_id: &str,
    ) -> Result<Option<MessageChannel>> {
        let query = MessageChannelQuery {
            channel_type: Some(ChannelType::Lark),
            only_enabled: true,
            ..Default::default()
        };
        let page = self
            .message_channel_dal
            .query_channels(ctx.clone(), query)
            .await?;

        for channel in page.items {
            let config = channel.config();
            let open_matched = config
                .lark_open_id
                .as_deref()
                .map(|id| id == open_id)
                .unwrap_or(false);
            if !open_matched {
                continue;
            }
            if self
                .resolve_channel_app_id(ctx.clone(), &channel)
                .await
                .as_deref()
                == Some(app_id)
            {
                return Ok(Some(channel));
            }
        }
        Ok(None)
    }

    /// 将飞书消息事件适配为内部 `AdaptedMessage`
    ///
    /// 适配流程：
    /// 1. 事件过滤：仅处理 P2P 文本消息
    /// 2. 解析消息内容：飞书文本消息 content JSON
    /// 3. 渠道查找：按 app_id + sender open_id 二维定位归属渠道
    /// 4. 用户映射：渠道的 user_id 作为 from_id
    ///
    /// **不做 Agent 路由**：返回的 `AdaptedMessage.to_agent_id` 为 `None`，
    /// consumer 层根据渠道绑定 agent_id 或 feishu_reception 角色策略自行决定。
    ///
    /// 返回 `None` 表示事件被过滤（非 P2P、非文本、未绑定渠道、内容为空），
    /// consumer 可据此决定是否推送"未绑定/不可用"提示。
    pub async fn adapt_lark(
        &self,
        ctx: RequestContext,
        app_id: &str,
        event: &LarkMessageEvent,
    ) -> Result<Option<AdaptedMessage>> {
        // 1. 事件过滤：仅处理 P2P 文本消息
        if !event.is_p2p() || !event.is_text() {
            log_debug!(
                &ctx,
                "lark_adapt",
                "skip non-p2p/text event: event_id={} chat_type={} msg_type={}",
                event.header.event_id,
                event.event.message.chat_type,
                event.event.message.message_type
            );
            return Ok(None);
        }

        // 2. 解析消息内容
        let content = match event.parse_text() {
            Some(t) => t,
            None => {
                log_debug!(
                    &ctx,
                    "lark_adapt",
                    "skip event with unparseable text content: event_id={}",
                    event.header.event_id
                );
                return Ok(None);
            }
        };
        if content.trim().is_empty() {
            return Ok(None);
        }

        // 3. 渠道查找 + 用户映射（app_id + open_id 二维定位）
        let open_id = event.sender_open_id();
        let channel = self
            .find_channel_by_lark_identity(ctx.clone(), app_id, open_id)
            .await?;

        let channel = match channel {
            Some(c) => c,
            None => {
                log_info!(
                    &ctx,
                    "lark_adapt",
                    "no bound channel for app_id={} open_id={} event_id={}",
                    app_id,
                    open_id,
                    event.header.event_id
                );
                return Ok(None);
            }
        };
        let from_id = channel.user_id().to_string();

        // 4. 渠道绑定的 agent_id（可选，未绑定时 consumer 层做路由）
        let to_agent_id = channel.agent_id().map(|s| s.to_string());

        log_info!(
            &ctx,
            "lark_adapt",
            "adapted event_id={} app_id={} from_user={} bound_agent={:?}",
            event.header.event_id,
            app_id,
            from_id,
            to_agent_id
        );

        Ok(Some(AdaptedMessage {
            from_id,
            from_role: common::enums::MessageRole::User,
            to_agent_id,
            content,
            project_id: None,
            task_id: None,
            reply_to_id: None,
        }))
    }

    // ==================== 监听生命周期 ====================

    /// 确保指定应用的事件监听已建立（幂等）
    ///
    /// 该 app 存在启用且开启入站监听的渠道 → 建连；否则无操作。
    /// 由 Domain 层在渠道创建/启用/开启监听后调用。
    pub async fn ensure_listener_for(&self, app_id: &str) -> Result<()> {
        if !self.is_running() {
            // 适配中台尚未启动（如服务启动早期），交由 start() 统一拉起
            return Ok(());
        }
        let candidates = self.query_enabled_lark_channels().await?;
        let sys_ctx = RequestContext::new_system();
        let mut cache: HashMap<String, Option<UserCredentialPo>> = HashMap::new();
        let mut found = None;
        for channel in candidates {
            if !listens_inbound(&channel) {
                continue;
            }
            let Some(credential_id) = channel
                .config()
                .lark_credential_id
                .as_deref()
                .filter(|s| !s.is_empty())
            else {
                continue;
            };
            let Some(row) = self
                .cached_credential(&mut cache, sys_ctx.clone(), credential_id)
                .await
            else {
                continue;
            };
            let Some(credentials) = Self::resolve_credentials_from_row(&row, &channel) else {
                continue;
            };
            if credentials.app_id == app_id {
                found = Some(credentials);
                break;
            }
        }
        let credentials = match found {
            Some(c) => c,
            None => return Ok(()),
        };
        let handler = Arc::new(LarkAdapterHandler::new(app_id, self.callback_or_none()));
        self.lark_dao
            .start_event_listener(credentials, handler)
            .await
    }

    /// 该应用已无启用且开启入站监听的渠道引用时停止监听
    ///
    /// 由 Domain 层在渠道禁用/删除/关闭监听后调用。
    pub async fn release_listener_if_unused(&self, app_id: &str) -> Result<()> {
        let channels = self.query_enabled_lark_channels().await?;
        let sys_ctx = RequestContext::new_system();
        let mut cache: HashMap<String, Option<UserCredentialPo>> = HashMap::new();
        let mut still_needed = false;
        for channel in channels {
            if !listens_inbound(&channel) {
                continue;
            }
            let Some(credential_id) = channel
                .config()
                .lark_credential_id
                .as_deref()
                .filter(|s| !s.is_empty())
            else {
                continue;
            };
            let Some(row) = self
                .cached_credential(&mut cache, sys_ctx.clone(), credential_id)
                .await
            else {
                continue;
            };
            let Some(credentials) = Self::resolve_credentials_from_row(&row, &channel) else {
                continue;
            };
            if credentials.app_id == app_id {
                still_needed = true;
                break;
            }
        }
        if !still_needed {
            self.lark_dao.stop_event_listener(app_id).await?;
        }
        Ok(())
    }

    // ==================== 监听联动编排（供 Domain 单一入口调用） ====================

    /// 单渠道状态变化后的监听同步（启用+开监听 → ensure；否则 release）
    ///
    /// 渠道创建/更新后的联动入口；解析或建停失败仅告警，不影响主操作。
    pub async fn sync_listener_for_channel(&self, ctx: RequestContext, channel: &MessageChannel) {
        let Some(app_id) = self.resolve_channel_app_id(ctx, channel).await else {
            return;
        };
        let listen = channel.config().lark_listen_inbound.unwrap_or(true);
        let result = if channel.is_enabled() && listen {
            self.ensure_listener_for(&app_id).await
        } else {
            self.release_listener_if_unused(&app_id).await
        };
        if let Err(e) = result {
            log_warn!(
                "lark listener sync failed (ignored): channel_id={} app_id={} err={}",
                channel.po.id,
                app_id,
                e
            );
        }
    }

    /// 渠道删除后的监听释放（该 app 无其他引用时才真正停连）
    ///
    /// 解析或停连失败仅告警，不影响主操作。
    pub async fn release_listener_for_channel(
        &self,
        ctx: RequestContext,
        channel: &MessageChannel,
    ) {
        let Some(app_id) = self.resolve_channel_app_id(ctx, channel).await else {
            return;
        };
        if let Err(e) = self.release_listener_if_unused(&app_id).await {
            log_warn!(
                "lark listener release failed (ignored): channel_id={} app_id={} err={}",
                channel.po.id,
                app_id,
                e
            );
        }
    }

    /// 凭证变更后的监听移交（失败仅告警）
    ///
    /// - app_id 变化：release 旧 app（引用计数）→ ensure 新 app；
    /// - app_id 不变但 secret 轮换：强制断连重建（旧连接 token 缓存持有旧 secret，
    ///   不重建会导致出站静默失败直到下次断线才自愈）。
    pub async fn handover_listeners_after_credential_change(
        &self,
        old_app_id: &str,
        new_app_id: &str,
        secret_changed: bool,
    ) {
        let result: Result<()> = async {
            if old_app_id != new_app_id {
                self.release_listener_if_unused(old_app_id).await?;
                self.ensure_listener_for(new_app_id).await
            } else if secret_changed {
                self.lark_dao.stop_event_listener(old_app_id).await?;
                self.ensure_listener_for(old_app_id).await
            } else {
                Ok(())
            }
        }
        .await;
        if let Err(e) = result {
            log_warn!(
                "lark credential change listener handover failed (ignored): old_app_id={} new_app_id={} err={}",
                old_app_id,
                new_app_id,
                e
            );
        }
    }

    /// 已注册回调句柄（start 后由 registry 注入；未启动时为 None）
    ///
    /// `ensure_listener_for` 在运行期建连时复用同一回调。
    fn callback_or_none(&self) -> Option<Arc<dyn MessageAdapterCallback>> {
        self.callback.read().map(|c| c.clone()).unwrap_or(None)
    }
}

// ==================== 基础渠道能力透传 ====================
//
// LarkMessageChannelDal 继承基础 MessageChannelDal 的能力，
// 通过组合方式透传调用，保持基础渠道 CRUD + 推送分发可用。

impl LarkMessageChannelDal {
    /// 基础渠道能力引用（供 consumer 层推送消息时使用）
    pub fn base(&self) -> &dyn MessageChannelDal {
        self.message_channel_dal.as_ref()
    }

    /// WS 连接监控快照透传（供 health metrics 聚合）
    pub async fn listener_stats(&self) -> common::api::LarkWsMetrics {
        self.lark_dao.listener_stats().await
    }
}

// ==================== MessageInboundAdapter 实现 ====================
//
// 实现消息入站适配器 trait，向中台注册后由 consumer 统一启停。
// 内部通过 LarkAdapterHandler 桥接 LarkEventHandler → MessageAdapterCallback。

#[async_trait::async_trait]
impl MessageInboundAdapter for LarkMessageChannelDal {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Lark
    }

    async fn start(&self, callback: Arc<dyn MessageAdapterCallback>) -> Result<()> {
        {
            let mut running = self
                .running
                .write()
                .map_err(|e| err!(Internal, "lark adapter running lock poisoned: {}", e))?;
            if *running {
                return Err(err!(Conflict, "lark message adapter already running"));
            }
            *running = true;
        }
        self.set_callback(Some(callback.clone()));

        // 渠道数据驱动：查询全部启用且开启入站监听的飞书渠道，按引用凭证 app_id 去重建连
        let channels = self.query_enabled_lark_channels().await?;
        let sys_ctx = RequestContext::new_system();
        let mut by_app: HashMap<String, (MessageChannel, LarkAppCredentials)> = HashMap::new();
        for channel in channels {
            if !listens_inbound(&channel) {
                continue;
            }
            let Some(credentials) = self
                .resolve_channel_credentials(sys_ctx.clone(), &channel)
                .await
            else {
                log_warn!(
                    "lark adapter start skipped channel {}: credential reference unresolved",
                    channel.po.id
                );
                continue;
            };
            by_app
                .entry(credentials.app_id.clone())
                .or_insert((channel, credentials));
        }

        for (app_id, (_channel, credentials)) in by_app {
            let handler = Arc::new(LarkAdapterHandler::new(&app_id, Some(callback.clone())));
            if let Err(e) = self
                .lark_dao
                .start_event_listener(credentials, handler)
                .await
            {
                // 单应用建连失败不阻塞其他应用
                log_error!("lark adapter start failed for app_id={}: {}", app_id, e);
            }
        }
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        {
            let mut running = self
                .running
                .write()
                .map_err(|e| err!(Internal, "lark adapter running lock poisoned: {}", e))?;
            if !*running {
                return Ok(());
            }
            *running = false;
        }
        self.set_callback(None);

        self.lark_dao.stop_all_event_listeners().await?;
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.read().map(|r| *r).unwrap_or(false)
    }
}

impl LarkMessageChannelDal {
    fn set_callback(&self, callback: Option<Arc<dyn MessageAdapterCallback>>) {
        if let Ok(mut guard) = self.callback.write() {
            *guard = callback;
        }
    }
}

/// LarkEventHandler → MessageAdapterCallback 桥接
///
/// 实现 `LarkEventHandler` trait，接收飞书原始事件，
/// 调用 `LarkMessageChannelDal.adapt_lark` 转换为 `AdaptedMessage`，
/// 再通过 `MessageAdapterCallback` 投递到上层。
///
/// 每条 WS 连接一个 handler 实例，持有归属 app_id。
struct LarkAdapterHandler {
    app_id: String,
    lark_dal: Arc<LarkMessageChannelDal>,
    callback: Option<Arc<dyn MessageAdapterCallback>>,
}

impl LarkAdapterHandler {
    fn new(app_id: &str, callback: Option<Arc<dyn MessageAdapterCallback>>) -> Self {
        // lark_dal 是全局单例，直接取单例保证存活（与注册中心生命周期一致）
        let lark_dal = dal();
        Self {
            app_id: app_id.to_string(),
            lark_dal,
            callback,
        }
    }
}

#[async_trait::async_trait]
impl LarkEventHandler for LarkAdapterHandler {
    async fn handle_message_event(&self, app_id: &str, event: LarkMessageEvent) -> Result<()> {
        // 以连接归属 app_id 为准（DAO 回传值应与之一致）
        let app_id = if app_id.is_empty() {
            &self.app_id
        } else {
            app_id
        };
        let ctx = RequestContext::new_system();
        let adapted = self
            .lark_dal
            .adapt_lark(ctx.clone(), app_id, &event)
            .await?;

        if let Some(msg) = adapted {
            let callback = self
                .callback
                .clone()
                .or_else(|| self.lark_dal.callback_or_none());
            match callback {
                Some(cb) => cb.on_message(msg).await?,
                None => log_warn!(
                    "lark adapter handler dropped message: no callback registered app_id={}",
                    app_id
                ),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
pub mod test_support {
    //! 测试支持：构造可注入隔离依赖的 LarkMessageChannelDal

    use super::*;

    /// 创建测试用 LarkMessageChannelDal（不注册到全局 registry）
    pub fn new_for_test_with_credential_dao(
        message_channel_dal: Arc<dyn MessageChannelDal>,
        lark_dao: Arc<dyn LarkDao>,
        credential_dao: Arc<dyn UserCredentialDao>,
    ) -> Arc<LarkMessageChannelDal> {
        new_with_credential_dao(message_channel_dal, lark_dao, credential_dao)
    }
}
