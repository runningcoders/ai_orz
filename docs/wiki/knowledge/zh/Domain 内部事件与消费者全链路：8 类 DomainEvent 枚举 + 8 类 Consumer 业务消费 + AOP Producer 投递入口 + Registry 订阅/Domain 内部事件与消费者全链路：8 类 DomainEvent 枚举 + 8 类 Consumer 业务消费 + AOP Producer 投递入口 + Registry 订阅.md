---
kind: knowledge_card
name: Domain 内部事件与消费者全链路：8 类 DomainEvent 枚举 + 8 类 Consumer 业务消费 + AOP Producer 投递入口 + Registry 订阅
category: 基础设施
scope:
  - src/models/event.rs
  - src/models/events/**/*.rs
  - src/consumer/**/*.rs
  - src/producer/**/*.rs
  - src/pkg/aop/**/*.rs
source_files:
  - src/pkg/aop/core/event.rs#L1-L24
  - src/pkg/aop/core/consumer.rs#L6-L71
  - src/pkg/aop/core/registry.rs#L12-L150
  - src/consumer/mod.rs#L16-L37
  - src/consumer/message.rs
  - src/consumer/scheduler.rs
  - src/producer/cron_trigger.rs
  - src/producer/message_channel.rs
  - docs/archive/design-archive/event_design.md
  - docs/archive/design-archive/consumer_architecture.md
  - docs/archive/plan-archive/agent_loop_engine_plan.md
  - docs/wiki/zh/content/基础设施/AOP 事件系统/AOP 事件系统.md
  - docs/wiki/zh/content/核心模块/AOP 事件系统/AOP 事件系统.md
  - docs/wiki/zh/content/架构设计/AOP 事件系统架构.md
---

## §1 概述与定位

本知识卡描述 ai_orz 项目基于 AOP 事件中心的 Domain 内部事件全链路，覆盖 8 类业务消费者注册、4 类 Trait 核心抽象、8 步启动顺序、双模式消费（Sync/Async）、ack/nack 重试机制。触发读取场景：新增 Domain 事件类型或业务 Consumer、排查事件丢失/重试/并发控制、理解 AOP 框架与业务分层边界、理解启动初始化顺序时。AOP 框架层（pkg/aop/）严格零业务依赖，业务层（consumer/producer）只实现 Trait 接入，Domain/DAL/DAO 层完全不感知 AOP 存在。

## §2 关键文件表

