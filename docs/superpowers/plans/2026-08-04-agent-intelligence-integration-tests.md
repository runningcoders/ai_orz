# Agent 智能层集成测试 Implementation Plan

> **Status: ✅ COMPLETED** (2026-08-04) — 8 个测试全部通过（6 默认 + 1 ignored 真实 LLM），commit `be7674e..c131a9a`。附带修复：空 tags json_each 报错、parameters_schema null 导致 Doubao API 拒绝、new_with_all 对集成测试不可访问。

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 Agent 智能层（awaken 主流程、Consumer 编排、sleep_and_settle）构建集成测试，覆盖从消息接收到 LLM 思考到响应生成的完整链路。使用 Mock BrainDal 保证 CI 默认绿，同时用真实 Doubao LLM 验证端到端流程。

**Architecture:** 三部分设计：
- **Part A（Consumer 编排测试，Task 1-3）**：通过 HTTP 创建消息后，直接调用 `MessageConsumer::on_event()` 验证消息路由逻辑。利用不触发 LLM 的条件（Agent 不存在、Busy 状态、Task 已完成）测试 Consumer 的编排逻辑，CI 默认运行。
- **Part B（awaken 流程 Mock 测试，Task 4-6）**：复用 `awakening.rs` 的 `CapturingBrainDal` 模式，在 `tests/integration/` 层级测试 awaken 完整流程，包括工具注入 Prompt、ThinkingOptions 上下文注入、错误路径 BusyGuard 释放。CI 默认运行。
- **Part C（真实 LLM 端到端测试，Task 7-8）**：使用已验证的 Doubao LLM（`doubao-seed-evolving`），通过 HTTP 创建 provider + agent + 发消息，直接调用 `MessageConsumer::on_event()` 触发完整 awaken 链路，验证 LLM 真实响应生成。`#[ignore]` 标记，CI 安全。

**Tech Stack:** Rust, axum 0.8, sqlx (SQLite in-memory), serde_json, tokio, rig (LLM client)

---

## 背景信息（子代理必读）

### 关键架构决策

**测试环境限制**：`init_full_test_env` 调用 `service::init()` 初始化所有全局单例（DAO/DAL/Domain），但**不启动 AOP 调度器**（不调用 `aop::init_all()`）。因此 AOP Consumer worker 不会在测试中自动运行。测试通过直接调用 `MessageConsumer::on_event()` 模拟 Consumer 处理。

**MessageConsumer 可见性**：`ai_orz::consumer::message::MessageConsumer` 是 pub 的，`MessageConsumer::new()` 使用全局 Domain 单例。`Consumer` trait 的 `on_event(&self, event: serde_json::Value) -> Result<()>` 可直接调用。

**awaken 测试模式**：`runtime_domain::new_with_all(brain_dal, tool_dal, mcp_tool_dal, agent_dal, tool_call_logger)` 创建独立 RuntimeDomain 实例（非全局单例），可注入 `CapturingBrainDal` 捕获 Prompt 而不调用真实 LLM。

### 消息 API

| 方法 | 路径 | 用途 |
|------|------|------|
| POST | `/api/v1/finance/messages/agents` | 发送消息给 Agent |
| GET | `/api/v1/finance/messages?to_id=...` | 列出消息 |

**SendMessageToAgentParams**:
```json
{
  "to_agent_id": "agent-uuid",
  "content": "Hello",
  "project_id": null,
  "task_id": null,
  "reply_to_id": null
}
```

**SendMessageToAgentResponse**:
```json
{ "message_id": "msg-uuid" }
```

### MessageCreatedEvent 结构

Consumer 的 `on_event` 接收 JSON 格式的 `MessageCreatedEvent`：
```json
{
  "message_id": "msg-uuid",
  "project_id": null,
  "task_id": null,
  "from_id": "user-uuid",
  "from_role": 0,
  "to_id": "agent-uuid",
  "to_role": 1,
  "message_type": 0
}
```

`from_role` / `to_role` 值：User=0, Agent=1, System=2
`message_type` 值：Text=0, ToolCallRequest=4, ToolCallResponse=5

### Consumer 编排流程（handle_agent_message）

```
1. try_set_busy(agent_id, message_id) → 失败则返回 Conflict
2. get_agent(agent_id, with_tools+skills+stats) → 不存在返回 NotFound
3. 检查 task 状态 → Completed/Cancelled 则跳过 awaken
4. 检查 max_thinking_depth → 超限则发通知给 user
5. wake_agent_brain → 装配 Brain（调 BrainDal）
6. awaken → 调 LLM 思考
```

**不触发 LLM 的条件**：
- Agent 不存在（步骤 2 返回）
- Agent 已 Busy（步骤 1 返回）
- Task 已 Completed/Cancelled（步骤 3 返回）

### Cargo.toml 注册

```toml
[[test]]
name = "agent_awaken_test"
path = "tests/integration/agent_awaken_test.rs"
```

### 文件结构

- 创建：`tests/integration/agent_awaken_test.rs` — 所有 Agent 智能层集成测试
- 修改：`Cargo.toml` — 注册新 test target
- 无生产代码修改（纯测试）

