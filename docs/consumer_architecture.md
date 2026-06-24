# 消费者架构设计文档

## ✅ 实现完成状态（2026-05-15 更新）

### 已完成模块

| 模块 | 状态 | 测试 | 文件位置 |
|------|------|------|---------|
| 通用消费者框架 | ✅ 完成 | ✅ 6 个核心场景测试通过 | `src/consumer/mod.rs` |
| Message Fetcher 实现 | ✅ 完成 | ✅ 8 个分发场景测试通过 | `src/consumer/message.rs` |
| Message Handler 实现 | ✅ 完成 | ✅ 全链路验证 | `src/consumer/message.rs` |
| Domain 集成 | ⚠️ 进行中 | 待接入真实 DAL | `src/service/domain/message/` |

---

## 📌 设计目标

消费者系统负责异步处理所有事件，是系统的核心后台处理引擎。设计目标：

1. **分层解耦**：通用消费者框架与具体 Topic 消费者分离
2. **可扩展**：新增 Topic 消费者只需实现 Trait，无需修改框架
3. **可测试**：Mock 友好，不依赖真实数据库即可测试框架逻辑
4. **高可用**：支持错误重试、失败回滚、并发控制
5. **可监控**：暴露状态接口，支持健康检查和统计

---

## 🏗️ 整体架构

```
┌─────────────────────────────────────────────────────────────┐
│                      GenericConsumer                         │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  Worker Pool (1-N concurrent workers)                  │  │
│  │                                                       │  │
│  │  fetch -> handle -> ack / nack -> repeat              │  │
│  └───────────────────────────────────────────────────────┘  │
│                                                               │
│  Trait Bounds:                                               │
│  - E: Event          (消息实体，如 Message)                 │
│  - F: Fetcher        (消息拉取器)                            │
│  - H: Handler        (消息处理器)                            │
└─────────────────────────────────────────────────────────────┘
                              ▲
                              │ 实现
                              │
┌─────────────────────────────────────────────────────────────┐
│                   Message Topic Consumer                    │
│                                                               │
│  MessageFetcherImpl  ──►  MessageDomain (dal)                │
│  MessageHandlerImpl  ──►  Brain / Gateway / System           │
└─────────────────────────────────────────────────────────────┘
```

---

## 📐 分层设计原则

### 1. 框架层 vs 业务层分离

| 层级 | 职责 | 文件 |
|---|---|---|
| **通用消费者框架** | 通用的 Worker 调度、空队列休眠、错误重试、并发控制 | `src/consumer/mod.rs` |
| **具体 Topic 消费者** | 针对特定 Topic 的消息拉取、业务处理逻辑 | `src/consumer/message.rs` |

**原则**：框架层不依赖任何具体业务逻辑，业务层只需要实现 Trait 即可接入。

---

### 2. Trait 抽象设计

```rust
/// 消息拉取器
#[async_trait]
pub trait MessageFetcher<E> {
    /// 拉取下一条消息
    async fn dequeue_next(&self) -> Result<Option<E>>;
    
    /// 确认消息处理完成
    async fn ack(&self, event_id: &str) -> Result<()>;
    
    /// 消息处理失败，放回队列重试
    async fn nack(&self, event_id: &str) -> Result<()>;
}

/// 消息处理器
#[async_trait]
pub trait MessageHandler<E> {
    /// 处理单条消息
    async fn handle(&self, message: &E) -> Result<()>;
}
```

**设计思考**：
- 将「拉取消息」和「处理消息」分离为两个独立 Trait
- 便于单元测试：可以 Mock Fetcher 返回特定消息，验证 Handler 逻辑
- 便于扩展：不同 Topic 可以复用相同 Fetcher 或 Handler

---

## 🎯 Message Topic 分发设计

### 核心原则：按接收者角色分发

**消息发给谁，就由谁处理**。不按消息类型分发，避免逻辑重复。

| to_role | 处理模块 | 说明 |
|---|---|---|
| **Agent** | Brain / Agent 模块 | Agent 思考、生成回复、可能调用工具 |
| **User** | 消息网关 | 推送给前端用户展示（SSE/WebSocket） |
| **System** | 系统/工具模块 | 执行工具、系统级任务处理 |

---

### 分发逻辑实现

