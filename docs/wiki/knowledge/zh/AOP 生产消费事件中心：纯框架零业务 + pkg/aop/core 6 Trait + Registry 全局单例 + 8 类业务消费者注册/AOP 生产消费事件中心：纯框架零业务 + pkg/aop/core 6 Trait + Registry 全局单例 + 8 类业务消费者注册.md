---
kind: RAG 原子知识卡
name: AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8 类业务消费者注册
category: 基础设施 / 异步事件
scope:
  - "src/pkg/aop/**"
  - "src/consumer/**"
  - "src/producer/**"
  - "src/service/domain/system/aop_monitor.rs"
  - "src/lib.rs (init 顺序)"
source_files:
  - src/pkg/aop/mod.rs#L1-L68 (AOP 模块总入口：全局 Registry OnceLock + publish() 便捷方法 + init_all/shutdown_all 生命周期)
  - src/pkg/aop/core/event.rs (Event trait：event_kind() 分类键 + to_json() 序列化；所有业务事件需实现，不要求 Clone/Send)
  - src/pkg/aop/core/consumer.rs (Consumer trait：consume_mode(Async/Sync) + handle(ctx, event_json) + ack/nack；业务消费者实现，禁止发新事件)
  - src/pkg/aop/core/registry.rs#L1-L120 (Registry：publish/register_consumer/register_producer；self_ref Arc 循环引用 + start_all 调度器启动)
  - src/pkg/aop/core/producer.rs (Producer trait：start/stop + tick 方法；定时扫描类生产者实现，如 CronTriggerProducer)
  - src/pkg/aop/core/scheduler.rs (调度器：异步消费者 worker 池 + 轮询间隔 + shutdown 标志位；零业务依赖纯并发框架)
  - src/pkg/aop/queue/in_memory.rs (InMemoryEventQueue：VecDeque + 互斥锁；ACK/NACK 语义；崩溃不恢复——持久化由业务层保证)
  - src/consumer/mod.rs#L1-L42 (业务消费者注册：consumer::init 按顺序注册 7 类消费者——消息/调度/工具日志/工具统计/Agent 循环/思考轮次/任务事件)
  - src/producer/mod.rs (生产者注册：producer::init 注册 CronTriggerProducer 每分钟扫到期触发器 + MessageChannelProducer 消息渠道 WS 入站 + A2aPollingProducer 外部任务轮询)
  - src/producer/cron_trigger.rs#L1-L90 (CronTriggerProducer 每分钟 tick：CronTriggerDal.list_due 查到期 → 逐个 publish(cron.trigger) 事件 → mark_trigger_executed 更新 next_run_at)
  - src/consumer/scheduler.rs#L1-L130 (CronTriggerConsumer：Sync 模式；payload.action match "agent_rest" → 调 load_and_settle 沉淀链路)
  - src/lib.rs#L150-L250 (启动总顺序严格对齐 §4.10：pkg::init_all → service::init → producer::init → consumer::init → init_base_data → stats hook → aop::init_all)
  - docs/design/consumer_architecture.md（§整体架构 pkg/aop 分层 + §启动顺序；§业务消费者 8 类一览表）
  - docs/design/event_design.md（⚠️ 旧版归档参考：EventQueueDao 废弃原因与迁移路径）
  - docs/plan/AOP生产消费事件中心重构.md（落地计划：registry 单例 + consumer::init 注册顺序 + ack/nack 语义设计）
  - docs/wiki/zh/content/基础设施/AOP 事件系统/AOP 事件系统.md（事件系统总入口：生产-消费-调度 三段架构图）
  - docs/wiki/zh/content/基础设施/AOP 事件系统/AOP 核心架构/注册中心与调度器.md（Registry 全局单例：self_ref 循环注入目的 + start_all 内部 worker 启动流程）
  - docs/wiki/zh/content/基础设施/AOP 事件系统/事件消费者/事件消费者.md（8 类消费者一览表：事件源 + handle 业务动作映射表）
  - docs/wiki/zh/content/基础设施/AOP 事件系统/事件生产者/定时触发生产者.md（CronTriggerProducer tick 流程：每分钟扫 list_due → 发事件 → mark_executed）
  - docs/wiki/zh/content/前端应用/页面模块/系统管理页面/AOP 监控面板.md（前端面板 5 指标卡片 + 事件分布饼图 + 时序折线）
  - 【平行卡 1】docs/wiki/knowledge/zh/DuckDB 多维统计双层互补：record_event! 宏自动表推断 + RuntimeStatsCollector 内存滑动窗口 + 5 维度开箱即用表/DuckDB 多维统计双层互补：record_event! 宏自动表推断 + RuntimeStatsCollector 内存滑动窗口 + 5 维度开箱即用表.md（AopStatsHook publish 前自动打点到 RuntimeStatsCollector）
  - 【平行卡 2】docs/wiki/knowledge/zh/Memory 系统增强与休息沉淀：四层记忆（Core／Working／Short／Long）+ agent_rest 每天 4 点 settle + load_and_settle 向量去重合并/Memory 系统增强与休息沉淀：四层记忆（Core／Working／Short／Long）+ agent_rest 每天 4 点 settle + load_and_settle 向量去重合并.md（CronTriggerProducer → cron.trigger 事件 → CronTriggerConsumer agent_rest 分支）