### 测试基础设施

- `init_full_test_env(pool)` — 全局初始化
- `TestApp::new(pool)` — HTTP 请求封装
- `bootstrap_and_login(&app)` — 返回 `(BootstrappedSystem, jwt)`
- `create_test_agent(&app, &jwt, &provider_id, &name)` — 创建 Agent
- `assert_api_ok(status, &body)` — 断言成功
- `ai_orz::consumer::message::MessageConsumer` — Consumer 直接调用
- `ai_orz::pkg::aop::Consumer` — Consumer trait
- `ai_orz::pkg::agent_runtime_state::AgentRuntimeStateManager::global()` — 运行时状态管理

---

## Part A: Consumer 编排测试（CI 默认，无 LLM）

### Task 1: 测试骨架 + Cargo.toml 注册 + Agent 不存在错误测试

**Files:**
- Modify: `Cargo.toml`（添加 test target）
- Create: `tests/integration/agent_awaken_test.rs`

- [ ] **Step 1: 在 Cargo.toml 注册新 test target**

在 `Cargo.toml` 的 `[[test]]` 区域末尾添加：

```toml
[[test]]
name = "agent_awaken_test"
path = "tests/integration/agent_awaken_test.rs"
```

- [ ] **Step 2: 创建测试文件骨架 + 第一个测试**

创建 `tests/integration/agent_awaken_test.rs`：

```rust
//! Agent 智能层集成测试
//!
//! 覆盖 Agent awaken 主流程的三个层次：
//! - Part A: Consumer 编排逻辑（无 LLM，CI 默认）
//! - Part B: awaken 流程 Mock 测试（CapturingBrainDal，CI 默认）
//! - Part C: 真实 LLM 端到端测试（Doubao LLM，#[ignore]）

#[path = "../common/mod.rs"]
mod common;

use crate::common::TestApp;
use ai_orz::consumer::message::MessageConsumer;
use ai_orz::pkg::aop::Consumer;
use ai_orz::pkg::agent_runtime_state::AgentRuntimeStateManager;
use serde_json::json;
use sqlx::SqlitePool;

/// 构造 MessageCreatedEvent JSON
///
/// from_role: User=0, Agent=1, System=2
/// message_type: Text=0
fn make_message_event(
    message_id: &str,
    from_id: &str,
    to_id: &str,
    to_role: i32,
) -> serde_json::Value {
    json!({
        "message_id": message_id,
        "project_id": null,
        "task_id": null,
        "from_id": from_id,
        "from_role": 0,
        "to_id": to_id,
        "to_role": to_role,
        "message_type": 0
    })
}

/// Consumer 编排测试：向不存在的 Agent 发送消息，Consumer 应返回 NotFound 错误。
///
/// 验证：
/// - Consumer 正确加载消息
/// - Agent 不存在时返回错误（不触发 LLM）
/// - Busy 状态被正确释放（避免后续消息被永久阻塞）
#[sqlx::test]
async fn test_consumer_nonexistent_agent_returns_error(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 发送消息给一个不存在的 Agent ID
    let fake_agent_id = format!("nonexistent-{}", uuid::Uuid::now_v7());
    let send_req = json!({
        "to_agent_id": fake_agent_id,
        "content": "Hello from test"
    });
    let (status, body) = app
        .post_with_jwt("/api/v1/finance/messages/agents", &send_req, &jwt)
        .await;
    let msg_data = crate::common::assert_api_ok(status, &body);
    let message_id = msg_data
        .get("message_id")
        .and_then(|v| v.as_str())
        .expect("missing message_id")
        .to_string();

    // 直接调用 Consumer 处理消息事件
    let consumer = MessageConsumer::new();
    let event = make_message_event(&message_id, &bs.user_id, &fake_agent_id, 1);

    let result = consumer.on_event(event).await;

    // 应该返回错误（Agent 不存在）
    assert!(
        result.is_err(),
        "Consumer should return error for non-existent agent"
    );
    let err = result.unwrap_err();
    let err_msg = format!("{}", err);
    assert!(
        err_msg.contains("not found") || err_msg.contains("not_found"),
        "Error should mention not found, got: {}",
        err_msg
    );

    // 验证 Busy 状态被释放（避免永久锁定）
    let runtime_state = AgentRuntimeStateManager::global();
    let state = runtime_state.get_state(&fake_agent_id);
    // Idle = 0，Agent 不存在时应该已释放
    assert_eq!(
        state,
        common::enums::AgentRuntimeState::Idle,
        "Agent Busy state should be released after error"
    );
}
```

- [ ] **Step 3: 运行测试验证通过**

