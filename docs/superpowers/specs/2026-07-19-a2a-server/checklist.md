# A2A Server 验收检查清单

> 配套：[spec.md](./spec.md) | [tasks.md](./tasks.md)

## Phase 0：路由收敛 + chat 缺陷修复

### Task 0.1：HrDomain 统一 `resolve_agent` 方法 + 前台 Agent 查询 HTTP API

- [x] `HrDomain` trait 添加 `#[async_trait]` 宏
- [x] `HrDomain` trait 新增 `resolve_agent(ctx) -> Result<Option<Agent>>` 方法（**只接受 ctx，不接受 project 参数**）
- [x] `impl HrDomain for HrDomainImpl` 块添加 `#[async_trait]` 并实现 `resolve_agent`
- [x] 实现包含两级兜底优先级（feishu_reception → 任意 Onboarded），**不查询/感知 project**
- [x] **agent 与 project 维度分离**：`resolve_agent` 只返回可用前台 Agent，不耦合 project
- [x] `AgentManage::get(ctx, id) -> Result<Option<Agent>>` 方法存在（Task 0.3 handler 层判断 owner_agent_id 后查询 agent 时需要，若不存在则补加）
- [x] `src/consumer/adapter.rs` 的 `find_reception_agent_id` 改为调用 `resolve_agent(ctx)`
- [x] 新增 `src/handlers/hr/agent/get_reception_agent.rs` HTTP API handler
  - 返回 `{ agent_id, agent_name }`
  - 供前端显示推荐前台 Agent（如默认对话框顶部显示"当前前台：XXX"）
  - 无可用 agent 返回 404
- [x] `src/handlers/hr/agent/mod.rs` 注册 `get_reception_agent` 子模块
- [x] `src/router.rs` 注册 `GET /api/v1/hr/agents/reception` 路由（JWT 保护）
- [x] `cargo check` 编译通过
- [x] `cargo test --lib consumer && cargo test --lib service::domain::hr` 全部 PASS
- [x] 提交 commit

### Task 0.2：CreateProjectRequest 支持 `owner_agent_id`（handler 纯粹透传，不做兜底路由）

- [x] `common/src/api/project.rs` 的 `CreateProjectRequest` 新增 `owner_agent_id: Option<String>`
- [x] `src/service/domain/project/mod.rs` 的 `ProjectManage::create` trait 新增 `owner_agent_id` 参数
- [x] `src/service/domain/project/project.rs` 的 `ProjectDomainImpl::create` 实现：
  - 接收 `owner_agent_id` 参数
  - **直接绑定到 Project，不内部调用 resolve_agent**
  - 不再硬编码 `None`
- [x] `src/handlers/project/project/create_project.rs` handler **纯粹透传 `params.owner_agent_id`**：
  - **不调 `resolve_agent`**
  - **不依赖 hr domain**（解耦）
  - 未传则为 None
- [x] `cargo check` 编译通过
- [x] `cargo test --lib handlers::project && cargo test --lib service::domain::project` 全部 PASS
- [x] 提交 commit

### Task 0.3：send_message_to_agent 支持默认对话框（无 project）+ to_agent_id 保持 Option + 后端 resolve_agent 兜底

- [x] `common/src/api/neural_tools.rs` 的 `SendMessageToAgentParams.to_agent_id` 改为 `Option<String>` + `#[serde(default)]`
- [x] `src/handlers/finance/message/send_message_to_agent.rs` handler 支持**两种对话上下文**：
  - `to_agent_id` 显式指定时直接使用（用户选定 Agent 优先）
  - `to_agent_id` 未指定 + `project_id` 存在（Project 对话框）：从 `project.owner_agent_id` 取；若为 None 则调 `resolve_agent(ctx)` 兜底
  - `to_agent_id` 未指定 + `project_id=None`（默认对话框）：调 `resolve_agent(ctx)` 兜底
  - **handler 层调 `resolve_agent(ctx)` 是合理的** — resolve_agent 本就是给 handler 用的统一路由方法
