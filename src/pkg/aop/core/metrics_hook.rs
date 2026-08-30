//! AOP 指标采集 Hook trait
//!
//! AOP 框架保持零业务依赖原则，统计采集逻辑通过 Hook 注入。
//! 业务层实现此 trait，在 lib.rs 启动时通过 `registry().set_metrics_hook()` 注入。
//!
//! 4 个回调方法对应 AOP 事件生命周期的关键节点：
//! - on_publish: 事件被发布到 Registry（每个消费者触发一次）
//! - on_consume_start: 消费者开始处理事件
//! - on_consume_success: 消费者成功处理事件
//! - on_consume_failure: 消费者处理事件失败
//!
//! 所有方法提供默认空实现，未注入 hook 时零开销。

use serde_json::Value;

use crate::pkg::request_context::{AOP_CONTEXT_CARRIER_KEY, ContextCarrier};

/// AOP 事件元信息（从 event_json 顶层提取）
#[derive(Debug, Clone)]
pub struct AopEventMeta {
    pub event_id: String,
    pub event_kind: String,
    pub order_key: String,
    pub priority: u8,
    pub created_at: i64,
    /// 随事件流转的可传输子 context（链路串联载体，可能为 None）
    pub context_carrier: Option<ContextCarrier>,
}

impl AopEventMeta {
    /// 从 event_json 顶层提取元信息（publish 时已注入到 JSON 顶层）
    pub fn from_json(event_json: &Value) -> Self {
        Self {
            event_id: event_json
                .get("event_id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            event_kind: event_json
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            order_key: event_json
                .get("order_key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            priority: event_json
                .get("priority")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u8,
            created_at: event_json
                .get("created_at")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
            context_carrier: event_json
                .get(AOP_CONTEXT_CARRIER_KEY)
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
        }
    }
}

/// AOP 指标采集 Hook trait
///
/// 业务层实现此 trait，通过 `aop::registry().set_metrics_hook()` 注入。
/// 所有方法提供默认空实现，未注入时零开销。
pub trait AopMetricsHook: Send + Sync {
    /// 事件被发布到 Registry 时触发（每个感兴趣的消费者触发一次）
    ///
    /// - `consumer_name`: 接收事件的消费者名称
    /// - `meta`: 事件元信息
    /// - `is_async`: true=异步入队，false=同步直接消费
    fn on_publish(&self, _consumer_name: &str, _meta: &AopEventMeta, _is_async: bool) {}

    /// 消费者开始处理事件时触发
    fn on_consume_start(&self, _consumer_name: &str, _meta: &AopEventMeta) {}

    /// 消费者成功处理事件时触发
    ///
    /// - `duration_ms`: 处理耗时（毫秒）
    fn on_consume_success(&self, _consumer_name: &str, _meta: &AopEventMeta, _duration_ms: u64) {}

    /// 消费者处理事件失败时触发
    ///
    /// - `duration_ms`: 处理耗时（毫秒）
    /// - `error`: 失败原因
    fn on_consume_failure(
        &self,
        _consumer_name: &str,
        _meta: &AopEventMeta,
        _duration_ms: u64,
        _error: &str,
    ) {
    }
}
