//! AOP 实时内存统计收集器
//!
//! 纯内存实现，不落库，重启即重置（与 AOP 事件本身生命周期一致）。
//! 提供：
//! - 总计数器（按 event_kind/consumer_name/status 三维索引）
//! - 滑动窗口时序数据（最近 60 分钟，按分钟桶）
//! - 查询快照方法（overview / time_series / distribution）
//!
//! 内存占用估算：60 桶 × 每桶 ~20 个 (kind,consumer,status) 组合 × 32 字节 ≈ 38KB

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::RwLock;

/// 滑动窗口保留的分钟数
const WINDOW_MINUTES: i64 = 60;

/// 按分钟对齐的时间戳（毫秒）
fn minute_bucket_millis(ts_millis: i64) -> i64 {
    (ts_millis / 60_000) * 60_000
}

/// 当前时间戳（毫秒）
fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

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

/// 维度键：(event_kind, consumer_name, status)
type DimKey = (String, String, String);

/// 单个时间桶（按分钟对齐）
#[derive(Debug, Clone, Default)]
struct TimeBucket {
    /// 桶起始时间（毫秒）
    minute: i64,
    /// 维度计数
    counts: HashMap<DimKey, u64>,
    /// 累计 duration_ms（用于计算平均耗时）
    total_duration_ms: u64,
    /// success + failed 总数（用于 avg_duration 除数）
    completed_count: u64,
}

/// 收集器内部状态
struct Inner {
    /// 总计数器（全生命周期，重启才重置）
    total_counts: HashMap<DimKey, u64>,
    /// 总累计耗时（用于全局 avg_duration）
    total_duration_ms: u64,
    /// 总完成数（success + failed）
    total_completed: u64,
    /// 滑动窗口时序桶（按时间升序，最老在前）
    buckets: std::collections::VecDeque<TimeBucket>,
    /// 启动时间（用于计算运行时长）
    started_at: i64,
}

impl Inner {
    fn new() -> Self {
        Self {
            total_counts: HashMap::new(),
            total_duration_ms: 0,
            total_completed: 0,
            buckets: std::collections::VecDeque::with_capacity(WINDOW_MINUTES as usize + 5),
            started_at: now_millis(),
        }
    }

    /// 清理超过滑动窗口的旧桶
    fn evict_old_buckets(&mut self, now_millis: i64) {
        let cutoff = minute_bucket_millis(now_millis) - WINDOW_MINUTES * 60_000;
        while let Some(front) = self.buckets.front() {
            if front.minute < cutoff {
                self.buckets.pop_front();
            } else {
                break;
            }
        }
    }

    /// 获取或创建当前分钟桶
    fn current_bucket(&mut self, now_millis: i64) -> &mut TimeBucket {
        let minute = minute_bucket_millis(now_millis);
        // 检查最后一个桶是否是当前分钟
        if self.buckets.back().map_or(true, |b| b.minute != minute) {
            self.buckets.push_back(TimeBucket {
                minute,
                ..Default::default()
            });
        }
        self.buckets.back_mut().unwrap()
    }

    /// 记录一个事件
    fn record(&mut self, kind: &str, consumer: &str, status: &str, duration_ms: u64, now: i64) {
        let key = (kind.to_string(), consumer.to_string(), status.to_string());

        // 更新总计数器
        *self.total_counts.entry(key.clone()).or_insert(0) += 1;

        // 对于 success/failed，累计耗时
        if status == "success" || status == "failed" {
            self.total_duration_ms += duration_ms;
            self.total_completed += 1;
        }

        // 更新当前时间桶
        self.evict_old_buckets(now);
        let bucket = self.current_bucket(now);
        *bucket.counts.entry(key).or_insert(0) += 1;
        if status == "success" || status == "failed" {
            bucket.total_duration_ms += duration_ms;
            bucket.completed_count += 1;
        }
    }
}

/// AOP 实时统计收集器
///
/// 业务层创建单例并通过 `AopStatsHook` 注入到 Registry。
/// SystemDomain 持有 `Arc<AopStatsCollector>` 引用，直接读取快照。
#[derive(Clone)]
pub struct AopStatsCollector {
    inner: Arc<RwLock<Inner>>,
}

