//! 通用 Webhook 渠道 DAO HTTP 实现

use super::WebhookDao;
use crate::models::message::Message;
use crate::models::message_channel::MessageChannel;
use crate::pkg::RequestContext;
use std::sync::{Arc, OnceLock};
use common::bail_err;

// ==================== 工厂方法 + 单例 ====================

static WEBHOOK_DAO: OnceLock<Arc<dyn WebhookDao>> = OnceLock::new();

/// 创建一个全新的通用 Webhook DAO 实例（用于测试）
pub fn new() -> Arc<dyn WebhookDao> {
    Arc::new(WebhookDaoHttpImpl::new())
}

/// 获取 WebhookDao 单例
pub fn dao() -> Arc<dyn WebhookDao> {
    WEBHOOK_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = WEBHOOK_DAO.set(new());
}

// ==================== 实现 ====================

struct WebhookDaoHttpImpl;

impl WebhookDaoHttpImpl {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl WebhookDao for WebhookDaoHttpImpl {
    async fn push(
        &self,
        _ctx: RequestContext,
        _message: &Message,
        _channel: &MessageChannel,
    ) -> std::result::Result<(), common::error::Error> {
        // TODO: 实现通用 Webhook 推送逻辑
        Err("通用 Webhook 推送功能尚未实现".to_string())
    }

    async fn test_connection(
        &self,
        _ctx: RequestContext,
        _channel: &MessageChannel,
    ) -> std::result::Result<(), common::error::Error> {
        // TODO: 实现通用 Webhook 连接测试逻辑
        Err("通用 Webhook 连接测试功能尚未实现".to_string())
    }
}
