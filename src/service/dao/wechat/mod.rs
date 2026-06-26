
//! 微信渠道 DAO 模块
//! 负责微信渠道的消息推送和连接测试。
//! 对微信开放平台 API 的封装，支持后续协议版本适配。

use common::error::Result;
use crate::models::message::Message;
use crate::models::message_channel::MessageChannel;
use crate::pkg::RequestContext;

/// 微信渠道 DAO 接口
#[async_trait::async_trait]
pub trait WechatDao: Send + Sync {
    /// 推送消息到微信
    ///
    /// # 参数
    /// - `ctx`: 请求上下文
    /// - `message`: 消息实体
    /// - `channel`: 消息渠道配置
    ///
    /// # 返回
    /// - `Ok(())`: 推送成功
    /// - `Err(String)`: 推送失败，返回错误信息
    async fn push(
        &self,
        ctx: RequestContext,
        message: &Message,
        channel: &MessageChannel,
    ) -> std::result::Result<(), common::error::Error>;

    /// 测试微信渠道连接
    ///
    /// # 参数
    /// - `ctx`: 请求上下文
    /// - `channel`: 消息渠道配置
    ///
    /// # 返回
    /// - `Ok(())`: 连接成功
    /// - `Err(String)`: 连接失败，返回错误信息
    async fn test_connection(
        &self,
        ctx: RequestContext,
        channel: &MessageChannel,
    ) -> std::result::Result<(), common::error::Error>;
}

pub mod http;
pub use self::http::{dao, init, new};
