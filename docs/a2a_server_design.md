# A2A Server 设计文档

## 🚀 实现完成状态（2026-07-20）

### P0 已完成（后端）

| 模块 | 状态 | 文件 |
|------|------|------|
| 协议实体定义 | ✅ | `common/src/api/a2a.rs` |
| 配置项 | ✅ | `common/src/config.rs` |
| resolve_agent 统一路由方法 | ✅ | `src/service/domain/hr/mod.rs` |
| 映射层 | ✅ | `src/handlers/a2a/mapper.rs` |
| Agent Card 端点 | ✅ | `src/handlers/a2a/agent_card.rs` |
| tasks/send 异步提交 | ✅ | `src/handlers/a2a/send_task.rs` |
| tasks/get 查询 | ✅ | `src/handlers/a2a/get_task.rs` |
| tasks/cancel 取消 | ✅ | `src/handlers/a2a/cancel_task.rs` |
| JSON-RPC 分发 | ✅ | `src/handlers/a2a/jsonrpc.rs` |
| 路由注册 | ✅ | `src/router.rs` |
| 集成测试 | ✅ | `src/handlers/a2a/integration_test.rs` |
| CreateProjectRequest 支持 owner_agent_id | ✅ | `common/src/api/project.rs` |
| send_message_to_agent 两种对话上下文 | ✅ | `src/handlers/finance/message/send_message_to_agent.rs` |
| 前台 Agent 查询 HTTP API | ✅ | `src/handlers/hr/agent/get_reception_agent.rs` |

### P1 已完成（前端）

| 模块 | 状态 | 文件 |
|------|------|------|
| 前端 chat 默认对话框（不创建 project） | ✅ | `frontend/src/pages/message/chat.rs` |
| 前端 get_reception_agent_api（显示推荐前台 Agent） | ✅ | `frontend/src/api/hr.rs` |
| 新建项目弹窗 + 自动绑定前台 Agent | ✅ | `frontend/src/pages/message/chat.rs` |
| 侧边栏固定置顶「默认对话」条目 | ✅ | `frontend/src/pages/message/chat.rs` |

### P2 已完成

| 模块 | 状态 | 文件 |
|------|------|------|
| tasks/sendSubscribe SSE 流式 | ✅ | `src/handlers/a2a/send_subscribe.rs` |
| PushNotifications 推送通知（Webhook 回调） | ✅ | `src/service/dal/message_channel.rs` |

## 定位

让 ai_orz 组织作为 **A2A Server**，对外暴露 A2A 协议（JSON-RPC 2.0）端点，外部 A2A Client 可通过协议调用组织内的前台 Agent 完成任务。

与 [external_agent_design.md](./external_agent_design.md) 互为反向能力：

| 方向 | 角色 | 文档 |
|------|------|------|
| ai_orz → 外部 Agent | A2A Client | [external_agent_design.md](./external_agent_design.md) |
| 外部 Client → ai_orz | A2A Server | 本文档 |

## 概念对齐

A2A 协议概念到 ai_orz 内部实体的映射：

| A2A 概念 | ai_orz 对应 | 说明 |
|---------|------------|------|
| `task` | `Project` | A2A 的 task 是一个完整工作单元，对应一个 project，不是 project 下的子 task |
| `task.status` | `Project.status` + 关联 `Task` 聚合状态 | submitted/working/input-required/completed/failed/canceled |
| `task.messages` | `MessagePo` 链 | 用户与 Agent 之间的消息流 |
| `task.artifacts` | `ProjectArtifact` | 任务产物 |
| `message` | `MessagePo` | A2A Message ↔ ai_orz Message |
| `session_id` | `Message.root_id` 链 | 会话保持 |
| 长任务（异步） | `Project` + consumer 异步消费 | `tasks/send` 立即返回 working，客户端轮询 `tasks/get` |
| Agent 身份 | 前台 Agent（不暴露具体 Agent ID） | 复用 `resolve_agent` 统一路由策略 |
| 调用方身份 | 系统注册用户（复用 JWT） | 不新增表，权限沿用用户角色 |

### Agent 与 Project 维度分离

**核心设计原则**：agent 与 project 是两个独立维度，不在 domain 层融合，由上层（handler 层）按需组合。

- **`resolve_agent(ctx)`**：只接受 ctx，返回当前可用的前台 Agent，不查询/感知 project
- **两种对话上下文（协作关系类比）**：
  - **默认对话框**：与前台 Agent 直接沟通（无 project），简单需求直接处理
  - **Project 对话框**：在 Project 上下文中沟通，可由用户主动创建或由 Agent 内部决策触发
