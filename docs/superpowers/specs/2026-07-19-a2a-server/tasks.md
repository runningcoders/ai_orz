# A2A Server 任务分解

> 配套：[spec.md](./spec.md) | [checklist.md](./checklist.md) | [实施计划全文](../plans/2026-07-19-a2a-server.md)
>
> **执行说明**：每个 Task 的完整代码和详细步骤见实施计划对应章节。本文档是任务索引和依赖图，便于排期和并行规划。
>
> **完成状态（2026-07-20）**：
>
> | Task | 状态 | 说明 |
> |------|------|------|
> | Task 0.1 | ✅ 已完成 | resolve_agent + 前台 Agent 查询 HTTP API |
> | Task 0.2 | ✅ 已完成 | CreateProjectRequest 支持 owner_agent_id |
> | Task 0.3 | ✅ 已完成 | send_message_to_agent 两种对话上下文 + resolve_agent 兜底 + 前端 chat 默认对话框 + get_reception_agent_api + 新建项目弹窗（自动绑定前台 Agent）+ 侧边栏置顶「默认对话」条目 |
> | Task 1 | ✅ 已完成 | A2A 协议实体定义（6 个测试） |
> | Task 2 | ✅ 已完成 | A2A Server 配置项 |
> | Task 4 | ✅ 已完成 | Handler 模块入口 + 映射层（10 个测试） |
> | Task 5 | ✅ 已完成 | Agent Card 端点 |
> | Task 6 | ✅ 已完成 | tasks/send 异步提交 |
> | Task 7 | ✅ 已完成 | tasks/get 查询 |
> | Task 8 | ✅ 已完成 | tasks/cancel 取消 |
> | Task 9 | ✅ 已完成 | JSON-RPC 入口和方法分发 |
> | Task 10 | ✅ 已完成 | 路由注册 |
> | Task 11 | ✅ 已完成 | 端到端集成测试（3 个测试） |
> | Task 12 | ✅ 已完成 | 文档更新和最终验证 |
>
> **遗留项（P2）**：PushNotifications 推送通知

## 任务依赖图

```
Phase 0（路由收敛 + chat 修复）
  Task 0.1 (resolve_agent(ctx) + GET /api/v1/hr/agents/reception)
    ↓
    ├→ Task 0.2 (create_project handler 纯粹透传 owner_agent_id)
    ↓     ↓
    ↓     └→ Task 0.3 (send_message_to_agent 支持默认对话框 + resolve_agent 兜底)
    ↓
    └→ Task 6 (tasks/send handler 调 resolve_agent(ctx))

Phase 1（A2A 基础设施）
  Task 1 (协议实体)  ← 独立
    ↓
  Task 2 (配置项)    ← 独立
    ↓
  Task 4 (mapper)    ← 依赖 Task 1
    ↓
  Task 5 (Agent Card) ← 依赖 Task 1, 2

Phase 2（A2A 方法实现）
  Task 6 (tasks/send) ← 依赖 Task 0.1, 4
    ↓
  Task 7 (tasks/get)  ← 依赖 Task 4
  Task 8 (tasks/cancel) ← 依赖 Task 4
    ↓
  Task 9 (JSON-RPC)   ← 依赖 Task 6, 7, 8

Phase 3（路由 + 测试 + 文档）
  Task 10 (路由)      ← 依赖 Task 5, 9
    ↓
  Task 11 (集成测试)  ← 依赖 Task 10
    ↓
  Task 12 (文档)      ← 最后
```

**关键架构原则**：
- **agent 与 project 是两个维度，不在 hr domain 中融合**：`resolve_agent(ctx)` 只接受 ctx，不查询/感知 project
- 两个维度由调用方（handler 层或前端）按需组合
- **协作关系类比（核心理念）**：默认对话框=与前台直接沟通（无 project）；Project 对话框=Agent 识别复杂需求后创建 Project 的上下文沟通。Project 创建由 Agent 内部决策触发，不在前端显式创建
- **to_agent_id 保持 Option**：用户选定 Agent 时前端显式传入；未指定时后端走 `resolve_agent(ctx)` 兜底
- **handler 层可调 resolve_agent**：resolve_agent 本就是给 handler 用的统一路由方法，handler 层在默认对话框或 project 未绑定 agent 时调 resolve_agent 兜底
- **create_project handler 纯粹透传**：`create_project` handler 纯粹透传 `owner_agent_id`，不调 resolve_agent，不依赖 hr domain
- A2A `tasks/send` 由 handler 层显式组合：先 `resolve_agent(ctx)` 拿 agent，再创建 project 绑定
- **tasks/send 异步流程**：handler 只创建 project + message 后立即返回 working 状态，唤醒由 consumer 异步闭环（复用现有链路），客户端通过 tasks/get 轮询。与飞书/前端聊天的唤醒链路完全一致

