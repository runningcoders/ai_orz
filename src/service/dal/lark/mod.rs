//! 飞书 DAL（LarkDal）——总 trait + 单例管理
//!
//! 按消费面拆分为两个子 trait，由 [`LarkDalImpl`]（impl.rs）统一实现：
//! - [`LarkCredentialDal`]（credentials.rs）：凭据解析 + 渠道定位查询
//!   （消费方：runtime domain 凭据编排、finance domain 凭证删除联动）
//! - [`LarkListenerDal`]（listener.rs）：WS 监听生命周期 + 运行监控
//!   （消费方：finance domain 渠道联动、凭证变更联动、health metrics 聚合）
//!
//! [`LarkDal`] 组合两个子 trait，供同时消费两个面的调用方（finance domain）
//! 以 `Arc<dyn LarkDal>` 持有。
//!
//! 入站适配段（`adapt_lark` / `MessageInboundAdapter` / `LarkAdapterHandler`）
//! 留在 impl.rs：已通过 pkg `MessageInboundAdapter` trait 暴露，不建新口。
//!
//! # 多应用模型
//!
//! 启停由渠道数据驱动：`start()` 查询全部启用且开启入站监听的飞书渠道，
//! 按 app_id 去重后逐个建立 WebSocket 长连接；渠道增删改时由 Domain 层
//! 调用 `ensure_listener_for` / `release_listener_if_unused` 联动。

mod credentials;
mod listener;
mod r#impl;

pub use credentials::LarkCredentialDal;
pub use listener::LarkListenerDal;
pub use r#impl::LarkDalImpl;

use std::sync::{Arc, OnceLock};

use crate::service::dao::lark::LarkDao;
use crate::service::dao::user_credential::UserCredentialDao;
use crate::service::dal::message_channel::MessageChannelDal;

// ==================== 总 trait ====================

/// 飞书 DAL 总 trait
///
/// 组合凭据面（[`LarkCredentialDal`]）与监听面（[`LarkListenerDal`]），
/// 供同时消费两个面的调用方以 `Arc<dyn LarkDal>` 持有。
pub trait LarkDal: LarkCredentialDal + LarkListenerDal + Send + Sync {}

// ==================== 单例管理 ====================

static LARK_DAL: OnceLock<Arc<LarkDalImpl>> = OnceLock::new();

/// 获取 LarkDalImpl 单例
pub fn dal() -> Arc<LarkDalImpl> {
    LARK_DAL
        .get()
        .cloned()
        .expect("LarkDalImpl not initialized, call init() first")
}

/// 初始化 LarkDalImpl 并注册到消息适配中台
///
/// 无条件注册：飞书启停由渠道数据驱动（无渠道时 `start()` 不建任何连接）。
pub fn init() {
    let instance = new_with_credential_dao(
        crate::service::dal::message_channel::dal(),
        crate::service::dao::lark::dao(),
        crate::service::dao::user_credential::dao(),
    );
    // 注册到消息入站适配中台
    if let Err(e) = crate::pkg::adapter::message::registry().register(instance.clone()) {
        log_warn!("lark message adapter register skipped: {}", e);
    }
    let _ = LARK_DAL.set(instance);
    sys_info!("lark message adapter registered to adapter registry");
}

/// 创建 LarkDalImpl 实例（测试可注入隔离依赖）
pub fn new_with_credential_dao(
    message_channel_dal: Arc<dyn MessageChannelDal>,
    lark_dao: Arc<dyn LarkDao>,
    credential_dao: Arc<dyn UserCredentialDao>,
) -> Arc<LarkDalImpl> {
    Arc::new(LarkDalImpl::new(
        message_channel_dal,
        lark_dao,
        credential_dao,
    ))
}

#[cfg(test)]
pub mod test_support {
    //! 测试支持：构造可注入隔离依赖的 LarkDalImpl

    use super::*;

    /// 创建测试用 LarkDalImpl（不注册到全局 registry）
    pub fn new_for_test_with_credential_dao(
        message_channel_dal: Arc<dyn MessageChannelDal>,
        lark_dao: Arc<dyn LarkDao>,
        credential_dao: Arc<dyn UserCredentialDao>,
    ) -> Arc<LarkDalImpl> {
        new_with_credential_dao(message_channel_dal, lark_dao, credential_dao)
    }
}
