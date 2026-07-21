# AOP 队列监控功能实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 SystemDomain 中增加 AOP 队列监控能力，提供队列状态统计和事件查询接口。

**Architecture:** 
- 扩展 `EventQueue` trait 增加监控方法（stats/query_events/get_event）
- 在 `Registry` 中提供聚合查询入口
- 在 `SystemDomain` 中新增 `AopMonitor` 子模块
- 通过 Handler 提供外部 API

**Tech Stack:** Rust + async_trait + serde_json

---

## 文件结构

| 文件 | 职责 |
|------|------|
| `src/pkg/aop/queue/mod.rs` | 扩展 EventQueue trait，定义监控数据结构 |
| `src/pkg/aop/queue/in_memory.rs` | 实现监控方法 |
| `src/pkg/aop/core/registry.rs` | 增加聚合监控方法 |
| `src/service/domain/system/aop_monitor.rs` | 新增 AopMonitor trait 和实现 |
| `src/service/domain/system/mod.rs` | 注册 AopMonitor 子模块 |
| `src/handlers/system/aop.rs` | 新增 Handler API |
| `src/handlers/system/mod.rs` | 注册 aop handler |
| `src/router.rs` | 注册路由 |

---

## Task 1: 扩展 EventQueue trait 监控数据结构

**Files:**
- Modify: `src/pkg/aop/queue/mod.rs`

- [ ] **Step 1: 在 EventQueue trait 之前定义监控数据结构**

```rust
use serde::Serialize;

/// 队列状态快照
#[derive(Debug, Clone, Serialize)]
pub struct QueueStats {
    /// 等待处理的事件总数
    pub pending_count: usize,
    /// 正在处理的事件数
    pub in_progress_count: usize,
    /// 各 order_key 的等待数量
    pub order_keys: Vec<OrderKeyStats>,
    /// 最老事件距今的秒数（如果有）
    pub oldest_event_age_secs: Option<u64>,
}

/// 单个 order_key 的统计
#[derive(Debug, Clone, Serialize)]
pub struct OrderKeyStats {
    pub order_key: String,
    pub pending_count: usize,
}

/// 事件摘要（列表查询返回）
#[derive(Debug, Clone, Serialize)]
pub struct EventSummary {
    pub event_id: String,
    pub event_kind: String,
    pub order_key: String,
    pub priority: u8,
    pub created_at: i64,
    pub status: EventStatus,
}

/// 事件状态
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EventStatus {
    Pending,
    Processing,
}

/// 事件详情（单个查询返回）
#[derive(Debug, Clone, Serialize)]
pub struct EventDetail {
    pub summary: EventSummary,
    /// 脱敏后的事件内容预览（前 200 字符）
    pub payload_preview: String,
}

/// 事件查询过滤条件
#[derive(Debug, Clone, Default)]
pub struct EventQueryFilter {
    pub order_key: Option<String>,
    pub status: Option<EventStatus>,
    pub limit: usize,
    pub offset: usize,
}

impl Default for EventQueryFilter {
    fn default() -> Self {
        Self {
            order_key: None,
            status: None,
            limit: 100,
            offset: 0,
        }
    }
}
```

- [ ] **Step 2: 在 EventQueue trait 中添加监控方法**

在 `pub use in_memory::InMemoryEventQueue;` 之后添加：

```rust
pub use in_memory::InMemoryEventQueue;

// 导出监控数据结构
pub use super::queue::{QueueStats, OrderKeyStats, EventSummary, EventDetail, EventStatus, EventQueryFilter};
```

在 EventQueue trait 中添加方法：

```rust
#[async_trait]
pub trait EventQueue: Send + Sync + std::fmt::Debug + 'static {
    // ... 现有方法 ...

    // ===== 监控方法 =====

    /// 获取队列状态统计
    fn stats(&self) -> QueueStats;

    /// 按条件查询事件列表
    fn query_events(&self, filter: EventQueryFilter) -> Vec<EventSummary>;

    /// 查询单个事件详情
    fn get_event(&self, event_id: &str) -> Option<EventDetail>;
}
```

- [ ] **Step 3: 验证编译**

Run: `cargo check --tests`
Expected: PASS（方法未实现，会有编译错误提示）

---

## Task 2: 实现 InMemoryEventQueue 的监控方法

**Files:**
- Modify: `src/pkg/aop/queue/in_memory.rs`

