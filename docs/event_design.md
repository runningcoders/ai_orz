# 事件总线设计

## 设计目标

ai_orz 项目需要一个**轻量级内存事件总线**，满足以下需求：

1. **消息持久化**：所有事件/消息先持久化到 SQLite `messages` 表，总线只存储元数据
2. **崩溃恢复**：服务重启后从数据库恢复未处理的事件
3. **优先级调度**：支持优先级排序，高优先级事件先处理
4. **顺序保证**：相同 `order_key` 的事件保证顺序处理，不同 `order_key` 可以并行处理
5. **内存占用小**：总线只存 `message_id` 元数据，不存完整消息内容

## 架构设计

### 数据层

#### 1. `messages` 表（持久化）

```sql
CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    message_type TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    created_by TEXT NOT NULL,
    modified_by TEXT NOT NULL
);
```

字段说明：
- `id`：UUID v7 生成，保证全局唯一
- `org_id`：组织 ID，多租户隔离
- `role`：消息角色（user/assistant/system）
- `content`：消息文本内容
- `message_type`：消息类型（普通消息/任务事件/系统事件）
- `status`：消息状态（pending/processing/completed/failed）
- `created_at/updated_at`：时间戳
- `created_by/modified_by`：创建/修改人用户 ID

#### 2. 附件存储

二进制附件（图片/文件等）不存入数据库，存储在文件系统：
```
data/artifact/YYYYMMDD/{random_uuid}.ext
```
- 按日期分层存储
- 数据库只存附件路径元数据

### 总线层

#### 事件 trait 定义

```rust
/// 通用事件 trait
pub trait Event: Send + Sync + std::fmt::Debug {
    /// 事件 ID，对应 messages 表 id
    fn id(&self) -> &str;

    /// 组织 ID，用于多租户隔离
    fn org_id(&self) -> &str;

    /// 优先级，越大越优先
    fn priority(&self) -> i32;

    /// 顺序键，相同 order_key 保证顺序处理
    ///
    ///  - `None`：不要求顺序，可以完全并行
    ///  - `Some(key)`：相同 key 顺序处理
    fn order_key(&self) -> Option<&str>;

    /// 执行事件处理
    async fn handle(self) -> Result<(), anyhow::Error>;
}
```

#### 内存事件队列设计

内存队列使用**二分插入 + Vec**维护有序队列：
- 按 `(priority DESC, created_at ASC)` 排序
- 出队从队首取，保证高优先级先出，同优先级先创建先出
- 相同 `order_key` 保证顺序出队

设计要点：
- `order_key = None` → 可以并行消费
- `order_key = Some(k)` → 同 key 顺序消费

#### 启动恢复逻辑

服务启动时：
1. 从 `messages` 表查询所有 `status = pending` 的消息
2. 按优先级和创建时间排序加入内存队列
3. 工作线程开始消费

### 并发模型

- 使用固定大小线程池（tokio 任务调度）
- 每个事件一个任务，异步执行
- 相同 `order_key` 保证顺序执行（使用 Tokio `Mutex` 锁住 key 顺序处理）

## 现有实现状态

| 模块 | 完成状态 |
|------|----------|
| `messages` 表 SQL 定义 | ✅ 完成 |
| `MessagePo` 实体定义 | ✅ 完成 |
| `MessageDao` 数据访问层 | ✅ 完成 |
| `Event` trait 定义 | ✅ 完成 |
| 内存事件队列实现 | ✅ 完成 |
| 启动恢复逻辑 | ✅ 完成 |
| 优先级排序 | ✅ 完成 |
| 同 order_key 顺序保证 | ✅ 完成 |
| 对接上层业务逻辑 | ⏱ 待完成 |

## 使用示例

```rust
// 创建一个事件
let event = MyEvent::new(...);

// 推入总线
event_queue.push(event).await?;

// 工作线程自动消费
```

## 设计决策

### Q: 为什么不用成熟的异步消息队列比如 Redis/RabbitMQ？
A: 当前项目是**单实例全栈部署**，不需要分布式消息队列，轻量内存队列足够满足需求，减少外部依赖，简化部署。

### Q: 为什么事件要持久化到数据库？
A: 保证**不丢消息**，服务崩溃重启后可以恢复未处理事件，不会丢失任务。

### Q: 为什么相同 order_key 要保证顺序？
A: Agent 对话场景需要顺序处理，同一个会话的消息不能乱序，满足对话场景需求。

### Q: 为什么优先级支持？
A: 不同类型任务优先级不同，比如用户交互优先级高于后台任务，保证响应用户请求更快。
