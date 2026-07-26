# 统计图表 Phase 3 实施计划：AOP 实时内存统计 + 轮询渲染

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 AOP 队列监控页面新增实时统计图表，通过 AopStatsCollector 内存收集器（滑动窗口 + 计数器）记录 publish/consume/success/failure 事件，前端统计 Tab 轮询查询并渲染 LineChart 时序图 + DonutChart 分布图 + 概览卡片。零 DuckDB 依赖、零 DAO、零 DAL，与 AOP 事件本身生命周期一致（重启即重置）。

**Architecture:** AOP 框架层定义 `AopMetricsHook` trait（保持零业务依赖），业务层实现 `AopStatsHook` 持有 `Arc<AopStatsCollector>`。Collector 用 `Arc<RwLock<Inner>>` 维护：① 按 (kind, consumer, status) 维度的总计数器；② 按分钟桶的滑动窗口时序数据（保留最近 60 分钟）。SystemDomain 新增 `AopStats` 子能力直接读 collector（无 DAO/DAL 中转），Handler 暴露 3 个查询端点，前端 Tab 布局 + 5 秒轮询 + 图表渲染。

**Tech Stack:** Rust + tokio RwLock + AtomicU64 + Dioxus 0.7 + web-sys Canvas 2D API + 现有 LineChart/DonutChart 组件

**设计决策：**
- **纯内存**：不落 DuckDB，重启即重置，与 AOP 事件生命周期一致
- **滑动窗口**：保留最近 60 分钟时序数据，按分钟桶聚合（60 个桶，内存占用 < 50KB）
- **零业务依赖**：`AopMetricsHook` trait 定义在 `pkg/aop/`，不引用任何业务模块
- **零 DAO/DAL**：SystemDomain 直接持有 `Arc<AopStatsCollector>`，无中间层
- **轮询而非 SSE**：5 秒轮询足够实时，实现简单，与现有 AOP 监控页一致
- **Tab 布局**：不破坏现有实时监控功能，统计图表独立 Tab

---

## 文件结构

| 文件 | 操作 | 职责 |
|------|------|------|
| `src/pkg/aop/core/metrics_hook.rs` | 新建 | `AopMetricsHook` trait（4 回调） + `AopEventMeta` |
| `src/pkg/aop/core/mod.rs` | 修改 | 注册 metrics_hook 模块 |
| `src/pkg/aop/core/registry.rs` | 修改 | Registry 加 `metrics_hook` 字段 + setter + 3 处埋点 |
| `src/pkg/aop/mod.rs` | 修改 | 导出 `AopMetricsHook`、`AopEventMeta` |
| `src/consumer/aop_stats_collector.rs` | 新建 | `AopStatsCollector` 内存收集器 + `AopStatsSnapshot` 快照 |
| `src/consumer/aop_stats_hook.rs` | 新建 | `AopStatsHook` 业务实现（4 回调写入 collector） |
| `src/consumer/mod.rs` | 修改 | 注册两个新模块 |
| `src/lib.rs` | 修改 | `run()` 中创建 collector + 注入 hook + 注入 SystemDomain |
| `src/service/domain/system/mod.rs` | 修改 | SystemDomain 加 `aop_stats()` getter + `AopStats` trait |
| `src/service/domain/system/aop_stats.rs` | 新建 | `impl AopStats for SystemDomainImpl`（读 collector） |
| `src/handlers/system/aop_stats.rs` | 新建 | 3 个 stats 端点 |
| `src/handlers/system/mod.rs` | 修改 | 注册 aop_stats 模块 |
| `src/router.rs` | 修改 | 注册 3 个新路由 |
| `common/src/api/system.rs` | 修改 | 新增 AOP stats 响应 DTO |
| `frontend/src/api/system.rs` | 修改 | 3 个 stats API 客户端函数 |
| `frontend/src/pages/system/aop.rs` | 修改 | Tab 布局 + 轮询 + LineChart + DonutChart |

---

## Task 1: AopMetricsHook trait + Registry 埋点机制

**Files:**
- Create: `src/pkg/aop/core/metrics_hook.rs`
- Modify: `src/pkg/aop/core/mod.rs`
- Modify: `src/pkg/aop/core/registry.rs`
- Modify: `src/pkg/aop/mod.rs`

**目标：** 在 AOP 框架层定义 `AopMetricsHook` trait（4 个回调方法），Registry 持有 `Option<Arc<dyn AopMetricsHook>>`，在 publish 和 worker 关键路径调用 hook。严格保持 AOP 框架零业务依赖。

- [ ] **Step 1: 创建 metrics_hook.rs**

创建 `src/pkg/aop/core/metrics_hook.rs`：

```rust
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

/// AOP 事件元信息（从 event_json 顶层提取）
#[derive(Debug, Clone)]
pub struct AopEventMeta {
    pub event_id: String,
    pub event_kind: String,
    pub order_key: String,
    pub priority: u8,
    pub created_at: i64,
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
```

- [ ] **Step 2: 在 core/mod.rs 注册 metrics_hook 模块**

修改 `src/pkg/aop/core/mod.rs`，加入：

```rust
pub mod metrics_hook;
pub use metrics_hook::{AopEventMeta, AopMetricsHook};
```

- [ ] **Step 3: 修改 Registry 加 stats_hook 字段 + setter**

修改 `src/pkg/aop/core/registry.rs`：

**3a. 在文件顶部导入**：

```rust
use crate::pkg::aop::core::metrics_hook::{AopEventMeta, AopMetricsHook};
```

**3b. 在 Registry 结构体加字段**（约 line 10-16）：

```rust
pub struct Registry {
    self_ref: RwLock<Option<Weak<Self>>>,
    consumers: RwLock<HashMap<EventKind, Vec<Arc<dyn Consumer>>>>,
    producers: RwLock<Vec<Arc<dyn Producer>>>,
    queues: RwLock<HashMap<String, Arc<dyn EventQueue>>>,
    started: RwLock<bool>,
    /// 指标采集 Hook（业务层注入，None 时零开销）
    metrics_hook: RwLock<Option<Arc<dyn AopMetricsHook>>>,
}
```