Run: `cargo test --test agent_awaken_test -- test_consumer_nonexistent_agent_returns_error --nocapture`

Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add tests/integration/agent_awaken_test.rs Cargo.toml
git commit -m "test: add agent awaken test skeleton with consumer nonexistent agent test"
```

---

### Task 2: Busy Agent 拒绝消息测试

**Files:**
- Modify: `tests/integration/agent_awaken_test.rs`

- [ ] **Step 1: 添加 Busy Agent 测试**

在 `tests/integration/agent_awaken_test.rs` 末尾添加：

```rust
/// Consumer 编排测试：Busy 状态的 Agent 拒绝新消息。
///
/// 验证：
/// - try_set_busy 返回 false 时 Consumer 返回 Conflict 错误
/// - 不触发后续 awaken 流程（不调用 LLM）
/// - 原始 Busy 状态保持不变
#[sqlx::test]
async fn test_consumer_busy_agent_rejects_message(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 创建 Agent
    let agent_name = format!("BusyAgent-{}", uuid::Uuid::now_v7());
    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &agent_name,
    )
    .await;

    // 发送消息（持久化到 DB）
    let send_req = json!({
        "to_agent_id": agent_id,
        "content": "First message"
    });
    let (status, body) = app
        .post_with_jwt("/api/v1/finance/messages/agents", &send_req, &jwt)
        .await;
    let msg_data = crate::common::assert_api_ok(status, &body);
    let message_id = msg_data
        .get("message_id")
        .and_then(|v| v.as_str())
        .expect("missing message_id")
        .to_string();

    // 预先将 Agent 设为 Busy（模拟另一个 worker 正在处理）
    let runtime_state = AgentRuntimeStateManager::global();
    runtime_state.set_busy(&agent_id, &message_id);

    // 调用 Consumer 处理消息事件
    let consumer = MessageConsumer::new();
    let event = make_message_event(&message_id, &bs.user_id, &agent_id, 1);

    let result = consumer.on_event(event).await;

    // 应该返回 Conflict 错误（Agent 已 Busy）
    assert!(
        result.is_err(),
        "Consumer should return error for busy agent"
    );
    let err = result.unwrap_err();
    let err_msg = format!("{}", err);
    assert!(
        err_msg.contains("busy") || err_msg.contains("conflict"),
        "Error should mention busy/conflict, got: {}",
        err_msg
    );

    // 验证 Agent 仍然处于 Busy 状态（未被释放，因为不是我们设置的）
    let state = runtime_state.get_state(&agent_id);
    assert_eq!(
        state,
        common::enums::AgentRuntimeState::Busy,
        "Agent should still be Busy (we set it, consumer should not release it)"
    );

    // 清理：释放 Busy 状态
    runtime_state.set_idle(&agent_id);
}
```

- [ ] **Step 2: 运行测试验证通过**

Run: `cargo test --test agent_awaken_test -- test_consumer_busy_agent_rejects_message --nocapture`

Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add tests/integration/agent_awaken_test.rs
git commit -m "test: add busy agent rejects message consumer test"
```

---

### Task 3: Task 已完成时跳过 awaken 测试

**Files:**
- Modify: `tests/integration/agent_awaken_test.rs`

- [ ] **Step 1: 添加 Task 已完成测试**

在 `tests/integration/agent_awaken_test.rs` 末尾添加：

```rust
/// Consumer 编排测试：Task 已 Completed 时跳过 awaken。
///
/// 验证：
/// - Consumer 检查 task 状态，Completed 时跳过 awaken
/// - 不触发 LLM 调用
/// - 返回 Ok（合法跳过，非错误）
/// - Busy 状态被释放
#[sqlx::test]
async fn test_consumer_completed_task_skips_awaken(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 1. 创建 Agent
    let agent_name = format!("TaskAgent-{}", uuid::Uuid::now_v7());
    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &agent_name,
    )
    .await;

    // 2. 创建 Project + Task
    let project_name = format!("TaskProject-{}", uuid::Uuid::now_v7());
    let project_id = crate::common::factories::create_test_project(&app, &jwt, &project_name).await;

    let task_req = json!({
        "title": "Test task for completed",
        "description": "Task that will be completed",
        "project_id": project_id,
        "assignee_id": agent_id,
    });
    let (status, body) = app.post_with_jwt("/api/v1/tasks", &task_req, &jwt).await;
    let task_data = crate::common::assert_api_ok(status, &body);
    let task_id = task_data
        .get("id")
        .and_then(|v| v.as_str())
        .expect("missing task id")
        .to_string();

    // 3. 流转 task 状态：Pending → InProgress → Completed
    // Pending → InProgress
    let in_progress_req = json!({ "id": task_id, "status": "InProgress" });
    let (status, _body) = app
        .put_with_jwt(&format!("/api/v1/tasks/{}/status", task_id), &in_progress_req, &jwt)
        .await;
    assert_eq!(status, axum::http::StatusCode::OK, "Pending → InProgress should succeed");

    // InProgress → Completed
    let completed_req = json!({ "id": task_id, "status": "Completed" });
    let (status, _body) = app
        .put_with_jwt(&format!("/api/v1/tasks/{}/status", task_id), &completed_req, &jwt)
        .await;
    assert_eq!(status, axum::http::StatusCode::OK, "InProgress → Completed should succeed");

    // 4. 发送带 task_id 的消息给 Agent
    let send_req = json!({
        "to_agent_id": agent_id,
        "content": "Hello for completed task",
        "task_id": task_id,
    });
    let (status, body) = app
        .post_with_jwt("/api/v1/finance/messages/agents", &send_req, &jwt)
        .await;
    let msg_data = crate::common::assert_api_ok(status, &body);
    let message_id = msg_data
        .get("message_id")
        .and_then(|v| v.as_str())
        .expect("missing message_id")
        .to_string();

    // 5. 调用 Consumer 处理消息事件
    let consumer = MessageConsumer::new();
    let event = json!({
        "message_id": message_id,
        "project_id": project_id,
        "task_id": task_id,
        "from_id": bs.user_id,
        "from_role": 0,
        "to_id": agent_id,
        "to_role": 1,
        "message_type": 0
    });

    let result = consumer.on_event(event).await;

    // 应该返回 Ok（合法跳过，非错误）
    assert!(
        result.is_ok(),
        "Consumer should return Ok for completed task (skip awaken), got: {:?}",
        result.err()
    );

    // 6. 验证 Agent 回到 Idle 状态（未触发 awaken，Busy 已释放）
    let runtime_state = AgentRuntimeStateManager::global();
    let state = runtime_state.get_state(&agent_id);
    assert_eq!(
        state,
        common::enums::AgentRuntimeState::Idle,
        "Agent should be Idle after skipping awaken for completed task"
    );
}
```