- **Project 创建**：双通道
  - 用户在前端主动新建（创建时自动绑定前台 Agent 作为 `owner_agent_id`）
  - Agent 内部决策触发（A2A `tasks/send` 场景由 handler 创建）

## 三条核心原则

### 1. 复用用户 JWT，不加表

现有 [jwt_auth_middleware](../src/middleware/jwt_auth.rs) 已支持 `Authorization: Bearer <token>` 提取 token，[Claims](../src/pkg/jwt.rs) 含 `user_id`/`organization_id`/`role`。A2A 端点直接挂现有 JWT 中间件：

- 每个注册用户都能用自己的 token 调用 A2A 端点
- 权限沿用该用户的角色权限
- token 管理（登录、过期、刷新）全部走老路
- **零新增表**

### 2. 外部只见前台 Agent，不暴露 Agent ID

复用 [hr domain](../src/service/domain/hr/mod.rs) 中 `resolve_agent(ctx)` 的统一路由策略：

- 优先按 `feishu_reception` 角色找已入职 Agent
- 找不到则 fallback 到任意已入职 Agent

A2A 入口完全复用这套逻辑：

- **Agent Card**：对外只暴露一个统一入口（组织级），不列具体 Agent
- **`tasks/send`**：外部不传 `agent_id`（或忽略该参数），由系统内部 `resolve_agent(ctx)` 路由到前台 Agent
- 外部调用方看到的是"一个 Agent"，内部实际可能是不同前台 Agent 接待
- **agent 与 project 是两个维度**：`resolve_agent(ctx)` 只接受 ctx，不耦合 project；两个维度由 handler 层按需组合

### 3. 只在 handler 层做内外实体转换

A2A 协议实体（`A2aTask`/`A2aMessage`/`AgentCard` 等）定义在 `common/src/api/a2a.rs`，转换逻辑全部封装在 `src/handlers/a2a/` 层。**Domain 层完全不知道 A2A 的存在**，只处理 ai_orz 自己的实体。

```
外部 A2A Client (持用户 JWT)
    │
    │  POST /a2a  JSON-RPC 2.0
    ▼
JWT 中间件（复用现有）→ 提取 user_id / org_id
    │
    ▼
Handler 层 (src/handlers/a2a/)
    │  ┌─────────────────────────────────────────┐
    │  │ 1. 解析 JSON-RPC 请求                    │
    │  │ 2. A2A 实体 → ai_orz 实体（转换）        │
    │  │ 3. 调用 Domain 层（全是内部实体）        │
    │  │ 4. ai_orz 实体 → A2A 实体（转换）        │
    │  │ 5. 封装 JSON-RPC 响应                    │
    │  └─────────────────────────────────────────┘
    │
    ▼
Domain 层 (src/service/domain/)
    │  项目管理、消息投递、Agent 唤醒
    │  ↑ 完全是 ai_orz 内部实体，无 A2A 概念
    ▼
返回 JSON-RPC response
```

## 协议实体定义

在 `common/src/api/a2a.rs` 新增（前后端共享，前端可复用展示）：

```rust
// ===== Agent Card =====
pub struct AgentCard {
    pub name: String,
    pub description: String,
    pub version: String,                    // 协议版本，如 "0.3.0"
    pub capabilities: AgentCapabilities,
    pub skills: Vec<AgentSkill>,            // 组织对外能力（非具体 Agent）
    pub default_input_modes: Vec<String>,   // ["text"]
    pub default_output_modes: Vec<String>,  // ["text"]
    pub security_schemes: serde_json::Value,
}

pub struct AgentCapabilities {
    pub streaming: bool,            // 是否支持 SSE 流式
    pub push_notifications: bool,   // 是否支持推送通知
}

pub struct AgentSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
}

// ===== JSON-RPC 2.0 =====
pub struct JsonRpcRequest<T> {
    pub jsonrpc: String,            // 固定 "2.0"
    pub id: serde_json::Value,      // 请求 ID（string | number | null）
    pub method: String,
    pub params: T,
}

pub struct JsonRpcResponse<T> {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub result: Option<T>,
    pub error: Option<JsonRpcError>,
}

pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

// ===== Task =====
pub struct A2aTask {
    pub id: String,                     // = ai_orz project id
    pub session_id: Option<String>,     // 会话保持
    pub status: A2aTaskStatus,
    pub messages: Vec<A2aMessage>,      // 消息流
    pub artifacts: Vec<A2aArtifact>,    // 产物
    pub metadata: serde_json::Value,
}

pub struct A2aTaskStatus {
    pub state: A2aTaskState,            // submitted/working/input-required/completed/failed/canceled
    pub timestamp: String,              // ISO 8601
    pub message: Option<String>,
}

pub enum A2aTaskState {
    Submitted,
    Working,
    InputRequired,
    Completed,
    Failed,
    Canceled,
}

// ===== Message =====
pub struct A2aMessage {
    pub role: String,               // "user" | "agent"
    pub parts: Vec<A2aMessagePart>,
    pub message_id: String,         // = ai_orz message id
    pub task_id: String,            // = ai_orz project id
}

pub enum A2aMessagePart {
    Text { text: String },
    File { file: A2aFilePart },
    Data { data: serde_json::Value },
}

pub struct A2aFilePart {
    pub name: String,
    pub mime_type: String,
    pub bytes: Option<String>,      // base64
    pub uri: Option<String>,
}

// ===== Artifact =====
pub struct A2aArtifact {
    pub artifact_id: String,        // = ai_orz artifact id
    pub name: String,
    pub parts: Vec<A2aMessagePart>,
}

// ===== 方法参数 =====
pub struct SendTaskParams {
    pub id: String,                 // 客户端生成的 task id（ai_orz 忽略，用自己生成的 project id）
    pub message: A2aMessage,
    pub session_id: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

pub struct GetTaskParams {
    pub id: String,
    pub history_length: Option<i32>,
}

pub struct CancelTaskParams {
    pub id: String,
}
```

