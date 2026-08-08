# 外部 Agent 接入设计

## 定位

支持将外部 Agent（Codex CLI、A2A 远程 Agent）注册到组织，与本地 Agent 共享同一调度链路。差异封装在 DAL 层，Domain 层保持通用。

## Agent 分类

`AgentKind` 枚举区分三类 Agent：

| Kind | 执行后端 | model_provider | external_config | Brain.cortex |
|------|---------|----------------|-----------------|--------------|
| `Local` | CortexDao.prompt (Rig) | 必填 | None | Some(Cortex) |
| `Cli` | agent_runtime::codex::execute_cli | 不需要 | `ExternalAgentConfig::Cli` | None |
| `Remote` | agent_runtime::a2a::execute_a2a | 不需要 | `ExternalAgentConfig::Remote` | None |

## 三层架构

```
Handler (用户行为差异)
  └─ Domain (通用业务)
      └─ DAL (差异化封装)
          └─ DAO (执行抽象)
              ├─ CortexDao (Local)
              └─ AgentRuntimeDao (Cli/Remote)
                  ├─ codex (CLI 子进程)
                  └─ a2a (HTTP/JSON-RPC)
```

- **Handler 层**：`create_agent` 创建 Local；`create_external_agent` 创建 Cli/Remote。Handler 内构造好 Agent（含 kind + external_config），调用通用 `create_agent`，Domain 层不提供同名语法糖。
- **Domain 层**：`HrDomain.agent_manage()` 通用管理；`RuntimeDomain.awakening()` 统一调度。
- **DAL 层**：`AgentDal`（Local 基础实现）+ `CodexAgentDal`/`A2aAgentDal`（派生 Dal，委托模式持有 `Arc<dyn AgentDal>`）。
- **DAO 层**：`CortexDao`（Rig/cortex）+ `AgentRuntimeDao`（外部 runtime 执行抽象）。

## Brain 装配链路（v5：内部分发方案）

Brain 持有 `kind + runtime_config + Option<Cortex>`，统一走 `BrainDal.think(brain, prompt)` 入口，内部按 `brain.kind` 分发：

```
consumer 处理消息
  └─ agent.brain.is_none() → RuntimeDomain.awakening().wake_agent_brain(ctx, &mut agent)
      └─ BrainDal.wake_brain(ctx, &agent_po, memories, tools)
          ├─ Local  → Brain::new_local (加载 model_provider + Cortex)
          └─ Cli/Remote → Brain::new_external (虚拟 Brain，cortex=None)
  └─ RuntimeDomain.awakening().awaken(ctx, &agent, &message)
      └─ Step 4: prompt_builder(agent)  ← 工厂方法按 kind 路由
      └─ Step 5: BrainDal.think(ctx, brain, &prompt)
          ├─ Local  → cortex_dao.prompt()
          ├─ Cli    → agent_runtime::codex::execute_cli()
          └─ Remote → agent_runtime::a2a::execute_a2a()
```

## PromptBuilder 工厂方法

每类 Agent Dal 配套自己的 PromptBuilder；没有专属 builder 时复用 trait 默认方法提供的 `DefaultPromptBuilder`，不引入笼统的"外部 builder"抽象。

```rust
// RuntimeDomainImpl.prompt_builder(agent)
match agent.po.kind {
    AgentKind::Local  => self.agent_dal.prompt_builder(),       // DefaultPromptBuilder
    AgentKind::Cli    => self.codex_agent_dal.prompt_builder(), // 未来替换为 CliPromptBuilder
    AgentKind::Remote => self.a2a_agent_dal.prompt_builder(),   // 未来替换为 RemotePromptBuilder
}
```

`PromptBuilder` trait 定义在 `src/models/prompt_builder.rs`，使用 `&mut self` 风格支持 `Box<dyn PromptBuilder>`。`build(&self)` 允许重复构建。

## HTTP API

### 创建外部 Agent

```
POST /api/v1/hr/agents/external
Content-Type: application/json

// CLI 类型
{
  "name": "Codex Coder",
  "kind": "cli",
  "command": "codex",
  "args": ["--auto"],
  "work_dir": "/workspace",
  "timeout_secs": 300,
  "description": "Codex CLI Agent"
}

// Remote 类型
{
  "name": "Remote Assistant",
  "kind": "remote",
  "endpoint": "https://a2a-server.example.com",
  "agent_name": "assistant-001",
  "auth_token": "Bearer xxx",
  "timeout_secs": 300
}
```

Handler 内按 kind 校验必填字段并构造 `ExternalAgentConfig`，写入 `runtime_config.external_config`。Domain 层 `create_agent` 按 kind 跳过 `model_provider_id` 校验（外部 Agent 不需要本地 provider）。

## 关键约束

