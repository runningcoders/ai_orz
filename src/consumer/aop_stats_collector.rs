//! AOP 实时内存统计收集器
//!
//! 纯内存实现，不落库，重启即重置（与 AOP 事件本身生命周期一致）。
//! 基于 `pkg::stats::runtime::RuntimeStatsCollector` 泛型收集器，
//! 在 snapshot 基础上实现 AOP 专属聚合逻辑：
//! - overview: 按 status 分类（published/consuming/success/failed）
//! - time_series: 按 event_kind/consumer_name/status 部分字段过滤
//! - distribution: 按 consumer/status/kind 维度分组
//!
//! 内存占用估算：60 桶 × 每桶 ~20 个 (kind,consumer,status) 组合 × 32 字节 ≈ 38KB

use std::collections::HashMap;

use crate::pkg::stats::runtime::RuntimeStatsCollector;

/// AOP 维度键：(event_kind, consumer_name, status)
type AopDimKey = (String, String, String);

/// 单个时序数据点（对齐 common::models::TimeSeriesPoint 的字段语义）
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct AopTimeSeriesPoint {
    pub interval_start: i64,
    pub call_count: u64,
}

/// 概览快照
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct AopOverview {
    pub total_published: u64,
    pub total_consumed: u64,
    pub total_success: u64,
    pub total_failed: u64,
    pub avg_duration_ms: f64,
}

/// 分布项
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct AopDistributionItem {
    pub label: String,
    pub value: u64,
}

/// AOP 实时统计收集器
///
/// 业务层创建单例并通过 `AopStatsHook` 注入到 Registry。
/// SystemDomain 持有 `Arc<AopStatsCollector>` 引用，直接读取快照。
///
/// 内部 wrap `RuntimeStatsCollector<AopDimKey>`，AOP 专属聚合在 snapshot 基础上实现。
#[derive(Clone)]
pub struct AopStatsCollector {
    inner: RuntimeStatsCollector<AopDimKey>,
}

impl AopStatsCollector {
    pub fn new() -> Self {
        Self {
            inner: RuntimeStatsCollector::new(),
        }
    }

    /// 记录一个事件（由 AopStatsHook 调用）
    ///
    /// 对于 success/failed 状态，累计耗时；其他状态不累计。
    pub async fn record(&self, kind: &str, consumer: &str, status: &str, duration_ms: u64) {
        let key = (kind.to_string(), consumer.to_string(), status.to_string());
        let duration = if status == "success" || status == "failed" {
            Some(duration_ms)
        } else {
            None
        };
        self.inner.record(key, duration).await;
    }

    /// 查询概览（全生命周期累计）
    pub async fn overview(&self) -> AopOverview {
        let snap = self.inner.snapshot().await;
        let mut total_published = 0u64;
        let mut total_consumed = 0u64;
        let mut total_success = 0u64;
        let mut total_failed = 0u64;

        for ((_kind, _consumer, status), count) in snap.total_counts.iter() {
            match status.as_str() {
                "published" | "published_sync" => total_published += count,
                "consuming" => {} // 不计入任何汇总
                "success" => {
                    total_success += count;
                    total_consumed += count;
                }
                "failed" => {
                    total_failed += count;
                    total_consumed += count;
                }
                _ => {}
            }
        }

        let avg_duration_ms = if snap.total_completed > 0 {
            snap.total_duration_ms as f64 / snap.total_completed as f64
        } else {
            0.0
        };

        AopOverview {
            total_published,
            total_consumed,
            total_success,
            total_failed,
            avg_duration_ms,
        }
    }

    /// 查询时序数据（滑动窗口内，按分钟桶）
    ///
    /// 可选过滤：event_kind / consumer_name / status
    pub async fn time_series(
        &self,
        event_kind: Option<&str>,
        consumer_name: Option<&str>,
        status: Option<&str>,
    ) -> Vec<AopTimeSeriesPoint> {
        let snap = self.inner.snapshot().await;
        let mut points = Vec::with_capacity(snap.buckets.len());
        for bucket in snap.buckets.iter() {
            // 按过滤条件聚合当前桶
            let mut count = 0u64;
            for ((k, c, s), n) in bucket.counts.iter() {
                if let Some(filter_kind) = event_kind
                    && k != filter_kind {
                        continue;
                    }
                if let Some(filter_consumer) = consumer_name
                    && c != filter_consumer {
                        continue;
                    }
                if let Some(filter_status) = status
                    && s != filter_status {
                        continue;
                    }
                count += n;
            }
            if count > 0 {
                points.push(AopTimeSeriesPoint {
                    interval_start: bucket.minute,
                    call_count: count,
                });
            }
        }
        points
    }

