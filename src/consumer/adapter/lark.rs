//! 飞书事件分发器（LarkEventDispatcher）
//!
//! 实现 `LarkEventHandler` trait，作为飞书 WebSocket 事件的回调入口。
//! 编排流程：
//! 1. 从 `pkg/adapter` 注册中心获取 `LarkMessageChannelDal`
//! 2. 调用 `dal.adapt_lark(event)` 得到 `Option<AdaptedMessage>`
//! 3. 若 `to_agent_id` 为 `None`，通过 `HrDomain::AgentManage::query` 路由
//!    - 优先 `feishu_reception` 角色的 Onboarded Agent
//!    - 兜底任意 Onboarded Agent
//! 4. 构造 `SendToAgentCommand` 并调用 `MessageDomain::send_to_agent`
//!
//! # 分层职责
//!
//! - **LarkMessageChannelDal（DAL 层）**：纯数据访问 + 事件转换，不做 Agent 路由
//! - **LarkEventDispatcher（consumer 层，本模块）**：业务编排
//!   - 调用 DAL 适配
//!   - 通过 `HrDomain` 查询 Agent（遵循 consumer → domain → dal 分层）
//!   - 通过 `MessageDomain` 发送消息

use std::sync::Arc;

use common::config::AppConfig;
use common::enums::{AgentStatus, ChannelType};
use common::error::{err, Result};

use crate::pkg::adapter::registry;
use crate::pkg::RequestContext;
use crate::service::dao::agent::AgentQuery;
use crate::service::dao::lark::{LarkDao, LarkEventHandler, LarkMessageEvent};
use crate::service::dal::lark::LarkMessageChannelDal;
use crate::service::domain::hr::{AgentManage, HrDomain};
use crate::service::domain::message::{MessageDomain, SendToAgentCommand};

/// 飞书前台 Agent 的角色标签
///
/// 携带此标签的 Onboarded Agent 优先作为飞书消息的路由目标。
pub const FEISHU_RECEPTION_ROLE: &str = "feishu_reception";

// ==================== 单例管理 ====================

static DISPATCHER: std::sync::OnceLock<Arc<LarkEventDispatcher>> = std::sync::OnceLock::new();
static LARK_DAO_HANDLE: std::sync::OnceLock<Arc<dyn LarkDao>> = std::sync::OnceLock::new();

/// 初始化飞书事件分发器并启动 WebSocket 事件监听
pub async fn init(_config: &AppConfig) -> Result<()> {
    // 1. 从注册中心获取 LarkMessageChannelDal 适配者
    let lark_dal = registry()
        .get::<LarkMessageChannelDal>(ChannelType::Lark)
        .ok_or_else(|| err!(Internal, "LarkMessageChannelDal not registered"))?;

    // 2. 获取 HrDomain（Agent 路由）和 MessageDomain（消息发送）
    let hr_domain = crate::service::domain::hr::domain();
    let message_domain = crate::service::domain::message::domain();

    // 3. 构造 dispatcher
    let dispatcher = Arc::new(LarkEventDispatcher {
        lark_dal,
        hr_domain,
        message_domain,
    });
    let _ = DISPATCHER.set(dispatcher.clone());

    // 4. 启动 LarkDao 事件监听，注入 dispatcher 作为回调
    let lark_dao = crate::service::dao::lark::dao();
    lark_dao
        .start_event_listener(dispatcher)
        .await
        .map_err(|e| err!(Internal, "start lark event listener failed: {}", e))?;
    let _ = LARK_DAO_HANDLE.set(lark_dao);

    sys_info!("lark event dispatcher initialized");
    Ok(())
}

/// 关闭飞书事件监听
pub async fn shutdown() {
    if let Some(lark_dao) = LARK_DAO_HANDLE.get() {
        if let Err(e) = lark_dao.stop_event_listener().await {
            sys_warn!("stop lark event listener error: {}", e);
        }
    }
}

// ==================== Dispatcher 实现 ====================