```rust
async fn handle(&self, message: &Message) -> Result<()> {
    tracing::debug!("received message: {:?} -> {:?}, type: {:?}", 
        message.from_role(), message.to_role(), message.message_type());

    // 第一层分发：根据 to_role 决定谁来处理
    match message.to_role() {
        MessageRole::Agent => {
            self.handle_agent_message(message).await?;
        }
        MessageRole::User => {
            self.handle_user_message(message).await?;
        }
        MessageRole::System => {
            self.handle_system_message(message).await?;
        }
        _ => {
            tracing::warn!("unknown message recipient: {:?}", message.to_role());
        }
    }

    Ok(())
}
```

---

### 典型消息流向示例

| 场景 | from_role | to_role | 处理逻辑 |
|---|---|---|---|
| 用户发消息给 Agent | User | Agent | Brain 思考、生成回复 |
| Agent 发文本回复给用户 | Agent | User | 网关推送，用户看到回复 |
| **Agent 调用工具** | Agent | **System** | 系统执行工具函数 |
| **工具结果通知 Agent** | System | **Agent** | Brain 拿到结果继续思考 |
| Agent 发工具过程给用户看 | Agent | User | 网关推送（仅 UI 展示） |
| 系统发通知给用户 | System | User | 网关推送系统通知 |

---

### ✅ 架构优势

1. **职责单一**：消息发给谁，就由谁处理，逻辑清晰
2. **无重复逻辑**：不需要并行判断多种条件，按角色分发即可
3. **扩展友好**：新增接收角色时，只需新增一个 `handle_*` 方法
4. **符合业务流**：工具调用是 Agent → System → Agent 的完整闭环

---

## 🧪 测试策略

### 测试文件组织原则

| 测试类型 | 文件位置 | 说明 |
|---|---|---|
| **通用框架测试** | `src/consumer/tests.rs` | `GenericConsumer` 通用逻辑，与具体 Topic 无关 |
| **具体 Topic 测试** | `src/consumer/{topic}_tests.rs` | 每个 Topic 独立测试文件 |

**为什么这样组织？**
- 通用框架测试只需要写一次，所有 Topic 消费者都受益
- 具体 Topic 测试可以针对性验证分发逻辑和业务处理
- 文件结构清晰，易于扩展新 Topic

---

### 通用框架测试覆盖

**6 个核心场景测试**：
1. 空队列行为 - 无消息时正确返回 `None`
2. 正常消费流程 - 拉取 → 处理 → ack 完整链路
3. 消费后移除 - 消息 ack 后不再出现在队列
4. 多条消息顺序 - FIFO 保证，优先级正确
5. 失败重试 - 处理失败 → nack → 可再次消费
6. 动态添加 - 消费过程中新增消息可被后续消费

**测试方式**：使用 MockFetcher 和 MockHandler，不依赖真实数据库。

---

### Message Topic 测试覆盖

**8 个分发场景测试**：
1. User → Agent 正确分发到 Agent Handler
2. Agent → User 正确分发到 User Handler
3. Agent → System 工具调用正确分发到 System Handler
4. System → Agent 工具结果正确分发到 Agent Handler
5. Agent Image → User 正确分发到 User Handler
6. Agent File → User 正确分发到 User Handler
7. System Notification → User 正确分发到 User Handler
8. 单例访问不 panic

---

## 🚀 启动流程

```rust
// 1. 主函数初始化所有消费者
pub async fn init_consumers(config: &AppConfig) -> Result<()> {
    // 初始化 Message 消费者
    message::init(&config.consumer.message).await?;
    
    // 未来新增其他 Topic 消费者...
    // tool_call::init(&config.consumer.tool_call).await?;
    // notification::init(&config.consumer.notification).await?;
    
    tracing::info!("all consumers initialized and started");
    Ok(())
}

// 2. Message 消费者内部初始化
pub async fn init(config: &TopicConsumerConfig) -> Result<()> {
    let fetcher = Arc::new(MessageFetcherImpl::new());
    let handler = Arc::new(MessageHandlerImpl::new());
    
    let consumer = GenericConsumer::new("message", config, fetcher, handler);
    let consumer = Arc::new(consumer);
    
    // 启动消费者后台任务
    let consumer_clone = consumer.clone();
    tokio::spawn(async move {
        consumer_clone.start().await;
    });
    
    // 保存单例
    MESSAGE_CONSUMER.set(consumer)
        .map_err(|_| format_err!("message consumer already initialized"))?;
    
    Ok(())
}
```

