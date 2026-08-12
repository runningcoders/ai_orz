//! 邮件渠道 DAO SMTP 实现

use super::EmailDao;
use crate::models::message::Message;
use crate::models::message_channel::MessageChannel;
use crate::pkg::RequestContext;
use common::error::err;
use std::sync::{Arc, OnceLock};

// ==================== 工厂方法 + 单例 ====================

static EMAIL_DAO: OnceLock<Arc<dyn EmailDao>> = OnceLock::new();

/// 创建一个全新的邮件 DAO 实例（用于测试）
pub fn new() -> Arc<dyn EmailDao> {
    Arc::new(EmailDaoSmtpImpl::new())
}

/// 获取 EmailDao 单例
pub fn dao() -> Arc<dyn EmailDao> {
    EMAIL_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = EMAIL_DAO.set(new());
}

// ==================== 实现 ====================

struct EmailDaoSmtpImpl;

impl EmailDaoSmtpImpl {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl EmailDao for EmailDaoSmtpImpl {
    async fn push(
        &self,
        _ctx: RequestContext,
        _message: &Message,
        _channel: &MessageChannel,
        _options: &crate::models::message_channel::ChannelPushOptions,
    ) -> std::result::Result<(), common::error::Error> {
        // TODO: 实现邮件推送逻辑
        Err(err!(UnsupportedOperation, "邮件推送功能尚未实现"))
    }

    async fn test_connection(
        &self,
        _ctx: RequestContext,
        _channel: &MessageChannel,
    ) -> std::result::Result<(), common::error::Error> {
        // TODO: 实现邮件连接测试逻辑
        Err(err!(UnsupportedOperation, "邮件连接测试功能尚未实现"))
    }
}
