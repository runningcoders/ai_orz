# 消费者与生产者架构设计文档

> 🎯 **本文档定位**：基于 AOP 事件中心的生产-消费异步处理框架设计（分层解耦/可扩展 Trait/ack-nack 重试）
> 状态：定稿（2026-07-21 接口收敛完成，功能已落地）
> 查阅场景：新增事件类型/业务 Consumer、排查消息重试/并发控制、理解 AOP 框架与业务分层边界时打开；具体 Consumer 注册点直接看代码
>
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 整体分层架构
> - [event_design.md](./event_design.md) — 归档：旧版 EventQueueDao 事件总线（已废弃对比参考）
> - [agent_loop_engine_design.md](./agent_loop_engine_design.md) — Agent 循环驱动：事件+定时双链路
> - 【② Plan 落地（Batch9 关联）】
>   - [agent_loop_engine_plan.md](../plan/agent_loop_engine_plan.md) — 8 类 DomainEvent → AgentLoopConsumer 唤醒
> - 【③ Wiki 长文（Batch9 新增 4 篇 + 原 7 篇保留）】
>   - [AOP 事件系统.md](docs/wiki/zh/content/%E5%9F%BA%E7%A1%80%E8%AE%BE%E6%96%BD/AOP%20%E4%BA%8B%E4%BB%B6%E7%B3%BB%E7%BB%9F/AOP%20%E4%BA%8B%E4%BB%B6%E7%B3%BB%E7%BB%9F.md) — 生产-消费-调度三段架构总览
>   - [AOP 事件系统.md](docs/wiki/zh/content/%E6%A0%B8%E5%BF%83%E6%A8%A1%E5%9D%97/AOP%20%E4%BA%8B%E4%BB%B6%E7%B3%BB%E7%BB%9F/AOP%20%E4%BA%8B%E4%BB%B6%E7%B3%BB%E7%BB%9F.md) — DomainEvent 枚举定义 + 事件消费链路
>   - [AOP 事件系统.md](docs/wiki/zh/content/%E5%8A%9F%E8%83%BD%E6%A8%A1%E5%9D%97/%E7%B3%BB%E7%BB%9F%E7%AE%A1%E7%90%86/AOP%20%E4%BA%8B%E4%BB%B6%E7%B3%BB%E7%BB%9F.md) — 系统管理面板 AOP 监控入口
>   - [AOP 事件系统架构.md](docs/wiki/zh/content/%E6%9E%B6%E6%9E%84%E8%AE%BE%E8%AE%A1/AOP%20%E4%BA%8B%E4%BB%B6%E7%B3%BB%E7%BB%9F%E6%9E%B6%E6%9E%84.md) — Event/Publisher/Consumer/Registry 四角色 + Sync/Async 双模式
>   - [注册中心与调度器.md](docs/wiki/zh/content/基础设施/AOP%20事件系统/AOP%20核心架构/注册中心与调度器.md) — self_ref Arc 循环注入 + start_all worker 启动流程
>   - [事件消费者.md](docs/wiki/zh/content/基础设施/AOP%20事件系统/事件消费者/事件消费者.md) — 8 类消费者动作映射表
>   - [定时触发生产者.md](docs/wiki/zh/content/基础设施/AOP%20事件系统/事件生产者/定时触发生产者.md) — CronTriggerProducer 每分钟 tick → list_due → publish → mark_executed
>   - [后台任务系统.md](docs/wiki/zh/content/基础设施/后台任务系统.md) — 启动总顺序红线 + 事件中心异步处理链路
>   - [AOP 监控面板.md](docs/wiki/zh/content/前端应用/页面模块/系统管理页面/AOP%20监控面板.md) — 5 指标卡片 + 饼图 + 时序折线 UI
>   - [系统领域编排.md](docs/wiki/zh/content/架构设计/分层架构设计/Domain%20层编排/System%20领域编排.md) — AOP 监控处理器与 Stats 查询集成
> - 【④ RAG 原子知识卡（Batch6 原有 1 张 + Batch9 新增 1 张）】
>   - [AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8 类业务消费者注册](docs/wiki/knowledge/zh/AOP%20生产消费事件中心：纯框架零业务%20+%20pkg%2Faop%2Fcore%206%20Trait%20+%20Registry%20全局单例%20+%208%20类业务消费者注册/AOP%20生产消费事件中心：纯框架零业务%20+%20pkg%2Faop%2Fcore%206%20Trait%20+%20Registry%20全局单例%20+%208%20类业务消费者注册.md) — 零业务耦合硬边界 + lib.rs 启动 6 步严格顺序 + 6 条回归红线（含 consumer::init 禁写 DB）
>   - [Domain 内部事件与消费者全链路：8 类 DomainEvent 枚举 + 8 类 Consumer 业务消费 + AOP Producer 投递入口 + Registry 订阅](docs/wiki/knowledge/zh/Domain%20%E5%86%85%E9%83%A8%E4%BA%8B%E4%BB%B6%E4%B8%8E%E6%B6%88%E8%B4%B9%E8%80%85%E5%85%A8%E9%93%BE%E8%B7%AF%EF%BC%9A8%20%E7%B1%BB%20DomainEvent%20%E6%9E%9A%E4%B8%BE%20+%208%20%E7%B1%BB%20Consumer%20%E4%B8%9A%E5%8A%A1%E6%B6%88%E8%B4%B9%20+%20AOP%20Producer%20%E6%8A%95%E9%80%92%E5%85%A5%E5%8F%A3%20+%20Registry%20%E8%AE%A2%E9%98%85/Domain%20%E5%86%85%E9%83%A8%E4%BA%8B%E4%BB%B6%E4%B8%8E%E6%B6%88%E8%B4%B9%E8%80%85%E5%85%A8%E9%93%BE%E8%B7%AF%EF%BC%9A8%20%E7%B1%BB%20DomainEvent%20%E6%9E%9A%E4%B8%BE%20+%208%20%E7%B1%BB%20Consumer%20%E4%B8%9A%E5%8A%A1%E6%B6%88%E8%B4%B9%20+%20AOP%20Producer%20%E6%8A%95%E9%80%92%E5%85%A5%E5%8F%A3%20+%20Registry%20%E8%AE%A2%E9%98%85.md) — DomainEvent 8 大类别枚举 + 事件 3 阶段生命周期 + 8 Consumer 全能力 + 9 条分层红线