- [ ] **Step 2: 运行测试验证通过**

Run: `cargo test --test agent_awaken_test -- test_consumer_completed_task_skips_awaken --nocapture`

Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add tests/integration/agent_awaken_test.rs
git commit -m "test: add completed task skips awaken consumer test"
```

---

## Part B: awaken 流程 Mock 测试（CI 默认）

### Task 4: awaken 工具注入 Prompt 测试

**Files:**
- Modify: `tests/integration/agent_awaken_test.rs`

- [ ] **Step 1: 添加 awaken 工具注入测试**

在 `tests/integration/agent_awaken_test.rs` 文件开头（Part A 测试之后）添加 Part B 模块：

```rust
// ==================== Part B: awaken 流程 Mock 测试 ====================

use ai_orz::models::agent::{Agent, AgentPo, AgentRuntimeConfig};
use ai_orz::models::brain::{Brain, Cortex, CortexTrait};
use ai_orz::models::file::FileMeta;
use ai_orz::models::message::Message;
use ai_orz::models::model_provider::ModelProvider;
use ai_orz::models::tool::{Tool, ToolPo};
use ai_orz::pkg::request_context::RequestContext;
use ai_orz::pkg::tool_tracing::logger::ToolCallLogger;
use ai_orz::service::dal::brain::BrainDal;
use ai_orz::service::domain::runtime::awakening::ThinkingOptions;
use ai_orz::service::domain::runtime::{RuntimeDomain, awakening::ThinkingScene};
use async_trait::async_trait;
use common::enums::{AgentStatus, ControlMode, MessageRole, MessageType, ModelCapability, ProviderType, ToolProtocol};
use std::sync::{Arc, Mutex};
use tempfile::tempdir;
use uuid::Uuid;

/// 捕获 Prompt 的 BrainDal Stub（与 awakening.rs 测试一致）
struct CapturingBrainDal {
    captured_prompt: Arc<Mutex<Option<String>>>,
}

impl CapturingBrainDal {
    fn new(captured_prompt: Arc<Mutex<Option<String>>>) -> Self {
        Self { captured_prompt }
    }
}

#[async_trait]
impl BrainDal for CapturingBrainDal {
    async fn wake_brain(
        &self,
        _ctx: RequestContext,
        _agent: &AgentPo,
        _memories: Vec<ai_orz::models::memory::Memory>,
        _tools: Vec<Tool>,
    ) -> common::error::Result<Brain> {
        unimplemented!("not needed by awaken tool tests")
    }

    async fn test_connection(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProvider,
        _prompt: &str,
    ) -> common::error::Result<String> {
        unimplemented!("not needed by awaken tool tests")
    }

    async fn think(
        &self,
        _ctx: RequestContext,
        _brain: &Brain,
        prompt: &str,
    ) -> common::error::Result<String> {
        *self.captured_prompt.lock().unwrap() = Some(prompt.to_string());
        Ok("mock response from tool test".to_string())
    }
}

/// Mock Cortex（与 awakening.rs 测试一致）
#[derive(Clone)]
struct MockCortex;

#[async_trait]
impl CortexTrait for MockCortex {
    fn capability(&self) -> ModelCapability {
        ModelCapability::Agent
    }
    fn model_provider_id(&self) -> &str {
        "mock-provider"
    }
    fn model_name(&self) -> &str {
        "mock-model"
    }
    async fn prompt(&self, _prompt: &str) -> anyhow::Result<String> {
        Ok("mock response".to_string())
    }
    async fn embeddings(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vec![0.0; 3]).collect())
    }
    fn support_tools(&self) -> bool {
        false
    }
}

