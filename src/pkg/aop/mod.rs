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
//! // 发布事件
//! aop::publish(MyEvent { ... }).await;
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
    AopEventMeta, AopMetricsHook, ConsumeMode, Consumer, Event, EventKind, Producer, Registry,
};
pub use queue::EventQueue;

use once_cell::sync::Lazy;
use std::sync::Arc;

/// 全局 Registry 单例（Arc 包装，允许 worker 协程持有引用）
static REGISTRY: Lazy<Arc<Registry>> = Lazy::new(|| {
    let registry = Arc::new(Registry::new());
    registry.set_self_ref(registry.clone());
    registry
});

/// 获取全局 Registry
pub fn registry() -> &'static Registry {
    &REGISTRY
}

/// 发布事件（便捷方法）
pub async fn publish<E: Event>(event: E) {
    REGISTRY.publish(event).await
}

/// 启动 AOP 调度器
///
/// 仅启动异步消费者的轮询 worker，**不负责注册业务消费者**。
/// 业务消费者的注册由 `consumer::init` 完成。
pub async fn init_all() -> common::error::Result<()> {
    REGISTRY.start_all().await?;
    Ok(())
}