- [x] `frontend/src/api/mod.rs` 新增 `get_reception_agent_api` 函数（可选，用于显示推荐前台 Agent）
- [x] `frontend/src/pages/message/chat.rs` chat 默认对话框逻辑：
  - **默认对话框不创建 project**：`project_id` 为 `None`，直接调 `send_message_to_agent`
  - `to_agent_id` 由用户选择决定（用户选定 Agent 时显式传入，未选定时后端走 resolve_agent 兜底）
  - Project 对话框用 `project_id`，`to_agent_id` 可不传（后端从 `project.owner_agent_id` 取）
  - **不在前端创建默认 project** — Project 创建由 Agent 内部决策触发（不在本次范围）
- [x] `cargo check && cd frontend && cargo check` 编译通过
- [x] `cargo test --lib handlers::finance::message` 全部 PASS
- [x] 提交 commit

## Phase 1：A2A 基础设施

### Task 1：A2A 协议实体定义

- [x] `common/src/api/mod.rs` 注册 `a2a` 模块和 re-export
- [x] `common/src/api/a2a.rs` 定义完整协议实体：
  - `AgentCard` / `AgentCapabilities` / `AgentSkill`
  - `JsonRpcRequest` / `JsonRpcResponse` / `JsonRpcError` / `error_codes` 模块
  - `A2aTask` / `A2aTaskStatus` / `A2aTaskState`（snake_case 序列化）
  - `A2aMessage` / `A2aMessagePart`（tag = "type"）/ `A2aFilePart`
  - `A2aArtifact`
  - `SendTaskParams` / `GetTaskParams` / `CancelTaskParams`
- [x] `JsonRpcResponse::success(id, result)` 和 `error(id, code, message)` 工厂方法
- [x] `common/src/api/a2a_test.rs` 覆盖 6 个序列化测试用例
- [x] `cargo test -p common --lib api::a2a_test` 6 tests PASS
- [x] 提交 commit

### Task 2：A2A Server 配置项

- [x] `common/src/config.rs` 的 `AppConfig` 新增 `a2a_server: A2aServerConfig` 字段（`#[serde(default)]`）
- [x] 新增 `A2aServerConfig` 结构体（enabled / protocol_version / endpoint / card_path）
- [x] `Default for A2aServerConfig` 实现（默认 enabled=false）
- [x] 三个 default 函数：`default_a2a_protocol_version` / `default_a2a_endpoint` / `default_a2a_card_path`
- [x] `cargo check -p common` 编译通过
- [x] 提交 commit

### Task 4：A2A Handler 模块入口 + 映射层

- [x] `src/handlers/mod.rs` 注册 `pub mod a2a;`
- [x] `src/handlers/a2a/mod.rs` 声明子模块（agent_card / cancel_task / get_task / jsonrpc / mapper / send_task）和 `#[cfg(test)] mod mapper_test`
- [x] `src/handlers/a2a/mapper.rs` 实现 4 个纯函数：
  - `project_status_to_a2a_state(status)` — 6 个 ProjectStatus 变体正确映射
  - `message_to_a2a(msg, task_id)` — User→"user"，其余→"agent"
  - `artifact_to_a2a(artifact)`
  - `build_a2a_task(task_id, status, messages, artifacts, session_id)` — 使用 `Utc::now().to_rfc3339()`
  - `extract_text_from_a2a_message(msg)` — 拼接所有 Text part，忽略 File/Data
- [x] `src/handlers/a2a/mapper_test.rs` 覆盖 10 个测试用例（含状态映射、文本提取、消息角色映射、build_a2a_task 组装）
- [x] 测试用 `Message::new_with_context` 15 参数构造函数
- [x] `cargo test --lib handlers::a2a::mapper_test` 10 tests PASS
- [x] 提交 commit

### Task 5：Agent Card 端点

- [x] `src/handlers/a2a/agent_card.rs` 实现 `get_agent_card() -> Json<AgentCard>`
- [x] 配置通过 `crate::config::get()` 读取（无 State 提取器）
- [x] 返回组织级能力描述（统一入口，不列内部 Agent）
- [x] `cargo check` 编译通过
- [x] 提交 commit