    /// 查询分布（按指定维度 group by）
    ///
    /// - `group_by`: "consumer" | "status" | "kind"
    /// - 可选过滤：status
    pub async fn distribution(
        &self,
        group_by: &str,
        status_filter: Option<&str>,
    ) -> Vec<AopDistributionItem> {
        let snap = self.inner.snapshot().await;
        let mut groups: HashMap<String, u64> = HashMap::new();

        for ((kind, consumer, status), count) in snap.total_counts.iter() {
            // 应用 status 过滤
            if let Some(filter) = status_filter
                && status != filter {
                    continue;
                }
            let label = match group_by {
                "consumer" => consumer.clone(),
                "status" => status.clone(),
                "kind" => kind.clone(),
                _ => continue,
            };
            *groups.entry(label).or_insert(0) += count;
        }

        let mut items: Vec<AopDistributionItem> = groups
            .into_iter()
            .map(|(label, value)| AopDistributionItem { label, value })
            .collect();
        // 按数值降序
        items.sort_by_key(|x| std::cmp::Reverse(x.value));
        items
    }

    /// 运行时长（秒）
    pub async fn uptime_secs(&self) -> u64 {
        self.inner.uptime_secs().await
    }
}

impl Default for AopStatsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_record_and_overview() {
        let collector = AopStatsCollector::new();
        collector
            .record("message.created", "agent.awakening", "published", 0)
            .await;
        collector
            .record("message.created", "agent.awakening", "consuming", 0)
            .await;
        collector
            .record("message.created", "agent.awakening", "success", 100)
            .await;
        collector
            .record("cron.trigger", "cron_trigger", "published_sync", 0)
            .await;
        collector
            .record("cron.trigger", "cron_trigger", "failed", 50)
            .await;

        let ov = collector.overview().await;
        assert_eq!(ov.total_published, 2); // published + published_sync
        assert_eq!(ov.total_consumed, 2); // success + failed
        assert_eq!(ov.total_success, 1);
        assert_eq!(ov.total_failed, 1);
        assert_eq!(ov.avg_duration_ms, 75.0); // (100 + 50) / 2
    }

    #[tokio::test]
    async fn test_distribution_by_status() {
        let collector = AopStatsCollector::new();
        collector.record("k1", "c1", "success", 0).await;
        collector.record("k1", "c1", "success", 0).await;
        collector.record("k1", "c1", "failed", 0).await;
        collector.record("k2", "c2", "success", 0).await;

        let dist = collector.distribution("status", None).await;
        let success_item = dist.iter().find(|i| i.label == "success").unwrap();
        let failed_item = dist.iter().find(|i| i.label == "failed").unwrap();
        assert_eq!(success_item.value, 3);
        assert_eq!(failed_item.value, 1);
    }

    #[tokio::test]
    async fn test_distribution_by_consumer() {
        let collector = AopStatsCollector::new();
        collector
            .record("k1", "agent.awakening", "success", 0)
            .await;
        collector.record("k1", "agent.awakening", "failed", 0).await;
        collector.record("k2", "cron_trigger", "success", 0).await;

        let dist = collector.distribution("consumer", None).await;
        assert_eq!(dist.len(), 2);
        // 按数值降序：agent.awakening(2) 应该在 cron_trigger(1) 前
        assert_eq!(dist[0].label, "agent.awakening");
        assert_eq!(dist[0].value, 2);
        assert_eq!(dist[1].label, "cron_trigger");
        assert_eq!(dist[1].value, 1);
    }

    #[tokio::test]
    async fn test_time_series_returns_buckets() {
        let collector = AopStatsCollector::new();
        // 同一分钟内记录多个事件
        collector.record("k1", "c1", "success", 0).await;
        collector.record("k1", "c1", "success", 0).await;
        collector.record("k1", "c1", "failed", 0).await;

        let ts = collector.time_series(None, None, None).await;
        assert!(!ts.is_empty());
        let last = ts.last().unwrap();
        assert!(last.call_count >= 3);
    }

    #[tokio::test]
    async fn test_time_series_with_filter() {
        let collector = AopStatsCollector::new();
        collector.record("k1", "c1", "success", 0).await;
        collector.record("k2", "c1", "success", 0).await;

        let ts = collector.time_series(Some("k1"), None, None).await;
        let total: u64 = ts.iter().map(|p| p.call_count).sum();
        assert_eq!(total, 1);
    }

    #[tokio::test]
    async fn test_distribution_with_status_filter() {
        let collector = AopStatsCollector::new();
        collector.record("k1", "c1", "success", 0).await;
        collector.record("k1", "c1", "failed", 0).await;
        collector.record("k1", "c2", "success", 0).await;

        // 只看 success 状态，按 consumer 分组
        let dist = collector.distribution("consumer", Some("success")).await;
        let total: u64 = dist.iter().map(|i| i.value).sum();
        assert_eq!(total, 2); // c1(1) + c2(1)
        assert!(dist.iter().all(|i| i.value <= 1));
    }
}
