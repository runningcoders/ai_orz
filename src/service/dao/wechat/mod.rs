//! 微信渠道 DAO 模块
//!
//! 负责微信渠道（iLink，阶段一）的消息推送与入站长轮询：
//! - 出站 `push`：`sendmessage`（context_token / 对端标识取自渠道，凭证由 DAL 解析传入）
//! - 入站：受管长轮询循环（`ilink.rs` registry，收帧 publish AOP 事件）
//!
//! 对 iLink 协议的封装集中在 `ilink.rs`（协议变更只影响该文件），
//! 详见 docs/design/wechat_channel_integration_design.md §5。

use crate::models::message_channel::MessageChannel;
use crate::pkg::RequestContext;

use self::ilink::IlinkChannelCredentials;

/// iLink 接入域默认值（登录响应 baseurl 优先）
pub use crate::pkg::wechat_ilink::ILINK_DEFAULT_BASE_URL;

/// 微信渠道 DAO 接口
///
/// 分层约束：DAO 不做凭证解析——出站与轮询启动均由 DAL 层按渠道
/// `wechat_credential_id` 引用解析出 [`IlinkChannelCredentials`] 后传入。
/// 入站事件经 AOP 事件总线（`WechatInboundEvent`）二次分发，DAO 不回调任何业务方。
#[async_trait::async_trait]
pub trait WechatDao: Send + Sync {
    /// 推送文本消息到微信对端（iLink `sendmessage`）
    ///
    /// 对端标识：渠道 `wechat_peer_id`，未配置时回落最近活跃会话；
    /// `context_token` 取自 `inbound_state.sessions`（会话令牌滚动刷新）。
    async fn push(
        &self,
        ctx: RequestContext,
        message: &crate::models::message::Message,
        channel: &MessageChannel,
        credentials: &IlinkChannelCredentials,
    ) -> std::result::Result<(), common::error::Error>;

    /// 测试微信渠道凭证可用性（凭证完整性校验；真实连通性待联调补协议探测）
    async fn test_connection(
        &self,
        ctx: RequestContext,
        credentials: &IlinkChannelCredentials,
    ) -> std::result::Result<(), common::error::Error>;

    /// 确保指定渠道的入站长轮询在运行（幂等；凭证指纹变化时停旧重建）
    async fn start_polling(
        &self,
        channel: &MessageChannel,
        credentials: &IlinkChannelCredentials,
    ) -> std::result::Result<(), common::error::Error>;

    /// 停止指定渠道的入站长轮询（未运行时幂等）
    async fn stop_polling(&self, channel_id: &str)
    -> std::result::Result<(), common::error::Error>;

    /// 停止全部入站长轮询（优雅退出）
    async fn stop_all_polling(&self) -> std::result::Result<(), common::error::Error>;

    /// 指定渠道是否正在轮询
    async fn is_polling(&self, channel_id: &str) -> bool;
}

pub mod http;
pub mod ilink;
pub use self::http::{dao, init, new};