**3c. 在 `new()` 初始化字段**（约 line 19-27）：

```rust
pub fn new() -> Self {
    Self {
        self_ref: RwLock::new(None),
        consumers: RwLock::new(HashMap::new()),
        producers: RwLock::new(Vec::new()),
        queues: RwLock::new(HashMap::new()),
        started: RwLock::new(false),
        metrics_hook: RwLock::new(None),
    }
}
```

**3d. 新增 setter 和 getter**（在 `set_self_ref` 后）：

```rust
/// 注入指标采集 Hook（业务层在启动时调用）
pub fn set_metrics_hook(&self, hook: Arc<dyn AopMetricsHook>) {
    let mut guard = self.metrics_hook.write().unwrap();
    *guard = Some(hook);
}

/// 读取 hook（内部辅助方法，None 时返回 None）
fn metrics_hook(&self) -> Option<Arc<dyn AopMetricsHook>> {
    self.metrics_hook.read().ok()?.clone()
}
```

- [ ] **Step 4: 在 publish 方法埋点 on_publish + sync on_consume_***

修改 `src/pkg/aop/core/registry.rs` 的 `publish` 方法（约 line 116-148），把分发循环替换为：

```rust
for consumer in interested {
    if !consumer.should_consume(&event_json).await {
        continue;
    }

    // 埋点：on_publish
    let meta = AopEventMeta::from_json(&event_json);
    let is_async = matches!(consumer.consume_mode(), ConsumeMode::Async);
    if let Some(hook) = self.metrics_hook() {
        hook.on_publish(consumer.name(), &meta, is_async);
    }

    match consumer.consume_mode() {
        ConsumeMode::Sync => {
            // 同步：直接调用 on_event
            let start = std::time::Instant::now();
            if let Some(hook) = self.metrics_hook() {
                hook.on_consume_start(consumer.name(), &meta);
            }
            match consumer.on_event(event_json.clone()).await {
                Ok(()) => {
                    let duration_ms = start.elapsed().as_millis() as u64;
                    if let Some(hook) = self.metrics_hook() {
                        hook.on_consume_success(consumer.name(), &meta, duration_ms);
                    }
                }
                Err(e) => {
                    let duration_ms = start.elapsed().as_millis() as u64;
                    let err_str = format!("{:?}", e);
                    sys_error!("consumer {} sync error: {}", consumer.name(), e);
                    if let Some(hook) = self.metrics_hook() {
                        hook.on_consume_failure(consumer.name(), &meta, duration_ms, &err_str);
                    }
                }
            }
        }
        ConsumeMode::Async => {
            let queue = {
                let queues = self.queues.read().unwrap();
                queues.get(consumer.name()).cloned()
            };
            if let Some(queue) = queue {
                let ctx = RequestContext::new(None, None);
                if let Err(e) = queue.enqueue(ctx, event_json.clone()).await {
                    sys_error!("consumer {} enqueue error: {}", consumer.name(), e);
                }
            }
        }
    }
}
```

- [ ] **Step 5: 在 worker 协程埋点 on_consume_start/success/failure**

修改 `src/pkg/aop/core/registry.rs` 的 `start_all` 方法中 worker 协程循环（约 line 248-296），把 `match registry_arc.dequeue_for(&consumer_name).await` 内部替换为：

```rust
match registry_arc.dequeue_for(&consumer_name).await {
    Ok(Some(event_json)) => {
        let event_id = event_json
            .get("event_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        // 提取元信息 + 记录开始时间
        let meta = AopEventMeta::from_json(&event_json);
        let start = std::time::Instant::now();

        // 埋点：on_consume_start
        if let Some(hook) = registry_arc.metrics_hook() {
            hook.on_consume_start(&consumer_name, &meta);
        }

        match consumer.on_event(event_json).await {
            Ok(()) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                // 埋点：on_consume_success
                if let Some(hook) = registry_arc.metrics_hook() {
                    hook.on_consume_success(&consumer_name, &meta, duration_ms);
                }
                if let Err(e) = consumer.ack(&event_id).await {
                    sys_error!("[{}] consumer ack error: {}", consumer_name, e);
                }
                if let Err(e) = registry_arc.ack(&consumer_name, &event_id).await {
                    sys_error!("[{}] queue ack error: {}", consumer_name, e);
                }
            }
            Err(e) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                let err_str = format!("{:?}", e);
                sys_error!(
                    "[{}] on_event error for {}: {}",
                    consumer_name,
                    event_id,
                    e
                );
                // 埋点：on_consume_failure
                if let Some(hook) = registry_arc.metrics_hook() {
                    hook.on_consume_failure(&consumer_name, &meta, duration_ms, &err_str);
                }
                if let Err(e) = consumer.nack(&event_id).await {
                    sys_error!("[{}] consumer nack error: {}", consumer_name, e);
                }
                if let Err(e) = registry_arc.nack(&consumer_name, &event_id).await {
                    sys_error!("[{}] queue nack error: {}", consumer_name, e);
                }
                tokio::time::sleep(Duration::from_millis(error_sleep)).await;
            }
        }
    }
    Ok(None) => {
        tokio::time::sleep(Duration::from_millis(empty_sleep)).await;
    }
    Err(e) => {
        sys_error!("[{}] dequeue error: {}", consumer_name, e);
        tokio::time::sleep(Duration::from_millis(error_sleep)).await;
    }
}
```

- [ ] **Step 6: 在 pkg/aop/mod.rs 导出 AopMetricsHook**

修改 `src/pkg/aop/mod.rs` 的 re-export（约 line 28）：

```rust
pub use core::{AopEventMeta, AopMetricsHook, ConsumeMode, Consumer, Event, EventKind, Producer, Registry};
```

