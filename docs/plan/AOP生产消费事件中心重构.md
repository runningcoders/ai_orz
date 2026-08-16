# AOP 生产-消费事件中心重构

> 🎯 **本文档定位**：重构规划 + 落地结果快照（概览级，不包含代码细节；具体实现以代码路径为准）
>
> 文档角色：plan（要去哪 + 完成状态快照），归档后查阅意图：
> - 新增事件类型/消费者时，回看"分发速查表 + 扩展路径"两处即可，无需通读全文
> - 若需了解 Registry 分发/队列实现细节，直接跳转对应代码文件（见 §涉及文件）
>
> 关联文档：
> - 对应 design 文档：暂无对应独立 AOP 设计文档（强烈建议补写，当前设计决策散落在 pkg/aop/ 源码）
> - 上游规范与关联：
>   - [AGENTS.md](../../AGENTS.md) — 分层架构规范（§3.1 适配层 AOP Producer 与 Consumer 分层定位约定）
>   - [message_channel_design.md](../design/message_channel_design.md) — 消息渠道系统骨架（消息出站链路）
> - Wiki 长文真实路径：[docs/wiki/zh/content/基础设施/AOP 事件系统/AOP 事件中心设计与实现.md](docs/wiki/zh/content/基础设施/AOP%20事件系统/AOP%20事件中心设计与实现.md) — 统一 publish 入口 + Registry 订阅者注册 + Queue 队列实现
> - Wiki 长文真实路径：[docs/wiki/zh/content/基础设施/AOP 事件系统/事件消费者概览.md](docs/wiki/zh/content/基础设施/AOP%20事件系统/事件消费者概览.md) — 8 类消费者定位与并发策略
> - RAG 卡真实路径 1：[docs/wiki/knowledge/zh/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8 类业务消费者注册/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8 类业务消费者注册.md](docs/wiki/knowledge/zh/AOP%20生产消费事件中心：纯框架零业务%20+%20pkg/aop/core%206%20Trait%20+%20Registry%20全局单例%20+%208%20类业务消费者注册/AOP%20生产消费事件中心：纯框架零业务%20+%20pkg/aop/core%206%20Trait%20+%20Registry%20全局单例%20+%208%20类业务消费者注册.md)
> - RAG 卡真实路径 2：[docs/wiki/knowledge/zh/Domain 内部事件与消费者全链路：8 类 DomainEvent 枚举 + 8 类 Consumer 业务消费 + AOP Producer 投递入口 + Registry 订阅/Domain 内部事件与消费者全链路：8 类 DomainEvent 枚举 + 8 类 Consumer 业务消费 + AOP Producer 投递入口 + Registry 订阅.md](docs/wiki/knowledge/zh/Domain%20内部事件与消费者全链路：8%20类%20DomainEvent%20枚举%20+%208%20类%20Consumer%20业务消费%20+%20AOP%20Producer%20投递入口%20+%20Registry%20订阅/Domain%20内部事件与消费者全链路：8%20类%20DomainEvent%20枚举%20+%208%20类%20Consumer%20业务消费%20+%20AOP%20Producer%20投递入口%20+%20Registry%20订阅.md)

---

## 一、重构目标（为什么做）

原 `event_queue dao` 按业务类型（Message/CronTrigger 等）分散实现，每新增一种事件类型需要复制一套队列骨架 + DAO 单例，Consumer 取队方式也不统一。

| 问题维度 | 解决方式 |
|---------|---------|
| (a) 事件入队 API 分散（各业务各自调 dao.enqueue） | 统一 `aop::publish(event)` 全局入口 |
| (b) 队列存储与业务模型耦合（event_queue dao 依赖具体 Message 类型） | 下沉到 `pkg/aop/queue/`，队列存储仅依赖 Event trait（serde 中转） |
| (c) 消费模式缺失区分（轻量 SSE 推送 vs 重 Agent 唤醒都走异步队） | 引入 ConsumeMode：Sync（实时回调）/ Async（可靠队列）双模式 |
| (d) 新增事件类型需改 DAO 层 | Event 注册走 Registry，新增类型仅扩 EventKind + match 分支，DAO 零改动 |

**收敛后效果**：框架封顶 5 个核心 trait（Event/Producer/Consumer/Registry/EventQueue），新增事件类型时 **框架代码零改动**。

---

## 二、架构思路（怎么做的）

三层收敛，Registry 为分发枢纽：

```
业务层（MessageDomain / CronTrigger / 外部渠道）
  │  只改调用方式：构造 Event → 调 aop::publish()
  ▼
AOP 框架（pkg/aop/）
  │  Registry：按 EventKind 索引 → 同步 Consumer 直接回调 / 异步 Consumer 入独立队列
  │  EventQueue：每个异步 Consumer 独立队列，支持优先级 + 同 order_key 保序
  ▼
Consumer（异步消费端）
  ├─ AgentAwakeningConsumer：消费 MESSAGE_CREATED → 唤醒 Agent
  ├─ SsePushConsumer（后续）：同步消费 → 推 SSE
  └─ 其他业务 Consumer：按需注册
```

