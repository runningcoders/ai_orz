# 用户-Agent 消息交互设计

## 核心理念

系统以**组织**形式管理多个 Agent，Agent 承担不同角色，以**项目**方式组织协作：

- **组织** = 团队，包含多个 Agent 和多个项目
- **Agent** = 团队成员，每个 Agent 有不同角色和能力
  - 支持**前台 Agent**：负责接待用户输入，理解意图，调度其他 Agent
  - 支持**工作 Agent**：负责具体任务执行（开发、分析、测试等）
  - 用户可以**直接给任意 Agent 发消息**，不限制必须经过前台
- **项目** = 一件需要完成的大事，拆解为多个任务
- **任务** = 分配给特定 Agent 执行的具体工作

### 前台 Agent 决策流程

```
用户发消息 → 前台 Agent 接收
    ↓
根据意图判断：
  ├─ 简单问题 → 直接回答 → 返回用户
  ├─ 中等问题 → 发起任务 → 调度工作 Agent → 汇总结果 → 返回用户
  └─ 复杂问题 → 先创建项目 → 产出项目计划文档 → 请用户审阅 → 批准后再拉起任务
```

---

## 数据库设计变更

### 1. messages 表变更

```sql
-- 新增 project_id 字段（支持项目上下文）
-- task_id 允许 NULL（支持没有任务的闲聊）
ALTER TABLE messages ADD COLUMN project_id TEXT;
ALTER TABLE messages ALTER COLUMN task_id DROP NOT NULL;

-- 新增 reply_to_id 字段（支持消息回复链，引用回复）
-- SQLite 不支持 ALTER COLUMN DROP NOT NULL，开发阶段建议直接重建表
ALTER TABLE messages ADD COLUMN reply_to_id TEXT;
```

### 最新 messages 表完整结构

| 字段 | 类型 | 说明 |
|------|------|------|
| id | TEXT | 消息ID |
| task_id | TEXT NULL | 关联任务ID（可为空，没有任务时为NULL） |
| project_id | TEXT NULL | 关联项目ID（可为空，没有项目时为NULL） |
| reply_to_id | TEXT NULL | 回复的消息ID（支持引用回复，形成对话链） |
| from_id | TEXT | 发送者ID（用户ID或AgentID） |
| to_id | TEXT | 接收者ID（用户ID或AgentID） |
| role | INTEGER | 发送者角色：0=User 1=Agent 2=System |
| message_type | INTEGER | 消息类型：0=Text 1=Image 2=File 3=ToolCallRequest 4=ToolCallResult |
| file_type | INTEGER | 文件类型（附件消息） |
| status | INTEGER | 处理状态：0=撤回 1=待处理 2=处理中 3=已完成 4=失败 |
| content | TEXT | 消息内容 |
| file_meta | TEXT | 文件元数据JSON |
| created_by | TEXT | 创建人 |
| modified_by | TEXT | 最后修改人 |
| created_at | INTEGER | 创建时间戳（毫秒） |
| updated_at | INTEGER | 更新时间戳（毫秒） |

### 最终 messages 表结构

| 字段 | 类型 | 说明 |
|------|------|------|
| id | TEXT | 消息ID |
| task_id | TEXT NULL | 关联任务ID（可为空，没有任务时为NULL） |
| project_id | TEXT NULL | 关联项目ID（可为空，没有项目时为NULL） |
| from_id | TEXT | 发送者ID（用户ID或AgentID） |
| to_id | TEXT | 接收者ID（用户ID或AgentID） |
| role | INTEGER | 发送者角色：0=User 1=Agent 2=System |
| message_type | INTEGER | 消息类型：0=Text 1=Image 2=File... |
| file_type | INTEGER | 文件类型（附件消息） |
| status | INTEGER | 处理状态：0=撤回 1=待处理 2=处理中 3=已完成 4=失败 |
| content | TEXT | 消息内容 |
| file_meta | TEXT | 文件元数据JSON |
| created_by | TEXT | 创建人 |
| modified_by | TEXT | 最后修改人 |
| created_at | INTEGER | 创建时间戳（毫秒） |
| updated_at | INTEGER | 更新时间戳（毫秒） |

### 上下文组合场景

| 场景 | project_id | task_id |
|------|------------|---------|
| 纯粹闲聊 | NULL | NULL |
| 项目下无特定任务讨论 | 项目ID | NULL |
| 任务下多轮讨论 | 项目ID | 任务ID |

---

### 2. ProjectStatus 枚举变更

新增 `PendingReview` 状态用于项目创建完成等待用户审阅：

```rust
pub enum ProjectStatus {
    Deleted = 0,
    Active = 1,
    PendingReview = 2,  // 新增：创建完成，等待用户审阅批准
    InProgress = 3,     // 已批准，正在进行
    Completed = 4,      // 已完成
    Archived = 5,       // 已归档
}
```

状态流转：
```
前台创建项目 → PendingReview → 用户批准 → InProgress → 完成 → Completed
                        ↓
                     用户驳回 → PendingReview（修改后重新等待）
```

### 3. ProjectPo 新增字段