---

## §1 概述

**本卡角色**：AOP 异步事件中心框架与业务注册的总知识卡。覆盖 `pkg/aop/` 纯框架层（Event/Consumer/Producer/Registry/Scheduler/Queue 6 Trait）、全局 Registry OnceLock 单例 + `self_ref` 循环引用模式、8 类业务消费者 + 3 类业务生产者的注册顺序与启动总顺序（严格在 `init_base_data` 之后，防止「系统默认触发器注入前就开始消费」的竞态）。**定位：新增事件类型/消费者/生产者、排查事件未消费、消费顺序错乱启动报错时读。**

- **零业务耦合硬边界**：`pkg/aop/`（纯框架层）里绝对不能出现任何业务 import——禁止 use `domain/`、`dal/`、`dao/`、`models/*Po`。所有业务逻辑在 `consumer/` 目录下的 Consumer impl 里调用 domain 完成。框架层只负责：事件 JSON 序列化分发、异步并发调度、ACK/NACK 语义、优雅停机。
- **启动总顺序红线（来自 AGENTS.md §4.10）**：`pkg::init_all`（基础设施 OnceLock）→ `service::init`（DAO/DAL/Domain 单例，纯内存不碰 DB）→ `producer::init`（Producer 注册到 Registry）→ `consumer::init`（Consumer 注册到 Registry）→ `service::init_base_data().await`（**唯一 DB 写入阶段：系统默认 cron 触发器等幂等注入**）→ `stats hook 注册` → `aop::init_all().await`（**真正开始消费/生产**）。前面 4 步都只是「注册订阅者信息到 Registry 内存」，绝对不能碰 DB 或发事件。
- **8 类业务消费者 + 3 类生产者**：
  - 消费者（7 Sync + 0 Async，目前没有需要异步 IO 的高吞吐消费场景）：MessageConsumer（消息投递）、CronTriggerConsumer（定时动作路由）、ToolExecLogConsumer（工具调用日志落 messages）、ToolExecStatsConsumer（工具统计打 DuckDB）、AgentLoopConsumer（Agent 唤醒下一轮驱动）、ThinkRoundStatsConsumer（思考轮次打 exit_reason 统计）、TaskEventConsumer（任务状态流转事件）
  - 生产者（3 类轮询 tick）：CronTriggerProducer（每分钟扫 cron 到期）、MessageChannelProducer（消息渠道 WS/Lark 入站扫描）、A2aPollingProducer（外部 A2A 任务结果每 30s 轮询兜底）

---

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 源码锚点 |
|------|------|---------|---------|
| aop/mod.rs | 框架 API 门面 | static REGISTRY: OnceLock<Arc<Registry>>；publish<E: Event> 便捷调用；init_all = start_all 调度器启动；shutdown_all = 停机标志位 | `:L1-L68` |
| aop/core/event.rs | Event trait | event_kind() 返回 &'static str（路由给 Consumer 的匹配键）；to_json() 序列化；所有业务事件通过 `#[derive(AopEvent)]` 实现 | 见 trait 定义 |
| aop/core/consumer.rs | Consumer trait | consume_mode() → ConsumeMode::Sync（当前事件处理完才能下一个）或 Async（并发消费，暂未启用）；handle(ctx, event_json) → Result<()>；ack/nack 由 Registry 调完 handle 后自动触发 | 见 trait 定义 |
| aop/core/registry.rs | 注册中心 | publish：匹配 event_kind → 找对应 Consumer 列表 → 写 Queue；register_consumer/register_producer 原子插入 Vec；self_ref Arc 循环引用让 Registry 内部 worker 能发新事件 | `:L1-L120` |
| aop/core/scheduler.rs | 调度器 worker | 异步 tokio::task::spawn 启动 N 个 worker（N = CPU 核数）；每 worker 循环 50ms 从 Queue pop 事件 → 调 Consumer.handle；检测 shutdown 标志位退出 | 见 scheduler.rs |
| consumer/mod.rs | 消费者注册入口 | consumer::init() 函数：严格按顺序 Arc::new(XxxConsumer) → aop::registry().register_consumer(...)；sys_info 打一行「all business consumers registered」结束 | `:L1-L42` |
| producer/cron_trigger.rs | 定时触发生产者 | impl Producer：tick() 间隔 60s → CronTriggerDal.list_due(ctx, now, max_events=20) → 逐个 publish(CronTriggerEvent) → mark_trigger_executed 更新 next_run_at | `:L1-L90` |
| consumer/scheduler.rs | CronTriggerConsumer | 动作路由：payload.action = "agent_rest"（agent_rest 休息沉淀）/ "project_followup"（项目巡检跟进）→ 调对应 domain::service 方法 | `:L1-L130` |
| lib.rs 启动主流程 | 全局启动顺序 | AGENTS.md §4.10 强制执行；顺序错乱会导致「init_base_data 还没插 cron trigger，CronTriggerProducer 就开始扫 list_due = 空」→ 永远不触发沉淀 | 见 lib.rs |

