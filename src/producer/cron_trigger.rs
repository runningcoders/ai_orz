use crate::models::events::{CronTriggerEvent, ScheduleChangeOp, ScheduleChangedEvent};
use crate::pkg::RequestContext;
use crate::pkg::aop::{Consumer, EventSink, Producer, ProducerLoop, Subscription};
use crate::producer::cron_dispatch_queue::CronDispatchQueue;
use crate::service::dao::cron_trigger::CronTriggerQuery;
use crate::service::domain::system;
use common::enums::EventTopic;
use common::error::{Error, Result};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// DB 兜底刷新间隔（秒）：扫表把未来窗口内的日程补进派发队列。
const REFRESH_INTERVAL_SECS: u64 = 60;
/// 派发出队节拍（秒）：只看队列头，队列为空时零 DB 查询。
const DISPATCH_INTERVAL_SECS: u64 = 1;
/// 提前入队窗口（毫秒）：`next_run_at <= now + 5min`。
const DISPATCH_WINDOW_MS: i64 = 5 * 60 * 1000;
/// 单轮兜底最多补入队的条数（防止一次性把大表全拉进内存）。
const REFRESH_LIMIT: usize = 500;

/// 租约时长（毫秒）：5 分钟 > 60s 兜底间隔 + 单轮最坏消费时间。
/// 领取后进程崩溃：最多等 5 分钟由下个 tick 重新领取，不会永久丢失。
const LEASE_MS: i64 = 5 * 60 * 1000;

/// cron 触发器生产者
///
/// 同时是 `schedule.changed` 的消费者 —— 同一实例持有派发队列：
/// - domain 的日程 CRUD 发原始事件（含 DB 版本）→ `on_event` 带版本写队列；
/// - 60s tick 扫 DB 兜底刷新（排除已在队列中的 id）；
/// - 1s tick 逐个取出到期项 → 按 id 回查 DB 校验时间 / 版本 → 按 id 原子领取 → emit。
///
/// 版本两道防线：入队时基建层 `upsert_if_newer` 拒绝乱序旧事件（堆仍只按 rank
/// 排序，比较不参与排序）；出队时版本随载荷带出，业务层按 id 回查 DB 再校验，
/// 防事件丢失 / DB 被外部改动。失效快照跳过，变更事件带新版本重新入队。
///
/// 生命周期由 AOP 统一管 —— `start()` spawn 后立即返回（契约 1），
/// `stop()` 置位并等 loop 退出（契约 2），loop 的停机自检交给 [`ProducerLoop`]。
///
/// **P3**：`mark_trigger_executed` 只在 [`Producer::on_consumed`] 里、即「业务真正
/// 成功」之后调用。消费者是 `ConsumeMode::Sync`，业务失败不会让 `publish`/`emit`
/// 返回 Err —— 在 tick 里紧跟 emit 标记等于「失败也算已执行」（日触发 = 丢一天）。
/// 现在：成功 → `on_consumed` 标记并带新版本重新入队；失败 → 不标记，但条目已出队、
/// 租约由 claim 持有 5 分钟，实际靠 60s DB 兜底刷新 / 租约过期后重捞重试，
/// 而不是「下个 tick 立即重来」。
pub struct CronTriggerProducer {
    /// 自持的循环控制器：`stop()` 后 sleep 在下一片（≤250ms）返回 false
    loop_ctl: Arc<ProducerLoop>,
    /// 自管 loop 的句柄：`stop()` 靠它「等 loop 真正退出」
    handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// 本实例租约标识：进程内稳定，多实例之间唯一
    instance_id: String,
    /// 派发队列：本类型的私有字段，消费原始事件与秒级出队共享同一份状态
    queue: Arc<CronDispatchQueue>,
    /// 已投递但尚未 mark 的 trigger_id → 入队时刻。DB 兜底刷新时与队列 id
    /// 一并排除，避免对「处理中」的日程重复入队；on_consumed（业务成功）后移除。
    /// 值带时间戳：投递失败 / 进程异常导致没有 mark 时，超过租约期的项会在
    /// 下一轮 refresh 里自动过期清理，不依赖 on_failed 回调。
    in_flight: Arc<Mutex<HashMap<String, Instant>>>,
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
            instance_id: format!("cron-producer-{}", common::new_id!(CronTrigger)),
            queue: Arc::new(CronDispatchQueue::new()),
            in_flight: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// DB 兜底刷新：把窗口内、且尚未在队列中的触发器补进来。
    ///
    /// `exclude_ids` 是「不重复入队」的执行点：已在队列的 id 由 SQL 直接排除，
    /// 避免反复覆盖队列里的时间点。`next_run_before` 含已到期 + 未来 5 分钟，
    /// 这样秒级 tick 才有提前量。
    async fn refresh_dispatch_queue(
        queue: &CronDispatchQueue,
        in_flight: &Arc<Mutex<HashMap<String, Instant>>>,
    ) -> Result<()> {
        let ctx = RequestContext::new_system();
        let now = common::constants::utils::current_timestamp_ms();
        // 已在队列的 + 已投递尚未 mark 的，一并排除，避免重复入队 / 重复投递。
        // 超租约期仍未 mark 的（投递失败 / 进程异常）视为处理中状态过期，
        // 清理掉让它重新参与兜底扫描 —— 租约本身也已在 claim_due 处过期。
        let mut exclude_ids = queue.ids();
        {
            let mut in_flight = in_flight.lock().unwrap_or_else(|e| e.into_inner());
            in_flight.retain(|_, at| at.elapsed() < Duration::from_millis(LEASE_MS as u64));
            exclude_ids.extend(in_flight.keys().cloned());
        }
        exclude_ids.sort();
        exclude_ids.dedup();
        let query = CronTriggerQuery {
            is_enabled: Some(true),
            next_run_before: Some(now + DISPATCH_WINDOW_MS),
            exclude_ids,
            order_by_due: true,
            limit: Some(REFRESH_LIMIT),
            ..Default::default()
        };

        let domain = system::domain();
        let triggers = domain.cron_manager().list_triggers(ctx, query).await?;
        // 版本带 DB 的 updated_at：消费时按 id 回查比对，识别队列快照是否已过期。
        let changed = queue.refresh(
            triggers
                .into_iter()
                .map(|t| (t.id, t.next_run_at, t.updated_at)),
        );
        log_debug!(
            "[cron_trigger] refreshed {} dispatch queue entries",
            changed
        );
        Ok(())
    }