- [ ] **Step 7: 编译验证**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo check --lib`
Expected: 编译通过

- [ ] **Step 8: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add src/pkg/aop/core/metrics_hook.rs src/pkg/aop/core/mod.rs src/pkg/aop/core/registry.rs src/pkg/aop/mod.rs
git commit -m "feat: AOP 框架新增 AopMetricsHook trait 和 Registry 埋点机制"
```

---

## Task 2: AopStatsCollector 内存统计收集器

**Files:**
- Create: `src/consumer/aop_stats_collector.rs`
- Modify: `src/consumer/mod.rs`

**目标：** 实现纯内存的 AOP 统计收集器，提供总计数器 + 滑动窗口时序数据（最近 60 分钟，按分钟桶）+ 查询快照方法。零 DuckDB 依赖，重启即重置。

- [ ] **Step 1: 创建 aop_stats_collector.rs**

创建 `src/consumer/aop_stats_collector.rs`：

```rust
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
```

- [ ] **Step 2: 在 consumer/mod.rs 注册 aop_stats_collector 模块**

修改 `src/consumer/mod.rs`，加入：

```rust
pub mod aop_stats_collector;
pub use aop_stats_collector::{
    AopDistributionItem, AopOverview, AopStatsCollector, AopTimeSeriesPoint,
};
```

- [ ] **Step 3: 编译验证 + 运行单元测试**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo test --lib aop_stats_collector`
Expected: 6 个测试全部通过

- [ ] **Step 4: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add src/consumer/aop_stats_collector.rs src/consumer/mod.rs
git commit -m "feat: 新增 AopStatsCollector 内存统计收集器（滑动窗口 60 分钟）"
```

---

## Task 3: AopStatsHook 业务实现 + lib.rs 注入

**Files:**
- Create: `src/consumer/aop_stats_hook.rs`
- Modify: `src/consumer/mod.rs`
- Modify: `src/lib.rs`

**目标：** 实现 `AopMetricsHook` trait 的业务版本 `AopStatsHook`，在 4 个回调中调用 `AopStatsCollector::record`。在 `lib.rs::run()` 创建 collector + hook 并注入 Registry。

- [ ] **Step 1: 创建 aop_stats_hook.rs**

创建 `src/consumer/aop_stats_hook.rs`：

```rust
//! AOP 统计采集 Hook 业务实现
//!
//! 实现 `AopMetricsHook` trait，在 4 个回调中调用 `AopStatsCollector::record`
//! 写入内存收集器。在 `lib.rs::run()` 启动阶段注入到 Registry。
//!
//! 设计要点：
//! - 持有 `AopStatsCollector` 引用（克隆 Arc，零成本）
//! - 4 回调同步调用 collector.record（内部 async，但 hook 是同步方法）
//! - 使用 `tokio::spawn` 把 async record 转为后台任务，避免阻塞 AOP 主流程

use std::sync::Arc;

use crate::consumer::aop_stats_collector::AopStatsCollector;
use crate::pkg::aop::core::metrics_hook::{AopEventMeta, AopMetricsHook};

/// AOP 统计采集 Hook 业务实现
pub struct AopStatsHook {
    collector: AopStatsCollector,
}

impl AopStatsHook {
    pub fn new(collector: AopStatsCollector) -> Self {
        Self { collector }
    }

    /// 内部辅助：spawn 后台 record 任务
    fn spawn_record(
        &self,
        kind: String,
        consumer: String,
        status: String,
        duration_ms: u64,
    ) {
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
        let status = if is_async { "published" } else { "published_sync" };
        self.spawn_record(
            meta.event_kind.clone(),
            consumer_name.to_string(),
            status.to_string(),
            0,
        );
    }

    fn on_consume_start(&self, consumer_name: &str, meta: &AopEventMeta) {
        self.spawn_record(
            meta.event_kind.clone(),
            consumer_name.to_string(),
            "consuming".to_string(),
            0,
        );
    }

    fn on_consume_success(&self, consumer_name: &str, meta: &AopEventMeta, duration_ms: u64) {
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
        self.spawn_record(
            meta.event_kind.clone(),
            consumer_name.to_string(),
            "failed".to_string(),
            duration_ms,
        );
    }
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
```

- [ ] **Step 2: 在 consumer/mod.rs 注册 aop_stats_hook 模块**

修改 `src/consumer/mod.rs`，加入：

```rust
pub mod aop_stats_hook;
pub use aop_stats_hook::AopStatsHook;
```

- [ ] **Step 3: 在 lib.rs::run() 创建 collector + 注入 hook**

修改 `src/lib.rs` 的 `run()` 函数，在 `consumer::init().await?` 之后、`aop::init_all().await?` 之前（约 line 135-138 之间）加入：

```rust
// 创建 AOP 统计收集器并注入 Hook（在 worker 启动前）
let aop_stats_collector = consumer::AopStatsCollector::new();
{
    use std::sync::Arc;
    let hook = Arc::new(consumer::AopStatsHook::new(aop_stats_collector.clone()))
        as Arc<dyn crate::pkg::aop::AopMetricsHook>;
    crate::pkg::aop::registry().set_metrics_hook(hook);
    sys_info!("AOP stats hook installed");
}
// 把 collector 注入 SystemDomain（供后续查询）
crate::service::domain::system::set_aop_stats_collector(aop_stats_collector);
```

- [ ] **Step 4: 编译验证 + 运行测试**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo test --lib aop_stats`
Expected: aop_stats_collector（6 个）+ aop_stats_hook（1 个）测试全部通过

- [ ] **Step 5: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add src/consumer/aop_stats_hook.rs src/consumer/mod.rs src/lib.rs
git commit -m "feat: 实现 AopStatsHook 业务实现并在启动时注入 Registry"
```

---

## Task 4: SystemDomain 新增 AopStats 子能力