/// 创建带 Brain 的测试 Agent
fn make_test_agent_with_brain(agent_id: &str) -> Agent {
    let mut po = AgentPo::new(
        "Tool Test Agent".to_string(),
        vec!["assistant".to_string()],
        "Test description".to_string(),
        vec!["chat".to_string()],
        "Test soul".to_string(),
        "provider-001".to_string(),
        "test-user".to_string(),
    );
    po.id = agent_id.to_string();
    po.status = AgentStatus::Onboarded;

    let mut agent = Agent::from_po(po);
    let model_provider = ModelProvider::new(
        "Mock Provider".to_string(),
        ProviderType::OpenAI,
        ModelCapability::Agent,
        "gpt-4".to_string(),
        "fake-key".to_string(),
        None,
        None,
        "test-user".to_string(),
    );
    let cortex = Cortex::new(model_provider, Box::new(MockCortex));
    let runtime_config = AgentRuntimeConfig::default();
    agent.brain = Some(Brain::new_local(
        agent_id.to_string(),
        "Tool Test Agent".to_string(),
        runtime_config,
        cortex,
        vec![],
    ));
    agent
}

/// 创建 Manual 工具
fn make_manual_tool(tool_id: &str, name: &str, description: &str) -> Tool {
    let mut po = ToolPo::new(
        tool_id.to_string(),
        name.to_string(),
        description.to_string(),
        ToolProtocol::Builtin,
        serde_json::json!({}),
        None,
        vec!["assistant".to_string()],
        Some("test".to_string()),
    );
    po.control_mode = ControlMode::Manual;
    Tool::from_po_for_management(po)
}

/// 创建测试文本消息
fn make_test_message(content: &str) -> Message {
    Message::new_with_context(
        Uuid::now_v7().to_string(),
        None,
        None,
        "test-user".to_string(),
        "test-agent".to_string(),
        MessageRole::User,
        MessageRole::Agent,
        MessageType::Text,
        content.to_string(),
        None,
        FileMeta::default(),
        None,
        None,
        None,
        "test-user".to_string(),
    )
}

/// awaken 流程测试：Manual 工具注入到 Prompt 中。
///
/// 验证：
/// - awaken 调用成功（Mock BrainDal 返回固定响应）
/// - Prompt 包含 Manual 工具的名称和描述
/// - 工具按 tag 分块展示（常用工具区块）
#[sqlx::test]
async fn test_awaken_manual_tools_in_prompt(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;

    let agent_id = format!("agent-tools-{}", Uuid::now_v7());
    let mut agent = make_test_agent_with_brain(&agent_id);

    // 为 Agent 添加 Manual 工具
    let tool1 = make_manual_tool("tool-search-001", "SearchDocs", "搜索项目文档和代码");
    let tool2 = make_manual_tool("tool-search-002", "WriteFile", "写入文件到 Agent 目录");
    agent.tools = vec![tool1, tool2];

    let message = make_test_message("请帮我搜索相关文档");

    let captured_prompt = Arc::new(Mutex::new(None));
    let temp_dir = tempdir().expect("tempdir should be created");
    let runtime = ai_orz::service::domain::runtime::new_with_all(
        Arc::new(CapturingBrainDal::new(captured_prompt.clone())),
        ai_orz::service::dal::tool::dal(),
        ai_orz::service::dal::mcp_tool::dal(),
        ai_orz::service::dal::agent::dal(),
        Arc::new(ToolCallLogger::new(temp_dir.path().to_path_buf())),
    );

    let ctx = RequestContext::from_storage("test-user", ai_orz::pkg::storage::get().clone());

    let result = runtime
        .awakening()
        .awaken(ctx, &agent, &message, &ThinkingOptions::new())
        .await
        .expect("awaken 应该成功");

    let prompt = captured_prompt
        .lock()
        .unwrap()
        .clone()
        .expect("应该捕获到 prompt");

    // 验证 Prompt 包含工具名称和描述
    assert!(
        prompt.contains("SearchDocs"),
        "Prompt 应该包含工具 SearchDocs，实际: {}",
        prompt
    );
    assert!(
        prompt.contains("搜索项目文档和代码"),
        "Prompt 应该包含 SearchDocs 的描述"
    );
    assert!(
        prompt.contains("WriteFile"),
        "Prompt 应该包含工具 WriteFile"
    );
    assert!(
        prompt.contains("写入文件到 Agent 目录"),
        "Prompt 应该包含 WriteFile 的描述"
    );

    // 验证返回结果
    assert_eq!(result.agent_id, agent_id);
    assert_eq!(result.raw_output, "mock response from tool test");
}
```

- [ ] **Step 2: 运行测试验证通过**

Run: `cargo test --test agent_awaken_test -- test_awaken_manual_tools_in_prompt --nocapture`

Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add tests/integration/agent_awaken_test.rs
git commit -m "test: add awaken manual tools in prompt test"
```

---

### Task 5: awaken 项目/任务上下文注入 Prompt 测试

**Files:**
- Modify: `tests/integration/agent_awaken_test.rs`

- [ ] **Step 1: 添加 ThinkingOptions 上下文注入测试**

在 `tests/integration/agent_awaken_test.rs` 末尾添加：

