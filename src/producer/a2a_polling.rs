use crate::models::events::A2aPollRequestedEvent;
use crate::pkg::RequestContext;
use crate::pkg::aop::{EventSink, Producer, ProducerLoop};
use crate::service::domain::hr as hr_domain;
use common::enums::EventTopic;
use common::error::Result;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// 轮询间隔（秒）
const POLL_INTERVAL_SECS: u64 = 30;

/// A2A 远端任务轮询生产者
///
/// 每 30s 做一次「认领」：列出全部远端 Agent，逐个 emit 一条
/// [`A2aPollRequestedEvent`]；真正的远端拉取 / 新消息投递 / 本地状态推进
/// 在 `a2a_poll` 消费者（Async）里执行。
///
/// 这是 Step 3 的形状变更：改造前 `poll()` 在轮询线程里**直接调 domain**
/// 做完整业务（远端 HTTP 拉取 + 消息投递），既拖长轮询周期、也不经过事件中心
/// （无归属、无回调、不进 AOP 监控）。搬进消费者后落回统一模型 ——
/// 与 cron 触发器 `agent_rest` 的「只派发事件」形态一致。
///
/// 生命周期由 AOP 统一管 —— `start()` spawn 后立即返回（契约 1），
/// `stop()` 置位并等 loop 退出（契约 2），停机自检交给 [`ProducerLoop`]。
///
/// ⚠️ 该 topic **不声明** `notify_producer`：进度账在 task tags 的
/// `a2a_synced_msgs` 里（消费者自己推进），生产者侧无业务收尾可做。
pub struct A2aPollingProducer {
    loop_ctl: Arc<ProducerLoop>,
    handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl Default for A2aPollingProducer {
    fn default() -> Self {
        Self::new()
    }
}

impl A2aPollingProducer {
    pub fn new() -> Self {
        Self {
            loop_ctl: Arc::new(ProducerLoop::new()),
            handle: Mutex::new(None),
        }
    }

    /// 一次认领：列出全部远端 Agent，逐个 emit 认领事件
    ///
    /// ⚠️ **不要在这里加回远端拉取 / 消息投递**：那是 `a2a_poll` 消费者的职责。
    /// 在轮询线程里做完整业务会把 30s 周期拖长（远端 HTTP 慢时下一轮直接顺延），
    /// 而且绕过事件中心 —— 无归属、无回调、不进 AOP 监控与失败指标。
    async fn claim(sink: &EventSink) -> Result<()> {
        let ctx = RequestContext::new_system();

        let all_agents = hr_domain::domain()
            .agent_manage()
            .list_agents(ctx.clone())
            .await?;

        let remote_agents: Vec<_> = all_agents
            .into_iter()
            .filter(|a| a.po.kind.is_remote())
            .collect();

        if remote_agents.is_empty() {
            return Ok(());
        }

        let tick_at = common::constants::utils::current_timestamp_ms();
        log_debug!(
            "a2a polling: claiming {} remote agents",
            remote_agents.len()
        );

        for agent in &remote_agents {
            // 发布句柄只能发本 producer 的 topic（`a2a.poll.requested`）
            sink.emit(&ctx, A2aPollRequestedEvent::new(&agent.po.id, tick_at))
                .await?;
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl Producer for A2aPollingProducer {
    fn name(&self) -> &str {
        "a2a_polling"
    }

    fn topic(&self) -> EventTopic {
        EventTopic::A2aPollRequested
    }

    /// 契约 1：spawn 后**立即返回**，不阻塞 `start_all`
    async fn start(&self, sink: EventSink) -> Result<()> {
        let loop_ctl = Arc::clone(&self.loop_ctl);

        let handle = tokio::spawn(async move {
            sys_info!("[a2a_polling] producer loop started");

            loop {
                if let Err(e) = A2aPollingProducer::claim(&sink).await {
                    sys_error!("[a2a_polling] claim error: {}", e);
                }

                // 可中断休眠：stop() 后下一片（≤250ms）返回 false → 退出 loop
                if !loop_ctl
                    .sleep(Duration::from_secs(POLL_INTERVAL_SECS))
                    .await
                {
                    break;
                }
            }

            sys_info!("[a2a_polling] producer loop exited");
        });

        *self.handle.lock().expect("a2a producer handle poisoned") = Some(handle);
        Ok(())
    }

    /// 契约 2：置位 **并等 loop 退出**
    async fn stop(&self) -> Result<()> {
        self.loop_ctl.stop();

        // guard 在 await 之前 drop（先 take 出来）—— 不跨 await 持有 std 锁
        let handle = self
            .handle
            .lock()
            .expect("a2a producer handle poisoned")
            .take();

        if let Some(handle) = handle {
            let _ = handle.await;
        }

        Ok(())
    }
}