- [ ] **Step 1: 实现 stats 方法**

在 `impl InMemoryEventQueue` 中添加辅助方法：

```rust
impl InMemoryEventQueue {
    // ... 现有代码 ...

    fn collect_order_key_stats(&self) -> Vec<super::OrderKeyStats> {
        let queues = unsafe { &*self.queues.get() };
        let has_active_message = unsafe { &*self.has_active_message.get() };

        let mut stats = Vec::new();
        for (order_key, queue) in queues.iter() {
            let pending = queue.len();
            if pending > 0 {
                stats.push(super::OrderKeyStats {
                    order_key: order_key.clone(),
                    pending_count: pending,
                });
            }
        }
        stats.sort_by(|a, b| b.pending_count.cmp(&a.pending_count));
        stats
    }

    fn find_oldest_event_age(&self) -> Option<u64> {
        use std::time::{SystemTime, UNIX_EPOCH};

        let events = unsafe { &*self.events.get() };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        events
            .values()
            .filter_map(|e| e.get("created_at").and_then(|v| v.as_i64()))
            .min()
            .map(|oldest| (now - oldest) as u64)
    }
}
```

在 `impl EventQueue for InMemoryEventQueue` 中添加：

```rust
fn stats(&self) -> super::QueueStats {
    let _guard = self.lock.lock().ok();

    let events = unsafe { &*self.events.get() };
    let in_progress = unsafe { &*self.in_progress.get() };

    super::QueueStats {
        pending_count: events.len() - in_progress.len(),
        in_progress_count: in_progress.len(),
        order_keys: self.collect_order_key_stats(),
        oldest_event_age_secs: self.find_oldest_event_age(),
    }
}
```

- [ ] **Step 2: 实现 query_events 方法**

```rust
fn query_events(&self, filter: super::EventQueryFilter) -> Vec<super::EventSummary> {
    let _guard = self.lock.lock().ok();

    let events = unsafe { &*self.events.get() };
    let in_progress = unsafe { &*self.in_progress.get() };
    let global_heap = unsafe { &*self.global_heap.get() };

    let mut results: Vec<super::EventSummary> = Vec::new();

    // 收集所有事件
    for (event_id, event) in events.iter() {
        let order_key = event.get("order_key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // 应用 order_key 过滤
        if let Some(ref ok) = filter.order_key {
            if &order_key != ok {
                continue;
            }
        }

        let status = if in_progress.contains_key(event_id) {
            super::EventStatus::Processing
        } else {
            super::EventStatus::Pending
        };

        // 应用 status 过滤
        if let Some(ref s) = filter.status {
            if *s != status {
                continue;
            }
        }

        let event_kind = event.get("kind")
            .and_then(|v| v.as_str())
            .or_else(|| event.get("message_id").map(|_| "message.created"))
            .unwrap_or("unknown")
            .to_string();

        let priority = event.get("priority")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u8;

        let created_at = event.get("created_at")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        results.push(super::EventSummary {
            event_id: event_id.clone(),
            event_kind,
            order_key,
            priority,
            created_at,
            status,
        });
    }

    // 按 created_at 降序排序（最新的在前）
    results.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    // 应用分页
    results.into_iter()
        .skip(filter.offset)
        .take(filter.limit)
        .collect()
}
```

- [ ] **Step 3: 实现 get_event 方法**

```rust
fn get_event(&self, event_id: &str) -> Option<super::EventDetail> {
    let _guard = self.lock.lock().ok();

    let events = unsafe { &*self.events.get() };
    let in_progress = unsafe { &*self.in_progress.get() };

    let event = events.get(event_id)?;
    let event_json = event.clone();

    let order_key = event_json.get("order_key")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let status = if in_progress.contains_key(event_id) {
        super::EventStatus::Processing
    } else {
        super::EventStatus::Pending
    };

    let event_kind = event_json.get("kind")
        .and_then(|v| v.as_str())
        .or_else(|| event_json.get("message_id").map(|_| "message.created"))
        .unwrap_or("unknown")
        .to_string();

    let priority = event_json.get("priority")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u8;

    let created_at = event_json.get("created_at")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    // 脱敏处理：截取前 200 字符
    let payload_preview = {
        let json_str = serde_json::to_string(&event_json).unwrap_or_default();
        if json_str.len() > 200 {
            format!("{}... (truncated, total {} bytes)", &json_str[..200], json_str.len())
        } else {
            json_str
        }
    };

    Some(super::EventDetail {
        summary: super::EventSummary {
            event_id: event_id.to_string(),
            event_kind,
            order_key,
            priority,
            created_at,
            status,
        },
        payload_preview,
    })
}
```

