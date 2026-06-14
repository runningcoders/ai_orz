# 项目管理系统设计文档

## 简介

ai_orz 项目管理系统采用统一的 Project Domain 架构，整合项目（Project）、任务（Task）、产物（Artifact）三个核心子模块，支持任务 DAG 依赖管理、产物版本控制、项目进度实时计算等功能。

**设计原则**：严格单向分层、单一职责、实体优先参数传递、避免 N+1 查询。

---

## 架构设计

### 模块结构

```
service/
├── dao/                      # 数据访问层
│   ├── project/              # 项目 DAO ✅ 已完成
│   ├── task/                 # 任务 DAO ✅ 已完成
│   └── artifact/             # 产物 DAO ✅ 已完成
├── dal/                      # 数据业务层
│   ├── project.rs            # ProjectDal ✅ 已完成
│   ├── task.rs               # TaskDal ✅ 已完成
│   └── artifact.rs           # ArtifactDal ✅ 已完成
└── domain/
    └── project/              # 统一的 Project Domain ✅ 已完成
        ├── mod.rs            # Domain 入口 + Trait 定义
        ├── project.rs        # 项目业务逻辑（含统一状态流转入口）
        ├── task.rs           # 任务业务逻辑
        ├── artifact.rs       # 产物业务逻辑
        └── project_test.rs   # 单元测试（11 个测试，100% 通过）
```

### 管理面 API

Batch 2.1 已补齐 Project 管理面 API，Batch 2.2 已补齐 Task 管理面 API。Handler 位于 `src/handlers/project/project/` 与 `src/handlers/project/task/`，每个用户 action 单独文件，且只调用 Domain，不直接调用 DAL/DAO。

Project：

```http
POST   /api/v1/projects
GET    /api/v1/projects
GET    /api/v1/projects/{id}
PUT    /api/v1/projects/{id}
PUT    /api/v1/projects/{id}/status
```

共享 DTO 位于 `common/src/api/project.rs`，包括：
- `CreateProjectRequest` / `CreateProjectResponse`
- `ListProjectsQuery` / `ProjectListItem`
- `GetProjectResponse`
- `UpdateProjectRequest` / `UpdateProjectResponse`
- `UpdateProjectStatusRequest` / `UpdateProjectStatusResponse`

Task：

```http
POST   /api/v1/tasks
GET    /api/v1/tasks/{id}
GET    /api/v1/projects/{project_id}/tasks
GET    /api/v1/agents/{agent_id}/tasks
PUT    /api/v1/tasks/{id}
PUT    /api/v1/tasks/{id}/status
```

共享 DTO 位于 `common/src/api/task.rs`，包括：
- `CreateTaskRequest` / `CreateTaskResponse`
- `ListTasksQuery` / `TaskListItem`
- `GetTaskResponse`
- `UpdateTaskRequest` / `UpdateTaskResponse`
- `UpdateTaskStatusRequest` / `UpdateTaskStatusResponse`

状态更新统一使用 `/status` action，不新增 `/start`、`/complete`、`/archive`、`/cancel` 等目标状态路由；合法性与流转副作用由 `ProjectDomain::transition_status(ctx, &mut project, target_status)` / `TaskDomain::transition_status(ctx, &mut task, target_status)` 承担。

### 实体持有关系

**纯单向持有链**，无反向依赖，无独立统计结构体：

```
Project (1)
└── tasks: Vec<Task>      // 1:N 持有所有任务

Task (2)
└── artifacts: Vec<Artifact>  // 1:N 持有所有产物

Artifact (3)
└── 不反向持有任何（单向链即可）
```

**实时计算方法**：所有统计数据通过方法实时计算，不需要额外的 `ProjectStats` 结构体：

```rust
impl Project {
    pub fn total_tasks(&self) -> u64;
    pub fn completed_tasks(&self) -> u64;
    pub fn progress_percent(&self) -> f64;
    pub fn total_artifacts(&self) -> u64;
    // ... 其他统计方法
}
```

---

## DAL 接口设计原则

### 核心规则：写操作接收实体，读操作接收 ID/参数