> **2026-07-20 更新**：本文档已全面更新，反映基于 AOP 事件中心的生产-消费架构。
> **2026-07-21 更新**：清理 Message DAL 中冗余的队列操作接口（`dequeue_next_message`/`ack_message`/`nack_message`）。队列的出队/确认/回退完全由 AOP 框架负责，业务 Consumer 的 `ack`/`nack` 仅更新 DB 消息状态。

## 📌 设计目标

消费者与生产者系统负责异步处理所有事件，是系统的核心后台处理引擎。设计目标：

1. **分层解耦**：AOP 框架层、业务生产者层、业务消费者层、领域层完全分离
2. **可扩展**：新增事件类型或渠道只需实现 Trait，无需修改框架
3. **可测试**：Mock 友好，不依赖真实数据库即可测试框架逻辑
4. **高可用**：支持错误重试（ack/nack）、并发控制
5. **零业务耦合**：AOP 框架不感知任何业务实体（domain/dal/dao）

---

## 🏗️ 整体架构

```text
┌──────────────────────────────────────────────────────────────────────┐
│                        pkg/aop/ (纯框架)                            │
│  ├─ core/        — Event, Consumer, Producer, Registry, Queue      │
│  └─ queue/       — EventQueue trait + InMemoryQueue                │
│                                                                    │
│  ✅ 零业务依赖（无 service/models/consumer/domain）                 │
└──────────────────────────────────────────────────────────────────────┘
                              ▲
                              │ 注册 + 发布
                              ▼
┌──────────────────────────────────────────────────────────────────────┐
│                        业务层                                        │
│                                                                    │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐          │
│  │  producer/   │    │  consumer/   │    │  service/    │          │
│  │              │    │              │    │              │          │
│  │  事件生产者   │    │  事件消费者   │    │  domain/     │          │
│  │              │    │              │    │  dal/        │          │
│  │  cron_trigger│    │  message     │    │  dao/        │          │
│  │  └─ poll()   │    │  └─ on_event │    │              │          │
│  │              │    │              │    │              │          │
│  │  message_    │    │  scheduler   │    │              │          │
│  │  channel     │    │  └─ on_event │    │              │          │
│  │  └─ callback │    │              │    │              │          │
│  └──────────────┘    └──────────────┘    └──────────────┘          │
│                                                                    │
│  pkg/adapter/ — 通用适配器基础设施（AdaptedMessage + Registry）      │
└──────────────────────────────────────────────────────────────────────┘
```

