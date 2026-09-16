# AOP 生产者-消费者契约重构设计

> 🎯 **本文档定位**：AOP 事件中心「生产者-消费者」契约的重构设计——解除 ack 落点错位、以 topic 归属收敛生产者形态、以订阅声明表达「顺序消费 / 回调通知」；trait 细节以实际代码为准
> 状态：草稿（2026-09-16，设计已拍板）。**已落地**：① 删除死代码 `models/event.rs`（§8）；② **Step 1 结构改造**（§7）—— `common::enums::EventTopic` 落库、`pkg::aop::EventKind` 全仓删除、`Subscription`/`subscriptions()` 取代 `interested_events()`、`finish_consumption` 单一收尾出口（`ack/nack` 按计划暂留）。**未落地**：Step 2+（`Producer` trait 改形 / DAL 实现 Producer / 删 `ack/nack` / `RetryDecision`），以及 §4.1 的 `ordered` 入队 wiring（有意延后，见 §4.1 注）。
> 查阅场景：需要理解生产者为什么不再持有 Registry、ack/nack 为什么从消费者 trait 上消失、`ordered` / `notify_producer` 声明何时必填、失败后要不要重试由谁定（`RetryDecision`）、topic 枚举为什么放 `common`（§3.6）、生产者为什么就该是 DAL 对象本身（§5.2）、新增一个生产者/消费者该实现什么时打开
>
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 项目整体分层架构与「单向依赖」红线
> - [runtime_design.md](./runtime_design.md) — §25.15「settle 并入 agent.awakening」（本设计前因：order_key 串行门闩按消费者作用域）
> - [AOP 事件系统.md](docs/wiki/zh/content/基础设施/AOP%20事件系统/AOP%20事件系统.md) — 现役生产-消费-调度三段架构总览
> - [AOP 生产消费事件中心（RAG 卡）](docs/wiki/knowledge/zh/AOP%20生产消费事件中心：纯框架零业务%20+%20pkg/aop/core%206%20Trait%20+%20Registry%20全局单例%20+%208%20类业务消费者注册/AOP%20生产消费事件中心：纯框架零业务%20+%20pkg/aop/core%206%20Trait%20+%20Registry%20全局单例%20+%208%20类业务消费者注册.md) — 6 Trait + Registry 全局单例 + 消费者注册清单（本次重构的直接改造对象）

---

## 一、设计理念（已拍板）

| # | 理念 | 落地形态 |
|---|---|---|
| 1 | **生产者 → AOP → 消费者**，三段解耦 | 已有：`Registry.consumers: HashMap<EventTopic, Vec<Arc<dyn Consumer>>>`（改造前键是 `EventKind` 字符串，§3.6）即 topic 路由表，publish 只按 `event.kind()` 查表 |
| 2 | **一个生产者只产出一个 topic**（1 producer : 1 topic） | 新增 `Producer::topic()`；`topic → producer` 反查索引。**这是让「按 kind 反查归属」成立的前提** |
| 3 | **消费者可订阅多个 topic**；默认直接分发；仅当声明 `ordered` 且事件带 `order_key` 才进等待队列 | `Subscription { kind, ordered, notify_producer }`；队列门闩判定从「事件带 order_key」改为「**订阅声明 ordered** ∧ 事件带 order_key」 |
| 4 | **消费者订阅时声明是否要回调生产者**，默认不通知 | `Subscription.notify_producer`；消费完成后由 AOP 回调 `Producer::on_consumed` / `on_failed` |
| 4.1 | **重试决策由生产者给出，AOP 只负责执行** | `Producer::on_failed(ctx, event, err, attempt) -> RetryDecision { Retry, Discard }`；默认 `Retry`（= 现状行为）；`Discard` 复用 `ack` 路径；**框架不设 `max_retry`、不做死信**；决策可对接 `pkg/policy` 策略引擎（§4.5） |
| 5 | **ack 是 AOP 层面的事，业务收尾是生产者的事** | 删 `Consumer::ack/nack`；投递生命周期由 worker 内部收尾，业务持久化回流给生产者 |
| 6 | **生产者提供开始/退出机制，AOP 统一管理时机** | `Producer::start(EventSink)` / `stop()` 全量走 AOP 的 `start_all` / `shutdown_all`；**框架侧不再托管轮询** |
| 7 | **topic 是一组前后端共享的枚举，不是散落的字符串** | `common::enums::EventTopic` 为唯一 SSOT；`pkg::aop::EventKind` 整体删除；前端 AOP 监控页按枚举筛选取代手打 kind 字符串（§3.6） |
| 8 | **生产者的落脚点 = 拥有该 topic 业务收尾能力的那个对象本身** | **不新建 `producer/` 类型**：`message.created` 的生产者就是消息 DAL 单例（`update_status` 本就在它身上）。发布点、业务状态、收尾回调三者在同一对象（§5.2） |

### 1.1 论点的关键推论：三类生产者收敛为两类

改造前按「触发时机」分三类（通知型 / 入站型 / 轮询型）。改造后判据换成**「有没有归属与回调」**——触发时机退化为生产者内部实现：

| | 判据 | AOP 提供什么 |
|---|---|---|
| **① 无归属（纯通知）** | 没有 Producer 对象，只是「发个通知」 | `aop::publish()`，at-most-once，无回调，无生命周期 |
| **② 有归属** | 有 Producer 对象：`topic()` + 回调 + 自管生命周期 | topic↔producer 反查索引、`on_consumed` / `on_failed`、统一 `start` / `stop` |

**为什么必须收敛**：`Producer` trait 上曾有一条「自管生命周期」分支（`poll_interval_secs() == 0 → start()`），但已注册的两个生产者**全是轮询** → 该分支在生产代码里零用户。也就是说，AOP 为了托管一个 60s 定时器，背上了 `producers: Vec<Arc<dyn Producer>>`、`start_all` 里的轮询 spawn 段、`shutting_down: AtomicBool`、`self_ref: RwLock<Option<Weak<Self>>>`，以及「持锁跨 await 点会死锁」的防坑注释——**这些复杂度只有一个用户**。把轮询退回生产者内部后，registry 从「生命周期容器」退化为纯粹的「topic 路由表 + 生命周期钩子」。

---

## 二、现状事实基线（改造前的代码级事实）

> 本节所有结论均可按 file:line 复核；后面所有设计取舍都建立在这些事实上，而非推测。

### 2.1 消费者 × 订阅 × 模式 × 并发 全表