## Phase 0：路由收敛 + chat 缺陷修复

### Task 0.1：HrDomain 统一 `resolve_agent` 方法 + 前台 Agent 查询 HTTP API

**目标**：在 `HrDomain` trait 新增 `resolve_agent(ctx) -> Result<Option<Agent>>`，**只接受 ctx，不耦合 project**（agent 与 project 是两个维度，不在 hr domain 中融合）。同时新增 HTTP API `GET /api/v1/hr/agents/reception` 供前端显示推荐前台 Agent。

**Files**：
- Modify: `src/service/domain/hr/mod.rs`
- Modify: `src/consumer/adapter.rs`
- Create: `src/handlers/hr/agent/get_reception_agent.rs`
- Modify: `src/handlers/hr/agent/mod.rs`
- Modify: `src/router.rs`

**关键步骤**：
1. `HrDomain` trait 加 `#[async_trait]` + 新增 `resolve_agent(ctx)` 方法签名（**不接受 project 参数**）
2. `impl HrDomain for HrDomainImpl` 加 `#[async_trait]` + 实现方法（两级兜底：feishu_reception → 任意 Onboarded，**不感知 project**）
3. 确认 `AgentManage::get` 方法存在（若无需补加）
4. `find_reception_agent_id` 改为调用 `resolve_agent(ctx)`
5. 新增 `get_reception_agent.rs` HTTP API handler，返回 `{ agent_id, agent_name }`，无可用 agent 返回 404
6. `handlers/hr/agent/mod.rs` 注册新子模块
7. `router.rs` 注册 `GET /api/v1/hr/agents/reception` 路由（JWT 保护）
8. `cargo check` + `cargo test --lib consumer && cargo test --lib service::domain::hr`
9. 提交

