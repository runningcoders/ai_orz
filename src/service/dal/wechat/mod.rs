//! 微信渠道 DAL（WechatDal）——总 trait + 单例管理
//!
//! 按消费面拆分为两个子 trait，由 [`WechatDalImpl`]（impl.rs）统一实现：
//! - [`WechatCredentialDal`]（credentials.rs）：凭证解析（引用模式）+ 渠道定位查询
//!   （消费方：finance domain 凭证删除联动、message_channel DAL 出站凭证解析）
//! - [`WechatListenerDal`]（listener.rs）：长轮询生命周期
//!   （消费方：finance domain 渠道联动、凭证变更联动）
//!
//! 入站适配段（`adapt_wechat` / `MessageInboundAdapter`）留在 impl.rs：
//! 已通过 pkg `MessageInboundAdapter` trait 暴露，不建新口；
//! 事件投递由 DAO 侧 publish AOP 事件，消费在 `consumer/wechat_inbound`。
//!
//! # 数据驱动模型（对齐 lark）
//!
//! 启停由渠道数据驱动：`start()` 查询全部启用且开启入站监听的微信渠道，
//! 逐个按引用凭证建立长轮询（channel_id 键控，一个 bot 微信号 = 一个 channel）。

mod credentials;
mod r#impl;
mod listener;

pub use credentials::WechatCredentialDal;
pub use r#impl::WechatDalImpl;
pub use listener::WechatListenerDal;

use std::sync::{Arc, OnceLock};

// ==================== 总 trait ====================

/// 微信 DAL 总 trait
///
/// 组合凭据面（[`WechatCredentialDal`]）与监听面（[`WechatListenerDal`]），
/// 供同时消费两个面的调用方（finance domain）以 `Arc<dyn WechatDal>` 持有。
pub trait WechatDal: WechatCredentialDal + WechatListenerDal + Send + Sync {}

// ==================== 单例管理 ====================

static WECHAT_DAL: OnceLock<Arc<WechatDalImpl>> = OnceLock::new();

/// 获取 WechatDalImpl 单例
pub fn dal() -> Arc<WechatDalImpl> {
    WECHAT_DAL
        .get()
        .cloned()
        .expect("WechatDalImpl not initialized, call init() first")
}

/// 初始化 WechatDalImpl 并注册到消息适配中台
///
/// 无条件注册：微信启停由渠道数据驱动（无渠道时不建任何长轮询）。
pub fn init() {
    let instance = new_with_credential_dao(
        crate::service::dal::message_channel::dal(),
        crate::service::dao::wechat::dao(),
        crate::service::dao::user_credential::dao(),
    );
    // 注册到消息入站适配中台
    if let Err(e) = crate::pkg::adapter::message::registry().register(instance.clone()) {
        log_warn!("wechat message adapter register skipped: {}", e);
    }
    let _ = WECHAT_DAL.set(instance);
    sys_info!("wechat message adapter registered to adapter registry");
}

/// 创建 WechatDalImpl 实例（测试可注入隔离依赖）
pub fn new_with_credential_dao(
    message_channel_dal: Arc<dyn crate::service::dal::message_channel::MessageChannelDal>,
    wechat_dao: Arc<dyn crate::service::dao::wechat::WechatDao>,
    credential_dao: Arc<dyn crate::service::dao::user_credential::UserCredentialDao>,
) -> Arc<WechatDalImpl> {
    Arc::new(WechatDalImpl::new(
        message_channel_dal,
        wechat_dao,
        credential_dao,
    ))
}