## Handler 层实现

### 目录结构

```
src/handlers/a2a/
├── mod.rs                      # 路由注册
├── agent_card.rs               # GET /.well-known/agent.json（公开）
├── jsonrpc.rs                  # POST /a2a 入口（JSON-RPC 解析 + 分发）
├── mapper.rs                   # A2A ↔ ai_orz 实体转换（核心）
├── send_task.rs                # tasks/send 异步提交
├── send_subscribe.rs           # tasks/sendSubscribe SSE 流式（P2）
├── get_task.rs                 # tasks/get
├── cancel_task.rs              # tasks/cancel
```

### Agent Card 端点

`GET /.well-known/agent.json`，**公开路由，不挂 JWT 中间件**。

返回组织级能力描述，对外是"一个虚拟 Agent"，不列具体内部 Agent：

```json
{
  "name": "<组织名>",
  "description": "ai_orz 组织对外能力入口",
  "version": "0.3.0",
  "capabilities": { "streaming": true, "pushNotifications": false },
  "skills": [
    { "id": "chat", "name": "对话协作", "description": "与组织前台 Agent 对话", "tags": ["chat"] }
  ],
  "defaultInputModes": ["text"],
  "defaultOutputModes": ["text"]
}
```

### JSON-RPC 端点

`POST /a2a`，**挂现有 JWT 中间件**。

入口 `jsonrpc.rs` 负责解析 JSON-RPC 2.0 请求，按 `method` 字段分发：

| JSON-RPC method | 分发到 | 说明 |
|----------------|--------|------|
| `tasks/send` | `send_task.rs` | 异步提交：创建 project + message → 返回 working task；唤醒由 consumer 异步闭环，客户端轮询 `tasks/get` |
| `tasks/get` | `get_task.rs` | 查询 project + 关联 message/artifact 状态 |
| `tasks/cancel` | `cancel_task.rs` | 取消 project |
| `tasks/sendSubscribe` | `send_subscribe.rs` | SSE 流式：创建 project + message → 返回 SSE 流，每次消息更新推送完整 A2A Task |

未知的 method 返回 JSON-RPC error（code -32601）。

### 映射层（mapper.rs，核心）

**A2A → ai_orz（入向）**：

```rust
// A2A Message → ai_orz MessagePo
pub fn a2a_message_to_message_po(
    msg: &A2aMessage,
    from_user_id: &str,
    to_agent_id: &str,
    project_id: &str,
) -> MessagePo {
    // role="user" 的 A2A message → from=外部用户, to=前台 Agent
    // parts 中的 Text → MessagePo.content
    // parts 中的 File/Data → 附件或 JSON 扩展字段
}

// A2A task params → ai_orz CreateProjectCommand
pub fn a2a_params_to_create_project(
    params: &SendTaskParams,
    user_id: &str,
    org_id: &str,
) -> CreateProjectCommand {
    // A2A task → ai_orz project
}
```

**ai_orz → A2A（出向）**：