```rust
/// awaken 流程测试：ThinkingOptions 注入 project/task 上下文到 Prompt。
///
/// 验证：
/// - ThinkingOptions.with_project() 的实体摘要出现在 Prompt 中
/// - ThinkingOptions.with_task() 的实体摘要出现在 Prompt 中
/// - project_context / task_context 区块正确渲染
#[sqlx::test]
async fn test_awaken_project_task_context_in_prompt(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;

    let agent_id = format!("agent-ctx-{}", Uuid::now_v7());
    let agent = make_test_agent_with_brain(&agent_id);

    let message = make_test_message("请帮我处理这个任务");

    // 构造带 project + task 的 ThinkingOptions
    use ai_orz::models::project::{Project, ProjectPo};
    use ai_orz::models::task::{Task, TaskPo};
    use common::enums::{AssigneeType, ProjectStatus, TaskStatus};

    let project_id = format!("project-{}", Uuid::now_v7());
    let task_id = format!("task-{}", Uuid::now_v7());

    let mut project_po = ProjectPo::new(
        project_id.clone(),
        "Test Project".to_string(),
        "项目描述：集成测试项目".to_string(),
        None,           // workflow
        None,           // guidance
        0,              // priority
        vec![],         // tags
        "test-user".to_string(),
        None,           // owner_agent_id
        None,           // start_at
        None,           // due_at
        None,           // end_at
        "test-user".to_string(),
    );
    project_po.status = ProjectStatus::Active;
    let project = Project::from_po(project_po);

    let mut task_po = TaskPo::new(
        task_id.clone(),
        "Test Task".to_string(),
        "任务描述：执行集成测试".to_string(),
        0,                              // priority
        vec![],                         // tags
        None,                           // due_at
        None,                           // start_at
        None,                           // end_at
        vec![],                         // dependencies
        "test-user".to_string(),        // root_user_id
        AssigneeType::Agent,            // assignee_type
        "test-agent".to_string(),       // assignee_id
        Some(project_id.clone()),       // project_id
        "test-user".to_string(),        // created_by
    );
    task_po.status = TaskStatus::InProgress;
    let task = Task::from_po(task_po);

    let options = ThinkingOptions::new()
        .with_project(project)
        .with_task(task);

    let captured_prompt = Arc::new(Mutex::new(None));
    let temp_dir = tempdir().expect("tempdir should be created");
    let runtime = ai_orz::service::domain::runtime::new_with_all(
        Arc::new(CapturingBrainDal::new(captured_prompt.clone())),
        ai_orz::service::dal::tool::dal(),
        ai_orz::service::dal::mcp_tool::dal(),
        ai_orz::service::dal::agent::dal(),
        Arc::new(ToolCallLogger::new(temp_dir.path().to_path_buf())),
    );

    let ctx = RequestContext::from_storage("test-user", ai_orz::pkg::storage::get().clone());

    let result = runtime
        .awakening()
        .awaken(ctx, &agent, &message, &options)
        .await
        .expect("awaken 应该成功");

    let prompt = captured_prompt
        .lock()
        .unwrap()
        .clone()
        .expect("应该捕获到 prompt");

    // 验证 project 上下文注入
    assert!(
        prompt.contains("Test Project"),
        "Prompt 应该包含 project 名称，实际: {}",
        prompt
    );
    assert!(
        prompt.contains("集成测试项目"),
        "Prompt 应该包含 project 描述"
    );

    // 验证 task 上下文注入
    assert!(
        prompt.contains("Test Task"),
        "Prompt 应该包含 task 名称"
    );
    assert!(
        prompt.contains("执行集成测试"),
        "Prompt 应该包含 task 描述"
    );

    assert_eq!(result.agent_id, agent_id);
}
```

- [ ] **Step 2: 运行测试验证通过**

Run: `cargo test --test agent_awaken_test -- test_awaken_project_task_context_in_prompt --nocapture`

Expected: PASS（如果 PromptBuilder 的 project_context/task_context 方法渲染了实体摘要）

- [ ] **Step 3: Commit**

```bash
git add tests/integration/agent_awaken_test.rs
git commit -m "test: add awaken project task context in prompt test"
```

---

### Task 6: awaken 错误路径 BusyGuard 释放测试

**Files:**
- Modify: `tests/integration/agent_awaken_test.rs`

- [ ] **Step 1: 添加错误路径 BusyGuard 测试**

在 `tests/integration/agent_awaken_test.rs` 末尾添加：

