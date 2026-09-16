use crate::models::events::CronTriggerEvent;
use crate::pkg::RequestContext;
use crate::pkg::aop::{EventSink, Producer, ProducerLoop};
use crate::service::domain::system;
use common::enums::EventTopic;
use common::error::Result;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// 轮询间隔（秒）
const POLL_INTERVAL_SECS: u64 = 60;

/// cron 触发器生产者
///
/// 自管 60s 轮询：列出到期的触发器 → 逐个 `emit` 一个 [`CronTriggerEvent`]。
/// 生命周期由 AOP 统一管 —— `start()` spawn 后立即返回（契约 1），
/// `stop()` 置位并等 loop 退出（契约 2），loop 的停机自检交给 [`ProducerLoop`]。
///
/// ⚠️ `mark_trigger_executed` **目前仍写在 tick 里**（P3 未修）：它是「先 publish
/// 再 mark」的顺序，业务被吞掉的失败也算「已执行」→ 一次跳过丢一个周期（日触发 = 丢一天）。
/// 修法是把 mark 移进 `on_consumed`（那时才需要声明 `notify_producer`）——
/// 本步（契约改形）保持行为等价，故先不动，也**不能**提前声明 `notify_producer`，
/// 否则会与 tick 里的 mark 双写。
pub struct CronTriggerProducer {
    /// 自持的循环控制器：`stop()` 后 sleep 在下一片（≤250ms）返回 false
    loop_ctl: Arc<ProducerLoop>,
    /// 自管 loop 的句柄：`stop()` 靠它「等 loop 真正退出」
    handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl Default for CronTriggerProducer {
    fn default() -> Self {
        Self::new()
    }
}

impl CronTriggerProducer {
    pub fn new() -> Self {
        Self {
            loop_ctl: Arc::new(ProducerLoop::new()),
            handle: Mutex::new(None),
        }
    }

    /// 一次轮询：列出到期触发器并逐个发布（原 `poll()` 的实现，一行未改）
    async fn tick(sink: &EventSink) -> Result<()> {
        let ctx = RequestContext::new_system();
        let now = common::constants::utils::current_timestamp_ms();

        let triggers = system::domain()
            .cron_manager()
            .list_due_triggers(ctx.clone(), now, 100)
            .await?;

        if triggers.is_empty() {
            return Ok(());
        }

        log_debug!("cron producer found {} due triggers", triggers.len());

        for trigger in &triggers {
            let event = CronTriggerEvent {
                event_id: format!("{}-{}", trigger.id, now),
                trigger_id: trigger.id.clone(),
                trigger_name: trigger.name.clone(),
                payload: trigger.payload.clone(),
                created_at: common::constants::utils::current_timestamp_ms(),
            };

            // 发布句柄只能发本 producer 的 topic（`cron.trigger`）
            sink.emit(&ctx, event).await?;

            system::domain()
                .cron_manager()
                .mark_trigger_executed(ctx.clone(), &trigger.id, now)
                .await?;
        }

        log_info!("cron producer published {} trigger events", triggers.len());

        Ok(())
    }
}

#[async_trait::async_trait]
impl Producer for CronTriggerProducer {
    fn name(&self) -> &str {
        "cron_trigger"
    }

    fn topic(&self) -> EventTopic {
        EventTopic::CronTrigger
    }

    /// 契约 1：spawn 自己的 loop 后**立即返回**
    ///
    /// 绝不在这里 await loop —— 那会把 `start_all` 连同其后所有生产者的启动一起卡死
    /// （不报错，只是永远起不来）。
    async fn start(&self, sink: EventSink) -> Result<()> {
        let loop_ctl = Arc::clone(&self.loop_ctl);

        let handle = tokio::spawn(async move {
            sys_info!("[cron_trigger] producer loop started");

            loop {
                if let Err(e) = CronTriggerProducer::tick(&sink).await {
                    sys_error!("[cron_trigger] tick error: {}", e);
                }

                // 可中断休眠：stop() 后下一片（≤250ms）返回 false → 退出 loop
                if !loop_ctl
                    .sleep(Duration::from_secs(POLL_INTERVAL_SECS))
                    .await
                {
                    break;
                }
            }

            sys_info!("[cron_trigger] producer loop exited");
        });

        *self.handle.lock().expect("cron producer handle poisoned") = Some(handle);
        Ok(())
    }

    /// 契约 2：置位 **并等 loop 退出**（不能只置位）
    async fn stop(&self) -> Result<()> {
        self.loop_ctl.stop();

        // guard 在 await 之前 drop（先 take 出来）—— 不跨 await 持有 std 锁
        let handle = self
            .handle
            .lock()
            .expect("cron producer handle poisoned")
            .take();

        if let Some(handle) = handle {
            let _ = handle.await;
        }

        Ok(())
    }
}