    /// 秒级派发：逐个从队列取出到期项，**先按 id 回查 DB 校验快照是否仍有效**，
    /// 校验通过才原子领取 + 投递。堆顶未到期时 `pop_one_due` 返回 `None`，本轮零
    /// DB 查询；到期项全部处理完才结束，没有批量上限。
    ///
    /// 校验口径（第二道防线，业务层做；入队门禁已先挡过一轮乱序）：
    /// - DB 里 `next_run_at` 与队列快照的时间对不上 → 数据已变更，跳过；
    /// - DB 里 `updated_at` 与快照版本对不上 → 配置被改过，跳过。
    ///
    /// 两种情况的变更事件都会带新版本重新入队，下轮自然按新状态执行。
    ///
    /// ⚠️ **不要在这里加回 `mark_trigger_executed`** —— 见 [`CronTriggerProducer`] 的
    /// P3 说明：Sync 消费者的业务失败不会让 `emit` 返回 Err，在这里标记等于
    /// 「失败也算已执行」。
    async fn dispatch_due(
        sink: &EventSink,
        queue: &CronDispatchQueue,
        in_flight: &Arc<Mutex<HashMap<String, Instant>>>,
        instance_id: &str,
    ) -> Result<()> {
        let ctx = RequestContext::new_system();
        let now = common::constants::utils::current_timestamp_ms();
        let domain = system::domain();
        let manager = domain.cron_manager();
        let mut emitted = 0usize;

        // 逐个出队：每取一个堆自动重排，取到 None 为止 = 处理完当前时间点前的全部。
        while let Some((trigger_id, snapshot_at, snapshot_version)) = queue.pop_one_due(now) {
            // 业务校验：按 id 读权威数据，时间 / 版本任一不符即跳过（数据已变更）。
            // 读失败同样只跳过本条，留给 60s 兜底重捞（理由见下方 claim 分支）。
            let stored = match manager.get_trigger(ctx.clone(), &trigger_id).await {
                Ok(Some(stored)) => stored,
                Ok(None) => continue, // 已删除：丢弃快照，无需补偿
                Err(e) => {
                    log_error!(
                        "[cron_trigger] read failed for {}: {}; will retry via DB refresh",
                        trigger_id,
                        e
                    );
                    continue;
                }
            };
            if !snapshot_matches(&stored, snapshot_at, snapshot_version) {
                log_debug!(
                    "[cron_trigger] skip stale snapshot: id={} queued_at={} db_at={} queued_ver={} db_ver={}",
                    trigger_id,
                    snapshot_at,
                    stored.next_run_at,
                    snapshot_version,
                    stored.updated_at
                );
                continue;
            }

            // 按 id 原子领取：校验与领取针对同一条，不会领到别的条目白占租约。
            // 多实例部署时，同一条到期记录在租约期内只会被一个实例领取。
            //
            // 领取失败只影响本条，不拖垮整轮派发：
            // - None：租约被其他实例持有 / 窗口已失效；
            // - Err（如 SQLITE_BUSY 写锁竞争）：本条已出队，留给 60s 兜底重捞。
            // ⚠️ 不能用 `?` 上抛 —— 那会让同轮剩余已出队项一起被跳过。
            let claimed = manager
                .claim_trigger_by_id(ctx.clone(), &trigger_id, now, instance_id, now + LEASE_MS)
                .await;
            let trigger = match claimed {
                Ok(Some(trigger)) => trigger,
                Ok(None) => continue,
                Err(e) => {
                    log_error!(
                        "[cron_trigger] claim failed for {} (id: {}): {}; will retry via DB refresh",
                        stored.name,
                        trigger_id,
                        e
                    );
                    continue;
                }
            };

            // 投递即视为「处理中」：DB 兜底刷新会排除这个 id，直到 mark 后移除。
            in_flight
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(trigger.id.clone(), Instant::now());

            let event = build_event(&trigger, now, instance_id);

            // 每次投递 spawn 独立 task：consumer 保持 Sync —— 同步维护动作的结果
            // 决定下一步更新（mark 与否），必须等；异步日程投递到队列即完成。
            // 两种语义都在 consumer 内部按 behavior 分流，producer 不感知。
            // 「等待」留在 spawn 的 task 里，1s tick 不被业务耗时拖住；mark 仍在
            // `on_consumed`，业务成功才推进 next_run_at。
            let emit_ctx = ctx.clone();
            let emit_sink = sink.clone();
            let tid = trigger.id.clone();
            let tname = trigger.name.clone();
            tokio::spawn(async move {
                if let Err(e) = emit_sink.emit(&emit_ctx, event).await {
                    log_error!(
                        "[cron_trigger] emit failed for {} (id: {}): {}; lease will expire",
                        tname,
                        tid,
                        e
                    );
                }
            });
            emitted += 1;
        }

        if emitted > 0 {
            log_info!("cron producer published {} trigger events", emitted);
        }

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
    /// 两个时间各司其职：
    /// - `fired_at` 取事件自身的 `created_at`（规则触发时刻 / 领取时刻），时刻型按它推进；
    /// - `finished_at` 取回调时刻（消费成功 = 业务真正跑完），间隔型按它推进 ——
    ///   `next_run_at = finished_at + interval`，执行耗时不会吃掉应得的空档。
    ///
    /// 标记后自行回读最新时间点：仍启用且落在提前窗口内 → 重新入队，不等 60s 兜底。
    ///
    /// 返回 `Err` 只会被框架记日志（回调失败**不改投递结论**）：标记失败 → 触发器保持 due
    /// → 下个 tick 重来，比"悄悄吞掉这次失败"安全。
    async fn on_consumed(&self, ctx: &RequestContext, event: &serde_json::Value) -> Result<()> {
        let (trigger_id, fired_at) = fired_at_of(event)?;
        let finished_at = common::constants::utils::current_timestamp_ms();

        let domain = system::domain();
        let manager = domain.cron_manager();
        manager
            .mark_trigger_executed(ctx.clone(), &trigger_id, fired_at, finished_at)
            .await?;

        // 业务成功：清除处理中标记，DB 兜底可按新的 next_run_at 重新入队。
        self.in_flight
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&trigger_id);

        if let Some(trigger) = manager.get_trigger(ctx.clone(), &trigger_id).await?
            && trigger.is_enabled == 1
            && trigger.next_run_at <= finished_at + DISPATCH_WINDOW_MS
        {
            // 带 mark 后的 DB 版本重新入队：消费时用最新版本校验通过才执行。
            self.queue
                .refresh([(trigger.id.clone(), trigger.next_run_at, trigger.updated_at)]);
        }

        log_debug!(
            "[cron_trigger] marked executed: trigger_id={} fired_at={} finished_at={}",
            trigger_id,
            fired_at,
            finished_at
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
        let instance_id = self.instance_id.clone();
        let queue = Arc::clone(&self.queue);
        let in_flight = Arc::clone(&self.in_flight);

        let handle = tokio::spawn(async move {
            sys_info!("[cron_trigger] producer loop started");

            // 首轮立即刷新，不等 60s；之后按固定间隔兜底。
            let mut last_refresh: Option<Instant> = None;
            loop {
                if last_refresh
                    .is_none_or(|at| at.elapsed() >= Duration::from_secs(REFRESH_INTERVAL_SECS))
                {
                    if let Err(e) =
                        CronTriggerProducer::refresh_dispatch_queue(&queue, &in_flight).await
                    {
                        sys_error!("[cron_trigger] refresh error: {}", e);
                    }
                    last_refresh = Some(Instant::now());
                }

                if let Err(e) =
                    CronTriggerProducer::dispatch_due(&sink, &queue, &in_flight, &instance_id).await
                {
                    sys_error!("[cron_trigger] dispatch error: {}", e);
                }

                // 可中断休眠：stop() 后下一片（≤250ms）返回 false → 退出 loop
                if !loop_ctl
                    .sleep(Duration::from_secs(DISPATCH_INTERVAL_SECS))
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

/// `schedule.changed` 的消费者：把 domain 的日程原始事件写进自己的派发队列。
///
/// Sync 模式、无队列（框架侧） —— 事件在 domain 的 publish 调用栈内直接落队，
/// 不产生 AOP 封套队列，也不与 `cron.trigger` 的消费者混淆。
#[async_trait::async_trait]
impl Consumer for CronTriggerProducer {
    fn name(&self) -> &str {
        "cron_dispatch"
    }

    fn subscriptions(&self) -> Vec<Subscription> {
        vec![Subscription::new(EventTopic::ScheduleChanged)]
    }

    async fn on_event(&self, _ctx: RequestContext, event: serde_json::Value) -> Result<()> {
        let change: ScheduleChangedEvent = serde_json::from_value(event).map_err(|e| {
            Error::internal(format!(
                "failed to deserialize schedule changed event: {}",
                e
            ))
        })?;
        enqueue_change(&self.queue, &change);
        Ok(())
    }
}

/// 把一条日程变更事件映射到派发队列操作（纯函数，便于单测钉住版本口径）。
///
/// Upsert 带变更后的 DB 版本入队（基建层门禁会拒绝旧版本）；删除 / 停用 / 缺省字段作废旧条目。
fn enqueue_change(queue: &CronDispatchQueue, change: &ScheduleChangedEvent) {
    match (change.op, change.next_run_at, change.is_enabled) {
        (ScheduleChangeOp::Upsert, Some(next_run_at), Some(true)) => {
            // 带变更后的 DB 版本：旧版本在基建层被门禁拒绝；消费时按 id 回查再校验一次。
            queue.refresh([(change.trigger_id.clone(), next_run_at, change.version)]);
        }
        // 删除 / 停用 / 缺省字段一律作废旧条目；DB 兜底会按最新状态重建
        _ => {
            queue.remove(&change.trigger_id);
        }
    }
}

/// 消费侧快照校验（第二道防线）：队列快照的时间 / 版本必须与 DB 权威行一致，
/// 否则视为「数据已变更」，跳过本次执行；变更事件会带新版本重新入队。
fn snapshot_matches(
    stored: &crate::models::cron_trigger::CronTriggerPo,
    at: i64,
    version: i64,
) -> bool {
    stored.next_run_at == at && stored.updated_at == version
}

/// 把一条已领取的触发器记录转成事件载荷（纯函数，便于单测钉住字段口径）。
///
/// `created_at` 用领取时刻 `now`：`on_consumed` 拿它当 `fired_at`，供时刻型按规则点
/// 推进；间隔型另取回调时刻当 `finished_at`，不受这里影响。
fn build_event(
    trigger: &crate::models::cron_trigger::CronTriggerPo,
    now: i64,
    instance_id: &str,
) -> CronTriggerEvent {
    CronTriggerEvent {
        event_id: format!("{}-{}", trigger.id, now),
        trigger_id: trigger.id.clone(),
        trigger_name: trigger.name.clone(),
        payload: trigger.payload.clone(),
        created_at: now,
        trigger_type: trigger.trigger_type.to_i32(),
        behavior: trigger.behavior.to_i32(),
        action: trigger.resolved_action(),
        description: trigger.description.clone(),
        org_id: trigger.org_id.clone(),
        owner_user_id: trigger.owner_user_id.clone(),
        creator_type: trigger.creator_type,
        creator_id: trigger.creator_id.clone(),
        agent_id: trigger.agent_id.clone(),
        lease_owner: Some(instance_id.to_string()),
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
    use crate::pkg::aop::Event;

    /// P3 的落点：`on_consumed` 必须能从封套里拿到 `trigger_id` 与**触发时刻**
    ///
    /// `fired_at` 用 `created_at`（触发时刻），与回调时刻 `finished_at` 严格分工。
    /// 这里把该约定钉在测试里。
    #[test]
    fn test_fired_at_of_extracts_trigger_id_and_fired_at() {
        let fired_at = 1_772_000_000_000_i64;
        let event = serde_json::json!({
            "event_id": format!("trigger-1-{}", fired_at),
            "trigger_id": "trigger-1",
            "trigger_name": "每日沉淀",
            "payload": r#"{"action":"agent_rest"}"#,
            "created_at": fired_at,
        });

        let (trigger_id, event_fired_at) =
            fired_at_of(&event).expect("合法封套应可提取 trigger_id / fired_at");
        assert_eq!(trigger_id, "trigger-1");
        assert_eq!(event_fired_at, fired_at);
    }

    /// 扩展字段必须完整进入事件：消费侧不再回查表
    #[test]
    fn test_build_event_carries_all_groups() {
        let mut trigger = crate::models::cron_trigger::CronTriggerPo::new(
            "t-1".to_string(),
            "daily".to_string(),
            common::enums::TriggerType::Schedule,
            1000,
            None,
        );
        trigger.action = Some("agent_schedule".to_string());
        trigger.owner_user_id = Some("user-1".to_string());
        trigger.agent_id = Some("agent-1".to_string());
        trigger.description = "每天汇总".to_string();
        trigger.behavior = common::enums::TriggerBehavior::AsyncAgentWakeup;

        let event = build_event(&trigger, 2000, "instance-a");
        assert_eq!(event.created_at, 2000);
        assert_eq!(event.behavior, 2);
        assert_eq!(event.trigger_type, 3);
        assert_eq!(event.action.as_deref(), Some("agent_schedule"));
        assert_eq!(event.owner_user_id.as_deref(), Some("user-1"));
        assert_eq!(event.agent_id.as_deref(), Some("agent-1"));
        assert_eq!(event.description, "每天汇总");
        assert_eq!(event.lease_owner.as_deref(), Some("instance-a"));
        assert_eq!(event.order_key(), "t-1");
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

        let (trigger_id, event_fired_at) = fired_at_of(&event).expect("多出字段应可解析");
        assert_eq!(trigger_id, "trigger-9");
        assert_eq!(event_fired_at, 100);
    }

    /// 变更事件入队必须携带 DB 版本；停用 / 删除事件作废旧条目。
    #[test]
    fn test_enqueue_change_carries_version_and_removes_on_disable() {
        let queue = CronDispatchQueue::new();
        let upsert = ScheduleChangedEvent::upsert("t-1", 1000, true, 88, 900);
        enqueue_change(&queue, &upsert);
        assert_eq!(queue.pop_one_due(1000), Some(("t-1".into(), 1000, 88)));

        // 停用事件：清掉队列里的旧快照
        enqueue_change(
            &queue,
            &ScheduleChangedEvent::upsert("t-2", 2000, false, 99, 901),
        );
        assert!(queue.is_empty());
    }

    /// 乱序事件：旧版本不得把已入队的新快照倒退回去（基建层门禁）。
    #[test]
    fn test_enqueue_change_rejects_out_of_order_event() {
        let queue = CronDispatchQueue::new();
        enqueue_change(
            &queue,
            &ScheduleChangedEvent::upsert("t-1", 2000, true, 90, 900),
        );
        enqueue_change(
            &queue,
            &ScheduleChangedEvent::upsert("t-1", 1000, true, 80, 899),
        );
        assert_eq!(queue.next_run_at(), Some(2000));
        assert_eq!(queue.pop_one_due(2000), Some(("t-1".into(), 2000, 90)));
    }

    /// 快照校验：时间或版本任一不符都跳过，避免执行已变更的日程。
    #[test]
    fn test_snapshot_matches_requires_both_time_and_version() {
        let mut trigger = crate::models::cron_trigger::CronTriggerPo::new(
            "t-1".into(),
            "daily".into(),
            common::enums::TriggerType::Cron,
            1000,
            None,
        );
        trigger.updated_at = 88;

        assert!(snapshot_matches(&trigger, 1000, 88));
        // 时间对不上（数据改了执行时刻）
        assert!(!snapshot_matches(&trigger, 2000, 88));
        // 版本对不上（配置被改过，时刻恰好没变）
        assert!(!snapshot_matches(&trigger, 1000, 87));
    }

    /// producer / consumer 双身份的订阅声明必须是 Sync + 非 ordered
    #[test]
    fn test_subscription_is_sync_and_unordered() {
        let producer = CronTriggerProducer::new();
        let subs = Consumer::subscriptions(&producer);
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].kind, EventTopic::ScheduleChanged);
        assert!(!subs[0].ordered);
        assert_eq!(Consumer::name(&producer), "cron_dispatch");
        assert_eq!(Producer::name(&producer), "cron_trigger");
    }
}
