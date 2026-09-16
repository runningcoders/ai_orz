mod in_memory;

use crate::pkg::RequestContext;
use async_trait::async_trait;
use common::error::Result;
use serde::Serialize;

pub use in_memory::InMemoryEventQueue;

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
#[derive(Debug, Clone)]
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

#[async_trait]
pub trait EventQueue: Send + Sync + std::fmt::Debug + 'static {
    async fn enqueue(&self, ctx: RequestContext, event: serde_json::Value) -> Result<()>;
    /// 不带门闩入队：即使事件带非空 `order_key` 也直接进堆（按 `(priority, created_at)` 排序）。
    ///
    /// 供**未声明 `ordered`** 的订阅走 —— 门闩（同 key FIFO）是订阅者的 opt-in，
    /// 由 publish 侧按 `Subscription.ordered` 选路（设计稿 §4.1）。
    async fn enqueue_ungated(&self, ctx: RequestContext, event: serde_json::Value) -> Result<()> {
        self.enqueue(ctx, event).await
    }
    async fn enqueue_batch(
        &self,
        ctx: RequestContext,
        events: Vec<serde_json::Value>,
    ) -> Result<()>;
    async fn dequeue_next(&self, ctx: RequestContext) -> Result<Option<serde_json::Value>>;
    async fn ack(&self, ctx: RequestContext, event_id: &str) -> Result<()>;
    /// 事件处理未成功：退回队列等待重投（per-event 指数退避，见 `retry_backoff_ms`）
    async fn nack(&self, ctx: RequestContext, event_id: &str) -> Result<()>;

    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn in_progress_count(&self) -> usize;
    fn recover(&self, ctx: RequestContext) -> Result<usize>;
    fn clear(&self);

    // ===== 监控方法 =====

    /// 获取队列状态统计
    fn stats(&self) -> QueueStats;

    /// 按条件查询事件列表
    fn query_events(&self, filter: EventQueryFilter) -> Vec<EventSummary>;

    /// 查询单个事件详情
    fn get_event(&self, event_id: &str) -> Option<EventDetail>;
}