- [ ] **Step 4: 验证编译**

Run: `cargo check`
Expected: PASS

---

## Task 3: Registry 中增加聚合监控方法

**Files:**
- Modify: `src/pkg/aop/core/registry.rs`

- [ ] **Step 1: 添加聚合 stats 方法**

在 `impl Registry` 中添加：

```rust
/// 获取所有队列的聚合统计
pub fn all_queue_stats(&self) -> Vec<(String, crate::pkg::aop::queue::QueueStats)> {
    let queues = match self.queues.read() {
        Ok(q) => q,
        Err(_) => return Vec::new(),
    };

    let mut result = Vec::new();
    for (name, queue) in queues.iter() {
        result.push((name.clone(), queue.stats()));
    }

    // 按队列名排序
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

/// 获取指定消费者的队列统计
pub fn queue_stats(&self, consumer_name: &str) -> Option<crate::pkg::aop::queue::QueueStats> {
    let queues = self.queues.read().ok()?;
    let queue = queues.get(consumer_name)?;
    Some(queue.stats())
}

/// 查询指定消费者队列中的事件
pub fn query_events(&self, consumer_name: &str, filter: crate::pkg::aop::queue::EventQueryFilter) -> Option<Vec<crate::pkg::aop::queue::EventSummary>> {
    let queues = self.queues.read().ok()?;
    let queue = queues.get(consumer_name)?;
    Some(queue.query_events(filter))
}

/// 获取指定消费者队列中的事件详情
pub fn get_event(&self, consumer_name: &str, event_id: &str) -> Option<crate::pkg::aop::queue::EventDetail> {
    let queues = self.queues.read().ok()?;
    let queue = queues.get(consumer_name)?;
    queue.get_event(event_id)
}
```

- [ ] **Step 2: 验证编译**

Run: `cargo check`
Expected: PASS

---

## Task 4: 创建 AopMonitor 子模块

**Files:**
- Create: `src/service/domain/system/aop_monitor.rs`

- [ ] **Step 1: 创建 AopMonitor trait 和实现**

```rust
use common::error::Result;
use crate::pkg::aop;

/// AOP 监控接口
pub trait AopMonitor: Send + Sync {
    /// 获取所有队列的聚合统计
    fn all_queue_stats(&self) -> Vec<(String, aop::QueueStats)>;

    /// 获取指定消费者队列统计
    fn queue_stats(&self, consumer_name: &str) -> Option<aop::QueueStats>;

    /// 查询队列中的事件列表
    fn list_events(&self, consumer_name: &str, filter: aop::EventQueryFilter) -> Option<Vec<aop::EventSummary>>;

    /// 获取事件详情
    fn get_event(&self, consumer_name: &str, event_id: &str) -> Option<aop::EventDetail>;
}

/// AOP 监控实现
pub struct AopMonitorImpl;

impl AopMonitor for AopMonitorImpl {
    fn all_queue_stats(&self) -> Vec<(String, aop::QueueStats)> {
        aop::registry().all_queue_stats()
    }

    fn queue_stats(&self, consumer_name: &str) -> Option<aop::QueueStats> {
        aop::registry().queue_stats(consumer_name)
    }

    fn list_events(&self, consumer_name: &str, filter: aop::EventQueryFilter) -> Option<Vec<aop::EventSummary>> {
        aop::registry().query_events(consumer_name, filter)
    }

    fn get_event(&self, consumer_name: &str, event_id: &str) -> Option<aop::EventDetail> {
        aop::registry().get_event(consumer_name, event_id)
    }
}
```

- [ ] **Step 2: 在 mod.rs 中注册 AopMonitor**

在 `src/service/domain/system/mod.rs` 中添加：

```rust
mod aop_monitor;
pub use aop_monitor::{AopMonitor, AopMonitorImpl};
```

在 `SystemDomain` trait 中添加：

```rust
pub trait SystemDomain: Send + Sync {
    fn cron_manager(&self) -> &dyn CronManager;
    fn backup_manager(&self) -> &dyn BackupManager;
    fn log_query(&self) -> &dyn LogQuery;
    fn aop_monitor(&self) -> &dyn AopMonitor;  // 新增
}
```

