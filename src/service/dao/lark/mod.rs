//! 飞书渠道 DAO 模块
//!
//! 负责飞书渠道的消息推送和事件接收，对飞书开放平台 API 的完整封装：
//! - HTTP API：tenant_access_token 获取、消息发送、连接测试
//! - WebSocket 长连接：订阅 `im.message.receive_v1` 事件
//!
//! 飞书 SDK 全部封装在本模块（DAO 层），符合"封装为 dao 即可"的架构决策。
//! 参考 `SsePushDao` 有状态 DAO 先例，本 DAO 管理 WebSocket 长连接状态。

use common::error::Result;
use crate::models::message::Message;
use crate::models::message_channel::MessageChannel;
use crate::pkg::RequestContext;
use std::sync::Arc;

pub mod error;
pub mod event;
pub mod http;
pub mod token;
pub mod ws;

pub use error::{LarkResponse, LarkWsError};
pub use event::{LarkMessageEvent, LarkTextContent};
pub use token::SharedTokenCache;

/// 飞书事件处理器 trait
///
/// 由外部消息适配层（adapter）实现，DAO 通过 trait 回调通知 adapter，
/// 避免 DAO 反向依赖 Domain 层。
#[async_trait::async_trait]
pub trait LarkEventHandler: Send + Sync {
    /// 处理飞书消息事件
    ///
    /// 实现方负责：
    /// - 过滤事件（仅处理 P2P 文本消息）
    /// - 查找信道信息
    /// - 路由到目标 Agent
    /// - 投递内部消息
    async fn handle_message_event(&self, event: LarkMessageEvent) -> Result<()>;
}

/// 飞书渠道 DAO 接口
///
/// 职责：
/// - `push`：推送消息到飞书用户（出站）
/// - `test_connection`：测试应用凭证是否可用
/// - `start_event_listener`：启动 WebSocket 长连接接收事件（入站）
/// - `stop_event_listener`：停止事件监听
#[async_trait::async_trait]
pub trait LarkDao: Send + Sync {
    /// 推送消息到飞书用户
    ///
    /// # 参数
    /// - `ctx`: 请求上下文
    /// - `message`: 消息实体
    /// - `channel`: 消息渠道配置（含 `lark_open_id`）
    async fn push(
        &self,
        ctx: RequestContext,
        message: &Message,
        channel: &MessageChannel,
    ) -> Result<()>;

    /// 测试飞书应用凭证是否可用（获取 tenant_access_token）
    async fn test_connection(
        &self,
        ctx: RequestContext,
        channel: &MessageChannel,
    ) -> Result<()>;

    /// 启动飞书事件监听（WebSocket 长连接）
    ///
    /// `handler` 由 adapter 层注入，DAO 通过 trait 回调通知，不依赖 Domain 层。
    /// 重复启动返回 `Conflict` 错误。
    async fn start_event_listener(&self, handler: Arc<dyn LarkEventHandler>) -> Result<()>;

    /// 停止事件监听
    ///
    /// 关闭 WebSocket 连接并等待任务退出。未启动时返回 Ok(())。
    async fn stop_event_listener(&self) -> Result<()>;
}

// ==================== 单例管理 ====================

pub use self::http::{dao, init, new, new_with_config};