### 各层职责

| 层级 | 职责 | 是否感知 AOP | 是否感知业务 |
|------|------|-------------|-------------|
| **pkg/aop/** | 纯框架（生产订阅调度） | - | ❌ 不感知 |
| **pkg/adapter/** | 通用适配器基础设施 | ❌ 不感知 | ❌ 不感知 |
| **producer/** | 事件生产（轮询 + 外部渠道回调） | ✅ 感知 | ✅ 感知 |
| **consumer/** | 事件消费（订阅 AOP 事件） | ✅ 感知 | ✅ 感知 |
| **service/** | 纯业务逻辑（domain/dal/dao） | ❌ 不感知 | - |

---

## 🎯 AOP 核心抽象

### Event Trait

```rust
pub trait Event: Send + Sync + Serialize + Deserialize {
    fn kind(&self) -> EventKind;
    fn id(&self) -> &str;
    fn order_key(&self) -> &str;
    fn created_at(&self) -> i64;
}

pub struct EventKind(pub &'static str);
impl EventKind {
    pub const fn new(name: &'static str) -> Self { Self(name) }
}
```

### Consumer Trait

```rust
#[async_trait]
pub trait Consumer: Send + Sync {
    fn name(&self) -> &str;
    fn interested_events(&self) -> Vec<EventKind>;
    async fn should_consume(&self, _event: &serde_json::Value) -> bool { true }
    fn consume_mode(&self) -> ConsumeMode { ConsumeMode::Sync }
    async fn on_event(&self, event: serde_json::Value) -> Result<()>;
    async fn ack(&self, _event_id: &str) -> Result<()> { Ok(()) }
    async fn nack(&self, _event_id: &str) -> Result<()> { Ok(()) }
    fn concurrency(&self) -> usize { 1 }
    fn empty_queue_sleep_ms(&self) -> u64 { 100 }
    fn error_retry_sleep_ms(&self) -> u64 { 1000 }
}

pub enum ConsumeMode {
    Sync,   // 发布时直接调用 on_event
    Async,  // 入队 + Worker 协程拉取消费
}
```

### Producer Trait

```rust
#[async_trait]
pub trait Producer: Send + Sync {
    fn name(&self) -> &str;
    fn poll_interval_secs(&self) -> u64 { 0 }   // 0 = 非轮询模式
    async fn poll(&self) -> Result<()> { Ok(()) }
}
```

---

## 📐 启动流程

> **两阶段初始化原则**：`service::init()` 只做内存里的同步单例注册（OnceLock），**不做任何 DB IO**；需要 DB 写入的幂等默认数据（如系统级 cron triggers）统一走第二阶段 `service::init_base_data().await`。分离原因是测试里大量使用 `Once::call_once(|| domain::init_all())` 同步闭包调初始化，无法在里面 `.await`。
>
> **事件总线前置原则**：producer 和 consumer 都注册完成后，再执行任何业务动作（包括 base data 注入）——防止未来某个 domain 的 `init_base_data` 顺手 publish 事件时订阅者还没到位。

```text
1. config::init()
2. pkg::init_all(&config)            — 初始化日志、存储、JWT、工具注册
3. service::init()                   — 初始化 service 层（domain/dal/dao 的同步单例注册，纯内存）
4. producer::init()                  — 注册业务生产者到 AOP
   ├─ CronTriggerProducer 注册（poll_interval_secs = 60）
   └─ message_channel::init()        — 启动外部渠道监听（基于 pkg/adapter/message）
5. consumer::init()                  — 注册业务消费者到 AOP（只注册订阅，不做任何 DB 基础数据注入）
   ├─ MessageConsumer 注册（Async 模式，并发 4）
   └─ CronTriggerConsumer 注册（Sync 模式）
6. service::init_base_data().await   — 第二阶段：各 Domain 幂等补齐 DB 基础数据（失败仅 warn，不阻塞）
   └─ system::init_base_data()
       └─ ensure_system_cron_triggers — 补齐 2 条系统级默认定时任务（agent_rest 4h / project_followup 1h，按 payload action 去重）
7. aop::init_all()                   — 启动 AOP 调度器（此时 producer/consumer 和 base data 均已就位）
   ├─ 为轮询 Producer 启动轮询协程
   └─ 为 Async Consumer 启动 N 个 Worker 协程
8. axum::serve(...)                  — 启动 HTTP 服务
```

---

### 💡 Consumer 的职责边界

- **`consumer::init()` 只负责「注册订阅者到 AOP Registry」**，不应该：
  - ❌ 帮任何 Domain 注入 DB 基础数据（那是 `init_base_data` 的职责）
  - ❌ 直接依赖 SystemDomain / FinanceDomain 之外的单例做写入
- **需要幂等补齐默认 DB 条目的能力，应该写在对应 Domain 的 `pub async fn init_base_data()` 里**，然后在 `domain::init_all_base_data()` 中追加一行 `.await`——这是唯一的标准接入点。

---

## 🎯 Producer 模块设计

### 1. CronTriggerProducer（定时轮询生产者）

```rust
pub struct CronTriggerProducer { /* ... */ }