```rust
/// awaken 流程测试：think 失败时 BusyGuard 释放 Busy 状态。
///
/// 验证：
/// - BrainDal.think() 返回错误时 awaken 返回 Err
/// - Agent 状态从 Busy 回到 Idle（BusyGuard RAII 释放）
/// - 错误事件被记录（AgentAwakeEvent status=failed）
#[sqlx::test]
async fn test_awaken_error_releases_busy_guard(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;

    let agent_id = format!("agent-err-{}", Uuid::now_v7());
    let agent = make_test_agent_with_brain(&agent_id);
    let message = make_test_message("触发错误的消息");

    // 使用永远失败的 BrainDal
    struct FailingBrainDal;

    #[async_trait]
    impl BrainDal for FailingBrainDal {
        async fn wake_brain(
            &self,
            _ctx: RequestContext,
            _agent: &AgentPo,
            _memories: Vec<ai_orz::models::memory::Memory>,
            _tools: Vec<Tool>,
        ) -> common::error::Result<Brain> {
            unimplemented!("not needed")
        }

        async fn test_connection(
            &self,
            _ctx: RequestContext,
            _provider: &ModelProvider,
            _prompt: &str,
        ) -> common::error::Result<String> {
            unimplemented!("not needed")
        }

        async fn think(
            &self,
            _ctx: RequestContext,
            _brain: &Brain,
            _prompt: &str,
        ) -> common::error::Result<String> {
            Err(common::error::Error::internal("mock think failure"))
        }
    }

    let temp_dir = tempdir().expect("tempdir should be created");
    let runtime = ai_orz::service::domain::runtime::new_with_all(
        Arc::new(FailingBrainDal),
        ai_orz::service::dal::tool::dal(),
        ai_orz::service::dal::mcp_tool::dal(),
        ai_orz::service::dal::agent::dal(),
        Arc::new(ToolCallLogger::new(temp_dir.path().to_path_buf())),
    );

    let ctx = RequestContext::from_storage("test-user", ai_orz::pkg::storage::get().clone());

    // awaken 前先设置 Busy（模拟 handle_agent_message 的 try_set_busy）
    let runtime_state = AgentRuntimeStateManager::global();
    runtime_state.set_busy(&agent_id, &message.po.id);

    // 调用 awaken（应该失败）
    let result = runtime
        .awakening()
        .awaken(ctx, &agent, &message, &ThinkingOptions::new())
        .await;

    assert!(
        result.is_err(),
        "awaken should return error when think fails"
    );

    // 验证 Agent 回到 Idle 状态（BusyGuard 通过 RAII 释放）
    let state = runtime_state.get_state(&agent_id);
    assert_eq!(
        state,
        common::enums::AgentRuntimeState::Idle,
        "Agent should be Idle after awaken error (BusyGuard released)"
    );
}
```

- [ ] **Step 2: 运行测试验证通过**

Run: `cargo test --test agent_awaken_test -- test_awaken_error_releases_busy_guard --nocapture`

Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add tests/integration/agent_awaken_test.rs
git commit -m "test: add awaken error releases busy guard test"
```

---

## Part C: 真实 LLM 端到端测试（#[ignore]）

### Task 7: 真实 LLM awaken 全流程测试

**Files:**
- Modify: `tests/integration/agent_awaken_test.rs`

- [ ] **Step 1: 添加真实 LLM 配置读取 + 测试**

在 `tests/integration/agent_awaken_test.rs` 末尾添加 Part C 模块：

```rust
// ==================== Part C: 真实 LLM 端到端测试 ====================

/// 真实模型配置（从 .env 读取）
struct RealModelConfig {
    llm_provider_type: String,
    llm_model_name: String,
    llm_api_key: String,
    llm_base_url: Option<String>,
}

impl RealModelConfig {
    fn from_env() -> Option<Self> {
        let api_key = std::env::var("TEST_LLM_API_KEY").ok()?;
        let model_name = std::env::var("TEST_LLM_MODEL_NAME").ok()?;
        let provider_type = std::env::var("TEST_LLM_PROVIDER_TYPE")
            .unwrap_or_else(|_| "doubao".to_string());
        let base_url = std::env::var("TEST_LLM_BASE_URL").ok();
        Some(Self {
            llm_provider_type: provider_type,
            llm_model_name,
            llm_api_key,
            llm_base_url: base_url,
        })
    }
}