在 `SystemDomainImpl` 中添加：

```rust
struct SystemDomainImpl {
    cron_trigger_dal: Arc<dyn CronTriggerDal>,
    backup_dal: Arc<dyn BackupDal + Send + Sync>,
    log_query_dal: Arc<dyn LogQueryDal + Send + Sync>,
    aop_monitor: AopMonitorImpl,  // 新增
}

impl SystemDomainImpl {
    fn new(
        cron_trigger_dal: Arc<dyn CronTriggerDal>,
        backup_dal: Arc<dyn BackupDal + Send + Sync>,
        log_query_dal: Arc<dyn LogQueryDal + Send + Sync>,
    ) -> Self {
        Self {
            cron_trigger_dal,
            backup_dal,
            log_query_dal,
            aop_monitor: AopMonitorImpl,
        }
    }
}

impl SystemDomain for SystemDomainImpl {
    fn aop_monitor(&self) -> &dyn AopMonitor {
        &self.aop_monitor
    }
    // ... 其他方法保持不变
}
```

- [ ] **Step 3: 验证编译**

Run: `cargo check`
Expected: PASS

---

## Task 5: 创建 Handler API

**Files:**
- Create: `src/handlers/system/aop.rs`
- Modify: `src/handlers/system/mod.rs`
- Modify: `src/router.rs`

- [ ] **Step 1: 创建 aop handler**

```rust
use axum::{
    extract::{Path, Query},
    Json,
};
use serde::{Deserialize, Serialize};
use crate::pkg::RequestContext;
use crate::service::domain::system;
use super::ApiResponse;

#[derive(Debug, Deserialize)]
pub struct EventListQuery {
    pub order_key: Option<String>,
    pub status: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

fn default_limit() -> usize { 100 }

#[derive(Debug, Serialize)]
pub struct QueueStatsResponse {
    pub consumer_name: String,
    pub pending_count: usize,
    pub in_progress_count: usize,
    pub order_keys: Vec<OrderKeyInfo>,
    pub oldest_event_age_secs: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct OrderKeyInfo {
    pub order_key: String,
    pub pending_count: usize,
}

#[derive(Debug, Serialize)]
pub struct EventSummaryResponse {
    pub event_id: String,
    pub event_kind: String,
    pub order_key: String,
    pub priority: u8,
    pub created_at: i64,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct EventDetailResponse {
    pub event_id: String,
    pub event_kind: String,
    pub order_key: String,
    pub priority: u8,
    pub created_at: i64,
    pub status: String,
    pub payload_preview: String,
}

/// GET /api/v1/system/aop/stats
pub async fn get_all_queue_stats() -> ApiResponse<Vec<QueueStatsResponse>> {
    let stats = system::domain().aop_monitor().all_queue_stats();

    let response: Vec<QueueStatsResponse> = stats
        .into_iter()
        .map(|(name, s)| QueueStatsResponse {
            consumer_name: name,
            pending_count: s.pending_count,
            in_progress_count: s.in_progress_count,
            order_keys: s.order_keys.into_iter()
                .map(|ok| OrderKeyInfo {
                    order_key: ok.order_key,
                    pending_count: ok.pending_count,
                })
                .collect(),
            oldest_event_age_secs: s.oldest_event_age_secs,
        })
        .collect();

    ApiResponse::success(response)
}

/// GET /api/v1/system/aop/:consumer/stats
pub async fn get_queue_stats(
    Path(consumer): Path<String>,
) -> ApiResponse<QueueStatsResponse> {
    let stats = system::domain().aop_monitor().queue_stats(&consumer);

    match stats {
        Some(s) => ApiResponse::success(QueueStatsResponse {
            consumer_name: consumer,
            pending_count: s.pending_count,
            in_progress_count: s.in_progress_count,
            order_keys: s.order_keys.into_iter()
                .map(|ok| OrderKeyInfo {
                    order_key: ok.order_key,
                    pending_count: ok.pending_count,
                })
                .collect(),
            oldest_event_age_secs: s.oldest_event_age_secs,
        }),
        None => ApiResponse::not_found(format!("Consumer queue '{}' not found", consumer)),
    }
}

/// GET /api/v1/system/aop/:consumer/events
pub async fn list_events(
    Path(consumer): Path<String>,
    Query(query): Query<EventListQuery>,
) -> ApiResponse<Vec<EventSummaryResponse>> {
    let status = query.status.and_then(|s| match s.to_lowercase().as_str() {
        "pending" => Some(crate::pkg::aop::EventStatus::Pending),
        "processing" => Some(crate::pkg::aop::EventStatus::Processing),
        _ => None,
    });

    let filter = crate::pkg::aop::EventQueryFilter {
        order_key: query.order_key,
        status,
        limit: query.limit.min(1000), // 上限保护
        offset: query.offset,
    };

    let events = system::domain().aop_monitor().list_events(&consumer, filter);

    match events {
        Some(list) => {
            let response: Vec<EventSummaryResponse> = list
                .into_iter()
                .map(|e| EventSummaryResponse {
                    event_id: e.event_id,
                    event_kind: e.event_kind,
                    order_key: e.order_key,
                    priority: e.priority,
                    created_at: e.created_at,
                    status: format!("{:?}", e.status).to_lowercase(),
                })
                .collect();
            ApiResponse::success(response)
        }
        None => ApiResponse::not_found(format!("Consumer queue '{}' not found", consumer)),
    }
}

/// GET /api/v1/system/aop/:consumer/events/:event_id
pub async fn get_event(
    Path((consumer, event_id)): Path<(String, String)>,
) -> ApiResponse<EventDetailResponse> {
    let event = system::domain().aop_monitor().get_event(&consumer, &event_id);

    match event {
        Some(e) => ApiResponse::success(EventDetailResponse {
            event_id: e.summary.event_id,
            event_kind: e.summary.event_kind,
            order_key: e.summary.order_key,
            priority: e.summary.priority,
            created_at: e.summary.created_at,
            status: format!("{:?}", e.summary.status).to_lowercase(),
            payload_preview: e.payload_preview,
        }),
        None => ApiResponse::not_found(format!("Event '{}' not found in queue '{}'", event_id, consumer)),
    }
}
```