**Files:**
- Modify: `src/service/domain/system/mod.rs`
- Create: `src/service/domain/system/aop_stats.rs`

**目标：** 在 SystemDomain trait 加 `aop_stats()` getter，新增 `AopStats` 子能力 trait，子模块文件实现，直接读取 `AopStatsCollector` 内存快照。

- [ ] **Step 1: 在 domain/system/mod.rs 加 AopStats trait + 全局 collector 设置**

修改 `src/service/domain/system/mod.rs`：

**1a. 在文件顶部加入导入**：

```rust
use crate::consumer::{AopDistributionItem, AopOverview, AopStatsCollector, AopTimeSeriesPoint};
```

**1b. 在 SystemDomain trait 加 getter**（约 line 80-85）：

```rust
pub trait SystemDomain: Send + Sync {
    fn cron_manager(&self) -> &dyn CronManager;
    fn backup_manager(&self) -> &dyn BackupManager;
    fn log_query(&self) -> &dyn LogQuery;
    fn aop_monitor(&self) -> &dyn AopMonitor;
    fn aop_stats(&self) -> &dyn AopStats;
}
```

**1c. 定义 AopStats trait**（在 AopMonitor trait 定义之后）：

```rust
#[async_trait::async_trait]
pub trait AopStats: Send + Sync {
    /// 查询概览（全生命周期累计）
    async fn overview(&self, ctx: RequestContext) -> Result<AopOverview>;

    /// 查询时序数据（滑动窗口 60 分钟，按分钟桶）
    async fn time_series(
        &self,
        ctx: RequestContext,
        event_kind: Option<String>,
        consumer_name: Option<String>,
        status: Option<String>,
    ) -> Result<Vec<AopTimeSeriesPoint>>;

    /// 查询分布
    async fn distribution(
        &self,
        ctx: RequestContext,
        group_by: String,
        status_filter: Option<String>,
    ) -> Result<Vec<AopDistributionItem>>;
}
```

**1d. 在 mod.rs 声明子模块**：

```rust
mod aop_stats;
```

**1e. 在 mod.rs 末尾新增全局 collector 设置函数**：

```rust
/// 全局 AopStatsCollector 引用（由 lib.rs 在启动时设置）
static AOP_STATS_COLLECTOR: once_cell::sync::OnceCell<AopStatsCollector> =
    once_cell::sync::OnceCell::new();

/// 启动时设置 AopStatsCollector（由 lib.rs 调用）
pub fn set_aop_stats_collector(collector: AopStatsCollector) {
    let _ = AOP_STATS_COLLECTOR.set(collector);
}

/// 获取 AopStatsCollector（内部使用）
pub(crate) fn aop_stats_collector() -> Option<&'static AopStatsCollector> {
    AOP_STATS_COLLECTOR.get()
}
```

- [ ] **Step 2: 创建 domain/system/aop_stats.rs**

创建 `src/service/domain/system/aop_stats.rs`：

```rust
//! AopStats 子能力实现
//!
//! 直接读取全局 AopStatsCollector 内存快照，无 DAO/DAL 中转。

use async_trait::async_trait;
use common::error::{Error, Result};

use crate::consumer::{AopDistributionItem, AopOverview, AopTimeSeriesPoint};
use crate::pkg::request_context::RequestContext;

use super::aop_stats_collector;
use super::{AopStats, SystemDomainImpl};

#[async_trait]
impl AopStats for SystemDomainImpl {
    async fn overview(&self, _ctx: RequestContext) -> Result<AopOverview> {
        let collector = aop_stats_collector().ok_or_else(|| {
            Error::internal("AopStatsCollector not initialized")
        })?;
        Ok(collector.overview().await)
    }

    async fn time_series(
        &self,
        _ctx: RequestContext,
        event_kind: Option<String>,
        consumer_name: Option<String>,
        status: Option<String>,
    ) -> Result<Vec<AopTimeSeriesPoint>> {
        let collector = aop_stats_collector().ok_or_else(|| {
            Error::internal("AopStatsCollector not initialized")
        })?;
        Ok(collector
            .time_series(
                event_kind.as_deref(),
                consumer_name.as_deref(),
                status.as_deref(),
            )
            .await)
    }

    async fn distribution(
        &self,
        _ctx: RequestContext,
        group_by: String,
        status_filter: Option<String>,
    ) -> Result<Vec<AopDistributionItem>> {
        let collector = aop_stats_collector().ok_or_else(|| {
            Error::internal("AopStatsCollector not initialized")
        })?;
        Ok(collector
            .distribution(&group_by, status_filter.as_deref())
            .await)
    }
}
```

- [ ] **Step 3: 在 SystemDomainImpl 加 aop_stats 实现**

修改 `src/service/domain/system/mod.rs` 的 `impl SystemDomain for SystemDomainImpl`，加入：

```rust
fn aop_stats(&self) -> &dyn AopStats {
    self
}
```

- [ ] **Step 4: 编译验证**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo check --lib`
Expected: 编译通过

- [ ] **Step 5: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add src/service/domain/system/mod.rs src/service/domain/system/aop_stats.rs
git commit -m "feat: SystemDomain 新增 AopStats 子能力（直接读内存 collector）"
```

---

## Task 5: 新增 3 个 HTTP 端点

**Files:**
- Modify: `common/src/api/system.rs`
- Create: `src/handlers/system/aop_stats.rs`
- Modify: `src/handlers/system/mod.rs`
- Modify: `src/router.rs`

**目标：** 新增 3 个 AOP stats HTTP 端点：overview / time_series / distribution。

- [ ] **Step 1: 在 common/src/api/system.rs 新增响应 DTO**

在 `common/src/api/system.rs` 末尾追加：

