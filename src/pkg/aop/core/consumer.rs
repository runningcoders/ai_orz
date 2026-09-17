use async_trait::async_trait;
use common::enums::EventTopic;
use common::error::Result;

use crate::pkg::RequestContext;

/// 消费模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsumeMode {
    /// 同步模式：事件发布时立即在发布线程中调用 on_event
    Sync,
    /// 异步模式：事件入队，由 AOP 调度器拉取并调用 on_event
    Async,
}

/// 消费者对某个 topic 的订阅声明
///
/// 取代原先的「感兴趣事件白名单」（`interested_events() -> Vec<EventKind>`）：
/// 订阅不只是「关心哪个 topic」，还要表达**消费语义**——是否需要顺序消费、
/// 消费完成后是否要回调通知生产者。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Subscription {
    /// 订阅的事件主题（`common::enums::EventTopic`）
    pub kind: EventTopic,
    /// 顺序消费：同一 `order_key` 的事件在本消费者内必须串行。
    ///
    /// ⚠️ 仅 **Async 且 `concurrency() > 1`** 时可观测（并发 1 天然串行）；
    /// Sync 消费者声明它是谎言（内联执行、无队列无门闩）→ 注册期直接拒。
    pub ordered: bool,
    /// 消费完成后是否回调通知该 topic 的生产者（`Producer::on_consumed` / `on_failed`）。
    ///
    /// 默认 false —— 消费本身是异步的，不需要业务收尾就别声明。
    pub notify_producer: bool,
}

impl Subscription {
    /// 订阅某 topic，不附加任何语义
    pub const fn new(kind: EventTopic) -> Self {
        Self {
            kind,
            ordered: false,
            notify_producer: false,
        }
    }

    /// 声明：同一 order_key 需在本消费者内串行
    pub const fn ordered(mut self) -> Self {
        self.ordered = true;
        self
    }

    /// 声明：消费完成后回调该 topic 的生产者
    pub const fn notify_producer(mut self) -> Self {
        self.notify_producer = true;
        self
    }
}

/// 消费失败后的终局判定：这个事件**还要不要再投一次**
///
/// ⚠️ **归属：它是消费者的答案，不是生产者的**。生产者在 `on_failed` 里只是
/// **收到**这个结论，据此决定「底层数据要不要改 / 怎么改」（见 `Producer::on_failed`）。
///
/// 为什么要分开：**只有消费者知道自己的失败意味着什么** —— 同样是 `Err`，
/// 「Agent 正忙」要重试、「Agent 已不存在」却该放弃；而生产者只拥有那份数据，
/// 它对「这条业务还能不能重来」的判断是从错误码倒推的猜测。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDecision {
    /// 可恢复 → `queue.nack()`，按既有 per-event 退避重投
    Retry,
    /// 不可恢复 / 已放弃 → `queue.ack()`，事件按已终结移除
    ///
    /// 走到这里意味着事件**永久移除且无死信存储**（本设计唯一不可逆的动作）：
    /// 框架必定打 error 日志并记独立埋点 `on_consume_discarded`（**不得混入失败率**
    /// —— 这是有意的业务决策而非失败），随后**仍回调 `on_consumed`** 做业务收尾
    /// （否则「放弃这封坏邮件」的游标型生产者跨不过它，下轮会重新拉到同一封）。
    Discard,
}

/// 默认累计尝试上限：达到即判失败终结
///
/// 取值考虑：瞬时故障（DB 抖动 / 网络超时）通常几次内恢复；给足 8 次仍失败，
/// 就不再指望重投 —— 转为留下审计痕迹并越过该条。
pub const DEFAULT_MAX_ATTEMPTS: u32 = 8;

/// 永久性错误码：命中即 [`RetryDecision::Discard`]（重投一万次也不会变好）
///
/// 取值与 `common/src/error/code.rs` 的 `code:` 字段逐字一致。
/// 失败原因串来自 `Error` 的 `Display`，格式为 `[<code>] <msg>`。
const PERMANENT_CODES: &[&str] = &[
    // 报文 / 参数非法（邮件格式不合法、协议不匹配）
    "invalid_request",
    // 渠道 / 凭证 / 关联资源不存在
    "not_found",
    "resource_not_found",
    // 语义上不支持（如渠道类型与事件类型不匹配）
    "unsupported_operation",
    // 超出体积上限：重投同样超限
    "payload_too_large",
    // 配置缺失 / 非法：没人改配置就永远缺
    "config_missing",
    "config_invalid",
];

