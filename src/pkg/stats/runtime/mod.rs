//! 运行时统计基础设施 — 泛型内存收集器（pkg/stats/ 的内存版补充）
//!
//! 提供通用的内存统计收集能力，与父模块 `pkg/stats/`（DuckDB 持久化）互补：
//! - `pkg/stats/`：持久化统计，跨重启保留，支持复杂 SQL 查询
//! - `pkg/stats/runtime/`（本模块）：内存统计，重启重置，提供快照式查询，零 DB 依赖
//!
//! 适用场景：
//! - AOP 事件统计（已使用）
//! - SSE/WS 连接数监控
//! - Channel 推送指标
//! - 内存队列深度时序
//! - 任何"运行时能力、无持久化价值"的统计场景
//!
//! # Examples
//!
//! ```ignore
//! use ai_orz::pkg::stats::runtime::RuntimeStatsCollector;
//!
//! // 1. 定义维度键（任何 Hash + Eq + Clone + Send + Sync 的类型都可以）
//! type MyKey = (String, String); // (category, action)
//!
//! let collector: RuntimeStatsCollector<MyKey> = RuntimeStatsCollector::new();
//!
//! // 2. 记录事件（duration 为 None 时不累计耗时）
//! collector.record(("click".to_string(), "button".to_string()), None).await;
//! collector.record(("click".to_string(), "button".to_string()), Some(150)).await;
//!
//! // 3. 获取快照，自行做业务聚合
//! let snap = collector.snapshot().await;
//! let total: u64 = snap.total_counts.values().sum();
//! ```

use std::collections::{HashMap, VecDeque};
use std::fmt::Debug;
use std::hash::Hash;
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

/// 单个时间桶快照（按分钟对齐）
///
/// 由 `snapshot()` 返回，供调用方读取桶内数据做聚合。
#[derive(Debug, Clone)]
pub struct TimeBucketSnapshot<K> {
    /// 桶起始时间（毫秒）
    pub minute: i64,
    /// 维度计数
    pub counts: HashMap<K, u64>,
    /// 累计耗时（毫秒），仅累计 duration=Some 的事件
    pub total_duration_ms: u64,
    /// 完成数（duration=Some 的事件数），用于 avg_duration 除数
    pub completed_count: u64,
}

/// 收集器完整快照
///
/// 由 `snapshot()` 返回，包含：
/// - `total_counts`：全生命周期累计计数（按维度键索引）
/// - `buckets`：滑动窗口内的时序桶（按时间升序）
/// - `total_duration_ms` / `total_completed`：全局耗时统计
/// - `started_at`：收集器启动时间（用于计算运行时长）
#[derive(Debug, Clone)]
pub struct RuntimeStatsSnapshot<K> {
    pub total_counts: HashMap<K, u64>,
    pub buckets: Vec<TimeBucketSnapshot<K>>,
    pub total_duration_ms: u64,
    pub total_completed: u64,
    pub started_at: i64,
}

/// 单个时间桶（内部可变状态）
#[derive(Debug, Clone)]
struct TimeBucket<K> {
    minute: i64,
    counts: HashMap<K, u64>,
    total_duration_ms: u64,
    completed_count: u64,
}

impl<K> Default for TimeBucket<K> {
    fn default() -> Self {
        Self {
            minute: 0,
            counts: HashMap::new(),
            total_duration_ms: 0,
            completed_count: 0,
        }
    }
}

/// 收集器内部状态
struct Inner<K> {
    /// 总计数器（全生命周期，重启才重置）
    total_counts: HashMap<K, u64>,
    /// 总累计耗时（用于全局 avg_duration）
    total_duration_ms: u64,
    /// 总完成数（duration=Some 的事件数）
    total_completed: u64,
    /// 滑动窗口时序桶（按时间升序，最老在前）
    buckets: VecDeque<TimeBucket<K>>,
    /// 启动时间（用于计算运行时长）
    started_at: i64,
}