/// 飞书事件分发器
///
/// 实现 `LarkEventHandler` trait，编排适配 + 路由 + 发送。
pub struct LarkEventDispatcher {
    /// 飞书消息渠道 DAL（适配者，提供 adapt_lark）
    lark_dal: Arc<LarkMessageChannelDal>,
    /// HR Domain（用于 Agent 路由查询）
    hr_domain: Arc<dyn HrDomain>,
    /// Message Domain（用于发送适配后的内部消息）
    message_domain: Arc<dyn MessageDomain>,
}

#[async_trait::async_trait]
impl LarkEventHandler for LarkEventDispatcher {
    async fn handle_message_event(&self, event: LarkMessageEvent) -> Result<()> {
        let ctx = RequestContext::new(None, None);
        let event_id = event.header.event_id.clone();

        // 1. 调用适配者转换事件
        let adapted = match self.lark_dal.adapt_lark(ctx.clone(), &event).await? {
            Some(msg) => msg,
            None => {
                log_debug!(
                    &ctx,
                    "lark_dispatch",
                    "event filtered or unbound: event_id={}",
                    event_id
                );
                return Ok(());
            }
        };

        // 2. Agent 路由：渠道未绑定时通过 HrDomain 查询
        let to_agent_id = match adapted.to_agent_id {
            Some(id) => id,
            None => match self.find_reception_agent_id(ctx.clone()).await? {
                Some(id) => id,
                None => {
                    log_warn!(
                        &ctx,
                        "lark_dispatch",
                        "no available onboarded agent for routing event_id={}",
                        event_id
                    );
                    return Ok(());
                }
            },
        };

        // 3. 构造 SendToAgentCommand 并发送
        let cmd = SendToAgentCommand {
            from_id: &adapted.from_id,
            from_role: adapted.from_role,
            to_agent_id: &to_agent_id,
            content: &adapted.content,
            project_id: adapted.project_id.as_deref(),
            task_id: adapted.task_id.as_deref(),
            reply_to_id: adapted.reply_to_id.as_deref(),
            attachment_ids: None,
        };

        self.message_domain
            .delivery()
            .send_to_agent(ctx.clone(), cmd)
            .await
            .map_err(|e| {
                err!(
                    Internal,
                    "lark dispatch send_to_agent failed event_id={}: {}",
                    event_id,
                    e
                )
            })?;

        log_info!(
            &ctx,
            "lark_dispatch",
            "event dispatched: event_id={} from={} to_agent={}",
            event_id,
            adapted.from_id,
            to_agent_id
        );
        Ok(())
    }
}

impl LarkEventDispatcher {
    /// 查找路由目标 Agent ID
    ///
    /// 优先级：
    /// 1. 带 `feishu_reception` 角色的 Onboarded Agent
    /// 2. 任意 Onboarded Agent
    ///
    /// 通过 `HrDomain::AgentManage::query` 访问 Agent 数据，遵循分层约束。
    async fn find_reception_agent_id(
        &self,
        ctx: RequestContext,
    ) -> Result<Option<String>> {
        // 优先：带 feishu_reception 角色的 Onboarded Agent
        let query = AgentQuery {
            roles: Some(vec![FEISHU_RECEPTION_ROLE.to_string()]),
            status: Some(AgentStatus::Onboarded),
            limit: Some(1),
            ..Default::default()
        };
        let agents = self
            .hr_domain
            .agent_manage()
            .query(ctx.clone(), query)
            .await?;
        if let Some(agent) = agents.into_iter().next() {
            return Ok(Some(agent.po.id));
        }

        // 兜底：任意 Onboarded Agent
        let query = AgentQuery {
            status: Some(AgentStatus::Onboarded),
            limit: Some(1),
            ..Default::default()
        };
        let agents = self
            .hr_domain
            .agent_manage()
            .query(ctx, query)
            .await?;
        Ok(agents.into_iter().next().map(|a| a.po.id))
    }
}
