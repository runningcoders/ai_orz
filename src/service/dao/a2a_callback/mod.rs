//! A2A Callback 渠道 DAO 模块
//!
//! 负责 A2A PushNotifications 回调推送，将消息变更推送到客户端注册的 notification_url。
//! 推送格式为完整的 A2A Task JSON，包含任务状态和消息历史。

use crate::models::message::Message;
use crate::models::message_channel::MessageChannel;
use crate::pkg::RequestContext;

/// A2A Callback 渠道 DAO 接口
#[async_trait::async_trait]
pub trait A2aCallbackDao: Send + Sync {
    /// 推送消息到 A2A Callback URL
    ///
    /// 每次推送都会查询完整的任务状态和消息历史，构建 A2A Task JSON 后发送。
    ///
    /// # 参数
    /// - `ctx`: 请求上下文
    /// - `message`: 触发推送的消息实体
    /// - `channel`: 消息渠道配置（含 webhook_url 和 scope_project）
    ///
    /// # 返回
    /// - `Ok(())`: 推送成功
    /// - `Err(Error)`: 推送失败，返回错误信息
    async fn push(
        &self,
        ctx: RequestContext,
        message: &Message,
        channel: &MessageChannel,
    ) -> std::result::Result<(), common::error::Error>;

    /// 测试 A2A Callback 渠道连接
    ///
    /// # 参数
    /// - `ctx`: 请求上下文
    /// - `channel`: 消息渠道配置
    ///
    /// # 返回
    /// - `Ok(())`: 连接成功
    /// - `Err(Error)`: 连接失败，返回错误信息
    async fn test_connection(
        &self,
        ctx: RequestContext,
        channel: &MessageChannel,
    ) -> std::result::Result<(), common::error::Error>;
}

pub mod http;
pub use self::http::{dao, init, new};