| 字段 | 类型 | 说明 |
|------|------|------|
| workflow | String | 项目运作流程描述，各角色协作方式 |
| guidance | String | 用户对项目的指导建议，Agent 执行时参考 |

两个字段都允许为空（空字符串表示使用默认流程）。

### 4. TaskStatus 枚举变更

新增 `PendingReview` 状态，支持任务创建完成等待用户审阅：

```rust
pub enum TaskStatus {
    Cancelled = 0,
    PendingReview = 1,  // 新增：创建完成，等待用户审阅批准
    Pending = 2,        // 默认：已批准，等待开始执行
    InProgress = 3,     // 执行中
    Completed = 4,      // 已完成
    Archived = 5,       // 已归档
}
```

使用方式：
- **需要审阅**：创建为 `PendingReview` → 用户批准后转为 `Pending`
- **不需要审阅**：直接创建为 `Pending`，按原流程执行

---

## Agent 角色设计

Agent 角色**不使用枚举**，保持开放灵活：

- `AgentPo.role: String` - 角色名称，如"前台Agent"、"Rust开发工程师"
- `AgentPo.description: String` - 角色职责描述
- `AgentPo.capabilities: Vec<String>` - 能力列表
- `AgentPo.soul: String` - 详细的角色性格/系统提示词

优点：
- ✅ 用户可以自由创建任意角色的 Agent
- ✅ 不需要修改代码即可扩展
- ✅ 符合"任意 Agent 都可以承担前台"的理念

---

## 交互流程（推送 + 拉取模式）

### 发送消息（用户 → 后端）

```
前端 → POST /api/chat/send-message
{
  "agent_id": "默认前台AgentID",  // 用户可指定任意Agent
  "project_id": "可选，当前项目上下文",
  "task_id": "可选，当前任务上下文",
  "content": "用户输入内容",
  "message_type": 0
}
```

后端处理：
1. 验证权限（当前用户有权限访问指定项目/任务）
2. 保存消息到 `messages` 表，状态 = `Pending`
3. 发布 `Message` 事件到事件总线
4. 立即返回 `{ message_id, created_at, project_id, task_id }`

### 拉取消息（前端 → 后端）

前端短轮询（间隔 1 秒）：
```
前端 → GET /api/chat/pull-messages
{
  "project_id": "项目ID",
  "task_id": "可选",
  "after_timestamp": 1234567890
}

后端返回：
{
  "messages": [...],  // 时间戳之后的所有新消息
  "has_more": false,
  "latest_timestamp": 1234567891
}
```

### 事件总线异步处理

```
消费者取出 Message 事件
  ↓
更新消息状态 → Processing
  ↓
拼装完整目标 Agent（包含工具）
  ↓
Agent 处理消息：
  ├─ 前台 Agent：理解用户意图，决策处理方式
  ├─ 工作 Agent：执行分配的任务
  ↓
Agent 生成回复，保存到 messages 表（role = Agent，status = Processed）
  ↓
更新原用户消息状态 → Processed
  ↓
完成
```

下次轮询前端就能拿到 Agent 回复。

---

## 代码分层架构

严格遵循项目分层规范 `handler → domain → dal → dao`：

```
src/
  handlers/chat/                ← HTTP 接口层
    ├─ send_message.rs
    ├─ pull_messages.rs
    ├─ get_history.rs
    ├─ list_conversations.rs
    └─ mod.rs

src/service/
  ├─ dao/message/              ← DAO：数据访问
  │  ├─ mod.rs                 ← trait 定义
  │  ├─ sqlite.rs              ← SQLite 实现
  │  └─ sqlite_test.rs         ← 单元测试
  ├─ dal/chat.rs               ← DAL：组合 DAO
  └─ domain/message/           ← DOMAIN：核心业务逻辑
      ├─ mod.rs
      ├─ receive_message.rs    ← 接收用户消息 → 保存 → 发布事件
      └─ process_message.rs    ← 处理消息 → 唤醒 Agent → 保存回复
```

优点：
- ✅ 所有消息核心业务逻辑聚合在 `message domain`
- ✅ 后续扩展飞书/微信等消息源，只需要新增 webhook handler，直接调用同一个 `domain::receive_message`
- ✅ 完全复用现有逻辑，不需要重复代码

---

## 前端交互设计

### 左侧边栏

```
┌─────────────────────────────┐
│  最近对话                    │
│  • 和前台Agent闲聊          │  (project_id = NULL)
│  • Rust 开发框架重构        │  (project_id = xxx)
│  • 产品需求文档编写          │  (project_id = yyy)
└─────────────────────────────┘
┌─────────────────────────────┐
│  我的项目                    │
│  • [待审阅] 重构前端对话页   │
│  • [进行中] 新增消息推送支持 │
│  • [已完成] 用户权限系统     │
└─────────────────────────────┘
```

### 用户操作流程

1. 用户可以从"最近对话"进入继续之前的讨论
2. 用户可以从"我的项目"进入项目讨论，自动带上项目上下文
3. 点击"新对话" → 选择目标 Agent（默认前台）→ 开始闲聊，`project_id = NULL`
4. 发消息时，如果已经在项目上下文中，自动带上 `project_id`