```rust
// ai_orz Project → A2A Task
pub fn project_to_a2a_task(project: &ProjectPo, messages: &[MessagePo], artifacts: &[ArtifactPo]) -> A2aTask {
    // project.id → A2A task.id
    // project.status → A2A task.state
    // messages → A2A task.messages
    // artifacts → A2A task.artifacts
}

// ai_orz MessagePo → A2A Message
pub fn message_po_to_a2a_message(msg: &MessagePo) -> A2aMessage {
    // from_agent 的回复 → role="agent"
    // from_user 的消息 → role="user"
    // content → parts: [Text]
}

// ai_orz ProjectStatus → A2A TaskState
pub fn project_status_to_a2a_state(status: &ProjectStatus) -> A2aTaskState {
    // pending → Submitted
    // in_progress → Working
    // waiting_input → InputRequired
    // completed → Completed
    // failed → Failed
    // canceled → Canceled
}
```

### `tasks/send` 异步提交流程

**异步流程**（与飞书/前端聊天的唤醒链路完全一致）：

```
handler 层（tasks/send）：
1. 解析 JSON-RPC，拿到 SendTaskParams
2. JWT 提取 user_id / org_id
3. handler 层显式组合 agent 与 project 两个维度：
   (a) resolve_agent(ctx) → 前台 Agent（只接受 ctx，不耦合 project）
   (b) 创建 project，将 agent.id 作为 owner_agent_id 绑定
   （agent 与 project 是两个维度，不在 hr domain 中融合）
4. 启动 project（start → InProgress）
5. 创建 message（send_to_agent）→ 自动入队 event_queue
6. 立即返回 working 状态的 A2aTask（不等待 Agent 回复）

consumer 异步闭环（复用现有链路，无需新代码）：
7. consumer worker dequeue → handle_agent_message
8. 内部自动 wake_agent_brain（幂等）+ awaken
9. Agent 回复 message → 客户端通过 tasks/get 轮询获取结果
```

**关键设计**：
- handler 层不调用 `wake_agent_brain` / `awaken`，唤醒由 consumer 异步闭环
- 与飞书/前端聊天的消息驱动架构完全一致，复用现有 consumer 链路
- 客户端通过 `tasks/get` 轮询 project 状态（InProgress → Completed）

### `tasks/get` 查询流程

```
1. 解析 JSON-RPC，拿到 GetTaskParams { id, history_length }
2. ProjectDomain.get_project(id) → ProjectPo
3. MessageDomain.list_by_project(id, history_length) → Vec<MessagePo>
4. ArtifactDomain.list_by_project(id) → Vec<ArtifactPo>
5. mapper: project + messages + artifacts → A2aTask
6. 封装 JSON-RPC response
```

### `tasks/cancel` 取消流程

```
1. 解析 JSON-RPC，拿到 CancelTaskParams { id }
2. ProjectDomain.update_status(id, Canceled)
3. mapper: project → A2aTask（status=canceled）
4. 封装 JSON-RPC response
```

## 路由注册

在 `src/router.rs` 新增：

```rust
// A2A 协议端点（公开发现 + JWT 保护的 JSON-RPC）
.route("/.well-known/agent.json", get(handlers::a2a::agent_card::get_agent_card))
.route("/a2a", post(handlers::a2a::jsonrpc::handle_jsonrpc))
```

- `/.well-known/agent.json`：无认证中间件
- `/a2a`：挂 `jwt_auth_middleware`（与 `/api/v1/*` 相同）

## 配置项

`ai_orz.toml` 新增：

```toml
[a2a_server]
enabled = false                # 是否启用 A2A Server
protocol_version = "0.3.0"     # 协议版本
endpoint = "/a2a"              # JSON-RPC 端点路径
card_path = "/.well-known/agent.json"  # Agent Card 路径
```

`enabled = false` 时，两个端点返回 404。

## 实现优先级

### P0（最小可用）

| 工作项 | 说明 | 复用程度 |
|--------|------|---------|
| 协议实体定义 | `common/src/api/a2a.rs` | 新增 |
| Agent Card 端点 | `GET /.well-known/agent.json` | 新增（简单） |
| JSON-RPC 入口 + 分发 | `POST /a2a`，挂现有 JWT 中间件 | 新增路由 |
| 映射层 | A2A ↔ ai_orz 实体转换 | 新增（核心） |
| 统一 Agent 路由 | `HrDomain::resolve_agent(ctx)`，Chat/A2A/飞书 IM 三场景共用 | 新增（复用现有查询逻辑） |
| chat 缺陷修复 | 支持两种对话上下文（默认对话框 + Project 对话框），to_agent_id 保持 Option | 修复现有缺陷 |
| `tasks/send` 异步 | handler 层显式组合 agent + project 两个维度，创建 message 后立即返回 working；唤醒由 consumer 异步闭环 | 复用 90%（复用现有 consumer 链路） |
| `tasks/get` | 复用 project/message/artifact 查询 | 复用 90% |
| `tasks/cancel` | 复用 project 取消（archive） | 复用 95% |
| 配置项 | `[a2a_server]` | 新增（简单） |