| 消费者（`name()`） | 订阅 kind | 模式 | 并发 | 实现位置 |
|---|---|---|---|---|
| `agent.awakening` | `message.created`、`agent.settle.requested` | Async | **4** | [message.rs](src/consumer/message.rs#L93-L98) |
| `cron_trigger` | `cron.trigger` | Sync | — | [scheduler.rs](src/consumer/scheduler.rs#L47-L53) |
| `agent_loop` | `agent.loop`、`agent.think.round` | Sync | — | [agent_loop_consumer.rs](src/consumer/agent_loop_consumer.rs#L33-L42) |
| `think_round_stats` | `agent.think.round` | Sync | — | [think_round_stats_consumer.rs](src/consumer/think_round_stats_consumer.rs#L36-L42) |
| `tool_exec_log` | `agent.tool.executed` | Sync | — | [tool_exec_log_consumer.rs](src/consumer/tool_exec_log_consumer.rs#L33-L39) |
| `tool_exec_stats` | `agent.tool.executed` | Sync | — | [tool_exec_stats_consumer.rs](src/consumer/tool_exec_stats_consumer.rs#L34-L40) |
| `task_event` | `task.status_changed` | Async | 1 | [task_event_consumer.rs](src/consumer/task_event_consumer.rs#L44-L50) |
| `lark_inbound` | `lark.inbound.message` | Async | 1 | [lark_inbound.rs](src/consumer/lark_inbound.rs#L34-L41) |
| `wechat_inbound` | `wechat.inbound.message` | Async | 1 | [wechat_inbound.rs](src/consumer/wechat_inbound.rs#L33-L40) |
| `email_inbound` | `email.inbound.message` | Async | 1 | [email_inbound.rs](src/consumer/email_inbound.rs#L34-L41) |
| `federation_inbound_task` | `federation.inbound.send_task` | Async | 1 | [federation_inbound_task.rs](src/consumer/federation_inbound_task.rs#L42-L49) |
| `federation_ws_outbound` | `federation.outbound` | Async | 1 | [federation_ws_outbound.rs](src/consumer/federation_ws_outbound.rs#L36-L43) |
| `federation_directory` | `organization.changed` | Async | 1 | [federation_directory.rs](src/consumer/federation_directory.rs#L38-L44) |

`concurrency()` 在生产代码里**只有一处**覆写（`agent.awakening` 返回 4，见 [message.rs](src/consumer/message.rs#L156-L158)），其余全部走 trait 默认值 1（[consumer.rs](src/pkg/aop/core/consumer.rs#L72-L75)）。

> ⚠️ 另有 `agent.state.changed` 这个 kind **没有任何消费者**：它由 [agent_runtime_state.rs](src/pkg/agent_runtime_state.rs#L485-L491) 在一个 `tokio::spawn` 里 fire-and-forget 地 publish；而 `Registry::publish` 在查不到订阅者时**直接 return、连日志都不打**（[registry.rs](src/pkg/aop/core/registry.rs#L118-L120)）→ 该事件一直在**静默丢弃**。它是「①类纯通知」最纯粹的样本（见 §8 待清理项）。

### 2.2 事件 `order_key()` 全表

| 事件 | order_key 取值 | 所在消费者 | 现网门闩是否生效 |
|---|---|---|---|
| `message.created` | 分层：收件人为 Agent 取 `to_id`；否则 `task_id → project_id`（[message.rs](src/models/events/message.rs#L27-L47)） | agent.awakening | ✅ 生效 |
| `agent.settle.requested` | `agent_id`（[agent_settle.rs](src/models/events/agent_settle.rs#L58-L60)） | agent.awakening | ✅ 生效 |
| `email.inbound.message` | `credential_id`（[email.rs](src/models/events/email.rs#L55-L57)） | email_inbound | ⚠️ 名义生效、实无差异（并发 1） |
| `lark.inbound.message` | `app_id`（[lark.rs](src/models/events/lark.rs#L131-L134)） | lark_inbound | ⚠️ 同上 |
| `wechat.inbound.message` | `bot_id`（[wechat.rs](src/models/events/wechat.rs#L125-L127)） | wechat_inbound | ⚠️ 同上 |
| `task.status_changed` | `task_id`（[task_status.rs](src/models/events/task_status.rs#L58-L60)） | task_event | ⚠️ 同上 |
| `organization.changed` | `organization_id`（[organization.rs](src/models/events/organization.rs#L43-L45)） | federation_directory | ⚠️ 同上 |
| `federation.outbound` | `peer_org`（[federation.rs](src/models/events/federation.rs#L100-L103)） | federation_ws_outbound | ⚠️ 同上 |
| `federation.inbound.send_task` | `peer_org`（[federation.rs](src/models/events/federation.rs#L126-L128)） | federation_inbound_task | ⚠️ 同上 |
| `cron.trigger` | `trigger_id`（[cron_trigger.rs](src/models/events/cron_trigger.rs#L22-L24)） | cron_trigger（**Sync**） | ❌ 不适用（Sync 无队列） |
| `agent.loop` / `agent.think.round` | `agent_id`（[agent_loop.rs](src/models/events/agent_loop.rs#L71-L73)、[think_round.rs](src/models/events/think_round.rs#L116-L118)） | agent_loop（**Sync**） | ❌ 不适用 |
| `agent.tool.executed` | `agent_id`（[tool_exec.rs](src/models/events/tool_exec.rs#L56-L59)） | tool_exec_log / tool_exec_stats（**Sync**） | ❌ 不适用 |
| `agent.state.changed` | `agent_id`（[agent_state.rs](src/models/events/agent_state.rs#L45-L47)） | （无消费者） | — |

### 2.3 ⭐ 两条决定设计取舍的推论

**推论 A：`order_key` 串行只在 `concurrency() > 1` 时可观测，而全项目只有一个消费者并发 > 1。**

队列门闩的实际形状是「同 order_key 的事件，前一条未 ack 则后继不可出队」（[in_memory.rs](src/pkg/aop/queue/in_memory.rs#L149-L163) 入队提升 + [#L226-L244](src/pkg/aop/queue/in_memory.rs#L226-L244) ack 后提升后继）。当并发 = 1 时，任何时刻只有一个事件在飞 —— **有没有门闩，可观测行为完全一致**。

由此得到本设计最重要的一条量化结论：

> `ordered` 改成 opt-in 后，**9 个 Async 订阅中 8 个行为不变**（并发 1，天然串行）；唯一必须显式声明 `ordered = true` 的是 `agent.awakening`（并发 4）——恰好就是上一次事故（`order_key` 串行静默失效）的那个消费者。

**推论 B：现状 1 producer : 1 topic 已经满足，topic 反查归属可立即成立。**

把 27 处 `publish` 调用点按 kind 归组后（口径：`aop::publish(..)` 24 处 + 多行写法 `registry().publish(..)` 3 处，二者合计即全部真实调用点），**没有任何一个 kind 有 ≥2 个不同生产者**。所以「按 `event.kind()` 反查生产者」不需要改动任何一个 publish 调用点。这是本方案最有力的可行性论据。

### 2.4 现状的两条分发路径与 ack 落点错位

```text
                          ┌── Sync  消费者：publish 线程内联执行 on_event
publish(ctx, event) ──────┤           收尾 = 埋点 ok/fail + sys_error!（无队列）
                          └── Async 消费者：入队 → worker 出队执行 on_event
                                      收尾 = consumer.ack/nack(kind, id)
                                           → registry.ack/nack → queue.ack/nack
```

- Sync 内联执行：[registry.rs](src/pkg/aop/core/registry.rs#L172-L205)
- Async worker：[registry.rs](src/pkg/aop/core/registry.rs#L372-L499)

**错位点**：`Consumer::ack/nack`（[consumer.rs](src/pkg/aop/core/consumer.rs#L52-L70)）名义上是「投递确认」，实际唯一用途是**业务持久化**——它的全部实现体就是 [message.rs](src/consumer/message.rs#L125-L149) 里那两行 `message_dal::dal().update_status(Processed/Pending)`，并且要靠 `if source != "message.created" { return Ok(()) }` 硬编码来排除别的 kind。也就是说：**框架级的投递生命周期已经自动收敛，业务级的持久化却被塞进了消费者**。

### 2.5 现存缺陷清单（本设计顺带修掉，非新增需求）

| # | 缺陷 | 证据 | 修法 |
|---|---|---|---|
| P1 | Async 消费者失败后**无限重投、无死信**，"最终失败" 概念不存在 | [registry.rs](src/pkg/aop/core/registry.rs#L465-L483) nack 后 `sleep(error_retry_sleep_ms)` 重投，无重试上限 | 不引入框架侧 `max_retry`（见 §8）：**重试决策下沉给生产者** —— `on_failed` 返回 `RetryDecision`（§4.4），默认 `Retry`（= 现状无限重投） |
| P2 | **游标型入站源 publish 后无条件推进游标** → 消费失败即丢消息（去重 ≠ 重放）。**两个渠道同形**：IMAP、微信 ilink | IMAP：[imap.rs](src/service/dao/email/imap.rs#L451-L454)（注释自称"重启基线由外部键去重兜底"）；微信：[ilink.rs](src/service/dao/wechat/ilink.rs#L439-L449)（"服务端返回新值才覆盖"，随后 [#L452-L456](src/service/dao/wechat/ilink.rs#L452-L456) 落库） | 游标推进移入生产者的 `on_consumed`（业务键已在信封内：`EmailInboundEvent.uid`、`WechatInboundEvent.message_key`） |
| P3 | cron 触发器「先 `publish` 再 `mark_trigger_executed`」→ 业务被吞掉的失败也算"已执行" | [cron_trigger.rs](src/producer/cron_trigger.rs#L75-L80) | `mark_trigger_executed` 移入生产者的 `on_consumed`；失败不 mark → 下个 tick 自然重试 |
| P4 | 业务收尾跑在「现造 `RequestContext::new_system()`」的断链 ctx 上 | [message.rs](src/consumer/message.rs#L129) / [#L144](src/consumer/message.rs#L144) | 回调由 worker 传入**从事件封套还原的同源 ctx**（`carried_ctx`），log_id 全程贯通 |
| P5 | `agent.state.changed` 无消费者，publish 静默丢弃 | [registry.rs](src/pkg/aop/core/registry.rs#L118-L120) | 独立议题（§8） |
| P6 | **三个入站消费者都把「适配失败」当成功 ack**（`return Ok(())`，注释写明"不 nack 重试"）→ 叠加 P2 的"游标已推进" = **消息确定性丢失**，且不进失败指标、无任何重放可能 | [wechat_inbound.rs](src/consumer/wechat_inbound.rs#L56-L65)、[lark_inbound.rs](src/consumer/lark_inbound.rs#L54-L60)、[email_inbound.rs](src/consumer/email_inbound.rs#L57-L67) | 正是 §4.4 的正例：消费者改为把失败上报 `Err`，由生产者 `on_failed` 判 `Discard`（永久性错误）→ **框架记 `on_consume_discarded` 埋点**。同样是丢，但留下可审计痕迹（今天只 `log_error!`） |

---

## 三、目标契约

### 3.1 `Consumer`：订阅声明取代「事件白名单」，ack/nack 删除

```rust
pub struct Subscription {
    /// 订阅的事件主题（`common::enums::EventTopic`，见 §3.6）
    pub kind: EventTopic,
    /// 顺序消费：同一 order_key 的事件在本消费者内必须串行。
    /// 仅 Async 且 concurrency() > 1 时可观测（并发 1 天然串行）。
    pub ordered: bool,
    /// 消费完成后是否回调通知该 topic 的生产者。
    /// 默认 false —— 消费本身是异步的，不需要业务收尾就别声明。
    pub notify_producer: bool,
}

pub trait Consumer: Send + Sync {
    fn name(&self) -> &str;
    fn subscriptions(&self) -> Vec<Subscription>;      // 取代 interested_events()
    async fn should_consume(&self, event: &Value) -> bool { true }
    fn consume_mode(&self) -> ConsumeMode { ConsumeMode::Sync }
    async fn on_event(&self, ctx: RequestContext, event: Value) -> Result<()>;

    // Async 专用
    fn concurrency(&self) -> usize { 1 }
    fn empty_queue_sleep_ms(&self) -> u64 { 100 }
    fn error_retry_sleep_ms(&self) -> u64 { 1000 }
    // 删除：fn ack(&self, source, event_id) / fn nack(&self, source, event_id)
}
```

> 当前实现：[Consumer trait](src/pkg/aop/core/consumer.rs#L26-L86)（`interested_events` 在 #L31、`ack`/`nack` 在 #L61-L70）

**为什么 `ack`/`nack` 可以整体删掉**：它俩的**唯一**业务实现在 `agent.awakening`，而那点业务本就要搬到生产者；搬完后方法体里没有任何东西可写，而 worker 本来就已经持有 `on_event` 的 `Result`（[registry.rs](src/pkg/aop/core/registry.rs#L402)）——「投递结果」不需要再经消费者转手。`source` 参数（上一轮为区分"这份 id 是否指向 messages 表"而引入）一并消失，因为**分发依据已经回到 kind 本身**。

### 3.2 `Producer`：归属 + 回调 + 自管生命周期；`register` 删除

```rust
pub trait Producer: Send + Sync {
    fn name(&self) -> &str;
    fn topic(&self) -> EventTopic;                       // 归属：1 producer : 1 topic（枚举见 §3.6）

    // 业务收尾回调（默认空实现）
    // on_consumed：事件生命周期**终结**时的业务收尾（成功消费 **或** 被放弃，见 §4.4）
    async fn on_consumed(&self, ctx: &RequestContext, event: &Value) -> Result<()> { Ok(()) }
    // on_failed：本次尝试失败时回调；返回值决定 AOP 是否重投（默认 Retry = 现状行为，见 §4.4）
    async fn on_failed(&self, ctx: &RequestContext, event: &Value, err: &str,
                       attempt: u32) -> Result<RetryDecision> { Ok(RetryDecision::Retry) }

    // 生命周期：机制在生产者（自持退出标志 + 自己的 loop），时机由 AOP 统一调用
    async fn start(&self, sink: EventSink) -> Result<()> { Ok(()) }
    async fn stop(&self) -> Result<()> { Ok(()) }

    // 删除：async fn register(&self, registry: Arc<Registry>)
    // 删除：fn poll_interval_secs(&self) -> u64
    // 删除：async fn poll(&self) -> Result<()>
}
```

> 当前实现：[Producer trait](src/pkg/aop/core/producer.rs#L8-L35)（`register` 在 #L11，`poll_interval_secs` 在 #L25，`poll` 在 #L32）
> `RetryDecision` 与 trait 同文件（`pkg/aop/core/producer.rs`），语义与三处写死的约定见 §4.4。

**谁来实现这个 trait：优先就是「拥有该 topic 业务状态的那个对象」本身，不新建 `producer/` 类型**（理念 8）。以 `message.created` 为例，它的三件事今天散在三处、而三处其实指向同一个对象：

| 事项 | 今天在哪 | 该在哪 |
|---|---|---|
| 发布事件 | 消息 DAL，[dal/message.rs](src/service/dal/message.rs#L163) | 消息 DAL（不动） |
| 业务状态（`messages.status`） | 消息 DAL，`MessageDal::update_status`（[dal/message.rs](src/service/dal/message.rs#L96) trait / [#L322](src/service/dal/message.rs#L322) impl） | 消息 DAL（不动） |
| 消费结果驱动的状态翻转 | 消费者，[consumer/message.rs](src/consumer/message.rs#L125-L149) | **同一个消息 DAL 对象**（`on_consumed` → `self.update_status(..)`） |

于是「新建一个 `MessageCreatedProducer`」这件事**根本不需要**：它只会把 DAL 已有的能力再包一层，还得额外把 DAL 依赖注入进去。**做法**：`impl Producer for MessageDalImpl`，并在装配点用同一个 `Arc` 注册（`MessageDalImpl` 是 `Arc<dyn MessageDal>` 背后的具体类型，装配时先拿到具体 `Arc` 再各 coerce 一次）：

```rust
// service/dal/message.rs::init() —— 装配期
pub fn init() {
    let dal: Arc<MessageDalImpl> = Arc::new(MessageDalImpl { /* ... */ });
    pkg::aop::registry()
        .register_producer(Arc::clone(&dal) as Arc<dyn Producer>)   // 同一对象，另一个身份
        .expect("register message producer");
    let _ = MESSAGE_DAL.set(dal);                                    // Arc<MessageDalImpl> → Arc<dyn MessageDal>
}
```

> 为什么可行：`register_producer` 随 `register` 的删除已退化为**纯 push（同步）**（§3.4），装配期可直接调用。
> **不要**给 `MessageDal` trait 加 `Producer` supertrait —— 那会强迫所有实现者（含测试桩）都实现 `Producer`；只对**具体实现类型** `MessageDalImpl` 实现即可。

**为什么 `register` 必须删**（它今天做的不是注册，是依赖注入）：

| 位置 | 事实 |
|---|---|
| [producer.rs](src/pkg/aop/core/producer.rs#L11) | `register(&self, registry: Arc<Registry>)` 在 trait 上 |
| [registry.rs](src/pkg/aop/core/registry.rs#L81-L101) | `register_producer` 第一步就是 `producer.register(registry_arc).await?` —— **把 Registry 反向塞给生产者** |
| [cron_trigger.rs](src/producer/cron_trigger.rs#L8-L10) | 为此存了 `RwLock<Option<Arc<Registry>>>`，唯一用途是 `poll()` 里取出来 publish |
| [a2a_polling.rs](src/producer/a2a_polling.rs#L14-L16) | 存了**同一个字段却从不 publish**（纯冗余） |

真正的注册早就是 push 模型（装配期由 [producer/mod.rs](src/producer/mod.rs#L9-L26) 主动 `register_producer(Arc::new(X))`），`Consumer` trait 里也从来没有 `register`。而生产者持有 `Registry` 的代价是**它有能力往任意 topic 发事件**，直接绕过 §1 那条「1 producer : 1 topic」不变量。正确解法不是让生产者长期持有 Registry，而是**在 `start()` 时注入一个只能发自己 topic 的句柄**。

### 3.3 `EventSink`：被删掉的 `register` + `RwLock<Option<Arc<Registry>>>` 的替代品

```rust
/// 发布句柄：由 AOP 构造并预先绑定生产者的 topic
#[derive(Clone)]
pub struct EventSink {
    registry: Arc<Registry>,   // 私有：生产者拿不到 Registry 本体
    topic: EventTopic,         // 预绑定：发不到别人的 topic
}

impl EventSink {
    /// 校验 kind 与自身 topic 一致后交给 registry.publish（fire-and-forget）
    pub async fn emit<E: Event>(&self, ctx: &RequestContext, event: E) -> Result<()>;
}
```

**为什么删不掉 `EventSink`**（结论：不该删，该收窄）。本质问题只有一个——「③类生产者怎么把事件交给 AOP」。两条路里「交回事件」走不通：

- AOP 的 `Event`（[event.rs](src/pkg/aop/core/event.rs#L12-L24)）要求 `Clone + Serialize + DeserializeOwned`，**三者都隐含 `Sized` → 非对象安全**，`Box<dyn Event>` 编不出来；`Registry::publish` 又是泛型函数（[registry.rs](src/pkg/aop/core/registry.rs#L103)）。要支持「`poll() -> Vec<事件>`」必须再加一层擦除（新 trait 或 `ProducedEvent` 信封）。
- 而真正的轮询生产者只有 1~2 个 → **为 1 个调用方加一层擦除不值**。

于是只剩「给一个发布句柄」。相比把 `&Registry` 整个交出去，`EventSink` 的价值是**接口隔离**：`register_consumer` / `start_all` / `shutdown_all` 一概不暴露，且 `emit` 校验 kind == topic。相比改造前，它是**净减代码**（替换掉 `register` + 生产者里的 `RwLock<Option<Arc<Registry>>>` 字段）。

### 3.4 `Registry`：新增 topic→producer 索引；`publish` 签名不变

```rust
pub struct Registry {
    consumers: RwLock<HashMap<EventTopic, Vec<Arc<dyn Consumer>>>>,     // 键换成枚举（§3.6），其余不变
    producers_by_topic: RwLock<HashMap<EventTopic, Arc<dyn Producer>>>, // 新增（原 Vec 无名字索引）
    queues: RwLock<HashMap<String, Arc<dyn EventQueue>>>,               // 不变：key = consumer.name()
    started: RwLock<bool>,
    shutting_down: AtomicBool,
    metrics_hook: RwLock<Option<Arc<dyn AopMetricsHook>>>,
}

impl Registry {
    /// 装配期主动调用
    pub fn register_consumer(&self, consumer: Arc<dyn Consumer>) -> Result<()>;
    /// 装配期主动调用。**已是同步函数**：随 `register` 删除（§3.2），不再需要把 Arc 反向注入生产者。
    /// 新增校验：同一 topic 已被别的生产者占用 → Err（§6.7）
    pub fn register_producer(&self, producer: Arc<dyn Producer>) -> Result<()>;

    /// 发布：只查 consumers 索引，签名与返回类型**一行不改** → 27 个调用点零改动
    pub async fn publish<E: Event>(&self, ctx: &RequestContext, event: E);

    /// 接收者改为 `&Arc<Self>`：构造 EventSink 需要 Arc（见下）
    pub async fn start_all(self: &Arc<Self>) -> Result<()>;   // 删掉轮询 spawn 段；逐个 producer.start(sink)
    pub async fn shutdown_all(&self) -> Result<()>;          // 已是逐个 producer.stop()，保持不变
}
```

> 当前实现：[Registry 字段与注册](src/pkg/aop/core/registry.rs#L30-L101)、[publish](src/pkg/aop/core/registry.rs#L103-L120)、[start_all 的轮询段](src/pkg/aop/core/registry.rs#L504-L556)、[shutdown_all](src/pkg/aop/core/registry.rs#L578-L594)

⚠️ **关键的分工切分**：`publish` **不需要**知道生产者——它只查 `consumers`。生产者索引只在**消费完成后的回调时刻**才需要（见 §4.2）。这个切分让 27 个 publish 站点全部免改。

**`start_all` 的形态**（轮询段整段删除）：

```rust
for producer in self.producers_by_topic.read()?.values() {
    let sink = EventSink { registry: Arc::clone(self), topic: producer.topic() };
    if let Err(e) = producer.start(sink).await { sys_error!(...); }
}
```

`shutdown_all` 已经是「置位 `shutting_down` → 逐个 `producer.stop().await`」（[registry.rs](src/pkg/aop/core/registry.rs#L578-L594)），**恰好就是「AOP 统一管理退出时机」**，无需改动。

⭐ **顺带净减：`self_ref` 字段可整体删除。** 它今天唯一的作用是「从 `&self` 造出一个 `Arc<Self>`」，服务三个场景：`register_producer` 里的 `producer.register(registry_arc)`（[registry.rs](src/pkg/aop/core/registry.rs#L82-L91)）、worker spawn（[#L361-L365](src/pkg/aop/core/registry.rs#L361-L365)）、轮询 spawn（[#L519-L527](src/pkg/aop/core/registry.rs#L519-L527)）。第一个随 `register` 删除而消失、第三个随轮询段删除而消失，剩下的 worker spawn 与 `EventSink` 构造只需把接收者改成 `self: &Arc<Self>` 即可拿到 Arc。于是可一并删掉：字段本身（[#L15](src/pkg/aop/core/registry.rs#L15)）、`set_self_ref`（[#L39-L41](src/pkg/aop/core/registry.rs#L39-L41)）、初始化调用（[pkg/aop/mod.rs](src/pkg/aop/mod.rs#L40)）、测试里的调用（[#L733](src/pkg/aop/core/registry.rs#L733)），以及那层「为绕开 `&self` 拿不到 Arc 而生」的 `RwLock` 读锁样板。**调用点无需改**：`REGISTRY.start_all()`（[pkg/aop/mod.rs](src/pkg/aop/mod.rs#L64)）与测试都在 `Arc<Registry>` 上调用，auto-ref 即得 `&Arc<Registry>`。

### 3.5 生产者自管 loop 的退出契约

改造前 registry 的轮询段把「interval 切成 500ms 片段 + 每片自检停机标志」写死在自己身上（[registry.rs](src/pkg/aop/core/registry.rs#L542-L547)）。退回生产者后，为避免每个生产者重复实现该细节、并保住「停机后最多 500ms 退出」的既有行为，框架提供一个**可复用的小工具**（是工具，不是注入的机制）：

```rust
/// 生产者自持的循环控制器：stop() 置位后，sleep 在下一片（≤ 250ms）返回 false
pub struct ProducerLoop { stopped: AtomicBool }
impl ProducerLoop {
    pub fn new() -> Self;
    pub fn stop(&self);
    pub fn is_stopped(&self) -> bool;
    /// 可中断休眠：返回 false = 已被要求退出
    pub async fn sleep(&self, d: Duration) -> bool;
}
```

生产者实现 `stop()` 即 `self.loop_ctl.stop()`，loop 在下一片醒来后退出 —— 契约与现存测试用生产者 `StoppableProducer`（自持 `Arc<AtomicBool>` + `stop()` 置位）一致。

### 3.6 `EventTopic`：topic 的共享枚举（落 `common`，前后端同一份）

现状 topic 是 `pkg::aop::EventKind(pub &'static str)`（[event.rs](src/pkg/aop/core/event.rs#L3-L10)）—— 一个裸字符串包装，**没有任何闭集约束**：拼错一个字母的结果是「查不到订阅者 → 静默丢弃」（[registry.rs](src/pkg/aop/core/registry.rs#L118-L120) 连日志都不打）。而它同时是**前端 AOP 监控页的查询维度**（`GetStatsTimeSeriesRequest.event_kind: Option<String>`，[common/src/api/system.rs](common/src/api/system.rs#L131-L135)）——前端只能手打字符串。

**做法：在 `common::enums` 落一组枚举作为 topic 的唯一 SSOT；`pkg::aop::EventKind` 整体删除。**

```rust
// common/src/enums/event_topic.rs
/// AOP 事件主题（topic）。
///
/// 不变量：**一个主题对应一个生产者**（1 topic : 1 producer）；消费者可订阅多个。
/// 前后端共用同一份 —— 后端用于路由与回调归属反查，前端用于监控页的筛选项与展示。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, JsonSchema)]
pub enum EventTopic {
    MessageCreated,             // message.created
    AgentSettleRequested,       // agent.settle.requested
    CronTrigger,                // cron.trigger
    AgentLoop,                  // agent.loop
    AgentThinkRound,            // agent.think.round
    AgentToolExecuted,          // agent.tool.executed
    AgentStateChanged,          // agent.state.changed
    TaskStatusChanged,          // task.status_changed
    OrganizationChanged,        // organization.changed
    EmailInboundMessage,        // email.inbound.message
    LarkInboundMessage,         // lark.inbound.message
    WechatInboundMessage,       // wechat.inbound.message
    FederationOutbound,         // federation.outbound
    FederationInboundSendTask,  // federation.inbound.send_task
    FederationInboundOther,     // federation.inbound.other
    A2aPollRequested,           // a2a.poll.requested（§5.3 新增）
}

impl EventTopic {
    /// 全部成员 —— 前端渲染筛选项直接遍历它，不再手打字符串
    pub const ALL: &'static [EventTopic] = &[ /* ... */ ];

    /// 线格式：与现状 `EventKind` 的字符串**逐字一致**（注入事件 JSON 的 `kind` 字段也是它）
    pub const fn as_str(&self) -> &'static str { /* match ... */ }

    /// 反向解析（`AopEventMeta.event_kind` 是 String）；未知返回 `None` = 无生产者 = ①类
    pub fn parse(s: &str) -> Option<Self> { /* match s { "message.created" => .., _ => None } */ }
}
```

**三个关键取舍（都为了不引入兼容风险）**：

| 取舍 | 决定 | 原因 |
|---|---|---|
| 线格式 | `as_str()` 逐字沿用现状字符串；`Serialize` / `Deserialize` **手写**并复用 `as_str()` / `parse()` | ① 注入事件 JSON 的 `kind`、`AopEventMeta.event_kind`、前端展示全部零变化；② 字符串字面量**只在 `as_str()` 出现一次**，避免 `serde(rename)` 与 `as_str()` 两处各写一遍而漂移 |
| 请求侧 | `GetStatsTimeSeriesRequest.event_kind: Option<String>` → **`Option<EventTopic>`**（严格） | 非法值直接 400，而不是"静默查空"；前端下拉天然只产出合法值 |
| 响应侧 | `EventSummaryResponse` / `EventDetailResponse` 的 `event_kind` **保持 `String`**（宽容） | ⚠️ 闭合枚举放进**响应**，一旦后端新增 topic 而前端未同步重建，**整个列表反序列化失败 → 页面全白**。展示层用 `EventTopic::parse(..)` 映射标签，`None` 原样显示字符串。**这是本设计里唯一刻意"不严格"的地方** |

**`AopEventMeta.event_kind` 保持 `String`**（[metrics_hook.rs](src/pkg/aop/core/metrics_hook.rs#L20-L28)）：它由事件 JSON 反解而来，来源不受限（可能来自更早版本写入的队列条目）→ 保持字符串，需要枚举时 `EventTopic::parse(..)`。**生产者反查落空即 §1 的「①类纯通知」**，语义与今天完全一致。

**收益**：

1. **后端**：`HashMap<EventTopic, _>` 取代字符串键；拼错 topic 从「静默丢弃」变成**编译错误**。
2. **前端**：AOP 监控页的 `event_kind` 筛选从「手打字符串」变成遍历 `EventTopic::ALL` 的下拉；事件列表/详情可显示中文标签而非裸 `message.created`。
3. **契约**：前后端同一枚举 → 后端改 topic 时前端编译期就能发现（今天只有运行时静默失效）。
4. **顺带净减**：`consumer/message.rs` 的 `const KIND_MESSAGE_CREATED: &str`、`models/events/agent_settle.rs` 的 `AGENT_SETTLE_EVENT_KIND: &str`、`federation.rs` 的 `KIND_SEND_TASK` / `KIND_OTHER` 四个字符串常量全部消失，由枚举变体取代。

⚠️ **命名说明**：本枚举复用了刚被删除的旧 `models/event.rs::EventTopic` 的名字。旧枚举（`Message` / `TaskChange` / `CronTrigger` / `Custom(u16)`）属于已废弃的事件总线体系、无任何消费者，名字让给新概念正当。**副作用**：wiki 里若干描述旧 `EventTopic` 的段落（如「支持 `Custom(u16)` 扩展」）需随之更新（§8）。

---

## 四、分发与回调时序

### 4.1 事件封套（不变）与 `ordered` 的生效位置

```text
publish: { event_id, kind, order_key, priority, created_at, context_carrier, ...业务字段 }
```

入队门闩判定**从「事件属性」搬到「订阅属性 ∧ 事件属性」**：

| 订阅.ordered | 事件.order_key | 去向 | 效果 |
|---|---|---|---|
| false | 空 | `global_heap` | 直接分发（现状） |
| false | 非空 | `global_heap` | **直接分发（行为变更点）** |
| true | 空 | `global_heap` | 直接分发（无 key 无法串行） |
| true | 非空 | `queues[order_key]` | 同 key 串行（现状） |

**队列本体零改动** —— 只是入队时多带一个 `ordered` 判定（`enqueue` 需要知道所属订阅是否 ordered；实现上由 publish 侧决定走哪个入队方法，或在封套里加一个 `_ordered` 标记，落地时二选一，倾向前者）。

> ⚠️ **该 wiring 未随 Step 1 落地（有意为之）**：Step 1 的验收要求「行为严格等价」，而今天的入队门闩是**无条件**按 `order_key` 非空生效的。若 Step 1 就把判定改成「`ordered` ∧ `order_key` 非空」，则 8 个并发 1 的消费者会从「同 key FIFO（闸门队列）」变成「进 `global_heap` 按 `(priority, created_at)` 排序」—— 并发 1 时两者可观测行为基本一致，但**同 key 且 `created_at` 同秒**（时间戳粒度）时堆序不保证 FIFO，属未被要求的语义变更。故 Step 1 只把 `ordered` 作为**声明数据**引入（字段可读、注册期已硬校验 Sync 声明 ordered），wiring 与 §6.1-2 的运行期告警一起留到 Step 2。

### 4.2 消费收尾：两条路径收敛到同一个内部函数

AOP 内部新增单一出口 `finish_consumption`，**Sync 内联路径与 Async worker 路径都调用它**，避免「又长出两条路径」。它同时负责两件事：触发生产者业务回调、**给出投递结论**（`Ack` / `Nack`），调用方只管把结论落到 `queue.ack` / `queue.nack`：

```rust
/// `on_event` 的投递结论（由 finish_consumption 判定，调用方执行）
enum DeliveryOutcome { Ack, Nack }

/// 消费收尾：按 Result 触发生产者回调 + 得出投递结论
/// （①类 topic 无 producer → 跳过回调，结论仍按 Result 给出）
async fn finish_consumption(
    registry: &Registry, consumer: &Arc<dyn Consumer>, meta: &AopEventMeta,
    ctx_cb: &RequestContext, event_json: &Value, outcome: Result<(), &str>,
) -> DeliveryOutcome
```

| 路径 | 位置 | 收尾动作 |
|---|---|---|
| Sync | [registry.rs](src/pkg/aop/core/registry.rs#L172-L205) | 埋点 → `finish_consumption` |
| Async | [registry.rs](src/pkg/aop/core/registry.rs#L402-L485) | 埋点 → `finish_consumption`（得出投递结论）→ `queue.ack` / `queue.nack` → `sleep(error_sleep)` |

`finish_consumption` 内部：

```text
producer = EventTopic::parse(&meta.event_kind)                // 未知 kind → None
              .and_then(|t| producers_by_topic.get(&t))       // 落空 = ①类，跳过回调
if producer.is_none() || !该消费者对应该 kind 声明了 notify_producer { return }
outcome.Ok  → producer.on_consumed(ctx_cb, event_json)                  → 结论 = ack
outcome.Err → decision = producer.on_failed(ctx_cb, event_json, err, attempt)
                decision == Retry   → 结论 = nack（按退避重投）
                decision == Discard → 结论 = ack，且再回调 on_consumed 做业务收尾
回调自身失败 → 只记 sys_error!，不影响投递结论（队列照常 ack/nack）
```

⚠️ **ctx 传参的现状缺口**：worker 里 `consumer.on_event(ctx, event_json)` 是**按值 move**（[registry.rs](src/pkg/aop/core/registry.rs#L402)），到收尾那一刻 ctx 已被移走。所以收尾需要**另取一份** `Self::carried_ctx(&event_json)`（框架已有该函数，[registry.rs](src/pkg/aop/core/registry.rs#L240-L245)）——这顺带修掉 P4（今天业务收尾用的是 `RequestContext::new_system()` 断链 ctx）。

### 4.3 三条必须写死的不变量

1. **业务回调先于 `queue.ack`**（与今天 `consumer.ack` 先于 `registry.ack` 一致，[registry.rs](src/pkg/aop/core/registry.rs#L413-L435)）。代价：回调成功、`queue.ack` 前进程崩溃 → 事件重投 → **回调必须幂等**（`update_status` / `mark_trigger_executed` / `cursor.last_uid = max(...)` 本身幂等，达标）。
2. **`on_failed` 的触发时机 = 每次尝试失败**。Sync 消费者 `on_event` 返回 `Err` 即触发（一次，无重投）；Async 消费者**每次重投失败都触发**（P1：无死信、无框架侧重试上限）。
   - 它**不是**「终态通知」，但**可以用来表达终态** —— 返回 `RetryDecision::Discard` 即由生产者宣告「不再重试」（见 §4.4）。这是本设计解开「无限重投 vs 死信队列」死结的地方：框架不引入 `max_retry`，把决策权交给唯一知道业务语义的一方。
   - 日志纪律：能返回 `Discard` 的位置**必须**由框架打 error（§6.4）；而 `on_failed` 内部**不要**为 async 消费者打 warn —— 框架已在 `on_event` 失败处打了 `sys_error!`（[registry.rs](src/pkg/aop/core/registry.rs#L440-L445)），生产者也打一遍就是重投风暴的第二份日志源。上一轮那次"日志刷屏"的教训就在这里。
3. **回调失败不改变投递结论**：`on_consumed` 返回 `Err` 时，事件**仍然 ack**（业务收尾是生产者的责任，不能靠"卡住队列"来重试；生产者若需要对账，靠自身幂等 + 周期自检）。反之如果是 `on_event` 失败，即使 `on_failed` 返回 `Retry` 也仍然 nack。

### 4.4 `RetryDecision`：重试决策由生产者给出，执行由 AOP 统一收口

```rust
/// `on_failed` 的返回值：告诉 AOP 这个事件还要不要重投
pub enum RetryDecision {
    /// 可恢复 → `queue.nack()`，按既有退避重投（**默认**，= 今天的无限重投行为）
    Retry,
    /// 不可恢复 / 已放弃 → `queue.ack()`，事件按已终结移除，并回调 `on_consumed` 做业务收尾
    Discard,
}
```

**为什么这条设计成立**：框架无法判断「这次失败能不能靠重投救回来」——那是业务知识（邮件解析永久失败 vs 数据库瞬时超时）。把它下沉给生产者，框架只负责执行；与「生产者提供退出机制、AOP 统一管理时机」（§3.2）是同一个形状：**业务提供决策，框架提供机制**。

**精确语义（三处必须写死）**：

| 项 | 约定 |
|---|---|
| `Discard` 走哪条路 | **复用 `queue.ack()`**（[in_memory.rs](src/pkg/aop/queue/in_memory.rs#L208-L245)：移除事件 + 清 `has_active_message` + 推进同 `order_key` 后继）→ **零新队列机制**，`Discard` 不引入任何新状态 |
| `Discard` 后是否回调 | **仍回调 `on_consumed`** —— 否则「放弃这封坏邮件」的生产者（如 IMAP 游标）无从跨过它，下轮会重新拉到同一封，形成**无限循环**。故 `on_consumed` 的准确定义是「**事件生命周期终结**（成功消费 **或** 被放弃）时的业务收尾」，不是「成功消费后」 |
| `on_failed` 自身返回 `Err` | **视为 `Retry`**（安全方向：宁可重投，绝不静默丢弃） |

**Sync 消费者上返回值被忽略**：Sync 是内联执行、无队列 → 没有重投的驱动者，「决策」无处可施。返回值在 Sync 下只作表达，不改变行为（**必须写进 trait 文档**，否则会误以为 `Retry` 在 Sync 下能触发重试）。

**Discard 的审计要求（不可省）**：`Discard` = 事件**永久移除且无死信存储** → 框架**必须**打 error 日志并记独立埋点 `on_consume_discarded`（与 `on_consume_failure` 分开）。**不得混入失败率** —— 它是有意的业务决策而非失败；混进去就会重演上一轮那种「假失败指标」刷屏。这是本设计中**唯一不可逆**的动作，取舍点在这里。

**决策依据（两类，均已纳入本轮）**：生产者作答可依据 ——

1. **错误内容**（`err: &str`）：零成本立即可用，用于判定「永久性错误 → `Discard`」（邮件格式非法、`ResourceNotFound` 等）。
2. **尝试次数**（`attempt: u32`，**已拍板：本轮带上**）：它**是新状态** —— 现状 `EventRef`（[in_memory.rs](src/pkg/aop/queue/in_memory.rs#L11-L17)）只有 `event_id / order_key / priority / created_at`，`nack` 也不自增（[#L247-L267](src/pkg/aop/queue/in_memory.rs#L247-L267)）。落地需三处小改：`EventRef` 加 `attempt: u32`、`nack` 时自增、`finish_consumption` 调用点透传（**无新表、无新队列**）。
   - 语义：**本事件被消费的累计次数**，首次失败即 `attempt == 1`。`ack` / `Discard` 后事件从队列消失，计数随之消亡，**无需持久化**。
   - 它是「退避 N 次后放弃」的必需信息，也是策略引擎的算子来源（见 §4.5）。

### 4.5 与策略引擎（`pkg/policy`）的对接

`on_failed` 的决策逻辑不该是散落的 `if err.contains(..) { .. }`。项目已有通用判断框架 [pkg/policy/mod.rs](src/pkg/policy/mod.rs)：`Policy::evaluate(&Metrics) -> Vec<String>`（命中 id 列表，空 = 未命中）+ `Metrics`（`HashMap<String, Value>`，算子灵活扩展）+ `policy_set!` 宏（And/Or 组合）。

**对接方式（零改动策略引擎）**：生产者把失败现场装成 `Metrics`，交给策略引擎判定，再把命中 id 映射为 `RetryDecision`：

```rust
let metrics = Metrics::new()
    .with("kind", self.topic().as_str())
    .with("err", err)
    .with("attempt", attempt as u64);            // ← attempt 正是为此而带
let hits = self.retry_policy.evaluate(&metrics); // 命中 id 列表；空 = 未命中
if hits.is_empty() { return Ok(RetryDecision::Retry); }   // 未命中 → 默认重试
// 业务侧按命中 id 映射（与 pkg/policy 既有约定一致：引擎只输出命中，语义由业务解释）
Ok(if hits.iter().any(|id| id == "mail_parse_fatal") {
    RetryDecision::Discard
} else {
    RetryDecision::Retry
})
```

**为什么不动 `PolicyAction`**：它的三个变体（`Deny` / `Confirm` / `Audit`）是「**执行前拦截**」语义 —— [mod.rs](src/pkg/policy/mod.rs#L17-L29) 明确写着「引擎只认识这三个词，不感知具体业务语义」；而 `Retry` / `Discard` 是「**失败后处置**」语义，塞进去会迫使引擎认识 AOP 概念，违背它的定位。按既有约定「业务侧按命中 id 自行映射」即可，零改动。

⚠️ **已知天花板（本轮不做，明确记录）**：`RetryDecision` 只表达「**要不要**重试」，**不表达「多久后」重试**。所以策略引擎暂时表达不了「前 3 次快速重试、之后每小时一次」这类**时间策略** —— 当前退避只有框架侧统一的 `error_retry_sleep_ms`（worker 级 `sleep`，非 per-event）。

若将来确需，扩展点清晰：`Retry` 变体带延迟（`Retry { after: Duration }`）→ 事件封套加 `not_before: i64` → `dequeue_next` 跳过未到期事件（`global_heap` 现按 `priority + created_at` 排序，需改为按生效时间）。**属于真新机制，按 YAGNI 延后**。

---

## 五、三类 → 两类的映射与改造清单

### 5.1 ① 无归属（零改动）

约 20 个散落的 `aop::publish()` 站点，典型如 `dal/message.rs`、`dal/organization/impl.rs`、`awakening.rs`、`think_loop.rs`、`compaction.rs`、`dal/task.rs`。它们**不注册 Producer 对象**，语义上就是「发个通知」：at-most-once、无回调、无生命周期。**`publish` 签名不改 → 一行不动**。

### 5.2 ② 有归属（需要注册 Producer 对象）

只有**声明了 `notify_producer = true` 的 topic 才必须有 Producer 对象**。而「Producer 对象是谁」按理念 8 定：**优先就是拥有该 topic 业务收尾能力的那个对象本身，不新建 `producer/` 类型**（§3.2 给了 `MessageDalImpl` 的完整写法与理由）。

| topic | Producer 对象（= 拥有该业务收尾能力的对象本身） | `topic()` | 发布点 | 回调做什么 | 优先级 |
|---|---|---|---|---|---|
| `message.created` | **消息 DAL 单例自身**：`impl Producer for MessageDalImpl`（[dal/message.rs](src/service/dal/message.rs#L45-L55) 的 `new()` 里装配） | `EventTopic::MessageCreated` | DAL，[dal/message.rs](src/service/dal/message.rs#L163) | `on_consumed` → `self.update_status(ctx, id, Processed)`；`on_failed` → `Retry`（DB 瞬时错误本该重投）—— 今天写在 [consumer/message.rs](src/consumer/message.rs#L125-L149) | P0 |
| `cron.trigger` | `CronTriggerProducer`（**沿用现有对象**，只改形状） | `EventTopic::CronTrigger` | 自身 loop | `on_consumed` → `mark_trigger_executed`（从 `poll()` 搬出，修 P3；该方法在 domain，[domain/system/mod.rs](src/service/domain/system/mod.rs#L153)） | P0 |
| `email.inbound.message` | **邮件 DAL 单例自身**：`impl Producer for EmailDalImpl`（[dal/email/impl.rs](src/service/dal/email/impl.rs#L42-L53) 已持有 `email_dao`，即 IMAP 轮询 registry 的持有者） | `EventTopic::EmailInboundMessage` | DAO，[imap.rs](src/service/dao/email/imap.rs#L435) | `on_consumed` → 按 `event.uid` 推进 `cursor.last_uid = max(...)`（修 P2；需 DAO 补一个"按 UID 推进游标"的方法）；`on_failed` → 瞬时错误 `Retry`（游标不推进，下轮重拉）；**永久失败 `Discard`** → 仍回调 `on_consumed` 越过该 UID（否则同一封坏邮件永远卡住收件箱 —— §4.4「`Discard` 仍回调」的现实场景） | P1 |
| `wechat.inbound.message` | **微信 DAL 单例自身**（`service/dal/wechat/`，与 email 同形） | `EventTopic::WechatInboundMessage` | DAO，[ilink.rs](src/service/dao/wechat/ilink.rs#L434-L436) | 同 email：`on_consumed` 推进 opaque 游标（修 P2 的微信一侧）；`on_failed` 同上 | P1（与 P6 一起） |
| `a2a.poll.requested` | `A2aPollingProducer`（沿用现有对象，改形状） | `EventTopic::A2aPollRequested` | 自身 `start()` loop | 无（进度账在 task tags 内，见 §5.3） | P1 |
| `lark.inbound.message` | **飞书 DAL 单例自身**（`service/dal/lark/`） | `EventTopic::LarkInboundMessage` | DAO，[lark/ws.rs](src/service/dao/lark/ws.rs#L187) | ⚠️ **没有游标可推进** —— `FrameAction::Continue` 是 WS 读循环控制（[pkg/ws/mod.rs](src/pkg/ws/mod.rs#L59-L66)），**不是**服务端确认（此处修正本稿早期版本的误判）→ 该 topic 的"收尾"目前无事可做，`notify_producer` 可先不声明 | P2 |
| `federation.inbound.send_task` | 待定 | — | DAO，[session.rs](src/service/dao/organization_link/ws/session.rs#L67-L77) | 待定 —— 应用层回执已由**响应帧**承载（对端 `pending()` 按 `correlation_id` 等超时），不是"帧级 ack"（见 §8） | P2（本轮不做） |

**其余 topic 不需要注册生产者**（`agent.loop`、`agent.think.round`、`agent.tool.executed`、`agent.state.changed`、`organization.changed`、`task.status_changed`、`federation.outbound`、`federation.inbound.other` + 各 inbound 渠道）—— 反查落空就是明确表达「①类纯通知」，**零改动**。其中 `agent.state.changed` 与 `federation.inbound.other` 更进一步：**连消费者都没有**（前者 §2.1 注，后者 [federation.rs](src/models/events/federation.rs#L79-L80) 自称"无 consumer 订阅，仅可观测"）→ publish 后直接静默丢弃（P5）。

### 5.3 `A2aPollingProducer`：加一个 topic（不摘成 ScheduledJob）

**为什么加 topic 而不是摘成独立的 `ScheduledJob`**：摘出去就多出**第三种注册路径**（`producer/mod.rs::init()` 直接 spawn / registry 增设 `register_job`），正是「总会有冲突」的来源。加 topic 后它落进统一模型：统一 `start/stop`、统一 `EventSink`。

**改造形状**：

```rust
impl Producer for A2aPollingProducer {
    fn name(&self) -> &str { "a2a_polling" }
    fn topic(&self) -> EventTopic { EventTopic::A2aPollRequested }

    /// 自管 30s 轮询：只做「认领」——列出 remote Agent，逐个 emit 事件
    async fn start(&self, sink: EventSink) -> Result<()> {
        // 每 30s：list_agents → filter(kind.is_remote()) → 每个 agent emit 一条
        //          A2aPollRequestedEvent { agent_id }（order_key = agent_id）
    }
    async fn stop(&self) -> Result<()> { self.loop_ctl.stop(); Ok(()) }
}
```

新增消费者 `a2a_poll`（Async、`ordered = true`、`notify_producer = false`）：

```rust
Subscription { kind: EventTopic::A2aPollRequested, ordered: true, notify_producer: false }
```

事件与 order_key：

```rust
pub struct A2aPollRequestedEvent { pub agent_id: String, pub event_id: String, pub created_at: i64 }
// order_key = agent_id  → 同一 Agent 的相邻两轮 tick 不会重叠
```

改造收益（不是形式主义）：

1. **轮询线程不再被业务阻塞** —— 现状 [a2a_polling.rs](src/producer/a2a_polling.rs#L86-L246) 在每个 tick 里同步做完「远端 HTTP 拉取 + `send_to_user` + `update_basic` + `transition_status`」，一个慢对端会拖长整个 tick；拆开后 tick 只做 DB 枚举。
2. **天然消除同 Agent 的重叠** —— `order_key = agent_id` + `ordered = true`。
3. **重复轮次自愈** —— 同步进度由 `A2A_SYNCED_MSG_COUNT_PREFIX` tag（`already_synced`）记录，重复事件是幂等空跑；因此不需要「在飞跟踪」这类新状态（见 §8）。
4. **`RwLock<Option<Arc<Registry>>>` 字段消失** —— 换成一个 `EventSink`。

---

## 六、风险与护栏

### 6.1 `ordered` 遗忘（最大的静默回归风险）

改 opt-in 后，唯一需要声明的是 `agent.awakening`（§2.3 推论 A），但**忘声明就会静默退回上一次事故的形态**（[runtime_design.md](./runtime_design.md) §25.15）。三道护栏：

1. **注册期硬校验**：`consume_mode() == Sync` 却声明 `ordered = true` → `register_consumer` 返回 Err（Sync 路径内联执行、无队列无门闩，声明 ordered 是谎言）。
2. **运行期一次性告警**：事件带非空 `order_key`，且该消费者 `concurrency() > 1` 且订阅 `ordered == false` → 按 `(consumer_name, kind)` 去重打**一次 warn**。这是唯一能在运行期抓到该 bug 的位置（`order_key` 是运行期属性，注册期判不了）。
3. **护栏单测（必须）**：断言 `MessageConsumer::subscriptions()` 恰好含 `message.created` 与 `agent.settle.requested` 两个 kind 且**均为 `ordered = true`**。这条直接把上次修掉的东西钉死在测试里。

### 6.2 声明了 `notify_producer` 却没有生产者

`notify_producer = true` 的 kind 在 `start_all` 时**必须**能在 `producers_by_topic` 里查到，否则回调永远不触发 → 业务收尾静默丢失。

**护栏**：`start_all` 做一次校验，缺失即返回 Err（启动失败）。之所以放在 `start_all` 而不是注册期：消费者与生产者的注册顺序不固定，注册期无法判定。这条把「忘注册生产者」从静默变成**启动即失败**。

### 6.3 同一 topic 多个消费者都声明 `notify_producer = true`

会回调 N 次。现状所有需要回调的 topic 都是**单消费者**（§5.2 表），所以不是现实问题；但语义上必须写明：**生产者回调可能收到多次，必须幂等**。不引入引用计数账本（§8）。

### 6.4 `Retry` / `Discard` 的日志纪律

- **`Retry` 分支**：框架已打 `sys_error!`（§4.3 第 2 条），生产者默认空实现即可；实现对 async 消费者打日志时要意识到**一次失败会被放大成 N 条**（N = 重投次数）。确有需要走限流/一次性。
- **`Discard` 分支**：框架**必须**打一条 error（含事件 id / kind / 错误摘要）并记 `on_consume_discarded`。这是该事件在系统里的唯一痕迹 —— 少了它，一次静默丢弃将无从追查。

### 6.5 生命周期：一条注册路径 + 两个必须写死的契约

`start_all` 统一 `start(sink)`、`shutdown_all` 统一 `stop()`（[registry.rs](src/pkg/aop/core/registry.rs#L578-L594) 已如此）。**禁止**任何生产者在自己模块里（`init()`）spawn 常驻任务 —— 那会绕过 `stop()`（正是把 a2a 保留在 Producer 家族里的原因）。

⚠️ **契约 1：`start()` 必须「spawn 后立即返回」，不得阻塞。**
现状 `start_all` 对非轮询生产者是**顺序 `await producer.start()`**（[registry.rs](src/pkg/aop/core/registry.rs#L550-L555)）—— 今天这条分支**生产零用户**（两个生产者全是轮询）所以没暴露；但删掉框架侧轮询后**每个生产者都走这条路**，第一个自管 loop 的生产者就会把 `start_all`、连同其后所有生产者的启动一起**卡死**（不报错，只是永远起不来）。落地必须：`start()` 内 `tokio::spawn` 自己的 loop 并保存 `JoinHandle`，随后立即 `Ok(())`。

⚠️ **契约 2：`stop()` 必须「置位并等 loop 退出」，不能只置位。**
删掉轮询段后，框架侧 `is_shutting_down()` + 500ms 分片休眠那套（[registry.rs](src/pkg/aop/core/registry.rs#L542-L547)）**一并消失**，而生产者又拿不到 registry（`register` 在 §3.2 被删）→ 停机信号只能来自 `stop()` 本身。若 `stop()` 只置位就返回，`shutdown_all` 返回后 loop 可能仍在跑（在途事件丢失、进程退出无保证）。故：生产者持 `Mutex<Option<JoinHandle>>`，`start()` 存、`stop()` 置位 + `await` 该 handle。

### 6.6 `EventSink` 的能力收敛

`registry` 字段**必须私有**；只暴露 `emit`。`emit` 校验 `event.kind() == self.topic`，不一致返回 Err —— 生产者想发错也发不出去。这是 §1「1 producer : 1 topic」的执行点。

### 6.7 两个生产者声明同一个 topic（理念 2 自身的护栏）

`producers_by_topic` 是 `HashMap<EventTopic, _>`，若两个生产者声明同一 topic，`insert` 会**静默覆盖** → 只剩后注册那个，而另一个生产者的事件回调**全部落到错误的生产者身上**（例如 cron 的 `mark_trigger_executed` 被一个无关生产者接管 → 触发器永不 mark → 反复重跑）。**现状风险为零**（只有一个生产者），但改造后要注册 4~5 个，且「新增生产者」是未来的常态动作 —— 必须有护栏。

> ⚠️ topic 枚举化（§3.6）**防不住这一条**：枚举保证「值合法」，不保证「只用一次」——两个生产者都返回 `EventTopic::CronTrigger` 完全合法。所以运行期查重仍然必需，别因为换成枚举就以为获得了编译期保护。

**护栏**：`register_producer` 时若该 `topic` 已被占用 → **返回 Err**（注册期硬失败）。代价只是一次 map 查询。这条把理念 2 从「文档约定」变成「机制保证」——否则 `topic()` 反查归属的整个前提（§2.4：没有任何 kind 有 ≥2 个生产者）失去保护。

> 与 §6.3 恰好对称：那条管「一个 topic 多个消费者」（**允许**，代价是回调可能多次 → 要求幂等）；这条管「一个 topic 多个生产者」（**禁止**）。

---

## 七、实施路线

> 原则：每一步都可独立编译、独立验证；**Step 1 行为严格等价**，可在不接任何生产者的前提下先落。
>
> ⚠️ **为什么「删 `ack/nack`」不能放在 Step 1**：它俩今天承载着 `message.created` 的业务收尾（`messages` 状态翻转，[message.rs](src/consumer/message.rs#L125-L149)）。生产者索引要到 Step 2 才有，先删就是**静默丢业务** —— 表现是 Step 1 到 Step 3 之间 `messages` 状态不再翻转，且因为「不报错」而很难察觉。所以删除点必须绑定在「接管者已就位」的那一步。

| Step | 内容 | 验收 |
|---|---|---|
| 1 ✅ **已落地（2026-09-16）** | **`common::enums::EventTopic` 落库**（16 变体；`as_str()`/`parse()`/`ALL`/`Display`；**手写** `Serialize`/`Deserialize` 与 `JsonSchema` —— derive 生成的是变体名、与点分线格式不符）**+ 删除 `pkg::aop::EventKind`**（`Event::kind()` 返回枚举；全仓 25 处机械替换）；`Subscription { kind, ordered, notify_producer }` + `subscriptions()` 取代 `interested_events()`；注册期硬校验 **Sync 声明 ordered → Err**（§6.1-1）；`finish_consumption` 单一收尾出口（Sync 内联与 Async worker 共用，结论 = `DeliveryOutcome::{Ack,Nack}`；Step 1 无 producer 索引 → 不触发回调）。**`ack/nack` 按计划暂留**。请求侧 `GetStatsTimeSeriesRequest.event_kind: Option<EventTopic>`（严格）；响应侧 `event_kind` 保持 `String` | ✅ 全仓无 `EventKind` 残留（`git grep` 仅测试桩历史名）；`cargo clippy --workspace --exclude frontend --all-targets -- -D warnings` 绿（2m11s）；`cargo test -p common --lib` **223 过**、`cargo test -p ai_orz --lib` **1484 过 / 0 挂**（47s）；护栏单测 `awakening_declares_ordered_for_both_topics` + 集成侧 `test_awakening_consumer_owns_both_kinds`（已补两个订阅均 `ordered` 的断言）**6/6 过**；`clippy-fe`（wasm32）+ `dx check` + `docs-lint`（659 files, 0 violations）绿 |
| 2 ✅ **已落地（2026-09-16）** | `Producer` trait 改形（`topic()` / `on_consumed` / `on_failed(→RetryDecision)` / `start(sink)` / `stop`；删 `register` / `poll_interval_secs` / `poll`）；`RetryDecision::{Retry,Discard}` + `EventRef.attempt` 自增（`nack` 时 `+1`、`ack` 后重新入队复位为 1；`dequeue_next` 把 `attempt` 注入事件封套）；`Registry.producers_by_topic` + **topic 占用校验（§6.7）** + **`register_producer` 改同步**（占位冲突返回 `err!(Conflict)`，不再靠 `register()` 注入 registry）+ **删 `self_ref`/`Weak` 导入**（worker 改用 `Arc::clone(self)`；`start_all` 接收者保持 `&Arc<Self>`）；`start_all` 删轮询段 + 校验（§6.2，早于 `started` 置位）+ 生命周期两契约（§6.5，逐个 `EventSink::new(self, producer.topic())` 后 `producer.start(sink)`）；**新增 `EventSink`（§3.3）与 `ProducerLoop`（§7.2-1）**；**`impl Producer for MessageDalImpl` 并在 `dal::init()` 里注册**（`on_consumed`→`update_status(Processed)` / `on_failed`→`update_status(Pending)`+`Retry`）→ **此时才删 `ack/nack`**（`Consumer` trait 与 `MessageConsumer` 的实现一并删除；`finish_consumption` 改为反查 topic→producer 并按「topic 归属」而非旧 `source` 参数分发）| ✅ `cargo clippy --workspace --exclude frontend --all-targets -- -D warnings` 绿；`cargo test -p common --lib` **223 过**；`cargo test -p ai_orz --lib` **1502 过 / 0 挂**（唯一挂 `lark_test::listener_lifecycle_is_safe_without_channel_reference` 系并行测试共享内存 DB 的**既有 flake**，单跑 5/5 过、与本次改动无关）；`pkg::aop` 单测 **17/17**（新增 11 条：§6.2 缺生产者启动失败 / §6.7 重复 topic 拒绝 / 回调先于 ack / `Retry`→nack / `Discard`→ack 且仍回调 / `on_failed` Err 回退 `Retry` / 未声明 `notify_producer` 跳过 / 无 producer 跳过 / `EventSink` 拒异 topic / `ProducerLoop` sleep 可中断 / Sync+ordered 注册拒绝）；`service::dal::message_test` **24/24**（新增 6 条：生产者占有 topic / `on_consumed` 置 Processed / `on_failed` 置 Pending 且 `Retry` / 回调幂等 / 缺 event_id 容错 / 无 kind 过滤为设计约定）；集成 `agent_settle_queue_test` **6/6**、`federation_ws_test` **3/3**；`clippy-fe`（wasm32）+ `dx check`（No issues found）+ `docs-lint`（659 files, 0 violations）+ `cargo fmt --all -- --check` 全绿 |
| 3 | 逐个落地生产者：`CronTriggerProducer`（修 P3）→ `impl Producer for EmailDalImpl` + DAO 补「按 UID 推进游标」（修 P2）→ 微信 DAL 同形（修 P2 微信侧）→ `A2aPollingProducer` + `a2a_poll` 消费者；**P6**：三个入站消费者改为把适配失败上报 `Err`，由生产者 `on_failed` 判 `Discard` | 每个生产者各自带回调单测；`notify_producer` 声明逐个打开；P6 的 `Discard` 能在指标里看到 `on_consume_discarded` |
| 4 | 清理：删除 `Producer::register` 遗留调用、`a2a_polling` 里的 domain 直调、更新 wiki 长文与 RAG 卡 | `make ci` 全绿 |

### 7.1 Step 1 落地注记（实施时遇到的两处取舍）

1. **`agent.awakening` 的订阅声明抽成关联函数 `MessageConsumer::declarations()`**，`Consumer::subscriptions()` 转发它。原因：护栏单测要断言「两个 topic 都声明 `ordered`」，而构造 `MessageConsumer` 会取 `runtime_domain::domain()` 等全局单例（`OnceLock` 在纯单测环境为 `None` → `unwrap()` panic）。订阅声明本身与实例状态无关，抽出后护栏单测回到纯 `#[test]`（无需拉起整个 domain 图）。集成测试 `test_awakening_consumer_owns_both_kinds` 仍对**真实实例**的 `subscriptions()` 断同一件事。
2. **`EventTopic::A2aPollRequested` 已声明但暂无引用**（属 Step 3）；`notify_producer` 全库仍为 `false` —— 生产者索引要到 Step 2 才有，Step 2 注册 `MessageDalImpl` 时同步把 `message.created` 的订阅改成 `.notify_producer()`。
3. **`ordered` 未接入入队判定**（§4.1 注）：Step 1 保持「门闩无条件按 `order_key` 生效」的原行为；`Subscription.ordered` 目前只被注册期校验读取。因此 §6.1-2 的运行期告警在 Step 1 **尚未生效**，随 Step 2 的 wiring 一起打开。
4. **请求侧严格性已核实成立**：`(false, true)` 分支（无 path、纯 query）走**整结构** `serde_json::from_value`，失败即 `bad_request("query 参数解析失败: ..")` → 非法 `event_kind` 确实 400（不是被逐字段容错吞掉 —— 那是 `(true, true)` 混合分支的行为）。

**测试清单**（新增/改造）：

- `pkg::aop` 单测：`ordered=true` 且带 order_key → 同 key 串行（已有 3 条，保留）；`ordered=false` 且带 order_key → 直接分发。
- 注册期校验：Sync + `ordered=true` → Err；`notify_producer=true` 无 producer → `start_all` Err。
- 回调顺序：`on_consumed` 在 `queue.ack` 之前（断言回调内可见事件仍 in_progress）。
- 回调幂等：同一事件回调两次结果一致。
- `RetryDecision`：`Retry` → 事件重新入队；`Discard` → 事件移除**且同 `order_key` 后继可继续**（证明走的是 `ack` 路径而非卡死门闩）；`on_failed` 返回 `Err` → 按 `Retry` 处理。
- `attempt` 自增：同一事件连续 `nack` 两次后第 3 次取出时 `attempt == 3`（队列层单测，独立实例）。
- topic 占用：两个生产者声明同一 topic → 第二个 `register_producer` 返回 `Err`（§6.7）。
- `agent.awakening` 订阅护栏单测（§6.1-3）。
- 集成：`tests/integration/` 下现有 `agent_settle_queue_test` 6 用例改为按 `Subscription` 断言。

### 7.2 Step 2 落地注记（实施时的四处取舍）

1. **新增 `ProducerLoop`（设计稿未提，实施时补的复用工具）**：§3.5 要求「每个生产者自管 loop、`stop()` 置位 + await `JoinHandle`」，但「可中断 sleep」若各自实现会重复三份。抽成 [producer.rs](src/pkg/aop/core/producer.rs) 的 `ProducerLoop`（`AtomicBool` + 250ms 分片 `sleep()`；返回 `false` 即收到停止）→ 它同时**接管了 `Registry` 里被删掉的「轮询线程自检」职责**（[cron_trigger.rs](src/producer/cron_trigger.rs) / [a2a_polling.rs](src/producer/a2a_polling.rs) 共用）。
2. **`cron.trigger` 的 `notify_producer` 本步保持关闭**：`mark_trigger_executed` 仍在 `tick()` 内（P3 的修法是把 mark 移进 `on_consumed`，属 Step 3）。若本步提前打开声明，`on_consumed` 与 tick 会**双写**已执行标记 → 故 §6.2 校验当前只覆盖 `message.created` 一个 topic。`A2aPollingProducer` 同理（只声明 topic，回调留 Step 3）。
3. **`ordered` 仍只用于注册期校验**：§6.1-2 的「未声明 `ordered` 却带 order_key → 运行期告警」**尚未接线**（`Registry::register_consumer` 只读 `ordered` 做「Sync + ordered → Err」）。它牵涉入队判定改造、与契约改形不同批 → 保持「门闩无条件按 `order_key` 生效」的原行为（行为等价）。§7.1-3 曾说「随 Step 2 wiring 一起打开」，**此处更正：本步不含该项**。
4. **`MessageDalImpl` 的 `new_impl` / `message_id_of` 放宽为 `pub(crate)`**：让 [message_test.rs](src/service/dal/message_test.rs) 能**直接构造**实例测 `on_consumed`/`on_failed`，而不经 `MESSAGE_DAL` 全局单例（单例在纯单测环境未装配）。全局单例装配路径仍由集成测试覆盖。

**与 Step 1 的衔接**：§7.1-2 预告的「Step 2 注册 `MessageDalImpl` 时同步把 `message.created` 订阅改成 `.notify_producer()`」**已兑现**（[message.rs](src/consumer/message.rs#L97-L99)）。**测试清单执行状态**：除「`ordered=false` 且带 order_key → 直接分发」（§6.1-2，见上注 3，未接线）外全部落地；`Discard` 用例只断言「走 ack 路径且仍回调 `on_consumed`」，「同 `order_key` 后继可继续」复用 `EventQueue` 既有 ack 语义、未另加例。

---

## 八、YAGNI 不做清单（明确延后）

| 项 | 为什么不做 |
|---|---|
| 死信队列 / 框架侧 `max_retry` | 引入重试上限 = 引入「最终失败」状态 + 死信存储 + 人工重放，属于新机制。**改由 `on_failed` 返回 `RetryDecision`（§4.4）表达终态**：框架不设上限，决策权归业务。仍不做的是**死信存储与重放** —— `Discard` 即永久移除，只留 error 日志与埋点 |
| 事件擦除层（`Box<dyn Event>` / `ProducedEvent`） | 为一个调用方（轮询生产者）加一层擦除不值（§3.3） |
| 引用计数账本（多消费者回调去重） | 需要回调的 topic 全是单消费者；改为「回调必须幂等」的文档约束（§6.3） |
| 「在飞跟踪」防轮询事件堆积 | a2a 的重复轮次由 `already_synced` tag 幂等吸收（§5.3-3） |
| ①类也注册 Producer 对象（全量统一） | 需要给 20 个散落 publish 站点各造一个对象，而它们不需要归属与回调；topic 反查落空即表达「纯通知」 |
| ~~删除死代码 `models/event.rs`~~ **✅ 已完成（2026-09-16）** | 项目里曾存在**两个同名 `Event`** trait：`pkg/aop/core/event.rs::Event`（现役）与 `models/event.rs::Event`（对象安全、`Box<dyn Event>` + 旧 `EventTopic` 枚举）。删除面经全仓核对：仅 [models/mod.rs](src/models/mod.rs)（模块声明）与 [models/message.rs](src/models/message.rs)（`impl Event for Message`，即 `clone_box`/`as_any`/`into_any`/`id`/`topic`/`order_key`/`priority`/`created_at` 一整块）引用；AOP 内部那个同名 `EventRef` 是独立类型、不受影响。删除后单数文件消失，复数目录 `models/events/` 是唯一体系 |
| `agent.state.changed` 无消费者（P5） | 单独议题：要么删事件，要么补消费者；`publish` 查不到订阅者**连 debug 日志都不打**（[registry.rs](src/pkg/aop/core/registry.rs#L118-L120)）这条也值得补。`federation.inbound.other` 是同类（兜底 kind，无消费者） |
| `lark` 的"协议级 ack" | **已澄清：不存在可做的帧级确认。** `FrameAction::Continue / Reconnect` 是 WS 读循环控制（[pkg/ws/mod.rs](src/pkg/ws/mod.rs#L59-L66)），与「向服务端确认」无关（此处修正本稿早期版本的误判）。飞书侧是否重推取决于平台策略、代码里没有任何 ack 调用 → 无改动点。**注意 wechat 不是这一类**：它有自己的 opaque 游标且**确实在丢**（P2，已升为 P1 改造项） |
| `federation.inbound` 的"对端回执" | **已澄清：回执已存在，且不是帧级的。** 入站命令执行完由消费者 publish 响应帧，对端 `pending().resolve(correlation_id)` 按超时等待（[session.rs](src/service/dao/organization_link/ws/session.rs#L48-L58)）。所以风险形态是**重复执行**（我方无限重投 + 对端超时重发）而非丢数据；真要收敛得引入幂等键，属独立议题 |
| **重试时间策略**（per-event 退避 / `Retry { after }`） | 用户明确 **"先不做，后续再单独拓展"（2026-09-16）**。`RetryDecision` 目前只表达「要不要」，不表达「多久后」；且 worker 的 `error_retry_sleep_ms` 是**全局 sleep** —— 一个事件退避会连带影响同 worker 的其他事件。真要做需 `Retry { after: Duration }` + 事件封套加 `not_before` + `dequeue_next` 改排序键，属真新机制（细节见 §4.5 天花板） |
| wiki 里旧 `EventTopic` / 已删 `models/event.rs` 的段落 | **已核实 wiki 是 repowiki 生成物 → 只能靠重新生成修，勿手改**：`docs/wiki/zh/meta/repowiki-metadata.json` 仍登记 `models/event.rs`，5 个内容页引用它 —— `docs/wiki/zh/content/数据模型/系统模型/{定时任务和事件系统,系统模型}.md`、`docs/wiki/zh/content/核心模块/AOP 事件系统/{AOP 事件系统,事件队列系统/事件持久化}.md`，以及 RAG 卡「Domain 内部事件与消费者全链路…」。**§3.6 的 topic 枚举落地时一并重生成**（`docs_lint` 不校验 wiki 的源码链接，故不会拦住这类失效引用） |

---

## 九、附：现状路由面速查（topic → 消费者）

| topic（`EventTopic` 变体） | 消费者 | 模式 | 归属（生产者对象） | 需回调 |
|---|---|---|---|---|
| `MessageCreated` | agent.awakening | Async(4) | **消息 DAL 单例自身**（§5.2） | ✅ |
| `AgentSettleRequested` | agent.awakening | Async(4) | ❌（发布者是 consumer，非 Producer） | ❌ |
| `CronTrigger` | cron_trigger | Sync | `CronTriggerProducer`（已有，改形） | ✅ |
| `AgentLoop` | agent_loop | Sync | ❌ | ❌ |
| `AgentThinkRound` | agent_loop + think_round_stats | Sync | ❌ | ❌ |
| `AgentToolExecuted` | tool_exec_log + tool_exec_stats | Sync | ❌ | ❌ |
| `TaskStatusChanged` | task_event | Async(1) | ❌ | ❌ |
| `OrganizationChanged` | federation_directory | Async(1) | ❌ | ❌ |
| `FederationOutbound` | federation_ws_outbound | Async(1) | ❌ | ❌ |
| `FederationInboundSendTask` | federation_inbound_task | Async(1) | 待定 | 待定 |
| `FederationInboundOther` | **无** | — | ❌ | ❌ |
| `LarkInboundMessage` | lark_inbound | Async(1) | 飞书 DAL（可先不声明） | ❌（无协议级 ack，§8） |
| `WechatInboundMessage` | wechat_inbound | Async(1) | **微信 DAL 单例自身** | ✅（推进 opaque 游标，修 P2） |
| `EmailInboundMessage` | email_inbound | Async(1) | **邮件 DAL 单例自身** | ✅（推进 `last_uid`，修 P2） |
| `A2aPollRequested` | a2a_poll（新增） | Async(1) | `A2aPollingProducer`（沿用，改形） | ❌ |
| `AgentStateChanged` | **无** | — | ❌ | ❌ |

> 表中变体与线格式字符串一一对应（`MessageCreated` ↔ `"message.created"`，见 §3.6）。

> ⚠️ 唯一的多消费者 topic 是 `agent.think.round` 与 `agent.tool.executed`，**两者都在 Sync 侧、都不需要回调** → §6.3 的「单消费者」前提在真正需要它的范围内零代价成立。
