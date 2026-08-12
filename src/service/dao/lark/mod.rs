//! 飞书渠道 DAO 模块
//!
//! 负责飞书渠道的消息推送和事件接收，对飞书开放平台 API 的完整封装：
//! - HTTP API：tenant_access_token 获取、消息发送、连接测试
//! - WebSocket 长连接：订阅 `im.message.receive_v1` 事件
//!
//! 飞书 SDK 全部封装在本模块（DAO 层），符合"封装为 dao 即可"的架构决策。
//! 参考 `SsePushDao` 有状态 DAO 先例，本 DAO 管理 WebSocket 长连接状态。
//!
//! # 多应用模型
//!
//! 凭证不再来自全局配置，而是按调用传入（渠道配置）：
//! - token 缓存按 app_id 键控（per-app）
//! - WS 长连接按 app_id 键控，一个自建应用一条连接

use crate::models::message::Message;
use crate::models::message_channel::{ChannelPushOptions, MessageChannel};
use crate::pkg::RequestContext;
use common::api::LarkWsMetrics;
use common::error::{Result, err};
use common::models::{CredentialDetail, CredentialKind, UserIdentityCredentials};
use std::sync::Arc;

pub mod error;
pub mod event;
pub mod http;
pub mod token;
pub mod ws;

pub use error::{LarkResponse, LarkWsError};
pub use event::{LarkMessageEvent, LarkTextContent};
pub use token::SharedTokenCache;

/// 飞书自建应用凭证（轻量结构，由渠道配置构造）
#[derive(Clone)]
pub struct LarkAppCredentials {
    /// 飞书 App ID
    pub app_id: String,
    /// 飞书 App Secret
    pub app_secret: String,
}

impl std::fmt::Debug for LarkAppCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 脱敏：绝不输出 app_secret 明文
        f.debug_struct("LarkAppCredentials")
            .field("app_id", &self.app_id)
            .field("app_secret", &"***")
            .finish()
    }
}

/// 从用户凭证库解析渠道引用的飞书应用凭证（纯函数，可测）
///
/// 解析规则：渠道 `lark_credential_id` → 凭证库按 ID 查找 → 校验 kind=LarkApp → 解密 secret；
/// 任一环节失败返回引导性错误（前端提示去飞书集成页绑定/补全凭证）。
pub fn resolve_lark_credentials(
    library: &UserIdentityCredentials,
    channel: &MessageChannel,
) -> Result<LarkAppCredentials> {
    let credential_id = channel
        .config()
        .lark_credential_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            err!(
                InvalidRequest,
                "飞书渠道缺少凭证引用 lark_credential_id channel_id={}，请先在飞书集成中绑定应用",
                channel.po.id
            )
        })?;
    let credential = library.find_by_id(credential_id).ok_or_else(|| {
        err!(
            InvalidRequest,
            "飞书渠道引用的凭证不存在 channel_id={} credential_id={}，请在飞书集成中重新绑定",
            channel.po.id,
            credential_id
        )
    })?;
    if credential.kind != CredentialKind::LarkApp {
        return Err(err!(
            InvalidRequest,
            "飞书渠道引用的凭证类型不匹配 channel_id={} credential_id={}",
            channel.po.id,
            credential_id
        ));
    }
    let CredentialDetail::LarkApp {
        app_id, app_secret, ..
    } = &credential.detail;
    let app_secret = crate::pkg::crypto::decrypt_channel_secret(app_secret).map_err(|e| {
        err!(
            Internal,
            "飞书凭证 app_secret 解密失败 channel_id={} credential_id={}: {}",
            channel.po.id,
            credential_id,
            e
        )
    })?;
    Ok(LarkAppCredentials {
        app_id: app_id.clone(),
        app_secret,
    })
}

/// 飞书事件处理器 trait
///
/// 由外部消息适配层（adapter）实现，DAO 通过 trait 回调通知 adapter，
/// 避免 DAO 反向依赖 Domain 层。
#[async_trait::async_trait]
pub trait LarkEventHandler: Send + Sync {
    /// 处理飞书消息事件
    ///
    /// # 参数
    /// - `app_id`: 事件归属的飞书应用（多应用路由依据）
    /// - `event`: 飞书消息事件
    ///
    /// 实现方负责：
    /// - 过滤事件（仅处理 P2P 文本消息）
    /// - 按 app_id + open_id 查找归属渠道
    /// - 路由到目标 Agent
    /// - 投递内部消息
    async fn handle_message_event(&self, app_id: &str, event: LarkMessageEvent) -> Result<()>;
}

/// 飞书渠道 DAO 接口
///
/// 职责：
/// - `push`：推送消息到飞书用户（出站，凭证取自渠道配置）
/// - `test_connection`：测试渠道凭证是否可用
/// - `start_event_listener`：按应用启动 WebSocket 长连接接收事件（入站）
/// - `stop_event_listener` / `stop_all_event_listeners`：停止事件监听
#[async_trait::async_trait]
pub trait LarkDao: Send + Sync {
    /// 推送消息到飞书用户
    ///
    /// # 参数
    /// - `ctx`: 请求上下文
    /// - `message`: 消息实体
    /// - `channel`: 消息渠道配置（含 `lark_credential_id`/`lark_open_id`）
    /// - `options`: 推送选项（携带归属用户时从其凭证库解析，否则兜底查库）
    async fn push(
        &self,
        ctx: RequestContext,
        message: &Message,
        channel: &MessageChannel,
        options: &ChannelPushOptions,
    ) -> Result<()>;

    /// 测试飞书渠道凭证是否可用（获取 tenant_access_token）
    async fn test_connection(&self, ctx: RequestContext, channel: &MessageChannel) -> Result<()>;

    /// 启动指定应用的飞书事件监听（WebSocket 长连接）
    ///
    /// `handler` 由 adapter 层注入，DAO 通过 trait 回调通知，不依赖 Domain 层。
    /// 按 app_id 去重：已连接时幂等返回 Ok。
    async fn start_event_listener(
        &self,
        credentials: LarkAppCredentials,
        handler: Arc<dyn LarkEventHandler>,
    ) -> Result<()>;

    /// 停止指定应用的事件监听
    ///
    /// 关闭 WebSocket 连接并等待任务退出。未启动时返回 Ok(())。
    async fn stop_event_listener(&self, app_id: &str) -> Result<()>;

    /// 停止全部应用的事件监听（优雅退出）
    async fn stop_all_event_listeners(&self) -> Result<()>;

    /// 查询指定应用是否正在监听
    async fn is_listening(&self, app_id: &str) -> bool;

    /// 全量 WS 连接监控快照（挂入 health metrics）
    async fn listener_stats(&self) -> LarkWsMetrics;
}

// ==================== 单例管理 ====================

pub use self::http::{dao, init, new};