好处：
- ✅ 用户明确指定上下文，减少 Agent 理解成本
- ✅ 支持有项目/没项目两种场景
- ✅ 用户可以随时切换项目

---

## API 接口清单（规划）

| 方法 | 接口 | 功能 |
|------|------|------|
| POST | `/api/chat/create-conversation` | 创建新对话（新项目或闲聊） |
| POST | `/api/chat/send-message` | 发送消息给 Agent |
| GET | `/api/chat/pull-messages` | 拉取最新消息（轮询） |
| GET | `/api/chat/get-history` | 分页加载历史消息 |
| GET | `/api/chat/list-conversations` | 列最近对话列表 |
| GET | `/api/projects/list-my-projects` | 列出当前用户的所有项目 |

---

## 创建时间

本文档创建于：2026-04-19，基于讨论总结。

最后更新：2026-04-28

---

## 混合模式工具调用设计

### 核心理念

系统支持**混合模式**工具调用，兼顾简洁性和可控性：

| 模式 | 适用场景 | 调用方式 | 可控性 |
|------|---------|---------|--------|
| **auto 模式** | 简单无状态工具（如搜索引擎、代码格式化） | 交给 rig 原生自动调用 | rig 完全控制 |
| **manual 模式** | 关键业务工具（如读写文件、执行命令、修改代码） | 自建链路控制，可收敛 | 可重试、可审核、可人工干预 |

### 存储设计

工具调用本身就是一种特殊消息，**复用现有 `messages` 表存储**，不需要单独新建表：

| 消息类型枚举 | 说明 | content 存储格式 |
|-------------|------|-----------------|
| `Text` | 普通文本消息 | 纯文本 |
| `ToolCallRequest` | 工具调用请求 | JSON 序列化的 `ToolCallRequest` |
| `ToolCallResult` | 工具调用结果 | JSON 序列化的 `ToolCallResult` |

### 代码组织规范

根据设计要求，按功能分组存放，不拆分小文件：

```
common/src/enums/
└── message.rs         # 统一存放所有消息相关枚举：
                        # MessageRole / MessageStatus / MessageType

src/models/
└── message.rs         # 统一存放 PO + 工具调用结构体：
                        # MessagePo / ToolCallRequest / ToolCallResult

src/pkg/
├── tool_registry/     # ContextTool trait 定义 + 注册中心
│   └── mod.rs
└── tool_tracing/      # ContextToolCallLogger 日志装饰器
    └── mod.rs
```

### manual 模式调用流程

```
Agent 决策需要调用工具 → 生成 ToolCallRequest → 保存为消息 → 发布事件
    ↓
服务端取出事件 → 执行工具 → 生成 ToolCallResult → 保存为消息
    ↓
Agent 拉取到 ToolCallResult → 继续处理 → 生成最终回复
```

优势：
- ✅ 完整链路可追溯，所有工具调用都记录在消息历史中
- ✅ 支持人工审核工具调用后再执行
- ✅ 支持重试失败的工具调用
- ✅ 复用现有消息存储和投递机制，不增加基础设施复杂度

---

## 消息投递队列设计

### 拉取模式消费

每个 Agent 有自己的投递队列，使用拉取模式消费：

1. Agent 启动后调用 `dequeue` 获取下一个 `Pending` 消息
2. 更新消息状态为 `Processing`
3. Agent 处理消息
4. 处理完成调用 `ack` 更新状态为 `Completed`
5. 如果处理失败调用 `nack` 改回 `Pending`，下次重新拉取

### 领域层接口设计

```rust
pub trait DeliveryDomain {
    /// 用户发送消息给 Agent
    async fn send_to_agent(
        &self,
        ctx: RequestContext,
        msg: NewMessage,
    ) -> Result<MessagePo, AppError>;

    /// Agent 发送消息给用户
    async fn send_to_user(
        &self,
        ctx: RequestContext,
        msg: NewMessage,
    ) -> Result<MessagePo, AppError>;

    /// Agent 拉取下一个待处理消息
    async fn dequeue(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Option<MessagePo>, AppError>;

    /// 确认消息处理完成
    async fn ack(
        &self,
        ctx: RequestContext,
        message_id: &str,
    ) -> Result<(), AppError>;

    /// 消息处理失败，放回队列
    async fn nack(
        &self,
        ctx: RequestContext,
        message_id: &str,
    ) -> Result<(), AppError>;
}
```

## 当前实现进度

- [x] 数据库 schema 设计完成，迁移已执行（含 reply_to_id 字段
- [x] 枚举定义完成（common）
- [x] PO 和工具调用结构体定义完成（models）
- [x] DAO 层完成，支持按会话/项目/任务查询
- [x] DAL 层完成
- [x] Domain 层完成：delivery + management，所有测试通过
- [x] 事件总线重构完成：泛型 topic 分离设计，彻底解决消息错乱
- [x] Tool Domain 层框架已搭建（management + execution）
- [ ] Handler 层开发进行中

所有单元测试：**165/165 全部通过 ✅

最后更新：2026-04-30