```rust
/// AOP 实时统计概览响应
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct AopStatsOverviewResponse {
    pub total_published: u64,
    pub total_consumed: u64,
    pub total_success: u64,
    pub total_failed: u64,
    pub avg_duration_ms: f64,
}

/// AOP 实时统计时序数据点
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct AopStatsTimeSeriesPoint {
    pub interval_start: i64,
    pub call_count: u64,
}

/// AOP 实时统计时序响应
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct AopStatsTimeSeriesResponse {
    pub points: Vec<AopStatsTimeSeriesPoint>,
}

/// AOP 实时统计分布项
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct AopStatsDistributionItem {
    pub label: String,
    pub value: u64,
}

/// AOP 实时统计分布响应
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct AopStatsDistributionResponse {
    pub items: Vec<AopStatsDistributionItem>,
}
```

- [ ] **Step 2: 创建 handlers/system/aop_stats.rs**

创建 `src/handlers/system/aop_stats.rs`：

```rust
//! AOP 实时统计 Handler（3 个端点）
//!
//! 直接读取内存中的 AopStatsCollector 快照，零 DB 查询，毫秒级响应。

use axum::extract::Query;
use axum::Json;
use common::api::{AopStatsDistributionItem, AopStatsDistributionResponse,
    AopStatsOverviewResponse, AopStatsTimeSeriesPoint, AopStatsTimeSeriesResponse, ApiError,
    ApiResponse};
use serde::Deserialize;

use crate::pkg::auth::request_context_extractor::RequestContextExtractor;
use crate::service::domain::system::domain;

/// 概览查询参数
#[derive(Debug, Deserialize, Default)]
pub struct OverviewQuery {}

/// 时序查询参数
#[derive(Debug, Deserialize, Default)]
pub struct TimeSeriesQuery {
    pub event_kind: Option<String>,
    pub consumer_name: Option<String>,
    pub status: Option<String>,
}

/// 分布查询参数
#[derive(Debug, Deserialize)]
pub struct DistributionQuery {
    pub group_by: String, // "consumer" | "status" | "kind"
    pub status: Option<String>,
}

/// GET /api/v1/system/aop/stats/overview
pub async fn overview(
    ctx: RequestContextExtractor,
    Query(_params): Query<OverviewQuery>,
) -> Result<Json<ApiResponse<AopStatsOverviewResponse>>, ApiError> {
    let result = domain()
        .aop_stats()
        .overview(ctx.0)
        .await
        .map_err(|e| ApiError {
            http_status: 500,
            error_code: "AOP_STATS_QUERY_FAILED".to_string(),
            message: format!("{:?}", e),
        })?;

    Ok(Json(ApiResponse::ok(AopStatsOverviewResponse {
        total_published: result.total_published,
        total_consumed: result.total_consumed,
        total_success: result.total_success,
        total_failed: result.total_failed,
        avg_duration_ms: result.avg_duration_ms,
    })))
}

/// GET /api/v1/system/aop/stats/time-series
pub async fn time_series(
    ctx: RequestContextExtractor,
    Query(params): Query<TimeSeriesQuery>,
) -> Result<Json<ApiResponse<AopStatsTimeSeriesResponse>>, ApiError> {
    let points = domain()
        .aop_stats()
        .time_series(ctx.0, params.event_kind, params.consumer_name, params.status)
        .await
        .map_err(|e| ApiError {
            http_status: 500,
            error_code: "AOP_STATS_QUERY_FAILED".to_string(),
            message: format!("{:?}", e),
        })?;

    let points: Vec<AopStatsTimeSeriesPoint> = points
        .into_iter()
        .map(|p| AopStatsTimeSeriesPoint {
            interval_start: p.interval_start,
            call_count: p.call_count,
        })
        .collect();

    Ok(Json(ApiResponse::ok(AopStatsTimeSeriesResponse { points })))
}

/// GET /api/v1/system/aop/stats/distribution
pub async fn distribution(
    ctx: RequestContextExtractor,
    Query(params): Query<DistributionQuery>,
) -> Result<Json<ApiResponse<AopStatsDistributionResponse>>, ApiError> {
    let items = domain()
        .aop_stats()
        .distribution(ctx.0, params.group_by, params.status)
        .await
        .map_err(|e| ApiError {
            http_status: 500,
            error_code: "AOP_STATS_QUERY_FAILED".to_string(),
            message: format!("{:?}", e),
        })?;

    let items: Vec<AopStatsDistributionItem> = items
        .into_iter()
        .map(|i| AopStatsDistributionItem {
            label: i.label,
            value: i.value,
        })
        .collect();

    Ok(Json(ApiResponse::ok(AopStatsDistributionResponse { items })))
}
```

- [ ] **Step 3: 在 handlers/system/mod.rs 注册 aop_stats 模块**

修改 `src/handlers/system/mod.rs`，加入：

```rust
pub mod aop_stats;
```

- [ ] **Step 4: 在 router.rs 注册 3 个新路由**

修改 `src/router.rs`，在现有 `/api/v1/system/aop` 路由组后（约 line 612-625 附近）加入：

```rust
// AOP 实时统计
.route(
    "/api/v1/system/aop/stats/overview",
    get(handlers::system::aop_stats::overview),
)
.route(
    "/api/v1/system/aop/stats/time-series",
    get(handlers::system::aop_stats::time_series),
)
.route(
    "/api/v1/system/aop/stats/distribution",
    get(handlers::system::aop_stats::distribution),
)
```

- [ ] **Step 5: 编译验证**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo check --lib`
Expected: 编译通过

- [ ] **Step 6: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add common/src/api/system.rs src/handlers/system/aop_stats.rs src/handlers/system/mod.rs src/router.rs
git commit -m "feat: 新增 AOP 实时统计 3 个 HTTP 端点（overview/time-series/distribution）"
```

---

## Task 6: 前端 API 客户端 + Tab 改造 + 轮询 + 图表

**Files:**
- Modify: `frontend/src/api/system.rs`
- Modify: `frontend/src/pages/system/aop.rs`

