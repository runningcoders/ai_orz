//! A2A Callback 渠道 DAO HTTP 实现
//!
//! 纯 HTTP 出站：接收组装好的 A2aTask 快照并 POST 到 webhook_url。
//! 项目/消息查询与状态映射等业务组装见 `dal/message_channel.rs`。

use super::A2aCallbackDao;
use crate::pkg::RequestContext;
use common::api::a2a::A2aTask;
use common::error::err;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// webhook 回调超时：回调是「通知」语义，对端处理慢不应拖住消息推送链路
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(10);

// ==================== 工厂方法 + 单例 ====================

static A2A_CALLBACK_DAO: OnceLock<Arc<dyn A2aCallbackDao>> = OnceLock::new();

/// 创建一个全新的 A2A Callback DAO 实例（用于测试）
pub fn new() -> Arc<dyn A2aCallbackDao> {
    Arc::new(A2aCallbackDaoHttpImpl::new())
}

/// 获取 A2aCallbackDao 单例
pub fn dao() -> Arc<dyn A2aCallbackDao> {
    A2A_CALLBACK_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = A2A_CALLBACK_DAO.set(new());
}

// ==================== 实现 ====================

struct A2aCallbackDaoHttpImpl {
    /// 共享 HTTP 客户端：此前每次回调都 `Client::new()`，等于每次新建连接池
    http: reqwest::Client,
}

impl A2aCallbackDaoHttpImpl {
    fn new() -> Self {
        Self {
            http: crate::pkg::http::presets::with_timeout(Some(CALLBACK_TIMEOUT))
                .build()
                .expect("构建 A2A callback HTTP 客户端失败"),
        }
    }
}

#[async_trait::async_trait]
impl A2aCallbackDao for A2aCallbackDaoHttpImpl {
    async fn push_task(
        &self,
        _ctx: RequestContext,
        webhook_url: &str,
        task: &A2aTask,
    ) -> std::result::Result<(), common::error::Error> {
        let body = serde_json::to_string(task)
            .map_err(|e| err!(Internal, "序列化 A2A Task 失败: {}", e))?;

        let resp = self
            .http
            .post(webhook_url)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
            .map_err(|e| err!(ChannelPushFailed, "A2A callback 请求失败: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(err!(
                ChannelPushFailed,
                "A2A callback 返回错误状态码 {}: {}",
                status,
                body
            ));
        }

        Ok(())
    }
}
