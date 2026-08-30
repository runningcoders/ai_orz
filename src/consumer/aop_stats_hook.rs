//! AOP 统计采集 Hook 业务实现
//!
//! 实现 `AopMetricsHook` trait，在 4 个回调中调用 `AopStatsCollector::record`
//! 写入内存收集器。在 `lib.rs::run()` 启动阶段注入到 Registry。
//!
//! 设计要点：
//! - 持有 `AopStatsCollector` 引用（克隆 Arc，零成本）
//! - 4 回调同步调用 collector.record（内部 async，但 hook 是同步方法）
//! - 使用 `tokio::spawn` 把 async record 转为后台任务，避免阻塞 AOP 主流程

use crate::consumer::aop_stats_collector::AopStatsCollector;
use crate::pkg::aop::{AopEventMeta, AopMetricsHook};

/// AOP 统计采集 Hook 业务实现
pub struct AopStatsHook {
    collector: AopStatsCollector,
}

impl AopStatsHook {
    pub fn new(collector: AopStatsCollector) -> Self {
        Self { collector }
    }

    /// 内部辅助：spawn 后台 record 任务
    fn spawn_record(&self, kind: String, consumer: String, status: String, duration_ms: u64) {
        let collector = self.collector.clone();
        tokio::spawn(async move {
            collector
                .record(&kind, &consumer, &status, duration_ms)
                .await;
        });
    }
}

impl AopMetricsHook for AopStatsHook {
    fn on_publish(&self, consumer_name: &str, meta: &AopEventMeta, is_async: bool) {
        let status = if is_async {
            "published"
        } else {
            "published_sync"
        };
        // 链路可观测：记录事件“进入”AOP（携带 log_id 以便串联排查）
        sys_info!(
            "[aop] log_id={} event={} -> {} (mode={})",
            log_id_of(meta),
            meta.event_kind,
            consumer_name,
            if is_async { "async" } else { "sync" }
        );
        self.spawn_record(
            meta.event_kind.clone(),
            consumer_name.to_string(),
            status.to_string(),
            0,
        );
    }

    fn on_consume_start(&self, consumer_name: &str, meta: &AopEventMeta) {
        // 链路可观测：记录事件“开始消费”
        sys_info!(
            "[aop] log_id={} event={} <- {} consume start",
            log_id_of(meta),
            meta.event_kind,
            consumer_name
        );
        self.spawn_record(
            meta.event_kind.clone(),
            consumer_name.to_string(),
            "consuming".to_string(),
            0,
        );
    }

    fn on_consume_success(&self, consumer_name: &str, meta: &AopEventMeta, duration_ms: u64) {
        // 链路可观测：记录事件“消费完成”及耗时（排查慢回复的关键指标）
        sys_info!(
            "[aop] log_id={} event={} <- {} success (duration_ms={})",
            log_id_of(meta),
            meta.event_kind,
            consumer_name,
            duration_ms
        );
        self.spawn_record(
            meta.event_kind.clone(),
            consumer_name.to_string(),
            "success".to_string(),
            duration_ms,
        );
    }

    fn on_consume_failure(
        &self,
        consumer_name: &str,
        meta: &AopEventMeta,
        duration_ms: u64,
        _error: &str,
    ) {
        sys_info!(
            "[aop] log_id={} event={} <- {} failed (duration_ms={})",
            log_id_of(meta),
            meta.event_kind,
            consumer_name,
            duration_ms
        );
        self.spawn_record(
            meta.event_kind.clone(),
            consumer_name.to_string(),
            "failed".to_string(),
            duration_ms,
        );
    }
}

/// 从 meta 提取链路 log_id（缺失时回退 unknown），供 AOP 日志串联排查
fn log_id_of(meta: &AopEventMeta) -> &str {
    meta.context_carrier
        .as_ref()
        .map(|c| c.log_id.as_str())
        .unwrap_or("unknown")
}

/// 单元测试
#[cfg(test)]
mod tests {
    use super::*;
    use crate::consumer::aop_stats_collector::AopStatsCollector;

    #[tokio::test]
    async fn test_hook_records_to_collector() {
        let collector = AopStatsCollector::new();
        let hook = AopStatsHook::new(collector.clone());

        let meta = AopEventMeta {
            event_id: "test-1".to_string(),
            event_kind: "message.created".to_string(),
            order_key: "order-1".to_string(),
            priority: 5,
            created_at: 1234567890,
            context_carrier: None,
        };

        // 调用 4 个回调
        hook.on_publish("agent.awakening", &meta, true);
        hook.on_consume_start("agent.awakening", &meta);
        hook.on_consume_success("agent.awakening", &meta, 100);

        // 等待 spawn 的任务完成
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let ov = collector.overview().await;
        assert_eq!(ov.total_published, 1);
        assert_eq!(ov.total_consumed, 1);
        assert_eq!(ov.total_success, 1);
        assert_eq!(ov.avg_duration_ms, 100.0);
    }
}
