//! AOP 生产-消费事件中心（纯框架，无业务逻辑）
//!
//! 统一事件分发框架，核心概念：
//! - Event: 事件（携带数据，纯数据结构）
//! - Consumer: 消费者 trait（业务层实现，调用 domain 完成业务逻辑）
//! - Registry: 注册中心（分发事件 + 调度异步消费）
//! - Queue: 底层队列（异步消费存储）
//!
//! 架构原则：AOP 层只负责事件流转和调度，**不感知任何业务实体**
//! （domain/dal/dao）。业务消费者由 `consumer/` 业务层实现并注册。
//!
//! 使用方式：
//! ```ignore
//! // 发布事件（必须传入当前请求 ctx，用于链路串联）
//! aop::publish(&ctx, MyEvent { ... }).await;
//!
//! // 业务层注册消费者（通常在 consumer::init 中完成）
//! aop::registry().register_consumer(Arc::new(MyConsumer)).unwrap();
//!
//! // 启动调度器（由 AOP init_all 调用）
//! aop::registry().start_all().await?;
//! ```

pub mod core;
pub mod queue;

// 重导出核心 API
pub use core::{
    AopEventMeta, AopMetricsHook, ConsumeMode, Consumer, DEFAULT_MAX_ATTEMPTS, Event, EventSink,
    Producer, ProducerLoop, Registry, RetryDecision, Subscription,
};
pub use queue::EventQueue;

use crate::pkg::request_context::RequestContext;
use once_cell::sync::Lazy;
use std::sync::Arc;

/// 全局 Registry 单例（Arc 包装，允许 worker 协程持有引用）
static REGISTRY: Lazy<Arc<Registry>> = Lazy::new(|| Arc::new(Registry::new()));

/// 获取全局 Registry
pub fn registry() -> &'static Registry {
    &REGISTRY
}

/// 发布事件（便捷方法）
///
/// 调用方必须传入当前请求 ctx，框架会从中抽取可传输子 context（[`ContextCarrier`]）
/// 随事件流转，使消费侧能重建出同源 ctx（保留 log_id 等链路线索）。
///
/// [`ContextCarrier`]: crate::pkg::request_context::ContextCarrier
pub async fn publish<E: Event>(ctx: &RequestContext, event: E) {
    REGISTRY.publish(ctx, event).await
}

/// 启动 AOP 调度器
///
/// 启动异步消费者的 worker，并逐个 `Producer::start(sink)`（把发布句柄交给生产者），
/// **不负责注册业务消费者/生产者**。业务侧的注册由 `consumer::init` / `producer::init`
/// / 各 DAL 的 `init()` 完成。
///
/// 启动前会校验「声明了 `notify_producer` 的 topic 必须有对应生产者」，
/// 缺失即返回 Err（把「忘注册生产者」从静默变成启动即失败）。
pub async fn init_all() -> common::error::Result<()> {
    REGISTRY.start_all().await?;
    Ok(())
}

/// 停机 AOP 调度器（优雅退出）
///
/// 置位停机标志：异步消费者 worker 处理完当前事件后退出；随后逐个调用
/// `Producer::stop()` —— 由生产者置位自己的 loop 标志**并等 loop 真正退出**
/// （见 [`Producer::stop`] 的契约 2）。
pub async fn shutdown_all() -> common::error::Result<()> {
    REGISTRY.shutdown_all().await
}
