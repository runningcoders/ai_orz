//! 邮件 DAL（EmailDal）——总 trait + 单例管理
//!
//! 按消费面拆分为两个子 trait，由 [`EmailDalImpl`]（impl.rs）统一实现：
//! - [`EmailCredentialDal`]（credentials.rs）：渠道定位查询 + IMAP 凭证解析
//!   （消费方：finance domain 凭证删除联动、监听同步/重建）
//! - [`EmailListenerDal`]（listener.rs）：IMAP 受管轮询生命周期
//!   （消费方：finance domain 渠道联动、凭证变更联动）
//!
//! [`EmailDal`] 组合两个子 trait，供同时消费两个面的调用方（finance domain）
//! 以 `Arc<dyn EmailDal>` 持有。
//!
//! 入站适配段（`adapt_email` / `MessageInboundAdapter`）留在 impl.rs：
//! 已通过 pkg `MessageInboundAdapter` trait 暴露，不建新口；
//! 事件投递由 DAO 侧 publish AOP 事件，消费在 `consumer/email_inbound`。
//!
//! # 轮询单元与二维路由
//!
//! 启停由渠道数据驱动，但轮询单元是 **邮箱凭证**（一个代理邮箱可能被 N 个
//! 渠道共用）：`start()` 按 `email_credential_id` 去重后逐凭证建立 IMAP 轮询；
//! 入站邮件按 (credential_id, from == email_to_address) 二维路由到渠道，
//! 未命中不自动建渠。渠道增删改时由 Domain 层调用
//! `sync_listener_for_channel` / `release_listener_for_channel` 联动；
//! 停轮询前会确认凭证不再被其他"启用 + 开监听"渠道引用。

mod credentials;
mod r#impl;
mod listener;

pub use credentials::EmailCredentialDal;
pub use r#impl::EmailDalImpl;
pub use listener::EmailListenerDal;

use std::sync::{Arc, OnceLock};

use crate::service::dal::message_channel::MessageChannelDal;
use crate::service::dao::email::EmailDao;
use crate::service::dao::user_credential::UserCredentialDao;

// ==================== 总 trait ====================

/// 邮件 DAL 总 trait
///
/// 组合凭据面（[`EmailCredentialDal`]）与监听面（[`EmailListenerDal`]），
/// 供同时消费两个面的调用方以 `Arc<dyn EmailDal>` 持有。
pub trait EmailDal: EmailCredentialDal + EmailListenerDal + Send + Sync {}

// ==================== 单例管理 ====================

static EMAIL_DAL: OnceLock<Arc<EmailDalImpl>> = OnceLock::new();

/// 获取 EmailDalImpl 单例
pub fn dal() -> Arc<EmailDalImpl> {
    EMAIL_DAL
        .get()
        .cloned()
        .expect("EmailDalImpl not initialized, call init() first")
}

/// 初始化 EmailDalImpl 并注册到消息适配中台
///
/// 无条件注册：邮件启停由渠道数据驱动（无渠道时 `start()` 不建任何轮询）。
pub fn init() {
    let instance = new_with_dao(
        crate::service::dal::message_channel::dal(),
        crate::service::dao::email::dao(),
        crate::service::dao::user_credential::dao(),
        crate::service::dao::message::new(),
    );
    // 注册到消息入站适配中台
    if let Err(e) = crate::pkg::adapter::message::registry().register(instance.clone()) {
        log_warn!("email message adapter register skipped: {}", e);
    }
    // 注册 AOP 生产者：`email.inbound.message` 的收尾归属（见 impl.rs 的 Producer impl ——
    // 消费确认后推进 IMAP 游标，修 P2）。
    // 冲突（topic 已被占用）不静默 —— `start_all` 的 §6.2 校验会在缺生产者时启动失败。
    let as_producer: Arc<dyn crate::pkg::aop::Producer> = instance.clone();
    if let Err(e) = crate::pkg::aop::registry().register_producer(as_producer) {
        log_warn!("email inbound producer register skipped: {}", e);
    }
    let _ = EMAIL_DAL.set(instance);
    sys_info!("email message adapter registered to adapter registry");
}

/// 创建 EmailDalImpl 实例（测试可注入隔离依赖）
pub fn new_with_dao(
    message_channel_dal: Arc<dyn MessageChannelDal>,
    email_dao: Arc<dyn EmailDao>,
    credential_dao: Arc<dyn UserCredentialDao>,
    message_dao: Arc<dyn crate::service::dao::message::MessageDao>,
) -> Arc<EmailDalImpl> {
    Arc::new(EmailDalImpl::new(
        message_channel_dal,
        email_dao,
        credential_dao,
        message_dao,
    ))
}