impl AopStatsCollector {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner::new())),
        }
    }

    /// 记录一个事件（由 AopStatsHook 调用）
    pub async fn record(
        &self,
        kind: &str,
        consumer: &str,
        status: &str,
        duration_ms: u64,
    ) {
        let now = now_millis();
        let mut inner = self.inner.write().await;
        inner.record(kind, consumer, status, duration_ms, now);
    }

    /// 查询概览（全生命周期累计）
    pub async fn overview(&self) -> AopOverview {
        let inner = self.inner.read().await;
        let mut total_published = 0u64;
        let mut total_consumed = 0u64;
        let mut total_success = 0u64;
        let mut total_failed = 0u64;

        for ((_kind, _consumer, status), count) in inner.total_counts.iter() {
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

        let avg_duration_ms = if inner.total_completed > 0 {
            inner.total_duration_ms as f64 / inner.total_completed as f64
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
        let inner = self.inner.read().await;
        let now = now_millis();
        let cutoff = minute_bucket_millis(now) - WINDOW_MINUTES * 60_000;

        let mut points = Vec::with_capacity(inner.buckets.len());
        for bucket in inner.buckets.iter() {
            if bucket.minute < cutoff {
                continue;
            }
            // 按过滤条件聚合当前桶
            let mut count = 0u64;
            for ((k, c, s), n) in bucket.counts.iter() {
                if let Some(filter_kind) = event_kind {
                    if k != filter_kind {
                        continue;
                    }
                }
                if let Some(filter_consumer) = consumer_name {
                    if c != filter_consumer {
                        continue;
                    }
                }
                if let Some(filter_status) = status {
                    if s != filter_status {
                        continue;
                    }
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
        let inner = self.inner.read().await;
        let mut groups: HashMap<String, u64> = HashMap::new();

        for ((kind, consumer, status), count) in inner.total_counts.iter() {
            // 应用 status 过滤
            if let Some(filter) = status_filter {
                if status != filter {
                    continue;
                }
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
        items.sort_by(|a, b| b.value.cmp(&a.value));
        items
    }

    /// 运行时长（秒）
    pub async fn uptime_secs(&self) -> u64 {
        let inner = self.inner.read().await;
        ((now_millis() - inner.started_at) / 1000) as u64
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
        collector
            .record("k1", "c1", "success", 0)
            .await;
        collector
            .record("k1", "c1", "success", 0)
            .await;
        collector
            .record("k1", "c1", "failed", 0)
            .await;
        collector
            .record("k2", "c2", "success", 0)
            .await;

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
        collector
            .record("k1", "agent.awakening", "failed", 0)
            .await;
        collector
            .record("k2", "cron_trigger", "success", 0)
            .await;

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
        collector
            .record("k1", "c1", "success", 0)
            .await;
        collector
            .record("k1", "c1", "success", 0)
            .await;
        collector
            .record("k1", "c1", "failed", 0)
            .await;

        let ts = collector.time_series(None, None, None).await;
        assert!(!ts.is_empty());
        let last = ts.last().unwrap();
        assert!(last.call_count >= 3);
    }

    #[tokio::test]
    async fn test_time_series_with_filter() {
        let collector = AopStatsCollector::new();
        collector
            .record("k1", "c1", "success", 0)
            .await;
        collector
            .record("k2", "c1", "success", 0)
            .await;

        let ts = collector.time_series(Some("k1"), None, None).await;
        let total: u64 = ts.iter().map(|p| p.call_count).sum();
        assert_eq!(total, 1);
    }

    #[tokio::test]
    async fn test_distribution_with_status_filter() {
        let collector = AopStatsCollector::new();
        collector
            .record("k1", "c1", "success", 0)
            .await;
        collector
            .record("k1", "c1", "failed", 0)
            .await;
        collector
            .record("k1", "c2", "success", 0)
            .await;

        // 只看 success 状态，按 consumer 分组
        let dist = collector.distribution("consumer", Some("success")).await;
        let total: u64 = dist.iter().map(|i| i.value).sum();
        assert_eq!(total, 2); // c1(1) + c2(1)
        assert!(dist.iter().all(|i| i.value <= 1));
    }
}