**目标：** 前端新增 3 个 stats API 函数，AOP 页面改造为 Tab 布局，统计 Tab 5 秒轮询 + LineChart + DonutChart + 概览卡片。

- [ ] **Step 1: 在 frontend/src/api/system.rs 新增 stats API**

在 `frontend/src/api/system.rs` 末尾追加：

```rust
// ===== AOP 实时统计 =====

#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
pub struct AopStatsOverviewResponse {
    pub total_published: u64,
    pub total_consumed: u64,
    pub total_success: u64,
    pub total_failed: u64,
    pub avg_duration_ms: f64,
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
pub struct AopStatsTimeSeriesPoint {
    pub interval_start: i64,
    pub call_count: u64,
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
pub struct AopStatsTimeSeriesResponse {
    pub points: Vec<AopStatsTimeSeriesPoint>,
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
pub struct AopStatsDistributionItem {
    pub label: String,
    pub value: u64,
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
pub struct AopStatsDistributionResponse {
    pub items: Vec<AopStatsDistributionItem>,
}

pub async fn get_aop_stats_overview() -> Result<AopStatsOverviewResponse, String> {
    api_get("/api/v1/system/aop/stats/overview").await
}

pub async fn get_aop_stats_time_series(
    event_kind: Option<&str>,
    consumer_name: Option<&str>,
    status: Option<&str>,
) -> Result<AopStatsTimeSeriesResponse, String> {
    let mut url = "/api/v1/system/aop/stats/time-series".to_string();
    let mut params = Vec::new();
    if let Some(v) = event_kind {
        params.push(format!("event_kind={}", v));
    }
    if let Some(v) = consumer_name {
        params.push(format!("consumer_name={}", v));
    }
    if let Some(v) = status {
        params.push(format!("status={}", v));
    }
    if !params.is_empty() {
        url.push_str("?");
        url.push_str(&params.join("&"));
    }
    api_get(&url).await
}

pub async fn get_aop_stats_distribution(
    group_by: &str,
    status: Option<&str>,
) -> Result<AopStatsDistributionResponse, String> {
    let mut url = format!("/api/v1/system/aop/stats/distribution?group_by={}", group_by);
    if let Some(v) = status {
        url.push_str(&format!("&status={}", v));
    }
    api_get(&url).await
}
```

**注意：** `api_get` 函数签名需对齐现有 API 客户端模式（可能需要 `&str` 或 `String`）。

- [ ] **Step 2: 修改 aop.rs 顶部导入**

在 `frontend/src/pages/system/aop.rs` 顶部加入导入：

```rust
use crate::api::system::{
    get_aop_stats_distribution, get_aop_stats_overview, get_aop_stats_time_series,
    AopStatsDistributionItem, AopStatsOverviewResponse, AopStatsTimeSeriesPoint,
};
use crate::components::charts::donut_chart::{DonutChart, DonutSlice};
use crate::components::charts::line_chart::LineChart;
```

- [ ] **Step 3: 添加 Tab 状态 + 切换 UI**

在 `SystemAop` 组件中添加 Tab 状态，并在最外层 div 内最前面加 Tab 按钮：

```rust
let mut active_tab: Signal<&'static str> = use_signal(|| "monitor");
```

在页面主体最外层 div 内，加入 Tab 切换按钮组（在现有内容之前）：

```rust
div { class: "flex gap-2 mb-4",
    button {
        class: "btn btn-sm {if *active_tab.read() == \"monitor\" { \"btn-primary\" } else { \"btn-ghost\" }}",
        onclick: move |_| active_tab.set("monitor"),
        "实时监控"
    }
    button {
        class: "btn btn-sm {if *active_tab.read() == \"stats\" { \"btn-primary\" } else { \"btn-ghost\" }}",
        onclick: move |_| active_tab.set("stats"),
        "统计图表"
    }
}
```

- [ ] **Step 4: 用条件渲染包裹现有 monitor 内容**

将现有的"队列统计卡片区 + 事件列表区 + 事件详情 Modal"用 `if *active_tab.read() == "monitor"` 包裹：

```rust
if *active_tab.read() == "monitor" {
    div {
        // ... 现有的队列统计卡片 + 事件列表 + 事件详情 Modal 全部内容 ...
    }
}
```

- [ ] **Step 5: 添加 stats Tab 内容**

在 monitor Tab 内容后，加入 stats Tab：

```rust
if *active_tab.read() == "stats" {
    AopStatsPanel {}
}
```

- [ ] **Step 6: 创建 AopStatsPanel 子组件**

在 `frontend/src/pages/system/aop.rs` 文件末尾（`SystemAop` 组件外）新增 `AopStatsPanel` 组件：

