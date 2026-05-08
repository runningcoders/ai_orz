//! Slack 渠道 DAO HTTP 实现

use super::SlackDao;
use crate::models::message::Message;
use crate::models::message_channel::MessageChannel;
use crate::pkg::RequestContext;
use std::sync::{Arc, OnceLock};

// ==================== 工厂方法 + 单例 ====================

static SLACK_DAO: OnceLock<Arc<dyn SlackDao>> = OnceLock::new();

/// 创建一个全新的 Slack DAO 实例（用于测试）
pub fn new() -> Arc<dyn SlackDao> {
    Arc::new(SlackDaoHttpImpl::new())
}

/// 获取 SlackDao 单例
pub fn dao() -> Arc<dyn SlackDao> {
    SLACK_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = SLACK_DAO.set(new());
}

// ==================== 实现 ====================

struct SlackDaoHttpImpl;

impl SlackDaoHttpImpl {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl SlackDao for SlackDaoHttpImpl {
    async fn push(
        &self,
        _ctx: RequestContext,
        _message: &Message,
        _channel: &MessageChannel,
    ) -> std::result::Result<(), String> {
        // TODO: 实现 Slack 推送逻辑
        Err("Slack 推送功能尚未实现".to_string())
    }

    async fn test_connection(
        &self,
        _ctx: RequestContext,
        _channel: &MessageChannel,
    ) -> std::result::Result<(), String> {
        // TODO: 实现 Slack 连接测试逻辑
        Err("Slack 连接测试功能尚未实现".to_string())
    }
}