- **DAL 不依赖 DAL**：BrainDal 调度 CortexDao 和 AgentRuntimeDao（平级），不让 AgentRuntimeDao 实现 CortexTrait。
- **派生 Dal 委托模式**：`CodexAgentDal`/`A2aAgentDal` 持有 `Arc<dyn AgentDal>`，所有管理方法委托 base，仅在有差异化需求时重写。
- **Brain 统一装配**：外部 Agent 也装配 Brain（cortex 为 None），统一走 `think()` 入口，避免上层区分调用不同方法。
- **Domain 层通用**：`create_agent`、`awaken` 等方法对 kind 透明，用户行为差异通过不同 Handler 处理。

## A2A Remote Agent 异步处理机制

### 数据模型对应

| A2A 概念 | ai_orz 对应 | 说明 |
|---------|------------|------|
| A2A Task | 本地 Task | 委托给外部 Agent 的工作单元（非 Project） |
| A2A Task.id | Task.tags 中的 `a2a_task_id:xxx` | 外部 task_id 通过 tags 存储，提供工具函数提取/构造 |
| A2A Task.messages | MessagePo 链 | 外部 Agent 回复的消息通过 `send_to_user` 投递给用户 |
| A2A Task.status | Task.status | 终态映射：completed→Completed, failed/canceled→Cancelled |

### 异步更新双通道

委托给外部 Remote Agent 的任务通过两种机制获取状态更新：

**1. Push 回调（推荐）**
- 公开端点：`POST /a2a/callback/:task_id`（无需 JWT）
- 外部 Agent 完成任务或有新消息时向此 URL 推送 A2aTask
- URL 中 `:task_id` 是本地 Task ID（调用 tasks/send 时构造 notification_url 传入）
- 校验流程：任务存在 → 状态活跃 → 外部 task_id 与本地记录一致 → **直接处理业务逻辑**

**2. Poll 轮询（兜底）**
- `A2aPollingProducer` 注册到 AOP 事件中心，每 30 秒执行一次
- 查询流程：
  1. 通过 `hr_domain().agent_manage().list_agents()` 获取所有 Remote Agent
  2. 对每个 Agent，通过 `project_domain().task_manage().list()` 查询分配给它的 `InProgress` Task（`assignee_type=Agent, assignee_id=agent_id, status=InProgress`）
  3. 从 Task.tags 解析外部 a2a_task_id
  4. 调用远程 A2A Agent 的 `tasks/get` 接口获取最新状态
  5. **直接处理业务逻辑**（新消息 → send_to_user；状态变更 → transition_status）

### 适配层直接处理（通用架构原则）

Handler/回调端点/Producer 同属**适配层（Adapter）**——Handler 是面向用户 HTTP API 的 Adapter，回调端点/Producer 是面向外部系统的 Adapter，它们在架构中处于同一层级，职责完全一致：在适配层完成外部协议到内部操作的转换后，直接调用 Domain 层方法完成业务处理，**外部协议数据绝不进入事件中心**。

> 详见 [LAYERED_ARCHITECTURE_PRACTICE.md - 实践 7：适配层架构原则](../LAYERED_ARCHITECTURE_PRACTICE.md)

#### 入站适配器对照

| 外部输入源 | 适配层组件 | 协议转换 | 业务动作 |
|-----------|-----------|---------|---------|
| 用户/前端 HTTP API | HTTP Handler（如 `create_agent`、`send_message`） | API DTO (JSON) → Command | 调用对应 Domain 方法，返回 Response DTO |
| 飞书 P2P 消息 | `src/producer/message_channel.rs` (AOP Producer) | `LarkMessageEvent` → `AdaptedMessage` → `SendToAgentCommand` | `MessageDomain.send_to_agent()`（创建消息→发布 MessageCreatedEvent→唤醒 Agent） |
| A2A 回调推送 | `src/handlers/a2a/callback.rs` (公开 HTTP Handler) | `A2aTask` → 提取消息文本/状态 → `SendToUserCommand` | `MessageDomain.send_to_user()`（创建消息→发布 MessageCreatedEvent→SSE 推送用户）+ `TaskManage.transition_status()` |
| A2A 轮询兜底 | `src/producer/a2a_polling.rs` (AOP Producer) | 同上（fetch_task 获取 A2aTask 后同样处理） | 同上 |

#### A2A 适配层处理流程

```
外部协议数据（A2aTask）进入适配层
  ├─ Push 回调：POST /a2a/callback/:task_id（公开 HTTP Handler）
  └─ Poll 轮询：A2aPollingProducer.poll()（每30秒，AOP Producer）
        │
        ▼
  [适配层] 解析与校验
        ├─ 校验本地 Task 存在且状态活跃
        ├─ 校验外部 task_id 与本地 tags 中记录一致
        ├─ 基于 tags 中 a2a_synced_msgs:N 做消息去重
        └─ 提取 agent/assistant 角色的文本消息
        │
        ▼
  [适配层] 直接调用 Domain 方法
        ├─ MessageDomain.delivery().send_to_user()  ← 新消息
        │     └─ 内部：创建 MessagePo → 持久化 → 发布 MessageCreatedEvent → SSE 推送给用户
        └─ ProjectDomain.task_manage().transition_status()  ← 状态变更
        │
        ▼
  [内部链路] 现有 Consumer 消费 MessageCreatedEvent（无需感知 A2A）
```