#[async_trait]
impl Producer for CronTriggerProducer {
    fn name(&self) -> &str { "cron_trigger" }
    fn poll_interval_secs(&self) -> u64 { 60 }

    async fn poll(&self) -> Result<()> {
        // 1. 调用 SystemDomain.cron_manager().list_due_triggers()
        // 2. 对每个到期触发器：
        //    - 构造 CronTriggerEvent 并发布到 AOP
        //    - 调用 mark_trigger_executed() 更新 next_run_at
    }
}
```

### 2. MessageChannelProducer（外部渠道生产者）

外部消息渠道作为事件驱动型生产者，通过 `MessageAdapterCallback` 投递消息：

```rust
pub struct MessageChannelProducer {
    hr_domain: Arc<dyn HrDomain>,
    message_domain: Arc<dyn MessageDomain>,
}

#[async_trait]
impl MessageAdapterCallback for MessageChannelProducer {
    async fn on_message(&self, msg: AdaptedMessage) -> Result<()> {
        // 1. 通过 HrDomain 解析目标 Agent
        // 2. 通过 MessageDomain 投递消息到 Agent
    }
}

pub async fn init() -> Result<()> {
    // 启动所有已注册的渠道适配器（pkg/adapter/message）
    registry.start_all(Arc::new(MessageChannelProducer::new())).await
}
```

---

## 🎯 Consumer 模块设计

### 1. MessageConsumer（消息消费者）

```rust
pub struct MessageConsumer {
    runtime_domain: Arc<dyn RuntimeDomain>,
    message_domain: Arc<dyn MessageDomain>,
    hr_domain: Arc<dyn HrDomain>,
    project_domain: Arc<dyn ProjectDomain>,
}

#[async_trait]
impl Consumer for MessageConsumer {
    fn name(&self) -> &str { "agent.awakening" }
    fn interested_events(&self) -> Vec<EventKind> {
        vec![EventKind::new("message.created")]
    }
    fn consume_mode(&self) -> ConsumeMode { ConsumeMode::Async }
    fn concurrency(&self) -> usize { 4 }

    async fn on_event(&self, event: serde_json::Value) -> Result<()> {
        // 1. 反序列化为 MessageCreatedEvent
        // 2. 从 DB 加载完整 Message
        // 3. 按 to_role 分发：Agent/User/System
    }

    async fn ack(&self, event_id: &str) -> Result<()> {
        // AOP 框架已从队列中移除该事件
        // 这里只更新 DB 状态为 Processed
        message_dal.update_status(event_id, MessageStatus::Processed).await
    }

    async fn nack(&self, event_id: &str) -> Result<()> {
        // AOP 框架已将事件放回队列
        // 这里只更新 DB 状态为 Pending
        message_dal.update_status(event_id, MessageStatus::Pending).await
    }
}
```

> **注意**：业务 Consumer 不需要手动操作队列。AOP Registry 在 `on_event` 返回 `Ok` 后自动调用 `ack()` 从队列移除事件，返回 `Err` 后自动调用 `nack()` 将事件回退到队列。业务 `ack`/`nack` 仅负责 DB 状态同步。

### 2. CronTriggerConsumer（Cron 触发器消费者）

```rust
pub struct CronTriggerConsumer {
    runtime_domain: Arc<dyn RuntimeDomain>,
}

#[async_trait]
impl Consumer for CronTriggerConsumer {
    fn name(&self) -> &str { "cron_trigger" }
    fn interested_events(&self) -> Vec<EventKind> {
        vec![EventKind::new("cron.trigger")]
    }
    fn consume_mode(&self) -> ConsumeMode { ConsumeMode::Sync }