**详见**：[实施计划 §Task 0.1](../plans/2026-07-19-a2a-server.md#task-01-hrdomain-统一路由方法-resolve_agent--前台-agent-查询-http-api)

---

### Task 0.2：CreateProjectRequest 支持 `owner_agent_id`（handler 纯粹透传，不做兜底路由）

**目标**：修复 `owner_agent_id` 硬编码 None 的缺陷。`ProjectDomainImpl::create` 直接绑定 `owner_agent_id`；**create_project handler 纯粹透传 `params.owner_agent_id`，不调 resolve_agent，不依赖 hr domain**。Project 创建由 Agent 内部决策触发（不在本次范围），create_project API 供 A2A tasks/send 等场景使用。

**Files**：
- Modify: `common/src/api/project.rs`
- Modify: `src/service/domain/project/mod.rs`
- Modify: `src/service/domain/project/project.rs`
- Modify: `src/handlers/project/project/create_project.rs`

**关键步骤**：
1. `CreateProjectRequest` 新增 `owner_agent_id: Option<String>`
2. `ProjectManage::create` trait 新增 `owner_agent_id` 参数
3. `ProjectDomainImpl::create` 实现直接绑定 `owner_agent_id`，不内部调用 resolve_agent
4. handler **纯粹透传 `params.owner_agent_id`**，**不调 resolve_agent**，**不依赖 hr domain**
5. `cargo check` + `cargo test --lib handlers::project && cargo test --lib service::domain::project`
6. 提交

**依赖**：Task 0.1

**详见**：[实施计划 §Task 0.2](../plans/2026-07-19-a2a-server.md#task-02-修复-chat-缺陷-createprojectrequest-支持-owner_agent_idhandler-纯粹接收参数不做兜底路由)

---

### Task 0.3：send_message_to_agent 支持默认对话框（无 project）+ to_agent_id 保持 Option + 后端 resolve_agent 兜底

**目标**：
1. handler 层支持**两种对话上下文**（协作关系类比）：
   - **默认对话框**：与前台 Agent 直接沟通（无 project_id），`to_agent_id` 未指定时后端调 `resolve_agent(ctx)` 兜底
   - **Project 对话框**：在 Project 上下文中沟通（有 project_id），从 `project.owner_agent_id` 取；若为 None 则调 `resolve_agent(ctx)` 兜底
2. `to_agent_id` 保持 `Option<String>`：用户选定 Agent 时前端显式传入；未指定时后端走 `resolve_agent(ctx)` 兜底
3. **不在前端创建默认 project**：Project 创建由 Agent 内部决策触发（识别复杂需求后创建），不在本次 A2A Server 范围

**Files**：
- Modify: `common/src/api/neural_tools.rs`
- Modify: `src/handlers/finance/message/send_message_to_agent.rs`
- Modify: `frontend/src/api/mod.rs`
- Modify: `frontend/src/pages/message/chat.rs`

**关键步骤**：
1. `SendMessageToAgentParams.to_agent_id` 改为 `Option<String>` + `#[serde(default)]`
2. handler 实现（支持两种对话上下文）：
   - `to_agent_id` 显式指定 → 直接用（用户选定 Agent 优先）
   - `to_agent_id` 未指定 + `project_id` 存在（Project 对话框）→ 从 `project.owner_agent_id` 取；若为 None 则调 `resolve_agent(ctx)` 兜底
   - `to_agent_id` 未指定 + `project_id=None`（默认对话框）→ 调 `resolve_agent(ctx)` 兜底
   - **handler 层调 `resolve_agent(ctx)` 是合理的** — resolve_agent 本就是给 handler 用的统一路由方法
3. 前端 `api/mod.rs` 新增 `get_reception_agent_api` 函数（可选，用于显示推荐前台 Agent）
4. 前端 chat.rs 默认对话框逻辑：
   - **默认对话框不创建 project**：`project_id` 为 `None`，直接调 `send_message_to_agent`
   - `to_agent_id` 由用户选择决定（用户选定 Agent 时显式传入，未选定时后端走 resolve_agent 兜底）
   - Project 对话框用 `project_id`，`to_agent_id` 可不传（后端从 `project.owner_agent_id` 取）
   - **不在前端创建默认 project** — Project 创建由 Agent 内部决策触发（不在本次范围）
5. `cargo check && cd frontend && cargo check` + `cargo test --lib handlers::finance::message`
6. 提交

**依赖**：Task 0.1, 0.2

**详见**：[实施计划 §Task 0.3](../plans/2026-07-19-a2a-server.md#task-03-修复-chat-缺陷-send_message_to_agent-支持默认对话框无-project--to_agent_id-保持-option--后端-resolve_agent-兜底)

---

## Phase 1：A2A 基础设施

### Task 1：A2A 协议实体定义

**目标**：定义 A2A 协议 v0.3.0 核心实体（AgentCard / JsonRpc / A2aTask / A2aMessage 等），前后端共享。

**Files**：
- Create: `common/src/api/a2a.rs`
- Create: `common/src/api/a2a_test.rs`
- Modify: `common/src/api/mod.rs`

**关键步骤**：
1. `common/src/api/mod.rs` 注册 `a2a` 模块和 re-export
2. 创建 `a2a.rs` 定义所有协议实体（详见实施计划代码）
3. 创建 `a2a_test.rs` 6 个序列化测试
4. `cargo test -p common --lib api::a2a_test` — 6 tests PASS
5. 提交

**详见**：[实施计划 §Task 1](../plans/2026-07-19-a2a-server.md#task-1-a2a-协议实体定义)

---

### Task 2：A2A Server 配置项

**目标**：新增 `A2aServerConfig` 配置段（enabled / protocol_version / endpoint / card_path），默认 enabled=false。

**Files**：
- Modify: `common/src/config.rs`

**关键步骤**：
1. `AppConfig` 新增 `a2a_server: A2aServerConfig` 字段
2. 新增 `A2aServerConfig` struct + `Default` impl + 三个 default 函数
3. `cargo check -p common`
4. 提交

**详见**：[实施计划 §Task 2](../plans/2026-07-19-a2a-server.md#task-2-a2a-server-配置项)

---

### Task 4：A2A Handler 模块入口 + 映射层

**目标**：创建 `src/handlers/a2a/` 模块入口和 `mapper.rs` 纯函数转换层（domain 层不感知 A2A）。

**Files**：
- Modify: `src/handlers/mod.rs`
- Create: `src/handlers/a2a/mod.rs`
- Create: `src/handlers/a2a/mapper.rs`
- Create: `src/handlers/a2a/mapper_test.rs`

**关键步骤**：
1. `src/handlers/mod.rs` 注册 `pub mod a2a;`
2. 创建 `a2a/mod.rs` 声明子模块
3. 创建 `mapper.rs` 实现 5 个纯函数（详见实施计划代码）：
   - `project_status_to_a2a_state`
   - `message_to_a2a`
   - `artifact_to_a2a`
   - `build_a2a_task`（用 `Utc::now().to_rfc3339()`）
   - `extract_text_from_a2a_message`
4. 创建 `mapper_test.rs` 10 个测试
5. `cargo test --lib handlers::a2a::mapper_test` — 10 tests PASS
6. 提交

**依赖**：Task 1

**详见**：[实施计划 §Task 4](../plans/2026-07-19-a2a-server.md#task-4-a2a-handler-模块入口--映射层)

> 注：原 Task 3「find_reception_agent 提取」已合并到 Task 0.1（`resolve_agent` 是其超集）。

---

### Task 5：Agent Card 端点

**目标**：实现 `GET /.well-known/agent.json` 公开端点，返回组织级能力描述。

**Files**：
- Create: `src/handlers/a2a/agent_card.rs`

**关键步骤**：
1. 创建 `agent_card.rs` 实现 `get_agent_card() -> Json<AgentCard>`
2. 配置通过 `crate::config::get()` 读取
3. `cargo check`
4. 提交

**依赖**：Task 1, 2

**详见**：[实施计划 §Task 5](../plans/2026-07-19-a2a-server.md#task-5-agent-card-端点)

---

## Phase 2：A2A 方法实现

### Task 6：tasks/send 异步提交

**目标**：实现 `tasks/send` 异步提交，handler 层**显式组合 agent 与 project 两个维度**：先 `resolve_agent(ctx)` 获取 agent（不传 project，agent 与 project 不耦合），再用 `agent.id` 作为 `owner_agent_id` 创建 project 绑定，创建 message 后立即返回 working 状态。**唤醒由 consumer 异步闭环**（复用现有链路），客户端通过 `tasks/get` 轮询结果。

**Files**：
- Create: `src/handlers/a2a/send_task.rs`

**关键步骤**：
1. 创建 `send_task.rs` 实现 `handle_send_task(ctx, params) -> Result<A2aTask>`
2. **异步流程**（与飞书/前端聊天的唤醒链路完全一致）：
   - **handler 层显式查询前台 Agent**：先 `resolve_agent(ctx)` 获取 agent（不传 project 参数）
   - 创建 project 时 `owner_agent_id=Some(agent_id.clone())`（handler 层已查询 agent，直接绑定）
   - 启动 project（`start`）
   - 创建 message（`send_to_agent`）→ 自动入队 event_queue
   - 立即返回 working 状态的 A2aTask（`ProjectStatus::InProgress`），**不等待 Agent 回复**
   - project_name 用 `char_indices().nth(50)` UTF-8 安全截断
   - **handler 层不调用 `wake_agent_brain` / `awaken`** — 唤醒由 consumer 异步闭环
   - **不需要 `runtime_domain` 导入** — handler 层不感知 runtime
   - `agent` 不声明为 `mut`
3. `cargo check`
4. 提交

**依赖**：Task 0.1, 4

**详见**：[实施计划 §Task 6](../plans/2026-07-19-a2a-server.md#task-6-taskssend-异步提交)

---

### Task 7：tasks/get 查询

**目标**：实现 `tasks/get` 查询任务状态。

**Files**：
- Create: `src/handlers/a2a/get_task.rs`

**关键步骤**：
1. 创建 `get_task.rs` 实现 `handle_get_task(ctx, params) -> Result<A2aTask>`
2. 流程：查 project → 查 messages → 查 artifacts → `build_a2a_task`
3. project 不存在返回 `Error::not_found`
4. `cargo check`
5. 提交

**依赖**：Task 4

**详见**：[实施计划 §Task 7](../plans/2026-07-19-a2a-server.md#task-7-tasksget-查询任务)

---

### Task 8：tasks/cancel 取消

**目标**：实现 `tasks/cancel` 取消任务（对应 project archive）。

**Files**：
- Create: `src/handlers/a2a/cancel_task.rs`

**关键步骤**：
1. 创建 `cancel_task.rs` 实现 `handle_cancel_task(ctx, params) -> Result<A2aTask>`
2. 流程：查 project → archive → 重新查 project → 查 messages + artifacts → `build_a2a_task`
3. project 不存在返回 `Error::not_found`
4. `cargo check`
5. 提交

**依赖**：Task 4

**详见**：[实施计划 §Task 8](../plans/2026-07-19-a2a-server.md#task-8-taskscancel-取消任务)

---

### Task 9：JSON-RPC 入口和方法分发

**目标**：实现 `POST /a2a` JSON-RPC 入口，按 method 分发到 send/get/cancel。

**Files**：
- Create: `src/handlers/a2a/jsonrpc.rs`

**关键步骤**：
1. 创建 `jsonrpc.rs` 实现 `handle_jsonrpc(Extension<ctx>, Json<req>) -> Json<JsonRpcResponse>`
2. 用 `Extension(ctx): Extension<RequestContext>` 提取上下文
3. 检查 `config.a2a_server.enabled` + `jsonrpc == "2.0"`
4. 按 method 分发，未匹配返回 `METHOD_NOT_FOUND`
5. 错误响应用 `format!("{}", e)`（不用 `{:?}`）
6. `cargo check`
7. 提交

**依赖**：Task 6, 7, 8

**详见**：[实施计划 §Task 9](../plans/2026-07-19-a2a-server.md#task-9-json-rpc-入口和方法分发)

---

## Phase 3：路由 + 测试 + 文档

### Task 10：路由注册

**目标**：在 `create_router` 注册 A2A 路由（Agent Card 公开 + JSON-RPC JWT 保护）。

**Files**：
- Modify: `src/router.rs`

**关键步骤**：
1. 新增 `GET /.well-known/agent.json` 路由（仅挂 `request_context_middleware`）
2. 新增 `POST /a2a` 路由（`jwt_auth_middleware` + `request_context_middleware`）
3. JWT 必须在 RequestContext 中间件外层（先执行）
4. `cargo check`
5. 手动 curl 测试 Agent Card + JSON-RPC 401
6. 提交

**依赖**：Task 5, 9

**详见**：[实施计划 §Task 10](../plans/2026-07-19-a2a-server.md#task-10-路由注册)

---

### Task 11：端到端集成测试

**目标**：新增 A2A Server 集成测试，覆盖路由、查询不存在、取消不存在等场景。

**Files**：
- Create: `src/handlers/a2a/integration_test.rs`
- Modify: `src/handlers/a2a/mod.rs`

**关键步骤**：
1. `a2a/mod.rs` 新增 `#[cfg(test)] mod integration_test;`
2. 创建 `integration_test.rs` 包含：
   - `init_a2a_test_env(pool)` 初始化所有 DAO/DAL/Domain（不含 `message_push::init`）
   - `create_test_reception_agent(ctx)` 创建 feishu_reception 角色 Onboarded Agent
   - 3 个 `#[sqlx::test]`：resolve_agent / get 不存在 / cancel 不存在
3. `cargo test --lib handlers::a2a::integration_test` — 3 tests PASS
4. `cargo test --lib` 全量回归 PASS
5. 提交

**依赖**：Task 10

**详见**：[实施计划 §Task 11](../plans/2026-07-19-a2a-server.md#task-11-端到端集成测试)

---

### Task 12：文档更新和最终验证

**目标**：更新设计文档实现状态和 README，全量验证。

**Files**：
- Modify: `docs/a2a_server_design.md`
- Modify: `README.md`

**关键步骤**：
1. `docs/a2a_server_design.md` 顶部新增「实现完成状态」表格
2. `README.md` 核心能力列表更新 A2A 条目（双向支持）
3. `cargo check && cargo test --lib` 全部通过
4. 提交并推送

**依赖**：Task 11

**详见**：[实施计划 §Task 12](../plans/2026-07-19-a2a-server.md#task-12-文档更新和最终验证)

---

## 任务统计

| Phase | Task 数量 | 新增文件 | 修改文件 | 新增测试 |
|-------|-----------|----------|----------|----------|
| Phase 0 | 3 | 1 | 9 | 0（依赖现有测试验证） |
| Phase 1 | 4 | 5 | 2 | 16（6 + 10） |
| Phase 2 | 4 | 4 | 0 | 0（依赖集成测试） |
| Phase 3 | 3 | 1 | 3 | 3 |
| **合计** | **14** | **11** | **15** | **19** |

## 执行节奏建议

- **每个 Task 单独提交**：保持 commit 粒度小，便于 review 和回滚
- **每个 Phase 切换做编译 + 测试验证**：确保 Phase 间无回归
- **Phase 0 优先完成**：路由收敛是后续所有 Task 的基础
- **Phase 1/2 可部分并行**：Task 1/2 独立，可与 Phase 0 后半段并行
- **Phase 3 必须最后**：依赖前面所有 Task

## 风险点

| 风险 | 缓解措施 |
|------|----------|
| `AgentManage::get` 方法可能不存在 | Task 0.1 Step 3 已明确：若不存在则补加 |
| `async_trait` crate 未在 dependencies | Task 0.1 需确认 Cargo.toml，缺失则添加 |
| `HrDomain` trait 添加 `#[async_trait]` 后所有 impl 块都要加 | Task 0.1 Step 1/2 已明确两处都加 |
| `request_context_middleware` 签名可能与 plan 描述不一致 | Task 10 实施时需核对实际签名 |
| 集成测试初始化遗漏某个 Domain | Task 11 init 函数已列出全部，实施时按实际为准 |