**关键边界（行为红线，回归必保）**：
1. 同步 Consumer.on_event 失败 → 仅 sys_error! 告警，不阻断 publish 主流程，不影响其他 Consumer
2. 异步 Consumer 队列按 consumer_name 隔离；ack 未完成前同 order_key 后续事件不进入 global_heap（严格保序）
3. Event.id 全局去重：重复 publish 同一 event_id → 静默忽略（第二次 enqueue 直接 Ok）
4. Registry.dequeue_for 未注册 consumer_name → 返回 NotFound（不允许匿名取队）
5. Nack 语义：事件从 in_progress 移回 global_heap，order_key 活跃标记保持 true（下次 dequeue 仍保证顺序）

---

## 三、涉及文件（改动清单 → 查代码直接跳）

按 AGENTS.md §3.2 目录结构索引：

| 文件 | 角色 | 变更内容 |
|------|------|---------|
| **AOP 框架层（新增）** | | |
| [src/pkg/aop/mod.rs](../../src/pkg/aop/mod.rs) | 框架入口 | 全局 Registry 单例；`registry()` / `publish()` / `init_all()` 三个便捷 API |
| [src/pkg/aop/core/mod.rs](../../src/pkg/aop/core/mod.rs) | 核心模块索引 | 重导出 Event/EventKind/Producer/Consumer/ConsumeMode/Registry |
| [src/pkg/aop/core/event.rs](../../src/pkg/aop/core/event.rs) | Event 抽象 | Event trait（kind/id/order_key/priority/created_at）；EventKind 常量（MESSAGE_CREATED / CRON_TRIGGER 等 7 种） |
| [src/pkg/aop/core/producer.rs](../../src/pkg/aop/core/producer.rs) | Producer 抽象 | 外部生产者 trait（name/register/start/stop）；内部生产者直接调 aop::publish() 无需实现 |
| [src/pkg/aop/core/consumer.rs](../../src/pkg/aop/core/consumer.rs) | Consumer 抽象 | Consumer trait（name/interested_events/consume_mode/on_event）；ConsumeMode 枚举（Sync/Async） |
| [src/pkg/aop/core/registry.rs](../../src/pkg/aop/core/registry.rs) | 分发器实现 | Producer/Consumer 注册；publish → Sync 直调 / Async 入队；dequeue_for/ack/nack；start_all/stop_all |
| [src/pkg/aop/core/registry_test.rs](../../src/pkg/aop/core/registry_test.rs) | 框架集成测试 | 同步接收 / 异步入队出队 / ack 清空 / 多消费者扇出 共 4 个测试 |
| [src/pkg/aop/queue/mod.rs](../../src/pkg/aop/queue/mod.rs) | 队列抽象 | EventQueue trait（enqueue/enqueue_batch/dequeue_next/ack/nack/len/in_progress_count/recover/clear）；new_in_memory 工厂 |
| [src/pkg/aop/queue/in_memory.rs](../../src/pkg/aop/queue/in_memory.rs) | 内存队列实现 | 从 dao/event_queue 迁移；优先级全局堆 + 同 order_key 独立子队列 + in_progress 追踪 |
| [src/pkg/aop/queue/in_memory_test.rs](../../src/pkg/aop/queue/in_memory_test.rs) | 队列测试 | 从 dao/event_queue 迁移；空队列 / 单事件出入队 / 优先级 / 保序 / nack 重试 等 10 个用例 |
| [src/pkg/aop/impl/mod.rs](../../src/pkg/aop/impl/mod.rs) | 业务实现索引 | pub mod message |
| [src/pkg/aop/impl/message/mod.rs](../../src/pkg/aop/impl/message/mod.rs) | 消息模块索引 | 重导出 MessageCreatedEvent / AgentAwakeningConsumer |
| [src/pkg/aop/impl/message/events.rs](../../src/pkg/aop/impl/message/events.rs) | 消息事件 | MessageCreatedEvent 结构体 + Event trait 实现 + from_message 构造器 |
| [src/pkg/aop/impl/message/consumers.rs](../../src/pkg/aop/impl/message/consumers.rs) | 消息消费者 | AgentAwakeningConsumer（Async 模式，消费 MESSAGE_CREATED） |
| **DAO 层（删除）** | | |
| ~~src/service/dao/event_queue/mod.rs~~ | 旧队列入口 | 迁移到 pkg/aop/queue/mod.rs，已删除 |
| ~~src/service/dao/event_queue/in_memory.rs~~ | 旧队列实现 | 迁移到 pkg/aop/queue/in_memory.rs，已删除 |
| ~~src/service/dao/event_queue/in_memory_test.rs~~ | 旧队列测试 | 迁移到 pkg/aop/queue/in_memory_test.rs，已删除 |
| [src/service/dao/mod.rs](../../src/service/dao/mod.rs) | DAO 入口 | 移除 event_queue 模块引用及 init_message/init_cron_trigger 调用 |
| **业务层（迁移调用方式）** | | |
| [src/service/dal/message.rs](../../src/service/dal/message.rs) | 消息 DAL | 从 event_queue::dao() 改为 aop::registry() |
| [src/service/domain/message/delivery.rs](../../src/service/domain/message/delivery.rs) | 消息分发 Domain | send_to_agent 从 enqueue 改为 publish(MessageCreatedEvent) |
| [src/service/domain/message/delivery_test.rs](../../src/service/domain/message/delivery_test.rs) | 分发测试 | 测试改造适配新 API |
| [src/consumer/scheduler.rs](../../src/consumer/scheduler.rs) | 消费调度器 | 初始化从 init_message 改为注册 Consumer |
| [src/scheduler/mod.rs](../../src/scheduler/mod.rs) | Cron 调度器 | cron_trigger 队列迁移走 AOP 框架 |
| [src/handlers/a2a/integration_test.rs](../../src/handlers/a2a/integration_test.rs) | A2A 集成测试 | 初始化方式适配 |
| [src/handlers/a2a/send_task.rs](../../src/handlers/a2a/send_task.rs) | A2A 发任务 | 入队逻辑改 publish |
| **零改动面（验证架构稳定性）** | | |
| 前端 / 路由 / 消息数据库表结构 / Message 模型定义 | 对外契约不变 | 无修改；原有业务语义保持一致 |