    async fn on_event(&self, event: serde_json::Value) -> Result<()> {
        // 1. 反序列化为 CronTriggerEvent
        // 2. 解析 payload.action
        // 3. 调用对应 handler（如 agent_rest → RuntimeDomain.rest_and_settle）
    }
}
```

---

## 🌐 消息流向

### 外部消息入站流程

```text
飞书 Webhook / 微信 / Slack
    ↓
service/dal/{channel}.rs — 渠道监听 + 适配（AdaptedMessage）
    ↓
pkg/adapter/message.rs — MessageAdapterRegistry 分发
    ↓
producer/message_channel.rs — MessageChannelProducer.on_message()
    ↓
service/domain/{hr,message}/ — HrDomain.resolve_agent() + MessageDomain.send_to_agent()
    ↓
service/dal/message.rs — save_message() → 发布 MessageCreatedEvent 到 AOP
    ↓
consumer/message.rs — MessageConsumer.on_event() (Async 模式)
    ↓
按 to_role 分发：
    ├─ Agent  → RuntimeDomain 思考循环
    ├─ User   → 网关推送
    └─ System → 工具调用
```

### Cron 定时任务流程

```text
AOP 轮询协程（每 60s）
    ↓
producer/cron_trigger.rs — CronTriggerProducer.poll()
    ↓
SystemDomain.cron_manager().list_due_triggers()
    ↓
对每个到期触发器：
    ├─ 构造 CronTriggerEvent → aop::publish()
    └─ mark_trigger_executed()
    ↓
AOP Registry 同步调用（Sync 模式）
    ↓
consumer/scheduler.rs — CronTriggerConsumer.on_event()
    ↓
按 payload.action 分发：
    └─ agent_rest → RuntimeDomain.rest_and_settle()
```

---

## 📂 文件索引

### pkg/aop/ — AOP 纯框架

| 文件 | 说明 |
|---|---|
| `src/pkg/aop/mod.rs` | AOP 模块入口，全局 Registry 单例 |
| `src/pkg/aop/core/event.rs` | Event trait + EventKind |
| `src/pkg/aop/core/consumer.rs` | Consumer trait + ConsumeMode |
| `src/pkg/aop/core/producer.rs` | Producer trait |
| `src/pkg/aop/core/registry.rs` | Registry（消费者注册 + 生产者注册 + 调度） |
| `src/pkg/aop/queue/mod.rs` | EventQueue trait + InMemoryQueue |

### pkg/adapter/ — 通用适配器基础设施

| 文件 | 说明 |
|---|---|
| `src/pkg/adapter/mod.rs` | AdapterRegistry + AdaptedMessage |
| `src/pkg/adapter/message.rs` | MessageInboundAdapter + MessageAdapterCallback + MessageAdapterRegistry |

### producer/ — 业务生产者

| 文件 | 说明 |
|---|---|
| `src/producer/mod.rs` | 生产者入口，注册到 AOP |
| `src/producer/cron_trigger.rs` | CronTriggerProducer（轮询模式） |
| `src/producer/message_channel.rs` | MessageChannelProducer（事件驱动模式） |

### consumer/ — 业务消费者

| 文件 | 说明 |
|---|---|
| `src/consumer/mod.rs` | 消费者入口，注册到 AOP |
| `src/consumer/message.rs` | MessageConsumer（Async 模式） |
| `src/consumer/scheduler.rs` | CronTriggerConsumer（Sync 模式） |

### models/events/ — 业务事件定义

| 文件 | 说明 |
|---|---|
| `src/models/events/mod.rs` | 事件模块入口 |
| `src/models/events/message.rs` | MessageCreatedEvent |
| `src/models/events/cron_trigger.rs` | CronTriggerEvent |

---

## ✨ 设计总结

核心设计哲学：

1. **AOP 是纯框架**：只负责事件流转和调度，不感知任何业务实体
2. **业务层封装为 AOP 单元**：生产者/消费者通过实现 trait 接入 AOP
3. **领域层零感知 AOP**：domain/dal/dao 完全不知道 AOP 的存在
4. **基础设施统一管理**：`pkg/adapter/` 集中所有适配器基础设施
5. **生产消费完全解耦**：producer/ 和 consumer/ 互不依赖，各自注册到 AOP