/// 从 `[<code>] <msg>` 形态里取出错误码
///
/// ⚠️ **必须是完整形态**（前缀 `[` + 闭合 `]`）：残缺形态（如 `[not_found`）一律当
/// 「看不懂」→ 交回 [`RetryDecision::Retry`]，而不是赌它属于哪个错误码。
fn error_code_of(err: &str) -> Option<&str> {
    let (code, _rest) = err.strip_prefix('[')?.split_once(']')?;
    Some(code)
}

/// AOP 消费者 trait
///
/// 统一的事件消费接口，支持同步和异步两种消费模式：
/// - **Sync**：事件发布时直接调用 `on_event`，适合轻量级处理
/// - **Async**：事件入队，由 AOP 调度器从队列拉取后调用 `on_event`
///
/// 异步消费者可通过 `concurrency` 控制并行 worker 数量，
/// 通过 `empty_queue_sleep_ms`/`error_retry_sleep_ms` 控制轮询节奏。
#[async_trait]
pub trait Consumer: Send + Sync {
    /// 消费者名称（全局唯一，用于队列路由和日志追踪）
    fn name(&self) -> &str;

    /// 订阅声明列表（取代原 `interested_events()`）
    fn subscriptions(&self) -> Vec<Subscription>;

    /// 事件过滤（默认全部通过）
    async fn should_consume(&self, _event: &serde_json::Value) -> bool {
        true
    }