```rust
/// AOP 统计面板（Tab 2 内容，5 秒轮询）
#[component]
fn AopStatsPanel() -> Element {
    let mut overview: Signal<Option<AopStatsOverviewResponse>> = use_signal(|| None);
    let mut time_series_points: Signal<Vec<AopStatsTimeSeriesPoint>> = use_signal(|| Vec::new());
    let mut consumer_dist: Signal<Vec<AopStatsDistributionItem>> = use_signal(|| Vec::new());
    let mut status_dist: Signal<Vec<AopStatsDistributionItem>> = use_signal(|| Vec::new());
    let mut loading: Signal<bool> = use_signal(|| true);
    let mut last_updated: Signal<Option<String>> = use_signal(|| None);

    let load_data = move || {
        spawn(async move {
            let ov = get_aop_stats_overview().await;
            let ts = get_aop_stats_time_series(None, None, None).await;
            let cd = get_aop_stats_distribution("consumer", None).await;
            let sd = get_aop_stats_distribution("status", None).await;

            if let Ok(o) = ov {
                overview.set(Some(o));
            }
            if let Ok(t) = ts {
                time_series_points.set(t.points);
            }
            if let Ok(c) = cd {
                consumer_dist.set(c.items);
            }
            if let Ok(s) = sd {
                status_dist.set(s.items);
            }
            loading.set(false);
            // 记录最后更新时间
            if let Some(date) = js_sys::Date::new_0().to_locale_string("zh-CN", &js_sys::Array::new()).as_string() {
                last_updated.set(Some(date));
            }
        });
    };

    // 5 秒轮询
    use_future(move || {
        load_data();
        async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                load_data();
            }
        }
    });

    let ov = overview.read().clone();
    let ts_points = time_series_points.read().clone();
    let cd_items = consumer_dist.read().clone();
    let sd_items = status_dist.read().clone();

    // 构造 LineChart 数据（Vec<TimeSeriesPoint>）
    let line_data: Vec<common::models::TimeSeriesPoint> = ts_points
        .iter()
        .map(|p| common::models::TimeSeriesPoint {
            interval_start: p.interval_start,
            tokens_input: 0,
            tokens_output: 0,
            call_count: p.call_count,
        })
        .collect();

    // 构造 status 分布 DonutChart 数据
    let status_slices: Vec<DonutSlice> = sd_items
        .iter()
        .map(|item| DonutSlice {
            label: item.label.clone(),
            value: item.value,
            color: aop_status_color(&item.label).to_string(),
        })
        .collect();

    // 构造 consumer 分布 DonutChart 数据
    let palette = ["#fa520f", "#10b981", "#3b82f6", "#f59e0b", "#8b5cf6", "#ec4899", "#14b8a6", "#6b7280"];
    let consumer_slices: Vec<DonutSlice> = cd_items
        .iter()
        .enumerate()
        .map(|(i, item)| DonutSlice {
            label: item.label.clone(),
            value: item.value,
            color: palette[i % palette.len()].to_string(),
        })
        .collect();

    rsx! {
        div { class: "space-y-4",
            // 概览卡片
            if let Some(o) = &ov {
                div { class: "grid grid-cols-2 md:grid-cols-5 gap-3",
                    div { class: "stat bg-base-100 rounded-box shadow-sm",
                        div { class: "stat-title", "总发布" }
                        div { class: "stat-value text-primary", "{o.total_published}" }
                    }
                    div { class: "stat bg-base-100 rounded-box shadow-sm",
                        div { class: "stat-title", "总消费" }
                        div { class: "stat-value text-info", "{o.total_consumed}" }
                    }
                    div { class: "stat bg-base-100 rounded-box shadow-sm",
                        div { class: "stat-title", "成功" }
                        div { class: "stat-value text-success", "{o.total_success}" }
                    }
                    div { class: "stat bg-base-100 rounded-box shadow-sm",
                        div { class: "stat-title", "失败" }
                        div { class: "stat-value text-error", "{o.total_failed}" }
                    }
                    div { class: "stat bg-base-100 rounded-box shadow-sm",
                        div { class: "stat-title", "平均耗时(ms)" }
                        div { class: "stat-value text-warning", "{:.0}", o.avg_duration_ms }
                    }
                }
            }

            // 时序折线图（最近 60 分钟，按分钟桶）
            if !line_data.is_empty() {
                div { class: "card bg-base-100 shadow-md",
                    div { class: "card-body",
                        h2 { class: "card-title", "事件趋势（最近 60 分钟，按分钟桶）" }
                        LineChart {
                            data: line_data,
                            width: Some(800.0),
                            height: Some(220.0),
                            title: Some("事件数量".to_string()),
                            value_label: Some("次数".to_string()),
                        }
                    }
                }
            }

            // 分布环形图（双列）
            div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                if !status_slices.is_empty() {
                    div { class: "card bg-base-100 shadow-md",
                        div { class: "card-body",
                            h2 { class: "card-title", "状态分布" }
                            DonutChart {
                                data: status_slices,
                                width: Some(240.0),
                                height: Some(240.0),
                                center_label: Some("事件数".to_string()),
                            }
                        }
                    }
                }
                if !consumer_slices.is_empty() {
                    div { class: "card bg-base-100 shadow-md",
                        div { class: "card-body",
                            h2 { class: "card-title", "消费者分布" }
                            DonutChart {
                                data: consumer_slices,
                                width: Some(240.0),
                                height: Some(240.0),
                                center_label: Some("事件数".to_string()),
                            }
                        }
                    }
                }
            }

            // 最后更新时间
            if let Some(t) = last_updated.read().as_ref() {
                div { class: "text-xs text-base-content/50 text-right",
                    "最后更新: {t}（5 秒自动刷新）"
                }
            }
        }
    }
}

/// AOP 事件状态对应的 HUD 风格颜色
fn aop_status_color(status: &str) -> &'static str {
    match status {
        "published" | "published_sync" => "#3b82f6",
        "consuming" => "#f59e0b",
        "success" => "#10b981",
        "failed" => "#ef4444",
        _ => "#6b7280",
    }
}
```

- [ ] **Step 7: 编译验证**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check`
Expected: 编译通过

- [ ] **Step 8: 视觉验证**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && dx serve --port 8080`

打开浏览器访问 `/system/aop`：
1. 默认显示"实时监控"Tab，内容与改造前完全一致
2. 点击"统计图表"Tab，显示概览卡片 + 时序折线图 + 状态分布环形图 + 消费者分布环形图
3. 图表视觉风格（HUD 深色背景 + 橙色主色）与知识图谱、其他详情页统计图一致
4. 页面右下角显示"最后更新"时间，每 5 秒自动刷新

- [ ] **Step 9: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/api/system.rs frontend/src/pages/system/aop.rs
git commit -m "feat: AOP 页面 Tab 布局 + 统计图表 + 5 秒轮询"
```

---

## Task 7: 全量测试 + 文档更新

**Files:**
- Modify: `AGENTS.md`

**目标：** 运行前后端全量测试确保无回归，更新 AGENTS.md 记录 Phase 3 里程碑。

- [ ] **Step 1: 运行后端全量测试**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo test --lib`
Expected: 所有测试通过（含新增的 7 个 aop_stats 测试：6 collector + 1 hook）