- [ ] **Step 2: 在 system/mod.rs 中注册**

在 `src/handlers/system/mod.rs` 中添加：

```rust
pub mod aop;

// 在 RouterExt 实现中添加路由注册
pub fn system_routes() -> Router {
    Router::new()
        // ... 现有路由 ...
        .route("/aop/stats", get(aop::get_all_queue_stats))
        .route("/aop/:consumer/stats", get(aop::get_queue_stats))
        .route("/aop/:consumer/events", get(aop::list_events))
        .route("/aop/:consumer/events/:event_id", get(aop::get_event))
}
```

- [ ] **Step 3: 在 router.rs 中注册**

确保 system_routes 已被正确注册（应该已经存在）。

- [ ] **Step 4: 验证编译**

Run: `cargo check`
Expected: PASS

---

## Task 6: 运行测试验证

- [ ] **Step 1: 运行全部测试**

Run: `cargo test`
Expected: 754 passed; 0 failed

- [ ] **Step 2: 手动测试 API（可选）**

启动服务后测试：

```bash
# 获取所有队列统计
curl http://localhost:3000/api/v1/system/aop/stats

# 获取指定消费者统计
curl http://localhost:3000/api/v1/system/aop/agent.awakening/stats

# 查询事件列表
curl "http://localhost:3000/api/v1/system/aop/agent.awakening/events?limit=10"

# 获取事件详情
curl http://localhost:3000/api/v1/system/aop/agent.awakening/events/<event_id>
```

---

## Task 7: 提交代码

- [ ] **Step 1: Git 提交**

```bash
git add -A
git commit -m "feat(system): 增加 AOP 队列监控接口

- 扩展 EventQueue trait 增加 stats/query_events/get_event 方法
- Registry 提供聚合监控入口
- SystemDomain 新增 AopMonitor 子模块
- Handler 提供 4 个 API：
  - GET /api/v1/system/aop/stats
  - GET /api/v1/system/aop/:consumer/stats
  - GET /api/v1/system/aop/:consumer/events
  - GET /api/v1/system/aop/:consumer/events/:event_id
"
```

---

## 总结

本次实现：
- ✅ 队列状态统计（pending/in_progress/order_key 分布）
- ✅ 事件列表查询（支持 order_key/status 过滤 + 分页）
- ✅ 单个事件详情（脱敏预览）

后续可扩展：
- 管理操作（重试特定事件、清空队列）
- 更丰富的过滤（时间范围、事件类型）
- Prometheus metrics 集成