    /// 消费模式（默认同步）
    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Sync
    }

    /// 处理事件（核心业务逻辑）
    ///
    /// 框架在分发前已从事件顶层 `context_carrier` 还原出与主 context 同源的
    /// `ctx` 并传入；消费侧可直接使用（其 log_id 等链路线索已贯通），
    /// 也可在此基础上追加/修饰业务字段。
    async fn on_event(&self, ctx: RequestContext, event: serde_json::Value) -> Result<()>;

    // ===== 以下仅 Async 模式消费者需要关注 =====
    //
    // ℹ️ `ack`/`nack` 已随「业务收尾回流生产者」整体删除：
    // 投递结论由框架在 `finish_consumption` 里按 `on_event` 的 `Result` 直接判定，
    // 业务持久化回到拥有该 topic 状态的对象（`Producer::on_consumed` / `on_failed`）。

    /// 并发 worker 数量（默认 1，仅 Async 模式生效）
    fn concurrency(&self) -> usize {
        1
    }

    /// 队列为空时休眠毫秒数（默认 100ms，仅 Async 模式生效）
    fn empty_queue_sleep_ms(&self) -> u64 {
        100
    }

    /// 处理出错时休眠毫秒数（默认 1000ms，仅 Async 模式生效）
    fn error_retry_sleep_ms(&self) -> u64 {
        1000
    }

    // ===== 终局判定（「还要不要再投」由消费者回答）=====

    /// 累计尝试上限：达到即判 [`RetryDecision::Discard`]
    fn max_attempts(&self) -> u32 {
        DEFAULT_MAX_ATTEMPTS
    }

    /// 本次尝试失败后要不要重投 —— **决定权在消费者**
    ///
    /// 默认判定（绝大多数消费者够用，不要重写它除非你真的更懂这个失败）：
    /// 1. `attempt >= max_attempts()` → `Discard`（兜底：避免一条坏事件占据队列/门闩刷到天荒地老）；
    /// 2. 错误码命中 [`PERMANENT_CODES`] → `Discard`（重投不会变好的那类失败）；
    /// 3. 其余 → `Retry`（含**解析不出错误码**的形态，安全方向）。
    ///
    /// ⚠️ **返回值同时决定生产者是否收到「业务收尾」**：回调 `on_failed(decision)` 之后，
    /// `Discard` 还会再回调一次 `on_consumed`（游标型生产者靠它跨过坏条目）。
    /// 所以对「永不言弃」的消费者（如 cron 触发器：一次失败只是下一个 tick 重来），
    /// **必须**显式覆写本方法返回 `Retry`。
    ///
    /// ⚠️ Sync 消费者虽然没有队列、投递结论被忽略，但这个决定**仍然生效**：
    /// `Err` + `Discard` 会让框架去回调 `on_consumed`（如「标记触发器已执行」）。
    fn decide_retry(&self, err: &str, attempt: u32) -> RetryDecision {
        if attempt >= self.max_attempts() {
            return RetryDecision::Discard;
        }
        match error_code_of(err) {
            Some(code) if PERMANENT_CODES.contains(&code) => RetryDecision::Discard,
            // 无 `[code]` 前缀（第三方串、手工构造）→ 按可恢复处理
            _ => RetryDecision::Retry,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    /// 只用于验证默认策略的最小消费者
    struct PolicyProbe;

    #[async_trait]
    impl Consumer for PolicyProbe {
        fn name(&self) -> &str {
            "policy_probe"
        }
        fn subscriptions(&self) -> Vec<Subscription> {
            vec![Subscription::new(EventTopic::TaskStatusChanged)]
        }
        async fn on_event(&self, _ctx: RequestContext, _event: serde_json::Value) -> Result<()> {
            Ok(())
        }
    }

    /// 永久性错误码 → 第一次失败就放弃（重投一万次也不会变好）
    #[test]
    fn permanent_code_is_discarded_on_first_attempt() {
        let c = PolicyProbe;
        assert_eq!(
            c.decide_retry("[invalid_request] 邮件格式非法", 1),
            RetryDecision::Discard
        );
        assert_eq!(
            c.decide_retry("[resource_not_found] 渠道不存在", 1),
            RetryDecision::Discard
        );
        assert_eq!(
            c.decide_retry("[config_missing] 凭证未配置", 1),
            RetryDecision::Discard
        );
    }

    /// 瞬时错误：不到上限一律重投
    #[test]
    fn transient_code_keeps_retrying_until_cap() {
        let c = PolicyProbe;
        for attempt in 1..DEFAULT_MAX_ATTEMPTS {
            assert_eq!(
                c.decide_retry("[db_query_failed] 查询失败", attempt),
                RetryDecision::Retry,
                "attempt={attempt} 应立即重投"
            );
        }
        // 到上限转放弃：避免一条坏事件占着队列/门闩刷到天荒地老
        assert_eq!(
            c.decide_retry("[db_query_failed] 查询失败", DEFAULT_MAX_ATTEMPTS),
            RetryDecision::Discard
        );
    }

    /// 解析不出错误码 → 安全方向：重投（宁可多试几次，不要静默丢弃）
    #[test]
    fn unparsable_err_is_treated_as_transient() {
        let c = PolicyProbe;
        for raw in [
            "something went wrong",
            "",
            "[",
            "not_found",
            // 前缀但没有闭合 `]` —— 残缺形态不许赌它的错误码
            "[not_found",
        ] {
            assert_eq!(
                c.decide_retry(raw, 1),
                RetryDecision::Retry,
                "无法解析的形态应按可恢复处理：{raw:?}"
            );
        }
    }

    /// `max_attempts` 可被消费者收窄（可观测地影响判定）
    #[test]
    fn max_attempts_is_overridable() {
        struct StrictProbe;

        #[async_trait]
        impl Consumer for StrictProbe {
            fn name(&self) -> &str {
                "strict_probe"
            }
            fn subscriptions(&self) -> Vec<Subscription> {
                vec![Subscription::new(EventTopic::TaskStatusChanged)]
            }
            fn max_attempts(&self) -> u32 {
                2
            }
            async fn on_event(
                &self,
                _ctx: RequestContext,
                _event: serde_json::Value,
            ) -> Result<()> {
                Ok(())
            }
        }

        let c = StrictProbe;
        assert_eq!(
            c.decide_retry("[db_query_failed] 抖动", 1),
            RetryDecision::Retry
        );
        assert_eq!(
            c.decide_retry("[db_query_failed] 抖动", 2),
            RetryDecision::Discard
        );
    }
}