**章节来源**
- [aop/mod.rs:L1-L68](src/pkg/aop/mod.rs#L1-L68)
- [aop/core/registry.rs:L1-L120](src/pkg/aop/core/registry.rs#L1-L120)
- [consumer/mod.rs:L1-L42](src/consumer/mod.rs#L1-L42)
- [producer/cron_trigger.rs:L1-L90](src/producer/cron_trigger.rs#L1-L90)

---

## §3 架构约定与扩展模式

### 3.1 新增业务事件（最小 5 步模板）

1. **定义事件结构体**：`src/models/internal_events/xxx_event.rs` → `pub struct CronTriggerEvent { pub trigger_id: String, pub action: String, pub extra: serde_json::Value }` + `#[derive(Serialize, Deserialize, Debug, Clone, AopEvent)]`（AopEvent 过程宏自动实现 Event trait，event_kind = stringify!(结构体名去掉 Event 后缀）
2. **在 models/mod.rs 导出**：加 `pub mod xxx_event; pub use xxx_event::*;`
3. **写 Consumer 实现**：`src/consumer/xxx_consumer.rs` → `pub struct XxxConsumer;` + `#[async_trait] impl Consumer for XxxConsumer { ... }`；handle 内部 `serde_json::from_str::<XxxEvent>(event_json)?` 反序列化 → 调 domain 完成业务逻辑；ACK/NACK 自动处理
4. **注册进 consumer::init()**：`aop::registry().register_consumer(Arc::new(XxxConsumer))?;`
5. **生产端 publish**：业务点 publish(XxxEvent { ... }).await；publish 永不返回错误（内部 Result 吞掉只打 debug 日志）

### 3.2 ConsumeMode::Sync vs Async 决策

- **Sync（默认）**：同一类事件严格顺序消费。适合所有涉及 DB 写入的场景（消息投递、任务状态更新等）—— 避免并发写 SQLite 造成 BUSY 错误
- **Async（慎用）**：同一事件类型并发消费。目前没有启用此模式的消费者。前提条件：① 消费逻辑无任何共享状态写入；② 业务能容忍乱序完成；③ QPS > 500 且 P99 延迟 > 500ms

### 3.3 Producer tick 模式 vs 事件驱动

- Producer 统一使用「轮询 tick」模式，不做纯 WS 真事件驱动（Lark WS 是一个例外：消息入站会立即 publish，但仍然每分钟有一次兜底 tick 检查连接健康）。原因：WS 断开时漏事件，轮询兜底防止；且定时任务（CronTrigger）本质就是轮询。
- Producer 的 tick 间隔：Cron=60s、A2A 轮询=30s、MessageChannel=5s（对实时性要求高）。间隔在 Producer 实现文件顶部 `const TICK_INTERVAL_MS`。

---

## §4 硬约束与回归红线

1. **consumer::init 禁止写 DB**（铁律，对应 AGENTS.md §4.10）：consumer::init 唯一允许做的事 = 调 aop::registry().register_consumer(...)。任何「初始化默认数据」的需求，必须移到 domain::init_base_data() 里（AOP init_all 之前的 DB 写入窗口）。写 DB 在这里会触发「测试环境 DB 隔离失效 + 真实启动重复插入」双 bug。
2. **Consumer.handle 禁止 publish 新事件（非严格禁止，但强不推荐）**：容易造成事件循环风暴。若必须生产新事件 → 应该直接把下游业务逻辑同步调用；实在需要异步 → 改用 Producer 模式在 handle 里手工触发新的 domain 方法，不要走 AOP 二次 publish。
3. **Registry 全局单例禁止被替换 mock**：测试环境也用真实 Registry，只是可以不调用 aop::init_all() 启动调度器（此时事件会积压在 InMemoryQueue 不被消费）。严禁把 Registry 换成 Mock 实现，否则 consumer 注册行为在测试和生产不一致。
4. **InMemoryQueue 崩溃不恢复是有意设计**：持久化由业务层在 publish 之前完成（如 messages 表先落 DB 再发 message.new 事件），ACK/NACK 只是更新业务表的 status=Consumed，不是确认队列里有一条。重启后业务 Producer / DB 扫描负责重新投递未消费的事件。
5. **启动严格顺序测试**：`tests/integration/order_startup_test.rs`（如果存在）模拟真实启动顺序，调换任意一步 → 断言失败。禁止删除或削弱该集成测试。
6. **consume_mode 切换必须改前端 AOP 监控面板**：从 Sync 切到 Async 后，`AopStatsCollector` 的 per-event 耗时会被并发平均稀释，前端平均耗时卡片必须加「并发 N 路」说明文字，否则误导用户判断消费性能。