impl<K> Inner<K> {
    fn new() -> Self {
        Self {
            total_counts: HashMap::new(),
            total_duration_ms: 0,
            total_completed: 0,
            buckets: VecDeque::with_capacity(WINDOW_MINUTES as usize + 5),
            started_at: now_millis(),
        }
    }
}

impl<K: Clone + Eq + Hash> Inner<K> {
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
    fn current_bucket(&mut self, now_millis: i64) -> &mut TimeBucket<K> {
        let minute = minute_bucket_millis(now_millis);
        if self.buckets.back().map_or(true, |b| b.minute != minute) {
            self.buckets.push_back(TimeBucket {
                minute,
                ..Default::default()
            });
        }
        self.buckets.back_mut().unwrap()
    }

    /// 记录一个事件
    fn record(&mut self, key: K, duration: Option<u64>, now: i64) {
        // 更新总计数器
        *self.total_counts.entry(key.clone()).or_insert(0) += 1;

        // 对于带 duration 的事件，累计耗时
        if let Some(ms) = duration {
            self.total_duration_ms += ms;
            self.total_completed += 1;
        }

        // 更新当前时间桶
        self.evict_old_buckets(now);
        let bucket = self.current_bucket(now);
        *bucket.counts.entry(key).or_insert(0) += 1;
        if let Some(ms) = duration {
            bucket.total_duration_ms += ms;
            bucket.completed_count += 1;
        }
    }
}

/// 泛型运行时统计收集器
///
/// K 是维度键类型，需实现 `Clone + Eq + Hash + Send + Sync + Debug + 'static`。
/// 常见选择：
/// - `String`（单维度）
/// - `(String, String)`（双维度，如 category+action）
/// - `(String, String, String)`（三维度，如 AOP 的 kind+consumer+status）
///
/// 收集器提供 `record` 和 `snapshot` 两个核心方法。
/// 业务层（如 AopStatsCollector）在 snapshot 基础上实现专属聚合逻辑。
#[derive(Clone)]
pub struct RuntimeStatsCollector<K> {
    inner: Arc<RwLock<Inner<K>>>,
}

impl<K: Clone + Eq + Hash + Send + Sync + Debug + 'static> RuntimeStatsCollector<K> {
    /// 创建新的收集器
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner::new())),
        }
    }

    /// 记录一个事件
    ///
    /// - `key`: 维度键
    /// - `duration`: 耗时（毫秒）。`None` 表示不累计耗时（如 "published" 状态只计数不计时），
    ///   `Some(ms)` 表示累计耗时（如 "success"/"failed" 状态）
    pub async fn record(&self, key: K, duration: Option<u64>) {
        let now = now_millis();
        let mut inner = self.inner.write().await;
        inner.record(key, duration, now);
    }

    /// 获取完整快照
    ///
    /// 返回当前收集器的完整状态快照，调用方在快照基础上做业务聚合。
    /// 快照是深拷贝，释放读锁后调用方可以安全处理。
    pub async fn snapshot(&self) -> RuntimeStatsSnapshot<K> {
        let inner = self.inner.read().await;
        let buckets = inner
            .buckets
            .iter()
            .map(|b| TimeBucketSnapshot {
                minute: b.minute,
                counts: b.counts.clone(),
                total_duration_ms: b.total_duration_ms,
                completed_count: b.completed_count,
            })
            .collect();
        RuntimeStatsSnapshot {
            total_counts: inner.total_counts.clone(),
            buckets,
            total_duration_ms: inner.total_duration_ms,
            total_completed: inner.total_completed,
            started_at: inner.started_at,
        }
    }

    /// 运行时长（秒）
    pub async fn uptime_secs(&self) -> u64 {
        let inner = self.inner.read().await;
        ((now_millis() - inner.started_at) / 1000) as u64
    }
}