```rust
// ✅ 写操作：接收实体（避免重复查询）
async fn create(&self, ctx: RequestContext, project: &ProjectPo) -> Result<(), AppError>;
async fn update(&self, ctx: RequestContext, project: &ProjectPo) -> Result<(), AppError>;

// ✅ 读操作：接收 ID/参数
async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<ProjectPo>, AppError>;
async fn query(&self, ctx: RequestContext, query: ProjectQuery) -> Result<Vec<ProjectPo>, AppError>;
```

### 设计优势

| 优势 | 说明 |
|------|------|
| 避免 N+1 查询 | 上层已查询到的实体直接传递，不需要 DAL 内部再查一次 |
| 事务一致性 | 实体状态由上层控制，更新时就是上层看到的状态 |
| 性能优化 | 减少不必要的数据库往返 |
| 灵活性 | 上层可以对实体做多次修改后一次性更新 |

---

## 分层架构落地细节

### PO 与业务实体边界

**严格分层规则：**

| 层级 | 可使用对象 | 说明 |
|------|------------|------|
| DAO 层 | 仅 PO | 持久化对象，仅负责数据库读写 |
| DAL 层 | 内部使用 PO，对外返回业务实体 | 完成 PO ↔ 业务实体转换 |
| Domain 层 | 仅业务实体 | 完全无 PO 依赖 |
| Handler 层 | 仅业务实体 + DTO | 不直接使用 PO |

**业务实体内部结构：**
```rust
pub struct Project {
    pub po: ProjectPo,    // 内部持有 PO
    // 业务方法...
}

pub struct Task {
    pub po: TaskPo,
    // 业务方法...
}

pub struct Artifact {
    pub po: ArtifactPo,
    // 业务方法...
}
```

**设计优势：**
- ✅ DAL 层通过 `xxx.po` 获取 PO 传递给 DAO，无需额外转换逻辑
- ✅ 业务逻辑与持久化字段完全隔离，修改互不影响
- ✅ 业务实体可以自由添加业务方法、状态流转逻辑
- ✅ 100% 向后兼容，无需修改现有测试

### RequestContext 传递规范

**统一规则：所有异步方法携带 RequestContext 参数**

```rust
// Domain 层统一签名
pub async fn create(&self, ctx: RequestContext, ...) -> Result<..., AppError>;
pub async fn get(&self, ctx: RequestContext, id: &str) -> Result<Option<...>, AppError>;
pub async fn list_by_project(&self, ctx: RequestContext, project_id: &str) -> Result<Vec<...>, AppError>;
```

**跨层传递使用 `ctx.clone()`：**
```rust
// ✅ 正确：clone 后传递，避免所有权移动问题
self.dal.find_by_id(ctx.clone(), id).await
```

---

## 数据库设计

### 1. projects 表（无需修改）

参考 `docs/project_design.md`，PO 模型与 DAO 已完成。

### 2. tasks 表（无需修改）

参考 `docs/task_design.md`，PO 模型与 DAO 已完成。

### 3. artifacts 表（需要改造）

**变更前**：
```sql
CREATE TABLE artifacts (
    id TEXT NOT NULL PRIMARY KEY,
    task_id TEXT NOT NULL,           -- 只关联任务
    -- ... 其他字段
) STRICT;
```

**变更后**：
```sql
CREATE TABLE artifacts (
    id TEXT NOT NULL PRIMARY KEY,
    project_id TEXT NOT NULL,        -- ✅ 新增：必选，关联项目
    task_id TEXT,                    -- ✅ 改造：可选，NULL 表示项目级产物
    -- ... 其他字段不变
) STRICT;
```

**含义**：
- `task_id = NOT NULL` → 任务级产物
- `task_id = NULL` → 项目级产物（不属于任何具体任务）

---

## 核心业务规则

### 任务依赖 DAG（有向无环图）

```rust
impl TaskManage {
    /// 添加任务依赖（自动检测环形）
    async fn add_dependency(
        &self,
        ctx: RequestContext,
        task_id: &str,
        dependency_task_id: &str
    ) -> Result<()> {
        // 1. DFS 深度优先检测环形依赖
        self.check_circular_dependency(ctx, task_id, dependency_task_id).await?;
        
        // 2. 添加依赖关系
        // ...
    }
    
    /// 环形依赖检测
    async fn check_circular_dependency(
        &self,
        ctx: RequestContext,
        start_task_id: &str,
        target_task_id: &str
    ) -> Result<()>;
}
```

