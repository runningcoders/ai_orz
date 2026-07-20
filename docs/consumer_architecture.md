# 消费者与生产者架构设计文档

> **2026-07-20 更新**：本文档已全面更新，反映基于 AOP 事件中心的生产-消费架构。

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

```text
1. config::init()
2. pkg::init_all(&config)    — 初始化日志、存储、JWT、工具注册
3. service::init()           — 初始化 service 层（domain/dal/dao）
4. producer::init()          — 注册业务生产者到 AOP
   ├─ CronTriggerProducer 注册（poll_interval_secs = 60）
   └─ message_channel::init() — 启动外部渠道监听（基于 pkg/adapter/message）
5. consumer::init()          — 注册业务消费者到 AOP
   ├─ MessageConsumer 注册（Async 模式，并发 4）
   └─ CronTriggerConsumer 注册（Sync 模式）
6. aop::init_all()           — 启动 AOP 调度器
   ├─ 为轮询 Producer 启动轮询协程
   └─ 为 Async Consumer 启动 N 个 Worker 协程
7. scheduler::init()         — Cron 触发器 DB 扫描器（仅扫描，发布事件通过 Producer）
8. axum::serve(...)          — 启动 HTTP 服务
```

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
}
```

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
