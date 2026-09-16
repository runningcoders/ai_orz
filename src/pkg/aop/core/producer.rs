use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use common::enums::EventTopic;
use common::error::Result;

use super::EventSink;
use crate::pkg::RequestContext;

/// `on_failed` 的返回值：告诉 AOP 这个事件还要不要再投
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDecision {
    /// 可恢复 → `queue.nack()`，按既有退避重投
    ///
    /// **默认值**，等价于改造前的「无限重投」行为。
    Retry,
    /// 不可恢复 / 已放弃 → `queue.ack()`，事件按已终结移除
    ///
    /// 走到这里意味着事件**永久移除且无死信存储**（本设计唯一不可逆的动作）：
    /// 框架必定打 error 日志并记独立埋点 `on_consume_discarded`（**不得混入失败率**
    /// —— 这是有意的业务决策而非失败），随后**仍回调 `on_consumed`** 做业务收尾
    /// （否则「放弃这封坏邮件」的游标型生产者跨不过它，下轮会重新拉到同一封）。
    Discard,
}

/// AOP 生产者 trait
///
/// **归属契约：1 producer : 1 topic。** `topic()` 声明归属，AOP 在 `start()` 时
/// 注入一个**预绑定该 topic** 的 [`EventSink`] —— 生产者因此发不出别人的 topic。
///
/// 由谁实现：**拥有该 topic 业务收尾能力的对象自身**，不新建 `producer/` 类型
/// （例如 `message.created` ⇒ `impl Producer for MessageDalImpl`）。
#[async_trait]
pub trait Producer: Send + Sync {
    /// 生产者名称（日志与排障用）
    fn name(&self) -> &str;

    /// 归属：本生产者拥有的事件主题
    fn topic(&self) -> EventTopic;

    /// 事件生命周期**终结**时的业务收尾 —— 成功消费 **或** 被放弃（[`RetryDecision::Discard`]）
    ///
    /// 由 AOP 在 `queue.ack` **之前**调用，所以实现**必须幂等**
    /// （回调成功但 ack 前进程崩溃 → 事件重投 → 回调会再跑一次）。
    ///
    /// 框架不关心本方法的返回值以外的语义：返回 `Err` 只记日志，**不改变投递结论**。
    async fn on_consumed(&self, _ctx: &RequestContext, _event: &serde_json::Value) -> Result<()> {
        Ok(())
    }

    /// 本次尝试失败时回调，返回值决定 AOP 是否重投
    ///
    /// - `Retry`（默认）= 现状行为：`queue.nack()` 按既有退避重投（框架侧**无**重试上限、无死信）；
    /// - `Discard` = 由生产者宣告「不再重试」→ 框架走 `queue.ack()` 路径移除事件，
    ///   再回调一次 `on_consumed`（见 [`RetryDecision::Discard`]）；
    /// - 本方法自身返回 `Err` → **视为 `Retry`**（安全方向：宁可重投，绝不静默丢弃）。
    ///
    /// ⚠️ **Sync 消费者上返回值被忽略**：Sync 是发布线程内联执行、无队列 →
    /// 没有重投的驱动者，`Retry` 在 Sync 下不会触发任何重试。
    ///
    /// ⚠️ 触发时机是**每次尝试失败**（不是终态通知）：Async 消费者每次重投失败都会再回调一次。
    /// 框架已在 `on_event` 失败处打了 `sys_error!`，实现里**不要**再为每次失败打 warn
    /// （一次失败会被重投放大成 N 条日志）。
    ///
    /// - `err`：失败原因（用于判定「永久性错误 → `Discard`」）
    /// - `attempt`：本事件被消费的**累计次数**，首次失败即 `1`
    async fn on_failed(
        &self,
        _ctx: &RequestContext,
        _event: &serde_json::Value,
        _err: &str,
        _attempt: u32,
    ) -> Result<RetryDecision> {
        Ok(RetryDecision::Retry)
    }

    /// 启动自管 loop（**机制在生产者**）
    ///
    /// ⚠️ **契约 1：必须「spawn 后立即返回」，不得阻塞。** 若 `start()` 里直接 await
    /// 自己的 loop，会卡死 `start_all` 连同其后所有生产者的启动（不报错，只是永远起不来）。
    ///
    /// ⚠️ **禁止**在生产者自己的 `init()` 里 spawn 常驻任务 —— 那会绕过 `stop()`。
    /// 生命周期时机一律由 AOP 统一调用（`start_all` / `shutdown_all`）。
    ///
    /// `sink` 由 AOP 构造，已预先绑定 `self.topic()`。
    async fn start(&self, _sink: EventSink) -> Result<()> {
        Ok(())
    }

    /// 停机（**时机由 AOP 统一管**）
    ///
    /// ⚠️ **契约 2：必须「置位并等 loop 退出」，不能只置位。** 只置位就返回的话，
    /// `shutdown_all` 返回后 loop 可能仍在跑（在途事件丢失、进程退出无保证）。
    /// 惯例实现：`self.loop_ctl.stop()` + `await` 保存的 `JoinHandle`。
    async fn stop(&self) -> Result<()> {
        Ok(())
    }
}

/// 生产者自持的循环控制器
///
/// 把「可中断休眠 + 停机自检」收成一个小工具，避免每个生产者重复实现：
/// `stop()` 置位后，`sleep()` 在**下一片（≤250ms）**返回 `false`，loop 随即退出。
pub struct ProducerLoop {
    stopped: AtomicBool,
}

impl Default for ProducerLoop {
    fn default() -> Self {
        Self::new()
    }
}

impl ProducerLoop {
    pub fn new() -> Self {
        Self {
            stopped: AtomicBool::new(false),
        }
    }

    /// 置位停机标志（由 `Producer::stop` 调用）
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
    }

    /// 是否已被要求退出（loop 每轮自检）
    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    /// 可中断休眠
    ///
    /// 返回 `false` = 已被要求退出（loop 应随即 `break`）。
    /// 内部按 250ms 切片，保证停机后最多 250ms 退出（对齐改造前 registry 轮询段的既有行为）。
    pub async fn sleep(&self, d: Duration) -> bool {
        /// 切片长度：兼顾「停机响应快」与「唤醒开销低」
        const SLICE_MS: u64 = 250;

        let total_ms = d.as_millis() as u64;
        let mut waited_ms = 0u64;

        while waited_ms < total_ms {
            if self.is_stopped() {
                return false;
            }
            let slice = SLICE_MS.min(total_ms - waited_ms);
            tokio::time::sleep(Duration::from_millis(slice)).await;
            waited_ms += slice;
        }

        !self.is_stopped()
    }
}
