use std::sync::Arc;

use common::enums::EventTopic;
use common::error::{Result, err};

use super::{Event, Registry};
use crate::pkg::RequestContext;

/// 发布句柄：由 AOP 构造并**预先绑定**生产者的 topic
///
/// 取代改造前「把 `Arc<Registry>` 反向注入生产者」的 `Producer::register()`：
///
/// - `registry` 字段**私有** → 生产者拿不到 `register_consumer` / `start_all` / `shutdown_all`；
/// - `topic` 预绑定 → [`EventSink::emit`] 校验 `event.kind() == self.topic`，
///   生产者**想发错也发不出去**（这是「1 producer : 1 topic」的执行点）。
///
/// 相比改造前，它是**净减代码**：替换掉 `register()` 及生产者里那个只为取出
/// `Arc<Registry>` 而存在的 `RwLock<Option<Arc<Registry>>>` 字段。
#[derive(Clone)]
pub struct EventSink {
    registry: Arc<Registry>,
    topic: EventTopic,
}

impl EventSink {
    /// 由 AOP 在 `start_all` 时构造（业务侧不直接调用）
    pub(crate) fn new(registry: Arc<Registry>, topic: EventTopic) -> Self {
        Self { registry, topic }
    }

    /// 本句柄绑定的 topic
    pub fn topic(&self) -> EventTopic {
        self.topic
    }

    /// 发布事件（fire-and-forget）
    ///
    /// 与 [`Registry::publish`] 一致：无订阅者即静默丢弃（①类纯通知的语义）。
    /// 唯一的失败情形是 `event.kind()` 与自身 topic 不一致。
    pub async fn emit<E: Event>(&self, ctx: &RequestContext, event: E) -> Result<()> {
        let kind = event.kind();
        if kind != self.topic {
            return Err(err!(
                InvalidRequest,
                "producer of {} cannot emit {} event (1 producer : 1 topic)",
                self.topic,
                kind
            ));
        }

        self.registry.publish(ctx, event).await;
        Ok(())
    }
}
