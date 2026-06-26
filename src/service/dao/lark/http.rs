//! 飞书渠道 DAO HTTP 实现

use super::LarkDao;
use crate::models::message::Message;
use crate::models::message_channel::MessageChannel;
use crate::pkg::RequestContext;
use common::error::err;
use std::sync::{Arc, OnceLock};

// ==================== 工厂方法 + 单例 ====================

static LARK_DAO: OnceLock<Arc<dyn LarkDao>> = OnceLock::new();

/// 创建一个全新的飞书 DAO 实例（用于测试）
pub fn new() -> Arc<dyn LarkDao> {
    Arc::new(LarkDaoHttpImpl::new())
}

/// 获取 LarkDao 单例
pub fn dao() -> Arc<dyn LarkDao> {
    LARK_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = LARK_DAO.set(new());
}

// ==================== 实现 ====================

struct LarkDaoHttpImpl;

impl LarkDaoHttpImpl {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl LarkDao for LarkDaoHttpImpl {
    async fn push(
        &self,
        _ctx: RequestContext,
        _message: &Message,
        _channel: &MessageChannel,
    ) -> std::result::Result<(), common::error::Error> {
        // TODO: 实现飞书推送逻辑
        Err(err!(UnsupportedOperation, "飞书推送功能尚未实现"))
    }

    async fn test_connection(
        &self,
        _ctx: RequestContext,
        _channel: &MessageChannel,
    ) -> std::result::Result<(), common::error::Error> {
        // TODO: 实现飞书连接测试逻辑
        Err(err!(UnsupportedOperation, "飞书连接测试功能尚未实现"))
    }
}