#### 幂等性策略

- **回调**：已在终态的任务直接返回 ok 跳过；外部 Agent HTTP 请求失败时可重试推送
- **轮询**：每次执行重新查询并基于已同步消息计数判断，天然支持重试（30秒后会再次处理）
- **消息去重**：通过 Task.tags 中的 `a2a_synced_msgs:N` 记录已同步的 agent 消息数量，只发送 N 之后的新消息

#### 状态映射规则

| A2A Task 状态 | 本地 Task 状态 |
|--------------|---------------|
| `Completed` | `Completed` |
| `Failed` / `Canceled` | `Cancelled` |
| `Working` / `Submitted` / `InputRequired` | `Pending` → `InProgress` |

### 相关文件

| 文件 | 说明 |
|------|------|
| `src/models/events/a2a_task_update.rs` | A2A tags 工具函数（a2a_task_id 提取/构造、消息计数、文本提取） |
| `src/handlers/a2a/callback.rs` | 回调端点 `POST /a2a/callback/:task_id`（适配层，直接处理业务） |
| `src/producer/a2a_polling.rs` | A2aPollingProducer（30秒轮询，适配层，直接处理业务） |
| `src/service/dao/agent_runtime/a2a.rs` | A2aRuntimeDao（出站 DAO：send_task/fetch_task 调用远程 A2A 接口） |
| `src/router.rs` | 注册回调路由 |
| `src/producer/mod.rs` | 注册 A2aPollingProducer |

### 后续迭代计划

1. **调用流程改造**：`execute_a2a` 支持异步模式，创建本地 Task、构造含 task_id 的 notification_url
2. **产物处理**：支持 A2aTask.artifacts 的保存和关联
3. **轮询性能优化**：批量查询所有 InProgress Agent 类型任务，减少查询次数
4. **输入请求处理**：支持 A2A `InputRequired` 状态，向用户请求补充输入

## 前端页面支持

前端通过 `kind` 字段区分三类 Agent，并提供外部 Agent 的注册入口和详情展示。

### Agent 列表页（`frontend/src/pages/hr/agents.rs`）

- **类型列**：在列表中展示 `kind` 徽章，使用不同颜色直观区分
  - `local` → `badge-info`（蓝色，本地 Agent）
  - `cli` → `badge-accent`（强调色，CLI Agent）
  - `remote` → `badge-success`（绿色，远程 Agent）
- **创建入口拆分**：原"创建 Agent"按钮拆分为两个入口
  - `+ 本地 Agent`：跳转本地 Agent 创建表单（需要 model_provider_id）
  - `+ 外部 Agent`：弹出外部 Agent 创建弹窗，支持选择 CLI / Remote 类型
- **响应式表单**：外部 Agent 创建弹窗根据 `kind` 动态切换字段
  - CLI 配置：启动命令、命令参数、工作目录、超时时间、自定义 Prompt 模板
  - Remote 配置：A2A Server 地址、目标 Agent 名称、超时时间

### Agent 详情页（`frontend/src/pages/hr/agent_detail.rs`）

- **类型徽章**：基本信息区域展示 `kind` 徽章
- **条件字段**：`model_provider_id` 仅对本地 Agent 展示（外部 Agent 不需要本地 provider）
- **运行时配置区域**：外部 Agent 专属区域，根据 `external_config` 动态渲染
  - CLI 配置展示：启动命令、命令参数、工作目录、超时时间、Prompt 模板
  - Remote 配置展示：A2A Server、目标 Agent、超时时间

### 前端 API 封装（`frontend/src/api/hr.rs`）

```rust
pub async fn create_external_agent(req: CreateExternalAgentRequest) -> Result<CreateExternalAgentResponse, String> {
    api_post("/api/v1/hr/agents/external", &req).await
}
```

## 相关文件

- `src/models/agent.rs`：`AgentRuntimeConfig`、`ExternalAgentConfig`
- `src/models/brain.rs`：`Brain::new_local` / `new_external`
- `src/models/prompt_builder.rs`：`PromptBuilder` trait
- `src/service/dal/brain.rs`：`BrainDal.think` 内部按 kind 分发
- `src/service/dal/agent_codex.rs` / `agent_a2a.rs`：派生 Dal
- `src/service/domain/runtime/awakening.rs`：`wake_agent_brain` + `awaken`
- `src/service/dao/agent_runtime/`：`codex.rs` + `a2a.rs` RuntimeDao 实现
- `src/handlers/hr/agent/create_external_agent.rs`：HTTP API
- `common/src/api/external_agent.rs`：API DTO
- `common/src/api/agent.rs`：`AgentListItem` / `GetAgentResponse` 新增 `kind` + `external_config` 字段
- `frontend/src/api/hr.rs`：`create_external_agent` 前端 API 封装
- `frontend/src/pages/hr/agents.rs`：Agent 列表页（类型列 + 外部 Agent 创建弹窗）
- `frontend/src/pages/hr/agent_detail.rs`：Agent 详情页（类型徽章 + 运行时配置展示）
