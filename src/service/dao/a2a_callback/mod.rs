//! A2A Callback 渠道 DAO 模块
//!
//! 负责 A2A PushNotifications 回调推送（纯 HTTP 出站）：将组装好的完整
//! A2A Task 快照 POST 到客户端注册的 webhook_url。
//!
//! 业务组装（项目/消息历史查询、状态映射、A2aTask 构建）在 DAL 层完成，
//! DAO 不做业务查询——严格单向依赖。

use crate::pkg::RequestContext;
use common::api::a2a::A2aTask;

/// A2A Callback 渠道 DAO 接口
#[async_trait::async_trait]
pub trait A2aCallbackDao: Send + Sync {
    /// 推送完整 A2A Task 快照到 webhook
    ///
    /// # 参数
    /// - `ctx`: 请求上下文
    /// - `webhook_url`: 对端注册的回调地址（从渠道配置解析，由 DAL 传入）
    /// - `task`: 组装好的 A2A Task（任务状态 + 消息历史）
    ///
    /// # 返回
    /// - `Ok(())`: 推送成功
    /// - `Err(Error)`: 推送失败，返回错误信息
    async fn push_task(
        &self,
        ctx: RequestContext,
        webhook_url: &str,
        task: &A2aTask,
    ) -> std::result::Result<(), common::error::Error>;
}

pub mod http;
pub use self::http::{dao, init, new};