- [ ] **Step 2: 运行前端全量测试**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo test --bin frontend`
Expected: 所有测试通过

- [ ] **Step 3: 运行 common 全量测试**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo test -p common --lib`
Expected: 所有测试通过

- [ ] **Step 4: 更新 AGENTS.md 里程碑记录**

在 `AGENTS.md` 的"## 六、工作流与开发记录"章节，在 Phase 2 里程碑后追加：

```markdown
**✅ 统计图表 Phase 3：AOP 实时内存统计 + 轮询渲染**
- **设计哲学**：AOP 是运行时能力，重启即丢，记录到 DuckDB 无持久化价值。采用纯内存统计收集器，与 AOP 事件本身生命周期一致
- **AopMetricsHook trait**：`pkg/aop/core/metrics_hook.rs` 新增 4 回调 trait（on_publish/on_consume_start/on_consume_success/on_consume_failure），Registry 持有 `Option<Arc<dyn AopMetricsHook>>`，业务层注入实现，保持 AOP 框架零业务依赖原则
- **AopStatsCollector 内存收集器**：`consumer/aop_stats_collector.rs` 纯内存实现（零 DuckDB 依赖），提供总计数器（按 event_kind/consumer_name/status 三维索引）+ 滑动窗口时序数据（最近 60 分钟，按分钟桶，内存占用 < 50KB）
- **AopStatsHook 业务实现**：`consumer/aop_stats_hook.rs` 实现 AopMetricsHook，4 回调用 `tokio::spawn` 调 collector.record（不阻塞 AOP 主流程）
- **3 处埋点**：publish 同步/异步分发 + worker 协程 on_event 调用前后；每个 AOP 事件产生 2-3 条记录（published + consuming + success/failed）
- **SystemDomain AopStats 子能力**：SystemDomain 新增 `aop_stats()` getter + AopStats trait，直接读全局 collector（零 DAO/DAL 中转）
- **3 个 HTTP 端点**：`GET /api/v1/system/aop/stats/{overview|time-series|distribution}`，毫秒级响应（纯内存查询）
- **前端 AOP 页面 Tab 改造**：Tab 1 实时监控（保留现有功能），Tab 2 统计图表（概览卡片 + LineChart 时序 + DonutChart 状态分布 + DonutChart 消费者分布），5 秒轮询自动刷新
- **测试统计**：后端 753 测试（+7 新增 aop_stats 测试）+ 前端 38 测试 + common 50 测试 100% 通过，总计 841 测试
```

- [ ] **Step 5: 更新 AGENTS.md 测试统计**

在 `AGENTS.md` 的"### 1.3 整体完成度与测试统计"表格中更新数值：

```
| **总测试数** | **841** | 后端 753 + 前端 38 + common 50，DAO + DAL + Domain + Handler + Pkg 完整覆盖 |
```

- [ ] **Step 6: 更新 AGENTS.md 功能表格**

更新"已实现核心功能"表格中 AOP 队列监控相关条目：

```
| 📊 AOP 队列监控 | ✅ | 队列运行时监控 + 实时统计图表（HUD 风格折线图时序 + 环形图分布，纯内存收集器 60 分钟滑动窗口，5 秒轮询，埋点 publish/consume/success/failure） |
```

- [ ] **Step 7: 提交文档更新**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add AGENTS.md
git commit -m "docs: 记录统计图表 Phase 3 里程碑（AOP 实时内存统计 + 轮询渲染）"
```

---

## Self-Review 检查

**1. Spec 覆盖：**
- ✅ AOP 框架 hook 机制（保持零业务依赖） → Task 1
- ✅ AopStatsCollector 内存收集器（滑动窗口 60 分钟） → Task 2
- ✅ AopStatsHook 业务实现 + 注入 → Task 3
- ✅ SystemDomain AopStats 子能力（直接读内存） → Task 4
- ✅ 3 个 HTTP 端点 → Task 5
- ✅ 前端 API + Tab + 轮询 + 图表 → Task 6
- ✅ 测试 + 文档 → Task 7

**2. Placeholder 扫描：** 无 TODO/TBD/占位符，所有步骤含完整代码

**3. Type 一致性：**
- `AopMetricsHook` trait 4 方法在 Task 1 定义，Task 3 实现签名一致 ✅
- `AopEventMeta` 在 Task 1 定义，Task 1 埋点和 Task 3 实现引用一致 ✅
- `AopStatsCollector` 在 Task 2 定义，Task 3 持有 + Task 4 读取 ✅
- `AopOverview` 在 Task 2 定义，Task 4/5 引用字段名一致 ✅
- `AopTimeSeriesPoint` 在 Task 2 定义，Task 5 DTO 字段名一致 ✅
- `AopStatsDistributionItem` 在 Task 5 DTO 定义，Task 6 前端 DTO 字段名一致 ✅

**4. 设计优势对比（vs 原 DuckDB 方案）：**
- ❌ 原 10 个 Task → ✅ 现 7 个 Task（减少 30%）
- ❌ 原需新增 DAO/DAL/stats 事件/专用表 → ✅ 现零 DAO/DAL/零 DuckDB
- ❌ 原历史数据持久化无价值 → ✅ 现与 AOP 生命周期一致
- ❌ 原查询需 DuckDB SQL → ✅ 现纯内存读，毫秒级响应
- ✅ 两者都保持 AOP 框架零业务依赖

**5. 已知风险点：**
- Task 3 的 `set_aop_stats_collector` 函数需在 `domain/system/mod.rs` 中定义（Task 4 Step 1e 已覆盖）
- Task 6 的 `spawn` / `use_future` / `js_sys::Date` 需对齐 Dioxus 0.7 实际 API
- Task 6 的 `api_get` 函数签名需对齐现有前端 API 客户端模式