## Phase 2：A2A 方法实现

### Task 6：tasks/send 异步提交

- [x] `src/handlers/a2a/send_task.rs` 实现 `handle_send_task(ctx, params) -> Result<A2aTask>`
- [x] **异步流程**（与飞书/前端聊天的唤醒链路完全一致）：
  1. JWT 提取 user_id，校验非空
  2. **handler 层显式查询前台 Agent**：`resolve_agent(ctx)` 获取 agent（agent 与 project 是两个维度，不耦合）
  3. 创建 project，将 `agent.id` 作为 `owner_agent_id` 绑定（`Some(agent_id.clone())`）
  4. 启动 project（`start`）
  5. 创建 message（`send_to_agent`）→ 自动入队 event_queue
  6. 立即返回 working 状态的 A2aTask（`ProjectStatus::InProgress`），**不等待 Agent 回复**
- [x] **handler 层不调用 `wake_agent_brain` / `awaken`** — 唤醒由 consumer 异步闭环
- [x] **不需要 `runtime_domain` 导入** — handler 层不感知 runtime
- [x] project_name 使用 `char_indices().nth(50)` 做 UTF-8 安全截断
- [x] `agent` 不声明为 `mut`（不调用 `wake_agent_brain`）
- [x] **resolve_agent 只接受 ctx**：不传 project 参数，符合「agent 与 project 是两个维度」原则
- [x] **handler 层显式组合两个维度**：先拿 agent，再创建 project 绑定
- [x] **客户端轮询**：tasks/get 返回 project 当前状态，客户端轮询直到 completed
- [x] `cargo check` 编译通过
- [x] 提交 commit

### Task 7：tasks/get 查询

- [x] `src/handlers/a2a/get_task.rs` 实现 `handle_get_task(ctx, params) -> Result<A2aTask>`
- [x] 流程：查 project → 查 messages → 查 artifacts → `build_a2a_task`
- [x] project 不存在时返回 `Error::not_found`
- [x] session_id 不持久化，get 时不返回（传 None）
- [x] `cargo check` 编译通过
- [x] 提交 commit

### Task 8：tasks/cancel 取消

- [x] `src/handlers/a2a/cancel_task.rs` 实现 `handle_cancel_task(ctx, params) -> Result<A2aTask>`
- [x] 流程：查 project → archive project → 重新查 project → 查 messages + artifacts → `build_a2a_task`
- [x] project 不存在时返回 `Error::not_found`
- [x] `cargo check` 编译通过
- [x] 提交 commit

### Task 9：JSON-RPC 入口和方法分发

- [x] `src/handlers/a2a/jsonrpc.rs` 实现 `handle_jsonrpc(Extension<ctx>, Json<req>) -> Json<JsonRpcResponse>`
- [x] `Extension(ctx): Extension<RequestContext>` 提取模式
- [x] 检查 `config.a2a_server.enabled`，未启用返回 `METHOD_NOT_FOUND` 错误
- [x] 验证 `jsonrpc == "2.0"`，否则返回 `INVALID_REQUEST`
- [x] 按 method 分发：`tasks/send` / `tasks/get` / `tasks/cancel`，未匹配返回 `METHOD_NOT_FOUND`
- [x] 错误响应使用 `format!("{}", e)` 输出友好消息（不用 `{:?}`）
- [x] 参数解析错误用 `Error::bad_request`
- [x] `cargo check` 编译通过
- [x] 提交 commit

## Phase 3：路由 + 测试 + 文档

### Task 10：路由注册

- [x] `src/router.rs` 的 `create_router` 新增两条路由：
  - `GET /.well-known/agent.json` — 仅挂 `request_context_middleware`（公开）
  - `POST /a2a` — 挂 `jwt_auth_middleware` + `request_context_middleware`（JWT 保护）
- [x] JWT 中间件在 RequestContext 中间件外层（先执行）
- [x] `ai_orz.toml` 示例配置 `[a2a_server] enabled = true`
- [x] 手动 curl Agent Card 返回正确 JSON
- [x] 手动 curl JSON-RPC 无 token 返回 401
- [x] 提交 commit