**DAG 约束**：
- ❌ 禁止自依赖（A 不能依赖 A）
- ❌ 禁止环形依赖（A→B→C→A）
- ✅ 支持多对多依赖（一个任务依赖多个，多个任务依赖同一个）

### 项目归档与激活

**完整级联操作**：

```rust
impl ProjectManage {
    /// 归档项目
    async fn archive_project(&self, ctx: RequestContext, project_id: &str) -> Result<()> {
        // 1. 更新项目状态 → Archived
        // 2. 级联归档所有下属任务 → Archived
        // 3. 级联标记所有下属产物
        // 4. 记录操作日志
    }
    
    /// 从归档中恢复
    async fn unarchive_project(&self, ctx: RequestContext, project_id: &str) -> Result<()>;
}
```

### 产物版本管理

- 版本号由 Agent 上传时自行指定（如 "v1", "v2", "draft-20240508"）
- 系统不做强校验，Agent 自行组织版本策略

### 产物存储路径

**简化后的路径规则**：
```
/data/artifacts/projects/{project_id}/{artifact_id}
```

**设计优势**：
- ✅ 100% 避免文件名冲突
- ✅ 路径最简单，层级最少
- ✅ 根据 artifact_id 直接定位文件，不需要查询数据库
- ✅ 文件扩展名、原始文件名等信息存储在 `file_meta` JSON 中

---

## DAL 接口完整签名

> **注意**：所有 DAL 接口统一使用业务实体（Project/Task/Artifact），内部完成 PO 转换，不对外暴露 PO。

### ProjectDal

```rust
#[async_trait]
pub trait ProjectDal: Send + Sync {
    // 写操作：引用传递，避免 clone
    async fn create(&self, ctx: RequestContext, project: &Project) -> Result<(), AppError>;
    async fn update(&self, ctx: RequestContext, project: &Project) -> Result<(), AppError>;
    
    // 读操作：返回业务实体
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Project>, AppError>;
    async fn list_by_user(&self, ctx: RequestContext, user_id: &str) -> Result<Vec<Project>, AppError>;
    async fn query(&self, ctx: RequestContext, query: ProjectQuery) -> Result<Vec<Project>, AppError>;
}
```

### TaskDal

```rust
#[async_trait]
pub trait TaskDal: Send + Sync {
    // 写操作
    async fn create(&self, ctx: RequestContext, task: &Task) -> Result<(), AppError>;
    async fn update(&self, ctx: RequestContext, task: &Task) -> Result<(), AppError>;
    async fn update_status(&self, ctx: RequestContext, id: &str, status: TaskStatus, modified_by: &str) -> Result<(), AppError>;
    async fn cancel(&self, ctx: RequestContext, id: &str, modified_by: &str) -> Result<(), AppError>;
    
    // 读操作
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Task>, AppError>;
    async fn list_by_project(&self, ctx: RequestContext, project_id: &str, status: Option<TaskStatus>) -> Result<Vec<Task>, AppError>;
    async fn list_by_assignee(&self, ctx: RequestContext, assignee_type: Option<AssigneeType>, assignee_id: &str, limit: Option<usize>) -> Result<Vec<TaskPo>, AppError>;
    async fn query(&self, ctx: RequestContext, query: TaskQuery) -> Result<Vec<Task>, AppError>;
    
    // 统计
    async fn count_by_assignee(&self, ctx: RequestContext, assignee_id: &str) -> Result<u64, AppError>;
    async fn count_by_assignee_and_status(&self, ctx: RequestContext, assignee_id: &str, status: TaskStatus) -> Result<u64, AppError>;
}
```

### ArtifactDal

```rust
#[async_trait]
pub trait ArtifactDal: Send + Sync {
    // 写操作
    async fn create(&self, ctx: RequestContext, artifact: &Artifact) -> Result<(), AppError>;
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<(), AppError>;
    
    // 读操作
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Artifact>, AppError>;
    async fn list_by_project(&self, ctx: RequestContext, project_id: &str) -> Result<Vec<Artifact>, AppError>;
    async fn list_by_task(&self, ctx: RequestContext, task_id: &str) -> Result<Vec<Artifact>, AppError>;
}
```

---

## 开发阶段规划

