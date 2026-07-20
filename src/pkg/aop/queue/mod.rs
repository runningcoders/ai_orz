mod in_memory;

use async_trait::async_trait;
use common::error::Result;
use crate::pkg::RequestContext;

pub use in_memory::InMemoryEventQueue;

#[async_trait]
pub trait EventQueue: Send + Sync + std::fmt::Debug + 'static {
    async fn enqueue(&self, ctx: RequestContext, event: serde_json::Value) -> Result<()>;
    async fn enqueue_batch(&self, ctx: RequestContext, events: Vec<serde_json::Value>) -> Result<()>;
    async fn dequeue_next(&self, ctx: RequestContext) -> Result<Option<serde_json::Value>>;
    async fn ack(&self, ctx: RequestContext, event_id: &str) -> Result<()>;
    async fn nack(&self, ctx: RequestContext, event_id: &str) -> Result<()>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn in_progress_count(&self) -> usize;
    fn recover(&self, ctx: RequestContext) -> Result<usize>;
    fn clear(&self);
}