### P1（增强）

| 工作项 | 说明 |
|--------|------|
| `tasks/sendSubscribe` SSE 流式 | 复用现有 [subscribe_sse.rs](../src/handlers/finance/message/subscribe_sse.rs) 推送机制 |
| 长任务异步模式 | `tasks/send` 立即返回 working，复用 consumer 异步处理 |

## 关键复用点

| 复用项 | 来源 | 用途 |
|--------|------|------|
| JWT 认证 | [jwt_auth_middleware](../src/middleware/jwt_auth.rs) | A2A 端点身份认证 |
| 前台 Agent 路由 | [HrDomain::resolve_agent](../src/service/domain/hr/mod.rs) | A2A 请求路由到前台 Agent（统一路由方法，Chat/A2A/飞书 IM 三场景共用） |
| Agent 唤醒链路 | `RuntimeDomain.wake_agent_brain()` + `awaken()` | A2A message 触发 Agent 思考 |
| 消息投递 | `MessageDomain.send_to_agent()` | A2A message 转为内部 message |
| Project 管理 | `ProjectDomain` | A2A task ↔ project |
| SSE 推送 | [subscribe_sse.rs](../src/handlers/finance/message/subscribe_sse.rs) | A2A 流式响应（P1） |

## 测试计划

### 单元测试

- `mapper.rs` 转换逻辑
  - A2A message → MessagePo（各 part 类型）
  - Project status → A2A task state（所有状态映射）
  - MessagePo → A2A message（user/agent 两种角色）
- `agent_card.rs` 生成正确的 AgentCard 结构
- JSON-RPC 解析：合法/非法请求、未知 method

### 集成测试

- `tasks/send` 同步：外部调用 → 前台 Agent 响应 → 正确映射回 A2A Task
- `tasks/get`：查询已存在的 project，返回正确的状态和消息历史
- `tasks/cancel`：取消 project，状态正确流转
- 认证：无 token / 错误 token / 过期 token 返回 401
- Agent Card：无认证访问，返回正确结构

### 端到端测试

ai_orz A2A Client → ai_orz A2A Server（自回路）：

```
注册一个 Remote Agent（指向自己的 /a2a 端点）
  → 该 Remote Agent 收到任务
  → 通过 A2A Client 调用自己的 /a2a
  → 前台 Agent 处理并回复
  → 响应正确返回
```

## 与现有系统的边界

| 边界 | 说明 |
|------|------|
| Domain 层零侵入（A2A 相关） | A2A 概念不出现在 `src/service/domain/` 任何文件中（A2A 协议转换全部在 handler 层） |
| Agent 与 Project 维度分离 | `resolve_agent(ctx)` 只接受 ctx，不耦合 project；两个维度由 handler 层按需组合 |
| 复用 consumer 异步链路 | tasks/send 创建 message 后立即返回 working，唤醒由 consumer 异步闭环（与飞书/前端聊天完全一致） |
| 复用 awaken 链路 | A2A 触发的 Agent 执行与飞书/前端聊天走完全相同的 consumer → wake_agent_brain → awaken 路径 |
| 无新增表 | 认证走用户 JWT，路由走现有前台 Agent 逻辑，project/message/artifact 复用现有表 |
| 无 Agent 暴露开关 | 外部根本看不到具体 Agent ID，天然不暴露 |
| 配置开关 | `[a2a_server].enabled = false` 时端点返回 404，不影响现有功能 |

## 协作关系类比（核心理念）

类比找公司团队帮忙做事的流程：

1. **前台接待**：用户进入默认对话框，与前台 Agent 直接沟通（无 project）
2. **简单需求直接处理**：前台 Agent 能直接回答的问题，就在默认对话框中完成
3. **复杂需求创建 Project**：Agent 识别到需求复杂，由 Agent 内部决策创建 Project，后续沟通在 Project 上下文中进行

对应到系统中的两种对话上下文：
- **默认对话框**：无 project_id，`to_agent_id` 未指定时后端走 `resolve_agent(ctx)` 兜底
- **Project 对话框**：有 project_id，从 `project.owner_agent_id` 取 agent；若为 None 则走 `resolve_agent(ctx)` 兜底

Project 创建由 Agent 内部决策触发，不在前端显式创建。