### 阶段 0：DAO 层改造 ✅ 已完成
1. [x] ArtifactPo 模型改造（新增 `project_id`，`task_id` 改为 Option）
2. [x] 数据库迁移文件
3. [x] Artifact DAO 接口改造与实现
4. [x] Artifact DAO 单元测试

### 阶段 1：DAL 层实现 ✅ 已完成
1. [x] ProjectDal 实现 + 单元测试
2. [x] TaskDal 实现 + 单元测试
3. [x] ArtifactDal 实现 + 单元测试
4. [x] PO ↔ 业务实体转换逻辑

### 阶段 2：Domain 层实现 ✅ 已完成
1. [x] Project 子模块（项目 CRUD + 状态流转）
2. [x] Task 子模块（任务 CRUD + 状态流转）
3. [x] Artifact 子模块（产物创建/查询/删除）
4. [x] 单元测试（9 个测试，100% 通过）

### 阶段 3：Handler 层实现 ⏳ 待开发
1. [ ] Project 相关 Handler
2. [ ] Task 相关 Handler
3. [ ] Artifact 相关 Handler
4. [ ] 路由注册
5. [ ] API 集成测试

---

## 关键设计决策记录

### 1. TaskStatus::Cancelled 软删除设计

**决策：** `TaskStatus::Cancelled = 0`，`find_by_id` 默认过滤 `status != 0`

**理由：**
- 取消的任务在业务上视为"已删除"，不应出现在常规查询中
- 保留数据库记录用于审计和历史追溯
- 需要恢复或查询历史时可使用 query 方法绕过过滤

**影响：** 任务 cancel 后通过 get 方法返回 None（测试需适配此行为）

### 2. 业务实体内部持有 PO

**决策：** 业务实体内部持有 `po: XxxPo` 字段，而非独立转换

**理由：**
- 避免编写大量重复的字段转换代码
- DAL 层直接通过 `&xxx.po` 传递给 DAO，无需 clone
- 修改 PO 字段时只需修改一处，减少出错概率
- 100% 向后兼容现有测试和业务逻辑

### 3. RequestContext 跨层传递使用 clone()

**决策：** 所有跨层 ctx 传递统一使用 `ctx.clone()`

**理由：**
- RequestContext 内部是 Arc 引用，clone 成本极低
- 避免所有权移动导致的编译错误
- 与 message domain 风格保持一致

---

## 版本历史

| 日期 | 变更 | 作者 |
|------|------|------|
| 2026-05-09 | 创建设计文档，完成架构对齐与技术方案确认 | 王挺 |
| 2026-05-11 | PO 与业务实体分层架构落地，DAO/DAL/Domain 三层全部完成，267 个测试 100% 通过 | 王挺 |

---

## Agent 自主思考架构（2026-05-11 更新）

### 核心理念

**Agent 是自主主体，系统仅提供基础设施。** 这是整个架构设计的出发点：

- 系统不预先为 Agent 组装所有数据和工具，Agent 在需要时自主查询获取
- Project Domain 作为唯一编排层，承载 Agent 的思考循环逻辑
- 所有中间过程（思考、工具调用、请求确认）均为可追溯的消息
- 遵循严格分层：Handler → Domain → DAL → DAO

### 设计决策总览

| 决策项 | 结论 | 理由 |
|--------|------|------|
| 编排层位置 | Project Domain 唯一编排 | 符合领域驱动设计，避免过度封装 |
| 工具绑定时机 | Agent 入职流程绑定，运行时动态注入 | 灵活配置，支持不同角色差异化工具集 |
| 会话 vs Project | 一一对应，无类型区分 | 简化设计，前端会话直接映射 |
| 工具调用实现 | 自定义消息格式，不依赖 LLM 原生 Function Calling | 跨模型兼容，支持多模板扩展 |
| 思考深度限制 | Agent 可配置属性，默认 10 层 | 超过阈值触发反思求助 |
| 复杂度升级 | Agent 判断 + 用户确认 | 避免简单聊天变成复杂项目 |
| 思考循环执行 | 独立异步任务，不阻塞消费者 | 提升系统并发能力 |

### 思考循环状态机