---

## ⚙️ 配置设计

```toml
[consumer.message]
concurrency = 1              # 并发 Worker 数量（默认 1，串行处理）
retry_max_attempts = 3       # 最大重试次数
retry_delay_ms = 1000        # 重试间隔
empty_queue_sleep_ms = 500   # 空队列休眠时间
error_retry_sleep_ms = 100   # 处理失败后重试前休眠
```

**配置设计思考**：
- **并发默认 1**：保证消息顺序处理，避免竞态条件
- **可配置扩展**：对于无顺序要求的 Topic，可以提高并发
- **分级休眠**：空队列休眠 > 错误重试休眠，避免 CPU 空转

---

## 🔮 未来扩展方向

### 1. 新增 Topic 消费者

只需三步：
1. 创建 `src/consumer/{topic}.rs`
2. 实现 `MessageFetcher` 和 `MessageHandler` Trait
3. 在 `init_consumers()` 中添加初始化代码

示例：工具调用消费者
```rust
// src/consumer/tool_call.rs
pub struct ToolCallFetcherImpl;
pub struct ToolCallHandlerImpl;

#[async_trait]
impl MessageFetcher<ToolCallEvent> for ToolCallFetcherImpl {
    // ...
}

#[async_trait]
impl MessageHandler<ToolCallEvent> for ToolCallHandlerImpl {
    // ...
}
```

---

### 2. 统计与监控

为 GenericConsumer 添加统计指标：
```rust
pub struct ConsumerMetrics {
    pub total_processed: AtomicU64,
    pub total_failed: AtomicU64,
    pub queue_depth: AtomicU64,
    pub avg_process_time_ms: AtomicU64,
}
```

暴露健康检查接口：
```rust
impl<E, F, H> GenericConsumer<E, F, H> {
    pub fn health_check(&self) -> HealthStatus {
        // ...
    }
    
    pub fn metrics(&self) -> ConsumerMetricsSnapshot {
        // ...
    }
}
```

---

### 3. 死信队列（DLQ）

对于超过重试次数的消息，移入死信队列：
```rust
if attempt >= config.retry_max_attempts {
    self.move_to_dlq(event_id).await?;
    return;
}
```

---

## 📝 关键决策记录

### 决策 1：按 `to_role` 分发，不按 `message_type` 分发

**背景**：最初设计按 `message_type` 分发，但发现：
- 不同类型的消息可能都需要推送给用户（Text/Image/File...）
- 工具调用消息可能需要同时发给 System 和 User 展示
- 逻辑重复，难以维护

**决策**：改为按 `to_role` 分发，消息发给谁就由谁处理。

**结果**：架构更清晰，逻辑无重复，易于扩展。

---

### 决策 2：测试文件按 Topic 分离，通用测试独立

**背景**：最初所有测试放在一个 `tests.rs` 文件中。

**决策**：通用框架测试放在 `tests.rs`，每个具体 Topic 的测试放在独立的 `{topic}_tests.rs`。

**结果**：文件结构清晰，扩展新 Topic 时不需要修改通用测试，职责分离。

---

### 决策 3：单例模式 + OnceLock

**背景**：消费者需要在整个应用生命周期内只初始化一次，并且需要能被监控接口访问。

**决策**：使用 `std::sync::OnceLock` 实现单例，暴露 `get_consumer()` 公共方法。

**结果**：线程安全的单例，支持监控和健康检查。

---

## 📂 文件索引

| 文件 | 说明 |
|---|---|
| `src/consumer/mod.rs` | 通用消费者框架实现 |
| `src/consumer/tests.rs` | 通用消费者框架测试 |
| `src/consumer/message.rs` | Message Topic 消费者实现 |
| `src/consumer/message_tests.rs` | Message Topic 消费者测试 |
| `docs/LAYERED_ARCHITECTURE_PRACTICE.md` | 分层架构实践文档 |
| `docs/message_interaction_design.md` | 消息交互设计文档 |

---

## ✨ 设计总结

消费者架构的核心设计哲学：

1. **分层解耦**：框架与业务分离，Trait 驱动设计
2. **职责单一**：按接收者角色分发，逻辑清晰无重复
3. **测试友好**：Mock 友好，不依赖数据库即可测试框架
4. **扩展友好**：新增 Topic 只需实现 Trait，无需修改框架
5. **文档先行**：设计决策沉淀为文档，便于传承和回顾

