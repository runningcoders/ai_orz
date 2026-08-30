use async_trait::async_trait;
use common::error::Result;

use super::EventKind;
use crate::pkg::RequestContext;

/// 消费模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsumeMode {
    /// 同步模式：事件发布时立即在发布线程中调用 on_event
    Sync,
    /// 异步模式：事件入队，由 AOP 调度器拉取并调用 on_event
    Async,
}

/// AOP 消费者 trait
///
/// 统一的事件消费接口，支持同步和异步两种消费模式：
/// - **Sync**：事件发布时直接调用 `on_event`，适合轻量级处理
/// - **Async**：事件入队，由 AOP 调度器从队列拉取后调用 `on_event`
///
/// 异步消费者需要实现 `ack`/`nack` 以支持消息确认机制，
/// 可通过 `concurrency` 控制并行 worker 数量，
/// 通过 `empty_queue_sleep_ms`/`error_retry_sleep_ms` 控制轮询节奏。
#[async_trait]
pub trait Consumer: Send + Sync {
    /// 消费者名称（全局唯一，用于队列路由和日志追踪）
    fn name(&self) -> &str;

    /// 感兴趣的事件类型列表
    fn interested_events(&self) -> Vec<EventKind>;

    /// 事件过滤（默认全部通过）
    async fn should_consume(&self, _event: &serde_json::Value) -> bool {
        true
    }

    /// 消费模式（默认同步）
    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Sync
    }

    /// 处理事件（核心业务逻辑）
    ///
    /// 框架在分发前已从事件顶层 `context_carrier` 还原出与主 context 同源的
    /// `ctx` 并传入；消费侧可直接使用（其 log_id 等链路线索已贯通），
    /// 也可在此基础上追加/修饰业务字段。
    async fn on_event(&self, ctx: RequestContext, event: serde_json::Value) -> Result<()>;

    // ===== 以下仅 Async 模式消费者需要关注 =====

    /// 确认事件处理成功（默认空实现，Sync 模式无需关注）
    async fn ack(&self, _event_id: &str) -> Result<()> {
        Ok(())
    }

    /// 标记事件处理失败，等待重试（默认空实现，Sync 模式无需关注）
    async fn nack(&self, _event_id: &str) -> Result<()> {
        Ok(())
    }

    /// 并发 worker 数量（默认 1，仅 Async 模式生效）
    fn concurrency(&self) -> usize {
        1
    }

    /// 队列为空时休眠毫秒数（默认 100ms，仅 Async 模式生效）
    fn empty_queue_sleep_ms(&self) -> u64 {
        100
    }

    /// 处理出错时休眠毫秒数（默认 1000ms，仅 Async 模式生效）
    fn error_retry_sleep_ms(&self) -> u64 {
        1000
    }
}