```
用户发消息 (UserMessage)
    ↓
进入 Project Domain process_agent_message
    ↓
┌─────────────────────────────────────┐
│      思考循环 (异步任务执行)         │
│  ┌─────────────────────────────┐   │
│  │ 1. 组装 Prompt (上下文+历史)│   │
│  │ 2. 调用 Cortex LLM 推理     │   │
│  │ 3. 解析 LLM 输出格式        │   │
│  └─────────────────────────────┘   │
│           ↓ 分支判断                │
│  ┌─────────┐  ┌──────────┐  ┌──────┐│
│  │ reply   │→│直接回用户│  │tool  ││
│  └─────────┘  └──────────┘  └──────┘│
│                              ↓      │
│                      发 ToolCallRequest │
│                      给 System 角色    │
│                              ↓      │
│                  System 消费者执行工具│
│                              ↓      │
│                   发 ToolCallResult  │
│                   回给 Agent 角色    │
│                              ↓      │
│                  回到思考循环起点    │
│  ┌─────────┐                        │
│  │ confirm │→ 发 ConfirmRequest 给用户│
│  └─────────┘                        │
│           ↓                          │
│     深度计数 +1，超过阈值？→ 触发反思求助│
└─────────────────────────────────────┘
```

### 消息格式扩展

现有 `MessageType` 枚举需新增 4 种类型：

```rust
pub enum MessageType {
    UserMessage = 0,      // 用户 → Agent
    AgentMessage = 1,     // Agent → 用户
    SystemMessage = 2,    // System → Agent
    ToolCallRequest = 3,  // Agent → System（工具调用请求）
    ToolCallResult = 4,   // System → Agent（工具执行结果）
    ConfirmRequest = 5,   // Agent → User（确认请求，如升级项目）
    ConfirmResponse = 6,  // User → Agent（确认回复）
}
```

**LLM 输出格式设计**（JSON 模板）：

```json
{
  "type": "reply|tool|confirm",
  "content": "回复内容或工具参数",
  "tool_name": "tool_name",     // type=tool 时必填
  "tool_args": {},              // type=tool 时必填
  "confirm_title": "标题",      // type=confirm 时必填
  "confirm_options": ["是","否"] // type=confirm 时必填
}
```

### 实现路径四阶段

**阶段 1：最小思考闭环**
- Project Domain 实现 `process_agent_message` 方法框架
- 基础 Prompt 组装（上下文 + 最近消息历史）
- 调用 CortexDao 完成 LLM 推理
- 解析 `reply` 类型输出并回发给用户
- 深度计数校验

**阶段 2：工具调用闭环**
- 工具注册表设计（ContextTool 统一接口）
- System 消费者实现工具执行逻辑
- 工具执行结果回发为 ToolCallResult 消息
- 完整多轮思考循环（工具调用 → 结果返回 → 继续思考）

**阶段 3：组织能力工具**
- `create_project`：升级当前会话为正式项目
- `create_task`：在 Project 下创建任务
- `update_task_status`：更新任务状态
- `assign_agent`：分配任务给其他 Agent
- 用户确认机制（ConfirmRequest/Response）

**阶段 4：记忆与技能工具**
- `query_memory`：查询 Agent 个人记忆
- `load_skill`：加载技能到 Agent
- `search_history`：搜索历史消息
- `summarize_context`：总结当前 Project 上下文

### 核心改动模块清单

| 模块 | 改动内容 |
|------|----------|
| `common/src/enums/message.rs` | MessageType 新增 4 个枚举值 |
| `src/service/dal/agent/` | Agent 新增 max_thinking_depth 属性 |
| `src/service/domain/project/` | 新增 process_agent_message 思考循环 |
| `src/consumer/message.rs` | handle_system_message 实现工具执行 |
| `src/pkg/tool_registry/` | 工具统一注册与调度接口 |
| `src/service/dao/cortex/` | 新增 Prompt 模板管理 |

## 参考文档

- [ai-orz-domain-layer-implementation Skill](../.hermes/skills/rust/ai-orz-domain-layer-implementation/SKILL.md)
- [consumer_architecture.md](./consumer_architecture.md) - 消息消费者框架
- [tool_design.md](./tool_design.md) - 工具调用链路设计
- [Task 模块设计](./task_design.md)
- [分层架构规范](./architecture.md)
- [测试规范](./testing_guidelines.md)