---

## 🌐 消息网关架构设计

### 核心理念

> **消息网关 = 外部消息 handlers + message domain**

所有外部消息源（飞书 Webhook、微信 Webhook、Slack 等）都通过独立的 handler 接入，最终全部汇聚到同一个 `message domain` 处理。

### 分层架构图

```
┌─────────────────────────────────────────────────────────────┐
│                     外部消息 Handlers                        │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐    │
│  │ Webhook  │  │ Webhook  │  │ Webhook  │  │ Webhook  │    │
│  │  飞书    │  │  微信    │  │  Slack   │  │  ...     │    │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘    │
└───────┼──────────────┼──────────────┼──────────────┼────────┘
        │              │              │              │
        └──────────────┴──────┬───────┴──────────────┘
                               ▼
                    ┌────────────────────┐
                    │   Message Domain   │  ← 核心业务逻辑唯一入口
                    └────────┬───────────┘
                             │
        ┌────────────────────┼────────────────────┐
        ▼                    ▼                    ▼
┌───────────────┐    ┌───────────────┐    ┌───────────────┐
│ Message DAL   │    │  Lark DAL     │    │  Wechat DAL   │
└───────┬───────┘    └───────┬───────┘    └───────┬───────┘
        │                    │                    │
        ▼                    ▼                    ▼
┌───────────────┐    ┌───────────────┐    ┌───────────────┐
│ Message DAO   │    │   Lark DAO    │    │  Wechat DAO   │
│  (SQLite)     │    │ (三方 API)    │    │ (三方 API)    │
└───────────────┘    └───────────────┘    └───────────────┘
```

### 发给用户消息的处理流程

#### 当前实现（拉取模式）

```rust
// MessageHandlerImpl::handle_user_message
async fn handle_user_message(&self, message: &MessagePo) -> Result<()> {
    tracing::debug!("message delivered to user: {}", message.id);
    
    // 直接调用 DAL 更新状态，不需要额外 domain 方法
    self.message_dal
        .update_status(ctx, &message.id, MessageStatus::Completed)
        .await?;
    
    Ok(())
}
```

#### 未来扩展（多渠道推送，零侵入）

**步骤 1：新增 DAO 层（纯三方 API 调用）**
```rust
// src/dao/lark/mod.rs
pub trait LarkDao {
    async fn send_message(&self, webhook: &str, content: &str) -> Result<()>;
}
```

**步骤 2：新增 DAL 层（组合 DAO）**
```rust
// src/dal/lark_message.rs
pub struct LarkMessageDalImpl {
    message_dal: Arc<dyn MessageDal>,
    lark_dao: Arc<dyn LarkDao>,
    message_channel_dal: Arc<dyn MessageChannelDal>,
}

#[async_trait]
impl LarkMessageDal for LarkMessageDalImpl {
    async fn deliver_to_lark(&self, ctx: RequestContext, msg: &MessagePo) -> Result<()> {
        // 查询用户绑定的飞书渠道
        if let Some(channel) = self.message_channel_dal
            .get_user_channel(ctx, msg.to_id(), ChannelType::Lark)
            .await?
        {
            self.lark_dao.send_message(&channel.webhook_url, &msg.content).await?;
        }
        
        Ok(())
    }
}
```

**步骤 3：Domain 层组合 DAL（修改 handle_user_message）**
```rust
// src/domain/message/process_message.rs
async fn handle_user_message(&self, ctx: RequestContext, msg: &MessagePo) -> Result<()> {
    // 基础投递：更新状态
    self.message_dal
        .update_status(ctx, &msg.id, MessageStatus::Completed)
        .await?;
    
    // 扩展渠道：飞书推送
    if let Some(lark_dal) = &self.lark_message_dal {
        lark_dal.deliver_to_lark(ctx, msg).await?;
    }
    
    // 扩展渠道：微信推送
    // if let Some(wechat_dal) = &self.wechat_message_dal { ... }
    
    Ok(())
}
```

### 架构优势

1. **✅ 零侵入扩展**：新增推送渠道完全不修改现有代码，只是新增 DAO + DAL
2. **✅ 职责单一**：DAO 只做 API 调用，DAL 负责组合，Domain 编排业务
3. **✅ 复用最大化**：所有渠道共享同一个 message domain 核心逻辑
4. **✅ 测试友好**：每个 DAO/DAL 可以独立 Mock 测试
5. **✅ 符合分层原则**：严格单向依赖，handler → domain → dal → dao