### Task 11：端到端集成测试

- [x] `src/handlers/a2a/mod.rs` 新增 `#[cfg(test)] mod integration_test;`
- [x] `src/handlers/a2a/integration_test.rs` 包含：
  - `init_a2a_test_env(pool)` 初始化所有 DAO/DAL/Domain（不含 message_push::init，因 OnceLock 惰性初始化）
  - `create_test_reception_agent(ctx)` 创建 `feishu_reception` 角色 Onboarded Agent
  - `test_find_reception_agent_returns_onboarded_agent` — 验证 resolve_agent 找到前台 Agent
  - `test_tasks_get_returns_not_found_for_nonexistent` — get 不存在 task 返回错误
  - `test_tasks_cancel_nonexistent_returns_error` — cancel 不存在 task 返回错误
- [x] `cargo test --lib handlers::a2a::integration_test` 3 tests PASS
- [x] `cargo test --lib` 全量测试 PASS，无回归
- [x] 提交 commit

### Task 12：文档更新

- [x] `docs/a2a_server_design.md` 顶部新增「实现完成状态」表格（11 个模块状态）
- [x] `README.md` 核心能力列表更新 A2A 条目（双向支持）
- [x] `cargo check && cargo test --lib` 全部通过
- [x] 提交并推送

## 最终验收

### 编译验证

- [x] `cargo check` 通过
- [x] `cargo check -p common` 通过
- [x] `cd frontend && cargo check` 通过

### 测试验证

- [x] `cargo test --lib` 全部 PASS，无回归
- [x] 新增测试数量：≥ 22 个（6 + 10 + 3 + Task 0.x 测试）
- [x] `cargo test -p common --lib api::a2a_test` PASS
- [x] `cargo test --lib handlers::a2a::mapper_test` PASS
- [x] `cargo test --lib handlers::a2a::integration_test` PASS

### 手动验证

- [x] `GET /.well-known/agent.json` 返回 AgentCard JSON
- [x] `POST /a2a` 无认证返回 401
- [x] `POST /a2a` 携带 JWT 调用 `tasks/send` 完整链路成功
- [x] chat 默认对话框（无 project）发消息：`to_agent_id` 未指定时后端走 `resolve_agent(ctx)` 兜底成功
- [x] chat Project 对话框发消息：`to_agent_id` 未指定时后端从 `project.owner_agent_id` 取成功

### 架构边界验证

- [x] Domain 层零侵入：只在 `hr/mod.rs` 新增方法，project/message/runtime domain 业务逻辑零改动（Trait 签名修改除外）
- [x] 无新增表：所有数据存储复用现有表
- [x] Handler 层转换：A2A 协议实体 ↔ ai_orz 实体转换全部在 `mapper.rs`
- [x] 统一路由：Chat / A2A / 飞书 IM 三场景共用 `HrDomain::resolve_agent(ctx)`（只接受 ctx，不耦合 project）
- [x] **agent 与 project 维度分离**：`resolve_agent` 不查询/感知 project，两个维度由 handler/前端按需组合
- [x] **协作关系类比**：默认对话框=与前台直接沟通（无 project）；Project 对话框=Agent 识别复杂需求后创建 Project 的上下文沟通
- [x] **不在前端创建默认 project**：Project 创建由 Agent 内部决策触发，不在本次 A2A Server 范围
- [x] **to_agent_id 保持 Option**：用户选定 Agent 时前端显式传入，未指定时后端走 `resolve_agent(ctx)` 兜底

### 文档验证

- [x] `docs/a2a_server_design.md` 实现状态表格完整
- [x] `README.md` A2A 能力描述准确
- [x] `docs/superpowers/specs/2026-07-19-a2a-server/` 三件套（spec.md / checklist.md / tasks.md）齐全

## 已知遗留（P1）

- `tasks/sendSubscribe` SSE 流式 — 设计文档已标注
- 长任务异步模式 — 设计文档已标注
- PushNotifications 推送通知 — AgentCapabilities 中已预留字段
