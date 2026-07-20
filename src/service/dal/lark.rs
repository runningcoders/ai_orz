//! 飞书消息渠道 DAL（LarkMessageChannelDal）
//!
//! 继承基础 MessageChannelDal 的能力，承载飞书私信接入场景的特有数据访问：
//! - `find_channel_by_lark_open_id`：按飞书 open_id 查找归属渠道
//! - `adapt_lark`：飞书事件 → 内部 `AdaptedMessage` 转换（不含 Agent 路由）
//!
//! 作为 `pkg/adapter` 注册中心的适配者，由 consumer 层（LarkEventDispatcher）
//! 获取并调用 `adapt_lark`，转换结果交由 message domain 发送。
//!
//! # 分层职责
//!
//! - **LarkMessageChannelDal（本模块）**：纯数据访问 + 事件转换
//!   - 仅依赖 `MessageChannelDao`（渠道查询）
//!   - `adapt_lark` 只做"事件 → AdaptedMessage"转换，**不负责 Agent 路由**
//!   - 转换结果中的 `to_agent_id` 为 `None`，由 consumer 层填充
//!
//! - **LarkEventDispatcher（consumer/adapter）**：业务编排
//!   - 调用 `adapt_lark` 获取适配结果
//!   - 通过 `HrDomain::AgentManage::query` 查询 Agent 路由
//!   - 通过 `MessageDomain::send_to_agent` 发送消息
//!
//! 这样设计遵循 DAL 层不跨 DAL 依赖的约束，业务编排由上层 consumer 完成。

use std::sync::{Arc, OnceLock, RwLock};

use common::enums::ChannelType;
use common::error::{err, Result};

use crate::models::message_channel::MessageChannel;
use crate::pkg::adapter::AdaptedMessage;
use crate::pkg::adapter::message::{
    MessageAdapterCallback, MessageInboundAdapter,
};
use crate::pkg::RequestContext;
use crate::service::dal::message_channel::MessageChannelDal;
use crate::service::dao::lark::{LarkDao, LarkEventHandler, LarkMessageEvent};
use crate::service::dao::message_channel::MessageChannelQuery;

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
/// 若 `[lark] enabled = false`，跳过注册（飞书功能不可用但不影响其他模块）。
pub fn init() {
    // 检查飞书配置是否启用
    let enabled = crate::config::try_get()
        .map(|c| c.lark.enabled)
        .unwrap_or(false);

    if !enabled {
        sys_info!("lark disabled by config, skip adapter registration");
        return;
    }

    let instance = new(
        crate::service::dal::message_channel::dal(),
        crate::service::dao::lark::dao(),
    );
    // 注册到消息入站适配中台
    if let Err(e) = crate::pkg::adapter::message::registry().register(instance.clone()) {
        log_warn!("lark message adapter register skipped: {}", e);
    }
    let _ = LARK_DAL.set(instance);
    sys_info!("lark message adapter registered to adapter registry");
}

/// 创建 LarkMessageChannelDal 实例（测试可注入隔离依赖）
pub fn new(
    message_channel_dal: Arc<dyn MessageChannelDal>,
    lark_dao: Arc<dyn LarkDao>,
) -> Arc<LarkMessageChannelDal> {
    Arc::new(LarkMessageChannelDal {
        message_channel_dal,
        lark_dao,
        running: RwLock::new(false),
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
    /// 飞书 DAO（HTTP API + WebSocket 长连接）
    lark_dao: Arc<dyn LarkDao>,
    /// 监听运行状态标记
    running: RwLock<bool>,
}

impl LarkMessageChannelDal {
    /// 按飞书 open_id 查找启用的飞书渠道
    ///
    /// 遍历所有启用的飞书渠道，匹配 config_json.lark_open_id。
    /// 渠道数量有限（每个用户最多几条），内存过滤可接受。
    pub async fn find_channel_by_lark_open_id(
        &self,
        ctx: RequestContext,
        open_id: &str,
    ) -> Result<Option<MessageChannel>> {
        let query = MessageChannelQuery {
            channel_type: Some(ChannelType::Lark),
            only_enabled: true,
            ..Default::default()
        };
        let channels = self.message_channel_dal.query_channels(ctx, query).await?;

        for channel in channels {
            if channel
                .config()
                .lark_open_id
                .as_deref()
                .map(|id| id == open_id)
                .unwrap_or(false)
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
    /// 3. 渠道查找：按 sender open_id 查找归属渠道
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

        // 3. 渠道查找 + 用户映射
        let open_id = event.sender_open_id();
        let channel = self
            .find_channel_by_lark_open_id(ctx.clone(), open_id)
            .await?;

        let channel = match channel {
            Some(c) => c,
            None => {
                log_info!(
                    &ctx,
                    "lark_adapt",
                    "no bound channel for open_id={} event_id={}",
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
            "adapted event_id={} from_user={} bound_agent={:?}",
            event.header.event_id,
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
            let mut running = self.running.write().map_err(|e| {
                err!(Internal, "lark adapter running lock poisoned: {}", e)
            })?;
            if *running {
                return Err(err!(Conflict, "lark message adapter already running"));
            }
            *running = true;
        }

        let handler = Arc::new(LarkAdapterHandler::new(self, callback));

        self.lark_dao.start_event_listener(handler).await?;
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        {
            let mut running = self.running.write().map_err(|e| {
                err!(Internal, "lark adapter running lock poisoned: {}", e)
            })?;
            if !*running {
                return Ok(());
            }
            *running = false;
        }

        self.lark_dao.stop_event_listener().await?;
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.read().map(|r| *r).unwrap_or(false)
    }
}

/// LarkEventHandler → MessageAdapterCallback 桥接
///
/// 实现 `LarkEventHandler` trait，接收飞书原始事件，
/// 调用 `LarkMessageChannelDal.adapt_lark` 转换为 `AdaptedMessage`，
/// 再通过 `MessageAdapterCallback` 投递到上层。
struct LarkAdapterHandler {
    lark_dal: Arc<LarkMessageChannelDal>,
    callback: Arc<dyn MessageAdapterCallback>,
}

impl LarkAdapterHandler {
    fn new(_lark_dal: &LarkMessageChannelDal, callback: Arc<dyn MessageAdapterCallback>) -> Self {
        // 用 Weak 不行，因为我们需要 handler 持有 Arc 保证 lark_dal 存活
        // 但 lark_dal 本身是全局单例，不会被释放，所以直接 clone Arc 没问题
        // 这里用 unsafe 或其他方式都可以，直接用 dal() 获取单例更简单
        let lark_dal = dal();
        Self { lark_dal, callback }
    }
}

#[async_trait::async_trait]
impl LarkEventHandler for LarkAdapterHandler {
    async fn handle_message_event(&self, event: LarkMessageEvent) -> Result<()> {
        let ctx = RequestContext::new(None, None);
        let adapted = self
            .lark_dal
            .adapt_lark(ctx.clone(), &event)
            .await?;

        if let Some(msg) = adapted {
            self.callback.on_message(msg).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
pub mod test_support {
    //! 测试支持：构造可注入隔离依赖的 LarkMessageChannelDal

    use super::*;

    /// 创建测试用 LarkMessageChannelDal（不注册到全局 registry）
    pub fn new_for_test(
        message_channel_dal: Arc<dyn MessageChannelDal>,
        lark_dao: Arc<dyn LarkDao>,
    ) -> Arc<LarkMessageChannelDal> {
        new(message_channel_dal, lark_dao)
    }
}