---

## 🤖 Agent 思考循环架构（2026-05-11 更新）

### 思考循环在消费者框架中的位置

Agent 思考循环是消费者框架的**核心业务逻辑**，但不直接在消费者主线程中执行，而是以**独立异步任务**的形式派发。

```
消息入队 (enqueue_message)
    ↓
消费者主线程 (message_channel_dal.start())
    ↓
按 to_role 分发 (match msg.to_role)
    ├── User   → handle_user_message (TODO)
    ├── Agent  → handle_agent_message → 派发思考循环异步任务 ✨
    └── System → handle_system_message → 执行工具调用 ✨
```

### 为什么采用独立异步任务

| 考量 | 结论 |
|------|------|
| **阻塞问题** | 单条消息 LLM 推理可能耗时 5-30 秒，阻塞整个消费队列 |
| **并发能力** | 独立任务允许同时处理多个 Agent 的思考请求 |
| **可扩展性** | 后续支持分布式执行时改动最小 |
| **容错隔离** | 单条消息失败不影响其他消息处理 |

### handle_agent_message 执行流程

```
收到 Agent 角色消息
    ↓
1. 校验：确认消息属于某个 Project 上下文
2. 校验：确认 thinking_depth < agent.max_thinking_depth（默认 10）
    ↓
3. 派发独立异步任务：tokio::spawn(async move { ... })
    │
    └─── 任务内部执行 ───┐
                        ↓
                从 Project Domain 获取上下文
                查询最近 N 条消息历史
                组装 Prompt
                调用 CortexDao 完成 LLM 推理
                解析输出格式 (reply/tool/confirm)
                根据类型发回对应角色的消息
                ↓
                reply → 发回 User 角色
                tool  → 发 ToolCallRequest 给 System 角色
                confirm → 发 ConfirmRequest 给 User 角色
                        ↓
                新消息入队 → 触发下一轮消费循环
```

### handle_system_message 执行流程

System 角色专门负责**工具执行**，不进行 LLM 推理：

```
收到 System 角色消息
    ↓
匹配 MessageType:
    ├── ToolCallRequest → 编排 Manual 工具调用
    │       ↓
    │   解析 ToolCallRequest 消息内容
    │   构造 RequestContext(agent_id/project_id/task_id)
    │   调用 Runtime Domain: call_manual_tool_for_agent(ctx, agent_id, tool_id, args)
    │   Runtime Domain 负责 Agent 绑定授权、Manual 校验与 Builtin/HTTP/MCP 协议路由
    │   成功返回 ToolExecutionResult(result + trace_ref)，失败返回已脱敏错误
    │   调用 Message Domain: send_tool_call_result(...)
    │   Message Domain 负责 ToolCallResult 消息形状、不复制 request args、大结果 inline bound 与 trace_ref 透传
    │   发回给 Agent 角色
    │
    └── 其他类型 → 记录日志后丢弃（System 不处理非工具消息）
```

### 深度计数与反思机制

**思考深度计数规则**：
- 每次进入 `handle_agent_message`，深度 +1
- 用户发送的消息重置深度为 0
- 深度超过 `max_thinking_depth`（默认 10）触发反思

**反思触发后的处理**：
```
深度超过阈值
    ↓
暂停思考循环
构造反思消息:
  "我已经进行了 10 轮思考，需要你的反馈：
   1. 是否继续深入思考？
   2. 是否需要我总结当前结论？
   3. 是否需要调用其他 Agent 协助？"
发回给用户，等待用户回复后重置深度
```

### 关键设计决策记录

| 决策 | 理由 | 替代方案 |
|------|------|----------|
| 思考循环放 Project Domain | 符合领域驱动，Project 是上下文容器 | 放 Message Domain |
| 消费者只做分发不做业务 | 单一职责，消费者是基础设施 | 消费者直接处理逻辑 |
| Tool 执行放 System 消费者 | 工具执行是系统能力，非 Agent 自主 | Agent 直接调用工具 |
| 深度由消息计数 | 简单可靠，无需额外状态存储 | 独立深度计数器 |

---

这是一个经过深思熟虑的架构，能够支撑系统未来很长一段时间的发展！