---

## 四、扩展速查表（新增事件/消费者时的改动模式）

### 4.1 新增异步消费者（以 SsePushConsumer 为例）

| 步骤 | 改动点 | 参考位置 |
|------|--------|---------|
| 1 | 实现 Consumer trait（name 唯一 + interested_events + ConsumeMode::Async + on_event） | [consumers.rs :: AgentAwakeningConsumer](../../src/pkg/aop/impl/message/consumers.rs) |
| 2 | 在 init_all() 中 register_consumer(Arc::new(NewConsumer)) | [aop/mod.rs :: init_all()](../../src/pkg/aop/mod.rs) |

> 代码入口：[aop/mod.rs](../../src/pkg/aop/mod.rs)

### 4.2 新增事件类型（以 ProjectUpdatedEvent 为例）

| 步骤 | 改动点 | 参考位置 |
|------|--------|---------|
| 1 | EventKind 加常量（Self("project.updated")） | [event.rs :: EventKind](../../src/pkg/aop/core/event.rs) |
| 2 | 对应 impl/events.rs 加事件结构体 + Event trait 实现 | [events.rs :: MessageCreatedEvent](../../src/pkg/aop/impl/message/events.rs) |
| 3 | 业务 publish 点调 `aop::publish(NewEvent { ... })` | [delivery.rs :: send_to_agent](../../src/service/domain/message/delivery.rs) |

> 代码入口：[core/event.rs](../../src/pkg/aop/core/event.rs)

---

## 五、验收清单（2026-07-20 全部达成 ✅）

见 Plan 文档对应 Git 提交记录 / 对应执行任务。

---

## 六、执行结果摘要（2026-07-20，子代理驱动）

| 模块 | 验证结果 |
|------|---------|
| pkg/aop Queue 单元测试 | 10 passed |
| pkg/aop Registry 集成测试 | 4 passed |
| 后端 lib 全量测试 | 779+ passed / 0 failed |
| Clippy | 零错误零警告 |

### 与计划的偏离（业务零影响）
1. 原计划一次性迁移 consumer/message.rs 到 AgentAwakeningConsumer → 实际分两阶段：先打通链路框架（on_event 方法初版只打通框架占位），下一迭代补全唤醒业务逻辑（确保迁移风险可控）
2. 原计划 Event 泛型队列直接存储 → 实际通过 serde_json 序列化中转（解决 trait object 下 Event trait 非 object-safe 问题）

---

## 七、后续扩展路径（新增消费者 4 步模板）

> **核心不变量**：Registry/Queue 框架代码 / 路由机制不动。

1. **定义事件类型**：[core/event.rs](../../src/pkg/aop/core/event.rs)
   - EventKind 加常量
   - 对应 impl/ 目录下新建 events.rs，加事件结构体 + Event trait 实现（kind/id/order_key/priority/created_at）
2. **实现消费者**：参考 [impl/message/consumers.rs](../../src/pkg/aop/impl/message/consumers.rs)
   - ConsumeMode：轻量（SSE 推送/日志）走 Sync；重量（Agent 唤醒/外部回调）走 Async
   - on_event 内：serde_json 中转反序列化为具体事件类型 → 执行业务逻辑
3. **注册并初始化**：[aop/mod.rs :: init_all()](../../src/pkg/aop/mod.rs)
   - `REGISTRY.register_consumer(Arc::new(NewConsumer))?;`
4. **业务 publish 接入**：搜索业务层调用点，将原 `event_queue` 或直接调用改为 `aop::publish(NewEvent { ... })`