impl<K: Clone + Eq + Hash + Send + Sync + Debug + 'static> Default for RuntimeStatsCollector<K> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用维度键：单字符串
    type TestKey = String;

    #[tokio::test]
    async fn test_record_increments_total_counts() {
        let collector: RuntimeStatsCollector<TestKey> = RuntimeStatsCollector::new();
        collector.record("click".to_string(), None).await;
        collector.record("click".to_string(), None).await;
        collector.record("view".to_string(), None).await;

        let snap = collector.snapshot().await;
        assert_eq!(snap.total_counts.get("click"), Some(&2));
        assert_eq!(snap.total_counts.get("view"), Some(&1));
    }

    #[tokio::test]
    async fn test_duration_accumulation() {
        let collector: RuntimeStatsCollector<TestKey> = RuntimeStatsCollector::new();
        // None 不累计耗时
        collector.record("published".to_string(), None).await;
        // Some 累计耗时
        collector.record("success".to_string(), Some(100)).await;
        collector.record("success".to_string(), Some(200)).await;

        let snap = collector.snapshot().await;
        assert_eq!(snap.total_duration_ms, 300);
        assert_eq!(snap.total_completed, 2);
    }

    #[tokio::test]
    async fn test_none_duration_not_counted_as_completed() {
        let collector: RuntimeStatsCollector<TestKey> = RuntimeStatsCollector::new();
        collector.record("a".to_string(), None).await;
        collector.record("b".to_string(), Some(50)).await;

        let snap = collector.snapshot().await;
        assert_eq!(snap.total_completed, 1); // 只有 b 计入 completed
    }

    #[tokio::test]
    async fn test_snapshot_returns_buckets() {
        let collector: RuntimeStatsCollector<TestKey> = RuntimeStatsCollector::new();
        collector.record("a".to_string(), None).await;
        collector.record("a".to_string(), None).await;
        collector.record("b".to_string(), None).await;

        let snap = collector.snapshot().await;
        // 至少有一个桶
        assert!(!snap.buckets.is_empty());
        // 最后一个桶应包含所有 3 个事件
        let last = snap.buckets.last().unwrap();
        let total: u64 = last.counts.values().sum();
        assert_eq!(total, 3);
    }

    #[tokio::test]
    async fn test_tuple_key_works() {
        // 测试元组作为维度键（AOP 场景）
        type TupleKey = (String, String, String);
        let collector: RuntimeStatsCollector<TupleKey> = RuntimeStatsCollector::new();
        collector
            .record(
                (
                    "message.created".to_string(),
                    "agent.awakening".to_string(),
                    "success".to_string(),
                ),
                Some(100),
            )
            .await;

        let snap = collector.snapshot().await;
        let key = (
            "message.created".to_string(),
            "agent.awakening".to_string(),
            "success".to_string(),
        );
        assert_eq!(snap.total_counts.get(&key), Some(&1));
        assert_eq!(snap.total_duration_ms, 100);
    }

    #[tokio::test]
    async fn test_uptime_secs_nonzero() {
        let collector: RuntimeStatsCollector<TestKey> = RuntimeStatsCollector::new();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let uptime = collector.uptime_secs().await;
        // 刚启动 100ms，uptime_secs 应为 0（整数秒）
        assert_eq!(uptime, 0);

        // 等待超过 1 秒
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        let uptime = collector.uptime_secs().await;
        assert!(uptime >= 1);
    }

    #[tokio::test]
    async fn test_snapshot_is_deep_copy() {
        let collector: RuntimeStatsCollector<TestKey> = RuntimeStatsCollector::new();
        collector.record("a".to_string(), None).await;

        let snap1 = collector.snapshot().await;
        // 在 snap1 之后再次 record
        collector.record("a".to_string(), None).await;
        let snap2 = collector.snapshot().await;

        // snap1 应该不受后续 record 影响
        assert_eq!(snap1.total_counts.get("a"), Some(&1));
        assert_eq!(snap2.total_counts.get("a"), Some(&2));
    }

    #[tokio::test]
    async fn test_clone_shares_inner_state() {
        let collector: RuntimeStatsCollector<TestKey> = RuntimeStatsCollector::new();
        let cloned = collector.clone();
        collector.record("a".to_string(), None).await;

        // clone 共享内部状态
        let snap = cloned.snapshot().await;
        assert_eq!(snap.total_counts.get("a"), Some(&1));
    }
}
