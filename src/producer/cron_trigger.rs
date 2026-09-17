use crate::models::events::CronTriggerEvent;
use crate::pkg::RequestContext;
use crate::pkg::aop::{EventSink, Producer, ProducerLoop};
use crate::service::domain::system;
use common::enums::EventTopic;
use common::error::{Error, Result};
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
/// **P3 已修（Step 3）**：`mark_trigger_executed` 在 [`Producer::on_consumed`] 里，
/// 即「业务真正成功」之后才标记。改动前它在 [`Self::tick`] 里紧跟 `emit` ——
/// 消费者是 `ConsumeMode::Sync`，其 `on_event` 的失败在 `finish_consumption` 里被判成
/// `Nack` 后**被丢弃**（Sync 无队列、无重投驱动者），`publish`/`emit` 因此恒返回成功 →
/// 「业务失败」与「业务成功」对 tick 不可区分 → 失败的触发器也被标记已执行 →
/// 要等下一个 cron 点才重来（日触发 = 丢一天）。
///
/// 现在：`on_event` 成功 → `on_consumed` 标记；失败 → [`Producer::on_failed`] 返回
/// `Retry`（不标记）→ 触发器保持 due，下个 tick 自然重试。
///
/// ⚠️ 代价（已知并被接受）：Cron 消费者是 Sync，`tick` 会等它做完才返回，所以
/// 「持续失败」的动作从此**每 60s 重试一次**，而不是等下一个 cron 点。这是设计取舍：
/// 宁可重试也不要静默跳过一整个周期（日触发 = 丢一天）。
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

    /// 一次轮询：列出到期触发器并逐个发布（原 `poll()` 的实现，只去掉末尾的 mark）
    ///
    /// ⚠️ **不要在这里加回 `mark_trigger_executed`** —— 见 [`CronTriggerProducer`] 的
    /// P3 说明：Sync 消费者的业务失败不会让 `emit` 返回 Err，在这里标记等于
    /// 「失败也算已执行」。
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

    /// 业务收尾（P3 的落点）：**只有消费者报成功**才标记触发器已执行
    ///
    /// `executed_at` 取事件自身的 `created_at`（≈ 本次触发时刻，即 tick 里的 `now`），
    /// **不是**回调发生的时刻 —— 后者会把 `Interval` 触发器的 `next_run_at` 往后拖，
    /// 且偏移随消费耗时增长（`mark_executed` 里 `next_run_at = executed_at + interval * 1000`）。
    ///
    /// 返回 `Err` 只会被框架记日志（回调失败**不改投递结论**）：标记失败 → 触发器保持 due
    /// → 下个 tick 重来，比"悄悄吞掉这次失败"安全。
    async fn on_consumed(&self, ctx: &RequestContext, event: &serde_json::Value) -> Result<()> {
        let (trigger_id, executed_at) = fired_at_of(event)?;

        system::domain()
            .cron_manager()
            .mark_trigger_executed(ctx.clone(), &trigger_id, executed_at)
            .await?;

        log_debug!(
            "[cron_trigger] marked executed: trigger_id={} executed_at={}",
            trigger_id,
            executed_at
        );

        Ok(())
    }

    // ⚠️ **刻意不实现 `on_failed`**：本 topic 没有「失败后要改的底层数据」——
    // 「失败 = 不 mark → 下个 tick 重来」完全由 `SchedulerConsumer::decide_retry`
    // 恒返回 `Retry` 表达（它决定了框架**不会**走到 `Discard` → 不会回调 `on_consumed`
    // → 不会把失败的那次标记成"已执行"）。这条语义保证在**消费者**那边，别在这边补刀。

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

/// 从事件封套里取「本次触发」的两个幂等要素：`trigger_id` + 触发时刻
///
/// 单独抽出来是为了能被纯单测覆盖 —— `on_consumed` 本体要走 `system::domain()`
/// 全局单例，纯单测拉不起来。
fn fired_at_of(event: &serde_json::Value) -> Result<(String, i64)> {
    let event: CronTriggerEvent = serde_json::from_value(event.clone())
        .map_err(|e| Error::internal(format!("failed to deserialize cron trigger event: {}", e)))?;
    Ok((event.trigger_id, event.created_at))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P3 的落点：`on_consumed` 必须能从封套里拿到 `trigger_id` 与**触发时刻**
    ///
    /// `executed_at` 用 `created_at`（触发时刻）而非回调时刻：后者会把 Interval
    /// 触发器的 next_run_at 往后拖。这里把该约定钉在测试里。
    #[test]
    fn test_fired_at_of_extracts_trigger_id_and_executed_at() {
        let fired_at = 1_772_000_000_000_i64;
        let event = serde_json::json!({
            "event_id": format!("trigger-1-{}", fired_at),
            "trigger_id": "trigger-1",
            "trigger_name": "每日沉淀",
            "payload": r#"{"action":"agent_rest"}"#,
            "created_at": fired_at,
        });

        let (trigger_id, executed_at) =
            fired_at_of(&event).expect("合法封套应可提取 trigger_id / executed_at");
        assert_eq!(trigger_id, "trigger-1");
        assert_eq!(executed_at, fired_at);
    }

    /// 封套缺字段 → 返回 Err（由框架记日志），**不能**静默当成功
    #[test]
    fn test_fired_at_of_rejects_malformed_envelope() {
        // AOP 会往封套里注入 `attempt` 等字段，所以正常封套是"超集可解析"；
        // 但缺 trigger_id / created_at 的残封套必须报错，否则会去 mark 一个不存在的触发器。
        for bad in [
            serde_json::json!({"event_id": "x", "created_at": 1_i64}),
            serde_json::json!({"event_id": "x", "trigger_id": "t"}),
            serde_json::json!("not-an-object"),
        ] {
            assert!(
                fired_at_of(&bad).is_err(),
                "残缺封套应报错，实际通过: {}",
                bad
            );
        }
    }

    /// 封套里多出的字段（AOP 注入的 `attempt` / `order_key` / carrier）不得影响解析
    #[test]
    fn test_fired_at_of_tolerates_extra_envelope_fields() {
        let event = serde_json::json!({
            "event_id": "trigger-9-100",
            "trigger_id": "trigger-9",
            "trigger_name": "t",
            "payload": "{}",
            "created_at": 100_i64,
            "attempt": 3,
            "order_key": "trigger-9",
            "carrier": {"log_id": "abc"},
        });

        let (trigger_id, executed_at) = fired_at_of(&event).expect("多出字段应可解析");
        assert_eq!(trigger_id, "trigger-9");
        assert_eq!(executed_at, 100);
    }
}