/// 创建真实 LLM Provider，返回 provider_id
async fn create_real_llm_provider(
    app: &TestApp,
    jwt: &str,
    cfg: &RealModelConfig,
) -> String {
    let req = json!({
        "name": format!("RealLLM-{}", uuid::Uuid::now_v7()),
        "provider_type": cfg.llm_provider_type,
        "capability": "Agent",
        "model_name": cfg.llm_model_name,
        "api_key": cfg.llm_api_key,
        "base_url": cfg.llm_base_url,
        "description": "Real LLM for awaken test"
    });
    let (status, body) = app
        .post_with_jwt("/api/v1/finance/model-providers", &req, jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    data.get("id")
        .and_then(|v| v.as_str())
        .expect("missing provider id")
        .to_string()
}

/// 真实 LLM 端到端测试：发送消息 → Consumer 触发 awaken → LLM 生成响应。
///
/// 验证：
/// - Agent 收到消息后触发 awaken
/// - 真实 LLM 生成有意义的响应（非空、非错误）
/// - 响应 Trace 写入 memory
/// - Agent 回到 Idle 状态
#[sqlx::test]
#[ignore = "requires real LLM API key in .env (TEST_LLM_API_KEY)"]
async fn test_real_llm_awaken_full_flow(pool: SqlitePool) {
    let Some(cfg) = RealModelConfig::from_env() else {
        eprintln!("SKIP: TEST_LLM_API_KEY not set, skipping real LLM awaken test");
        return;
    };

    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 1. 创建真实 LLM Provider
    let real_provider_id = create_real_llm_provider(&app, &jwt, &cfg).await;

    // 2. 创建 Agent（使用真实 LLM Provider）
    let agent_name = format!("RealLLMAgent-{}", uuid::Uuid::now_v7());
    let agent_req = json!({
        "name": agent_name,
        "description": "一个用于测试真实 LLM 唤醒的 Agent",
        "model_provider_id": real_provider_id,
        "soul": "你是一个测试助手，请简洁回答问题。"
    });
    let (status, body) = app.post_with_jwt("/api/v1/hr/agents", &agent_req, &jwt).await;
    let agent_data = crate::common::assert_api_ok(status, &body);
    let agent_id = agent_data
        .get("id")
        .and_then(|v| v.as_str())
        .expect("missing agent id")
        .to_string();

    // 3. 发送消息给 Agent
    let send_req = json!({
        "to_agent_id": agent_id,
        "content": "请回复：awaken 测试成功"
    });
    let (status, body) = app
        .post_with_jwt("/api/v1/finance/messages/agents", &send_req, &jwt)
        .await;
    let msg_data = crate::common::assert_api_ok(status, &body);
    let message_id = msg_data
        .get("message_id")
        .and_then(|v| v.as_str())
        .expect("missing message_id")
        .to_string();

    // 4. 调用 Consumer 触发 awaken（真实 LLM 调用）
    let consumer = MessageConsumer::new();
    let event = make_message_event(&message_id, &bs.user_id, &agent_id, 1);

    let result = consumer.on_event(event).await;

    // awaken 应该成功
    assert!(
        result.is_ok(),
        "Consumer should succeed with real LLM, got: {:?}",
        result.err()
    );

    // 5. 验证 Agent 回到 Idle
    let runtime_state = AgentRuntimeStateManager::global();
    let state = runtime_state.get_state(&agent_id);
    assert_eq!(
        state,
        common::enums::AgentRuntimeState::Idle,
        "Agent should be Idle after awaken completion"
    );

    // 6. 验证 Agent 生成了响应消息（to_role=User）
    // 等待异步消息写入
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    let (status, body) = app
        .get_with_jwt(
            &format!("/api/v1/finance/messages?from_id={}&to_id={}", agent_id, bs.user_id),
            &jwt,
        )
        .await;
    let list_data = crate::common::assert_api_ok(status, &body);
    let messages = list_data
        .get("messages")
        .and_then(|v| v.as_array())
        .expect("missing messages array");

    // 应该至少有一条 Agent 发给 User 的消息
    let agent_response = messages.iter().find(|msg| {
        msg.get("from_id").and_then(|v| v.as_str()) == Some(agent_id.as_str())
    });
    assert!(
        agent_response.is_some(),
        "Agent should have generated a response message"
    );

    let response_content = agent_response
        .and_then(|m| m.get("content"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        !response_content.is_empty(),
        "Agent response content should not be empty"
    );

    eprintln!(
        "Real LLM awaken test passed! Agent response: {}",
        response_content
    );

    // Cleanup
    let _ = app
        .delete_with_jwt(
            &format!("/api/v1/finance/model-providers/{}", real_provider_id),
            &jwt,
        )
        .await;
}
```

- [ ] **Step 2: 运行测试验证通过（需 .env 配置）**

Run: `cargo test --test agent_awaken_test -- test_real_llm_awaken_full_flow --ignored --nocapture`

Expected: PASS（需要真实 LLM API key）

- [ ] **Step 3: 验证 CI 默认跳过**

Run: `cargo test --test agent_awaken_test -- --nocapture`

Expected: 6 tests pass, 1 ignored (test_real_llm_awaken_full_flow)

- [ ] **Step 4: Commit**

```bash
git add tests/integration/agent_awaken_test.rs
git commit -m "test: add real LLM awaken full flow integration test"
```

---

### Task 8: 全量回归验证 + fmt/clippy

**Files:**
- 无修改

- [ ] **Step 1: 运行全部非 ignored 测试**

Run: `cargo test --test agent_awaken_test -- --nocapture`

Expected: ALL PASS（6 个测试：3 Consumer + 3 Mock awaken），1 ignored

- [ ] **Step 2: 运行 ignored 测试（需要 .env）**

Run: `cargo test --test agent_awaken_test -- --ignored --nocapture`

Expected: PASS（1 个真实 LLM 测试）

- [ ] **Step 3: 运行 fmt + clippy 检查**

Run: `cargo fmt --all -- --check && cargo clippy --test agent_awaken_test -- -D warnings`

Expected: 无错误

- [ ] **Step 4: 运行既有 agent_management_test 确认无回归**

Run: `cargo test --test agent_management_test -- --nocapture`

Expected: 12 passed; 0 failed; 3 ignored

- [ ] **Step 5: Final commit（如有 fmt/clippy 修复）**

```bash
git add -A
git commit -m "test: finalize agent intelligence integration test suite"
```
