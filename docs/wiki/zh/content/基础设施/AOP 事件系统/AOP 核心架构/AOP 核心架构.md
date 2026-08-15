# AOP 核心架构

<cite>
**本文引用的文件**
- [src/pkg/aop/mod.rs](src/pkg/aop/mod.rs)
- [src/pkg/aop/core/mod.rs](src/pkg/aop/core/mod.rs)
- [src/pkg/aop/core/event.rs](src/pkg/aop/core/event.rs)
- [src/pkg/aop/core/producer.rs](src/pkg/aop/core/producer.rs)
- [src/pkg/aop/core/consumer.rs](src/pkg/aop/core/consumer.rs)
- [src/pkg/aop/core/registry.rs](src/pkg/aop/core/registry.rs)
- [src/pkg/aop/queue/mod.rs](src/pkg/aop/queue/mod.rs)
- [src/pkg/aop/queue/in_memory.rs](src/pkg/aop/queue/in_memory.rs)
- [src/models/events/mod.rs](src/models/events/mod.rs)
- [src/models/events/message.rs](src/models/events/message.rs)
- [src/models/events/task_status.rs](src/models/events/task_status.rs)
- [src/consumer/mod.rs](src/consumer/mod.rs)
</cite>

## 目录
1. [简介](#简介)
2. [项目结构](#项目结构)
3. [核心组件](#核心组件)
4. [架构总览](#架构总览)
5. [详细组件分析](#详细组件分析)
6. [依赖关系分析](#依赖关系分析)
7. [性能考量](#性能考量)
8. [故障排查指南](#故障排查指南)
9. [结论](#结论)
10. [附录：事件定义与使用示例](#附录：事件定义与使用示例)

## 简介
本文件围绕 AOP 事件系统的核心架构进行系统化说明，覆盖事件中心设计理念、Event 抽象接口、Producer 生产者模式、Consumer 消费者 trait、Registry 注册中心、队列实现、事件生命周期管理、异步处理机制、错误处理与性能优化策略。同时给出事件定义规范、注册流程、发布消费模式的完整示例路径，并解释与其他模块的集成方式与扩展点设计。

## 项目结构
AOP 事件系统位于 src/pkg/aop 下，采用“纯框架、无业务感知”的设计原则：
- core：定义 Event、Consumer、Producer、Registry、Scheduler 等核心抽象与调度逻辑
- queue：底层队列抽象与内存实现（InMemoryEventQueue）
- mod：暴露全局 Registry 单例与便捷 publish/init_all API
- models/events：领域事件定义（如消息创建、任务状态变更等），由业务层实现 Event trait
- consumer：业务消费者注册入口，统一在 init 中完成注册

```mermaid
graph TB
subgraph "AOP 框架"
MOD["aop/mod.rs"]
CORE_MOD["core/mod.rs"]
REGISTRY["core/registry.rs"]
CONSUMER_IF["core/consumer.rs"]
PRODUCER_IF["core/producer.rs"]
EVENT_IF["core/event.rs"]
QUEUE_IF["queue/mod.rs"]
QUEUE_IMPL["queue/in_memory.rs"]
end
subgraph "业务事件"
EVENTS_MOD["models/events/mod.rs"]
MSG_EVT["models/events/message.rs"]
TASK_EVT["models/events/task_status.rs"]
end
subgraph "业务消费者"
CONSUMER_INIT["consumer/mod.rs"]
end
MOD --> CORE_MOD
CORE_MOD --> REGISTRY
CORE_MOD --> CONSUMER_IF
CORE_MOD --> PRODUCER_IF
CORE_MOD --> EVENT_IF
CORE_MOD --> QUEUE_IF
QUEUE_IF --> QUEUE_IMPL
EVENTS_MOD --> MSG_EVT
EVENTS_MOD --> TASK_EVT
CONSUMER_INIT --> REGISTRY
MSG_EVT --> REGISTRY
TASK_EVT --> REGISTRY
```

**图示来源**
- [src/pkg/aop/mod.rs:1-61](src/pkg/aop/mod.rs#L1-L61)
- [src/pkg/aop/core/mod.rs:1-14](src/pkg/aop/core/mod.rs#L1-L14)
- [src/pkg/aop/core/registry.rs:1-561](src/pkg/aop/core/registry.rs#L1-L561)
- [src/pkg/aop/core/consumer.rs:1-72](src/pkg/aop/core/consumer.rs#L1-L72)
- [src/pkg/aop/core/producer.rs:1-36](src/pkg/aop/core/producer.rs#L1-L36)
- [src/pkg/aop/core/event.rs:1-25](src/pkg/aop/core/event.rs#L1-L25)
- [src/pkg/aop/queue/mod.rs:1-107](src/pkg/aop/queue/mod.rs#L1-L107)
- [src/pkg/aop/queue/in_memory.rs:1-449](src/pkg/aop/queue/in_memory.rs#L1-L449)
- [src/models/events/mod.rs:1-21](src/models/events/mod.rs#L1-L21)
- [src/models/events/message.rs:1-53](src/models/events/message.rs#L1-L53)
- [src/models/events/task_status.rs:1-66](src/models/events/task_status.rs#L1-L66)
- [src/consumer/mod.rs:1-43](src/consumer/mod.rs#L1-L43)

**章节来源**
- [src/pkg/aop/mod.rs:1-61](src/pkg/aop/mod.rs#L1-L61)
- [src/pkg/aop/core/mod.rs:1-14](src/pkg/aop/core/mod.rs#L1-L14)

## 核心组件
- Event 抽象：统一事件数据结构与元信息（kind/id/order_key/priority/created_at），所有领域事件需实现该 trait。
- Consumer 消费者：支持同步/异步两种消费模式；异步模式下具备 ack/nack、并发 worker、空队列休眠、错误重试休眠等能力。
- Producer 生产者：封装外部数据源或定时轮询的生产者，支持 start/stop/poll 生命周期管理。
- Registry 注册中心：维护消费者与生产者集合，负责事件分发、队列路由、工作协程启动、指标埋点注入。
- Queue 队列：抽象出 enqueue/dequeue/ack/nack/stats/query 等能力，当前提供 InMemoryEventQueue 实现，支持优先级、order_key 顺序、in_progress 跟踪与统计查询。

**章节来源**
- [src/pkg/aop/core/event.rs:1-25](src/pkg/aop/core/event.rs#L1-L25)
- [src/pkg/aop/core/consumer.rs:1-72](src/pkg/aop/core/consumer.rs#L1-L72)
- [src/pkg/aop/core/producer.rs:1-36](src/pkg/aop/core/producer.rs#L1-L36)
- [src/pkg/aop/core/registry.rs:1-561](src/pkg/aop/core/registry.rs#L1-L561)
- [src/pkg/aop/queue/mod.rs:1-107](src/pkg/aop/queue/mod.rs#L1-L107)
- [src/pkg/aop/queue/in_memory.rs:1-449](src/pkg/aop/queue/in_memory.rs#L1-L449)

## 架构总览
AOP 事件系统以 Registry 为核心，将事件从 Producer 或业务调用方发布后，按 EventKind 路由到对应 Consumer。同步模式直接在发布线程执行 on_event；异步模式入队并由独立 worker 拉取处理，支持 ack/nack 与重试退避。队列层通过 order_key 保证同 key 的顺序性，并通过优先级与创建时间决定出队顺序。

```mermaid
sequenceDiagram
participant Caller as "调用方/业务层"
participant Reg as "Registry"
participant Q as "EventQueue(内存)"
participant C as "Consumer(业务)"
participant Hook as "指标Hook"
Caller->>Reg : publish(Event)
Reg->>Reg : 序列化并注入元字段(kind/id/order_key/priority/created_at)
alt 同步模式
Reg->>C : on_event(event_json)
C-->>Reg : Ok/Err
Reg->>Hook : on_consume_success/failure(可选)
else 异步模式
Reg->>Q : enqueue(event_json)
Note over Q : 按优先级+创建时间排序<br/>order_key 单独队列
loop Worker 循环
Q-->>Reg : dequeue_next()
Reg->>C : on_event(event_json)
alt 成功
C-->>Reg : Ok
Reg->>Q : ack(event_id)
Reg->>Hook : on_consume_success
else 失败
C-->>Reg : Err
Reg->>Q : nack(event_id)
Reg->>Hook : on_consume_failure
Reg->>Reg : sleep(error_retry_sleep_ms)
end
end
end
```

**图示来源**
- [src/pkg/aop/core/registry.rs:97-206](src/pkg/aop/core/registry.rs#L97-L206)
- [src/pkg/aop/core/registry.rs:260-487](src/pkg/aop/core/registry.rs#L260-L487)
- [src/pkg/aop/queue/in_memory.rs:104-267](src/pkg/aop/queue/in_memory.rs#L104-L267)

## 详细组件分析

### Event 抽象接口
- kind：事件类型标识，用于路由到对应消费者
- id：唯一事件 ID，用于 ack/nack 与去重
- order_key：顺序键，相同 key 的事件串行处理
- priority：优先级，越大越优先
- created_at：创建时间，用于排序与监控

典型实现：
- MessageCreatedEvent：根据接收者角色选择 order_key（Agent 用 to_id，非 Agent 用 task_id→project_id 降级）
- TaskStatusChangedEvent：以 task_id 作为 order_key，确保同一任务的状态变更有序

**章节来源**
- [src/pkg/aop/core/event.rs:1-25](src/pkg/aop/core/event.rs#L1-L25)
- [src/models/events/message.rs:1-53](src/models/events/message.rs#L1-L53)
- [src/models/events/task_status.rs:1-66](src/models/events/task_status.rs#L1-L66)

### Producer 生产者模式
- name：生产者名称
- register：注册时获取 Registry 引用，以便后续发布事件
- start/stop：非轮询模式的生命周期管理
- poll_interval_secs：轮询间隔（秒），>0 表示由 Registry 定时调用 poll
- poll：一次生产逻辑，通常读取外部数据并发布事件

**章节来源**
- [src/pkg/aop/core/producer.rs:1-36](src/pkg/aop/core/producer.rs#L1-L36)
- [src/pkg/aop/core/registry.rs:75-95](src/pkg/aop/core/registry.rs#L75-L95)
- [src/pkg/aop/core/registry.rs:451-487](src/pkg/aop/core/registry.rs#L451-L487)

### Consumer 消费者 trait
- name：消费者名称（全局唯一）
- interested_events：感兴趣的事件类型列表
- should_consume：事件过滤（默认全部通过）
- consume_mode：同步/异步模式
- on_event：核心处理逻辑
- ack/nack：异步模式下的确认与重试标记
- concurrency：并发 worker 数量（仅异步生效）
- empty_queue_sleep_ms/error_retry_sleep_ms：轮询节奏控制

**章节来源**
- [src/pkg/aop/core/consumer.rs:1-72](src/pkg/aop/core/consumer.rs#L1-L72)

### Registry 注册中心
职责：
- 注册消费者与生产者
- 发布事件：序列化并注入元字段，按消费者模式分发
- 启动调度器：为每个异步消费者启动指定数量的 worker，并为轮询生产者启动定时器
- 指标采集：通过可插拔 Hook 记录发布、消费开始/成功/失败
- 队列管理：为异步消费者分配独立队列，提供 dequeue/ack/nack/stats/query

关键流程：
- publish：提取元字段 → 序列化 → 注入元字段 → 同步直接调用 or 异步入队
- start_all：原子标记 started → 收集异步消费者 → 启动 worker 循环 → 启动生产者（轮询或非轮询）

**章节来源**
- [src/pkg/aop/core/registry.rs:1-561](src/pkg/aop/core/registry.rs#L1-L561)

### 队列实现（InMemoryEventQueue）
数据结构：
- events：事件内容映射（event_id → json）
- queues：按 order_key 划分的 BinaryHeap（优先级队列）
- global_heap：全局优先级堆（无 order_key 或各 order_key 的活跃头）
- in_progress：正在处理的事件（event_id → (ref, order_key)）
- has_active_message：标记某 order_key 是否有活跃消息

算法要点：
- enqueue：去重 → 插入 events → 若 order_key 为空则入 global_heap；否则入对应 order_key 队列，若队列为空且无活跃消息则将队首推入 global_heap
- dequeue_next：从 global_heap 弹出，放入 in_progress 并返回
- ack：从 in_progress 移除，删除 events；若有下一个元素则推回 global_heap 并更新 has_active_message
- nack：从 in_progress 移除并重新入 global_heap，保持 has_active_message=true

统计与查询：
- stats：pending_count、in_progress_count、order_keys 分布、最老事件年龄
- query_events/get_event：支持分页、过滤、脱敏预览

**章节来源**
- [src/pkg/aop/queue/mod.rs:1-107](src/pkg/aop/queue/mod.rs#L1-L107)
- [src/pkg/aop/queue/in_memory.rs:1-449](src/pkg/aop/queue/in_memory.rs#L1-L449)

### 事件生命周期管理
- 发布阶段：Registry.publish 序列化并注入元字段，记录 on_publish 指标
- 消费阶段：
  - 同步：on_event 直接执行，记录耗时与结果
  - 异步：worker 循环 dequeue → on_event → ack/nack → 队列状态更新
- 重试与退避：on_event 失败时调用 nack 并 sleep(error_retry_sleep_ms)，避免紧密自旋
- 顺序保证：order_key 相同的消息串行处理，通过 has_active_message 与队列头管理

**章节来源**
- [src/pkg/aop/core/registry.rs:97-206](src/pkg/aop/core/registry.rs#L97-L206)
- [src/pkg/aop/core/registry.rs:260-487](src/pkg/aop/core/registry.rs#L260-L487)
- [src/pkg/aop/queue/in_memory.rs:104-267](src/pkg/aop/queue/in_memory.rs#L104-L267)

### 异步处理机制
- 每个异步消费者拥有独立队列与多个 worker 协程
- worker 循环：dequeue_for → on_event → ack/nack → sleep（空队列或错误）
- 并发度由 consumer.concurrency() 控制
- 轮询生产者：poll_interval_secs > 0 时由 Registry 定时触发 poll

**章节来源**
- [src/pkg/aop/core/registry.rs:260-487](src/pkg/aop/core/registry.rs#L260-L487)
- [src/pkg/aop/core/consumer.rs:1-72](src/pkg/aop/core/consumer.rs#L1-L72)

### 错误处理
- 序列化失败：记录错误并跳过该事件
- 消费者同步错误：记录错误并上报指标
- 异步 on_event 失败：nack 并 sleep，避免 CPU 自旋
- 队列操作失败：记录错误但不中断主流程
- ack/nack 失败：记录错误，但继续推进（避免阻塞）

**章节来源**
- [src/pkg/aop/core/registry.rs:97-206](src/pkg/aop/core/registry.rs#L97-L206)
- [src/pkg/aop/core/registry.rs:260-487](src/pkg/aop/core/registry.rs#L260-L487)

### 性能优化策略
- 优先级 + 创建时间排序：BinaryHeap 保证高优先级先出，同优先级先进先出
- order_key 顺序控制：减少跨 worker 竞争，降低 busy 状态重试开销
- 去重：events map 防止重复事件入队
- 最小锁粒度：队列内部使用 Mutex 保护关键区，尽量缩短持锁时间
- 退避策略：error_retry_sleep_ms 与 empty_queue_sleep_ms 避免忙等
- 指标零开销：未注入 Hook 时不产生额外开销

**章节来源**
- [src/pkg/aop/queue/in_memory.rs:104-267](src/pkg/aop/queue/in_memory.rs#L104-L267)
- [src/pkg/aop/core/registry.rs:260-487](src/pkg/aop/core/registry.rs#L260-L487)

## 依赖关系分析
- Registry 依赖 Consumer、Producer、EventQueue、RequestContext、AopMetricsHook
- InMemoryEventQueue 依赖 RequestContext、serde_json、标准库容器
- 业务事件（MessageCreatedEvent、TaskStatusChangedEvent）依赖 Event trait 与公共枚举
- consumer::init 统一注册业务消费者到 Registry

```mermaid
classDiagram
class Registry {
+register_consumer(consumer)
+register_producer(producer)
+publish(event)
+start_all()
+dequeue_for(name)
+ack(name, event_id)
+nack(name, event_id)
+stats()
}
class Consumer {
<<trait>>
+name()
+interested_events()
+should_consume(event)
+consume_mode()
+on_event(event)
+ack(event_id)
+nack(event_id)
+concurrency()
+empty_queue_sleep_ms()
+error_retry_sleep_ms()
}
class Producer {
<<trait>>
+name()
+register(registry)
+start()
+stop()
+poll_interval_secs()
+poll()
}
class Event {
<<trait>>
+kind()
+id()
+order_key()
+priority()
+created_at()
}
class EventQueue {
<<trait>>
+enqueue(ctx, event)
+dequeue_next(ctx)
+ack(ctx, event_id)
+nack(ctx, event_id)
+stats()
+query_events(filter)
+get_event(event_id)
}
class InMemoryEventQueue {
+enqueue(...)
+dequeue_next(...)
+ack(...)
+nack(...)
+stats()
+query_events(...)
+get_event(...)
}
Registry --> Consumer : "管理"
Registry --> Producer : "管理"
Registry --> EventQueue : "使用"
InMemoryEventQueue ..|> EventQueue
```

**图示来源**
- [src/pkg/aop/core/registry.rs:1-561](src/pkg/aop/core/registry.rs#L1-L561)
- [src/pkg/aop/core/consumer.rs:1-72](src/pkg/aop/core/consumer.rs#L1-L72)
- [src/pkg/aop/core/producer.rs:1-36](src/pkg/aop/core/producer.rs#L1-L36)
- [src/pkg/aop/core/event.rs:1-25](src/pkg/aop/core/event.rs#L1-L25)
- [src/pkg/aop/queue/mod.rs:1-107](src/pkg/aop/queue/mod.rs#L1-L107)
- [src/pkg/aop/queue/in_memory.rs:1-449](src/pkg/aop/queue/in_memory.rs#L1-L449)

**章节来源**
- [src/pkg/aop/core/registry.rs:1-561](src/pkg/aop/core/registry.rs#L1-L561)
- [src/pkg/aop/queue/mod.rs:1-107](src/pkg/aop/queue/mod.rs#L1-L107)

## 性能考量
- 队列复杂度：入队 O(log n)（BinaryHeap），出队 O(log n)，查询 O(n)（受分页限制）
- 锁竞争：队列内部使用单一 Mutex，建议合理设置 concurrency 避免过多 worker 争抢
- 内存占用：events map 存储完整 JSON，注意大事件体对内存的影响
- 顺序与并行：order_key 串行化可能成为瓶颈，应合理拆分 order_key 粒度
- 指标开销：仅在注入 Hook 时产生，默认零开销

[本节为通用性能讨论，不直接分析具体文件]

## 故障排查指南
常见问题与定位：
- 事件未消费：检查消费者是否注册、interested_events 是否匹配、consume_mode 是否正确
- 顺序错乱：确认 order_key 设置是否符合预期（如 MessageCreatedEvent 的 Agent 维度串行）
- 队列堆积：查看 queue.stats 中的 pending_count 与 oldest_event_age_secs，调整 concurrency 或优化 on_event 耗时
- 频繁重试：关注 error_retry_sleep_ms 与 on_event 错误日志，定位下游依赖问题
- 死锁风险：Registry.start_all 已避免长持锁，确保消费者 on_event 不长时间持有外部锁

**章节来源**
- [src/pkg/aop/core/registry.rs:260-487](src/pkg/aop/core/registry.rs#L260-L487)
- [src/pkg/aop/queue/in_memory.rs:300-449](src/pkg/aop/queue/in_memory.rs#L300-L449)

## 结论
AOP 事件系统以 Registry 为中心，结合 Event/Consumer/Producer/Queue 抽象，提供了轻量、可扩展、可观测的事件分发与调度能力。通过 order_key 与优先级保障顺序与时效，通过 ack/nack 与退避策略提升可靠性，通过指标 Hook 实现可观测性。业务层只需实现事件与消费者，即可无缝接入。

[本节为总结性内容，不直接分析具体文件]

## 附录：事件定义与使用示例

### 事件定义规范
- 实现 Event trait：提供 kind/id/order_key/priority/created_at
- 推荐为每个事件定义独立的 struct，便于序列化与反序列化
- order_key 设计应遵循业务语义（如任务级、会话级、Agent 级）

参考实现：
- MessageCreatedEvent：按接收者角色选择 order_key
- TaskStatusChangedEvent：以 task_id 作为 order_key

**章节来源**
- [src/models/events/message.rs:1-53](src/models/events/message.rs#L1-L53)
- [src/models/events/task_status.rs:1-66](src/models/events/task_status.rs#L1-L66)
- [src/models/events/mod.rs:1-21](src/models/events/mod.rs#L1-L21)

### 注册流程
- 在 consumer::init 中注册所有业务消费者
- 通过 aop::registry().register_consumer 完成注册
- 启动时调用 aop::init_all 启动调度器与生产者

**章节来源**
- [src/consumer/mod.rs:1-43](src/consumer/mod.rs#L1-L43)
- [src/pkg/aop/mod.rs:1-61](src/pkg/aop/mod.rs#L1-L61)

### 发布消费模式示例
- 发布事件：调用 aop::publish(event)
- 同步消费：实现 Consumer 并返回 ConsumeMode::Sync
- 异步消费：实现 Consumer 并返回 ConsumeMode::Async，实现 ack/nack 与并发配置

**章节来源**
- [src/pkg/aop/mod.rs:1-61](src/pkg/aop/mod.rs#L1-L61)
- [src/pkg/aop/core/consumer.rs:1-72](src/pkg/aop/core/consumer.rs#L1-L72)

### 与其他模块的集成方式
- 业务 DAL/Domain：通过发布事件解耦副作用（如任务状态变更后通知 Owner Agent）
- 前端/HTTP：通过 Handler 触发 Domain 操作，Domain 发布事件，消费者异步处理
- 监控：注入 AopMetricsHook 采集发布与消费指标

**章节来源**
- [src/models/events/task_status.rs:1-66](src/models/events/task_status.rs#L1-L66)
- [src/pkg/aop/core/registry.rs:1-561](src/pkg/aop/core/registry.rs#L1-L561)

### 扩展点设计
- 新增事件：定义事件 struct 并实现 Event trait
- 新增消费者：实现 Consumer trait 并在 consumer::init 注册
- 新增生产者：实现 Producer trait 并注册，支持轮询或非轮询模式
- 自定义队列：实现 EventQueue trait 替换 InMemoryEventQueue（如持久化队列）

**章节来源**
- [src/pkg/aop/core/producer.rs:1-36](src/pkg/aop/core/producer.rs#L1-L36)
- [src/pkg/aop/queue/mod.rs:1-107](src/pkg/aop/queue/mod.rs#L1-L107)