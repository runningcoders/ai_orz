//! 微信渠道 DAO HTTP 实现

use super::WechatDao;
use crate::models::message::Message;
use crate::models::message_channel::MessageChannel;
use crate::pkg::RequestContext;
use common::error::err;
use std::sync::{Arc, OnceLock};

// ==================== 工厂方法 + 单例 ====================

static WECHAT_DAO: OnceLock<Arc<dyn WechatDao>> = OnceLock::new();

/// 创建一个全新的微信 DAO 实例（用于测试）
pub fn new() -> Arc<dyn WechatDao> {
    Arc::new(WechatDaoHttpImpl::new())
}

/// 获取 WechatDao 单例
pub fn dao() -> Arc<dyn WechatDao> {
    WECHAT_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = WECHAT_DAO.set(new());
}

// ==================== 实现 ====================

struct WechatDaoHttpImpl;

impl WechatDaoHttpImpl {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl WechatDao for WechatDaoHttpImpl {
    async fn push(
        &self,
        _ctx: RequestContext,
        _message: &Message,
        _channel: &MessageChannel,
        _options: &crate::models::message_channel::ChannelPushOptions,
    ) -> std::result::Result<(), common::error::Error> {
        // TODO: 实现微信推送逻辑
        Err(err!(UnsupportedOperation, "微信推送功能尚未实现"))
    }

    async fn test_connection(
        &self,
        _ctx: RequestContext,
        _channel: &MessageChannel,
    ) -> std::result::Result<(), common::error::Error> {
        // TODO: 实现微信连接测试逻辑
        Err(err!(UnsupportedOperation, "微信连接测试功能尚未实现"))
    }
}