| 文件 | 角色 | 核心入口/约束 |
|------|------|---------------|
| [pkg/aop/core/event.rs](src/pkg/aop/core/event.rs) | Event Trait + EventKind 抽象 | Event trait 五方法：kind/id/order_key/priority/created_at；EventKind(&'static str) 静态事件类型标识 |
| [pkg/aop/core/consumer.rs](src/pkg/aop/core/consumer.rs) | Consumer Trait 双模式定义 | ConsumeMode::Sync（发布时立即调用）/Async（入队+Worker拉取）；Async 模式需实现 ack/nack + concurrency + 两个 sleep 参数 |
| [pkg/aop/core/registry.rs](src/pkg/aop/core/registry.rs) | Registry 全局注册调度 | self_ref Weak 循环注入；register_consumer（Async 模式自动创建 InMemoryEventQueue）；register_producer（需 registry_arc）；publish 统一注入 event_id/kind/order_key/priority/created_at 到 JSON 顶层 |
| [consumer/mod.rs](src/consumer/mod.rs) | 8 类消费者 init 注册 | consumer::init() 顺序注册 7 条 + 1 条 AopStatsCollector + 1 条 AopStatsHook；MessageConsumer / CronTriggerConsumer / ToolExecLogConsumer / ToolExecStatsConsumer / AgentLoopConsumer / ThinkRoundStatsConsumer / TaskEventConsumer |
| [consumer/message.rs](src/consumer/message.rs) | 消息消费者 | Async 模式，concurrency=4；interested_events=message.created；on_event 反序列化→按 to_role 分发 Agent/User/System；ack 仅更新 DB status=Processed |
| [consumer/scheduler.rs](src/consumer/scheduler.rs) | Cron 调度消费者 | Sync 模式；interested_events=cron.trigger；on_event 按 payload.action 分发 handler |
| [producer/cron_trigger.rs](src/producer/cron_trigger.rs) | 定时轮询生产者 | poll_interval_secs=60；poll() → list_due_triggers → 逐个 publish CronTriggerEvent → mark_trigger_executed 更新 next_run_at |
| [consumer_architecture.md](docs/archive/design-archive/consumer_architecture.md) | 生产消费架构设计 | 两阶段初始化 + 事件总线前置原则；启动 8 步严格顺序；consumer::init() 禁写 DB 红线 |

## §3 架构与约定

```
pkg/aop/ (纯框架零业务)
├─ core/
│   ├─ Event trait        — kind/id/order_key/priority/created_at
│   ├─ Consumer trait     — Sync/Async 双模式 + ack/nack + concurrency
│   ├─ Producer trait     — poll_interval_secs + poll() 轮询
│   └─ Registry           — self_ref Arc 循环注入 + publish/register_* + queues
└─ queue/
    └─ InMemoryEventQueue — Async 模式每个 Consumer 独立队列

业务层 (producer + consumer)
├─ producer/  (事件生产)
│   ├─ CronTriggerProducer      — 60s 轮询 list_due → publish → mark_executed
│   ├─ MessageChannelProducer   — 外部渠道回调 on_message → HrDomain + MessageDomain
│   └─ A2aPollingProducer       — Agent-to-Agent 轮询
└─ consumer/  (8 类事件消费)
    ├─ MessageConsumer          — Async(4)  message.created  → Agent思考循环
    ├─ CronTriggerConsumer      — Sync       cron.trigger     → agent_rest
    ├─ ToolExecLogConsumer      — Sync       tool.*           → 工具执行日志
    ├─ ToolExecStatsConsumer    — Sync       tool.*           → DuckDB 工具统计
    ├─ AgentLoopConsumer        — Sync       agent.*          → Agent 循环状态机
    ├─ ThinkRoundStatsConsumer  — Sync       think.*          → 思考轮次统计
    ├─ TaskEventConsumer        — Sync       task.*           → 任务事件落库/通知
    └─ AopStatsCollector        — 旁路采集 RuntimeStats 滑动窗口
```

**核心机制要点：**

1. **启动 8 步严格顺序**：config→pkg init→service::init(同步单例，禁写 DB)→producer::init→consumer::init(仅注册订阅，禁写 DB)→service::init_base_data().await(幂等补默认数据，失败仅 warn)→aop::init_all(启动 Worker+轮询协程)→axum::serve。事件总线前置原则：producer+consumer 都注册完后再做任何业务动作，防止 base data publish 时订阅者缺位。

2. **Sync vs Async 双模式消费选型**：Sync（默认）= 发布线程直接调用 on_event，适合轻量统计/日志类（ToolExecStatsConsumer/AopStatsCollector 等），无队列无 ack，失败 publish 返回 Err。Async = 事件入独立 InMemoryEventQueue，由 concurrency 个 Worker 协程拉取消费，on_event 成功自动 ack（从队列移除 + 调用 consumer.ack 更新 DB），失败自动 nack（放回队列 + sleep error_retry_sleep_ms 重试）。MessageConsumer 是典型 Async 模式（concurrency=4，思考循环耗时长）。

3. **Registry publish 统一 JSON 元字段注入**：publish 时先从 Event trait 提取 event_id/kind/order_key/priority/created_at，序列化为 JSON 后再统一注入到顶层 obj。确保队列调度（order_key 顺序）、监控采集（event_kind 分类）、Consumer.should_consume 过滤，三者看到一致的元字段，不需要 Consumer 各自从业务结构体解析。

4. **ack/nack 职责分工**：AOP 框架层的 ack/nack 负责队列语义（从 InMemoryQueue 移除 vs 放回队尾+重试延时）；业务 Consumer 的 ack/nack 仅负责 DB 状态同步（MessageConsumer.ack → update_status(Processed)，MessageConsumer.nack → update_status(Pending)）。业务层**绝不手动操作队列**，AOP 框架根据 on_event Result 自动调用。

5. **零业务分层铁律**：pkg/aop/core/ + pkg/aop/queue/ 严禁 import 任何 service/models/consumer/domain 类型。新增事件在 src/models/events/ 定义（实现 Event trait），新增生产者在 src/producer/ 实现 Producer trait，新增消费者在 src/consumer/ 实现 Consumer trait，三者全部在各自 mod.rs init 中调用 Registry 注册，Domain/DAL/DAO 完全不 import aop。

## §4 硬约束与红线

1. **AOP 框架零业务依赖红线**：pkg/aop/ 下任何文件禁止 use crate::service/models/consumer/domain，违反即架构入侵。
2. **两阶段初始化禁写 DB**：service::init() 与 consumer::init() 是同步单例注册阶段，禁止任何 .await DB 写入，幂等默认数据统一写在 init_base_data().await 第二阶段。
3. **事件总线前置原则**：producer::init + consumer::init 必须全部完成后再执行 init_base_data 或任何业务 publish，防止订阅者缺位丢事件。
4. **Async Consumer ack/nack 仅改 DB 状态**：业务 ack 禁止再次 publish 或操作队列，队列语义由 AOP 框架 on_event Result 自动处理。
5. **Registry self_ref 必须初始化**：所有 Producer.register() 需通过 self_ref.upgrade() 获取 Arc，未调用 set_self_ref 会报 Internal "registry not initialized"。
6. **publish JSON 元字段不可覆盖**：event_id/kind/order_key/priority/created_at 由 Registry 统一 entry().or_insert 注入，禁止业务事件中重名覆盖。
7. **Consumer name 全局唯一**：同名 Consumer 注册会覆盖原队列路由，Async 模式下队列丢失消息，name 必须稳定不冲突。
8. **Domain/DAL/DAO 零感知 AOP**：任何业务实体层代码禁止 use crate::pkg::aop，事件 publish 统一走 pkg 封装的入口（如消息保存后内部 publish）。
9. **error_retry_sleep_ms 禁止设 0**：Async 消费失败后无间隔重试会导致 CPU 空转，默认 1000ms。
10. **concurrency 需匹配业务幂等性**：Async 模式相同 order_key 的事件由 Registry 内部保证顺序消费，跨 order_key 并行需确保 Consumer on_event 幂等。
