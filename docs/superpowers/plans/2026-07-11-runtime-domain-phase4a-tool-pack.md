# Phase 4 方向 A: 工具包机制 + 任务执行闭环

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现工具包（tool pack）机制，Agent 入职时自动安装项目管理工具包，唤醒时注入神经工具 + 已安装工具包工具，免绑定校验支持已安装 tag。同时完成工具调用异步消息链路和任务分配消息机制。

**Tech Stack:** Rust, Axum, SQLx, ai-orz-macros

---

## 设计背景

### Agent 能力分层模型

能力不仅包含工具，也包含 skill。天生的不只有工具，也有天生的 skill；入职培训的也不只有工具，也有培训教的 skill。本阶段聚焦工具维度，skill 维度后续讨论。

| 层级 | 来源 | 获取方式 | 工具 | Skill |
|------|------|---------|------|-------|
| 神经能力 | 天生认知 | 自动拥有，免绑定 | 神经工具（search_memory、send_message 等） | 天生 skill（后续讨论） |
| 工具包 | 后天培训 | 入职时统一安装 | project_management 等工具包 | 入职培训 skill（后续讨论） |
| 外骨骼 | 外部授权 | 按需绑定 | 外骨骼工具（写文件、调 API 等） | — |

### 核心决策

**项目/任务工具不是神经工具**：Agent "认为做完" ≠ 任务真的完成。任务进度是外部系统的真实数据，操作任务系统是使用外部系统的行为，属于工具包而非天生能力。

**项目管理能力是入职培训内容**：Agent 入职时统一安装 project_management 工具包，不需要逐个绑定。

### 工具调用两种模式

| 模式 | 适用场景 | 执行方式 | Agent 感知 |
|------|---------|---------|-----------|
| **同步（auto）** | 神经工具、工具包工具 | rig 框架直接调用 Handler 函数 | 一次 awaken 内拿到结果 |
| **异步（manual）** | 外骨骼工具 | Agent 调用 `send_tool_call_message` 神经工具发消息 → 消费者执行 → ToolCallResult 消息回 Agent | 跨 awaken 轮次 |

### 三种角色定位

| 角色 | 职责 | 示例 |
|------|------|------|
| **神经工具 Handler** | 封装 Message Domain 的投递方法，注册为神经工具供 Agent 调用 | `send_tool_call_message`（发 ToolCallRequest 消息）、`send_message`（发 Text 消息） |
| **普通 HTTP Handler** | 直接调用 Domain 完成业务，不注册为工具 | `request_tool_call`（同步调用工具，供 HTTP API 或后续复杂架构使用） |
| **Consumer** | 同服务内直接通过 Domain 执行真实业务逻辑 | `handle_tool_call_request` → `call_manual_tool_for_agent()` + `send_tool_call_result()` |

---

## 子阶段拆分

### 4A-1: 工具调用消息改造（不引入新消息类型）

完成同步/异步工具调用链路的分离：
- 新增 `send_tool_call_message` 神经工具（封装 `send_tool_call_request`）
- `request_tool_call` 从神经工具中移除，保留为普通 HTTP Handler
- 补齐 `send_message`、`send_tool_call_message` 的 neural flag
- 工具包 tag 机制（installed_tags、免绑定校验、唤醒注入）
- 项目管理工具包标记
- Agent 入职自动安装工具包
- 工具包安装/卸载 API

**子阶段拆分**：

| 子阶段 | 包含 Task | 主题 | 依赖关系 |
|--------|----------|------|---------|
| 4A-1a: 基础设施 | Task 1 (installed_tags) + Task 2 (神经工具修正) | 数据模型 + 工具标记 | 无依赖，可并行 |
| 4A-1b: 工具包机制 | Task 3 (tag 标记) + Task 4 (免绑定校验) + Task 5 (唤醒注入) | 核心工具包逻辑 | 依赖 4A-1a |
| 4A-1c: 入职 + API | Task 6 (入职安装) + Task 7 (安装/卸载 API) + Task 8 (验证) | 业务层接入 | 依赖 4A-1b |

### 4A-2: TaskAssignment 消息（引入新消息类型）

完成任务分配通知机制：
- 新增 `MessageType::TaskAssignment`
- Message Domain 新增 `send_task_assignment` 投递方法
- 新增 `send_task_assignment_message` 神经工具（供 Agent 间分配任务）
- 任务创建 Handler 编排：创建任务后自动发 TaskAssignment 消息
- PromptBuilder 支持 TaskAssignment 差异化提示

---

## 4A-1: 工具调用消息改造

### 4A-1a: 基础设施

### File Structure

| File | Responsibility | Change Type |
|------|---------------|-------------|
| `src/models/agent.rs` | AgentRuntimeConfig 新增 `installed_tags` 字段 | Modify |
| `src/handlers/finance/message/send_message.rs` | 补齐 `neural` flag | Modify |
| `src/handlers/finance/tool/request_tool_call.rs` | 去掉 `register_handler_tool`，保留 HTTP Handler | Modify |
| `src/handlers/finance/tool/send_tool_call_message.rs` | 新增神经工具：发送工具调用消息 | New |
| `src/handlers/project/task/mark_done.rs` | 加 `tags = "project_management"` | Modify |
| `src/handlers/project/task/create_task.rs` | 加 `tags = "project_management"` | Modify |
| `src/handlers/project/task/update_task.rs` | 加 `tags = "project_management"` | Modify |
| `src/handlers/project/task/get_task.rs` | 加 `tags = "project_management"` | Modify |
| `src/handlers/project/task/list_project_tasks.rs` | 加 `tags = "project_management"` | Modify |
| `src/handlers/project/task/list_agent_tasks.rs` | 加 `tags = "project_management"` | Modify |
| `src/handlers/project/task/update_task_status.rs` | 加 `tags = "project_management"` | Modify |
| `src/handlers/project/project/create_project.rs` | 加 `tags = "project_management"` | Modify |
| `src/handlers/project/project/update_project.rs` | 加 `tags = "project_management"` | Modify |
| `src/handlers/project/project/get_project.rs` | 加 `tags = "project_management"` | Modify |
| `src/handlers/project/project/list_projects.rs` | 加 `tags = "project_management"` | Modify |
| `src/handlers/project/project/update_project_status.rs` | 加 `tags = "project_management"` | Modify |
| `src/service/domain/runtime/tool_execution.rs` | 免绑定校验扩展支持 installed_tags | Modify |
| `src/service/domain/runtime/awakening.rs` | `load_neural_tools` → `load_builtin_tools` | Modify |
| `src/service/domain/hr/agent.rs` | 入职时自动安装 project_management tag | Modify |
| `src/service/domain/hr/mod.rs` | AgentManage trait 新增工具包安装/卸载方法 | Modify |
| `src/handlers/hr/agent/install_tool_pack.rs` | 安装工具包 Handler | New |
| `src/handlers/hr/agent/uninstall_tool_pack.rs` | 卸载工具包 Handler | New |
| `src/handlers/hr/agent/list_installed_tool_packs.rs` | 列出已安装工具包 Handler | New |

---

### Task 1: AgentRuntimeConfig 新增 installed_tags 字段

**Files:**
- Modify: `src/models/agent.rs`

### 背景

[AgentRuntimeConfig](file:///Users/aman/Technology/rust/ai_orz/src/models/agent.rs#L17-L38) 是 Agent 的运行时配置，存储在 `agents.runtime_config` 字段（JSON 格式）。当前包含 `max_thinking_depth`、`thinking_interval_ms` 等字段。

新增 `installed_tags` 字段记录 Agent 已安装的工具包 tag 列表。

- [ ] **Step 1: 添加 installed_tags 字段**

在 [src/models/agent.rs](file:///Users/aman/Technology/rust/ai_orz/src/models/agent.rs#L17-L38) 的 `AgentRuntimeConfig` 结构体中新增字段：

```rust
/// 已安装的工具包 tag 列表
///
/// 记录 Agent 通过入职培训等方式安装的工具包。
/// 唤醒时，这些 tag 对应的工具会自动注入到 Prompt 中（免绑定）。
/// 典型值："project_management"、"data_analysis" 等
#[serde(default)]
pub installed_tags: Vec<String>,
```

- [ ] **Step 2: 更新 Default 实现**

在 [AgentRuntimeConfig::default()](file:///Users/aman/Technology/rust/ai_orz/src/models/agent.rs#L40-L50) 中添加：

```rust
impl Default for AgentRuntimeConfig {
    fn default() -> Self {
        Self {
            max_thinking_depth: default_max_thinking_depth(),
            thinking_interval_ms: 0,
            max_tool_calls_per_step: default_max_tool_calls_per_step(),
            enable_reflection: false,
            require_user_confirm: true,
            installed_tags: Vec::new(),  // 新增：默认空列表
        }
    }
}
```

- [ ] **Step 3: 添加便捷方法**

在 `impl AgentRuntimeConfig` 中添加：

```rust
/// 安装工具包 tag
pub fn install_tag(&mut self, tag: &str) {
    if !self.installed_tags.contains(&tag.to_string()) {
        self.installed_tags.push(tag.to_string());
    }
}

/// 卸载工具包 tag
pub fn uninstall_tag(&mut self, tag: &str) {
    self.installed_tags.retain(|t| t != tag);
}

/// 检查是否已安装某个 tag
pub fn has_tag(&self, tag: &str) -> bool {
    self.installed_tags.contains(&tag.to_string())
}
```

- [ ] **Step 4: 在 AgentPo 上添加便捷方法**

在 [AgentPo](file:///Users/aman/Technology/rust/ai_orz/src/models/agent.rs#L216-L292) 的 impl 中添加：

```rust
/// 获取已安装的工具包 tags
pub fn get_installed_tags(&self) -> Vec<String> {
    self.get_runtime_config().installed_tags
}

/// 安装工具包 tag 并持久化到 runtime_config
pub fn install_tag(&mut self, tag: &str) {
    let mut config = self.get_runtime_config();
    config.install_tag(tag);
    self.set_runtime_config(&config);
}
```

- [ ] **Step 5: 验证编译**

Run: `cargo check`
Expected: PASS（新增字段有 serde default，向后兼容）

- [ ] **Step 6: 运行测试**

Run: `cargo test agent`
Expected: PASS（所有现有 agent 测试通过，因为 serde default 保证向后兼容）

- [ ] **Step 7: Commit**

```bash
git add src/models/agent.rs
git commit -m "feat(agent): add installed_tags to AgentRuntimeConfig for tool pack support"
```

---

### Task 2: 修复神经工具标记 + 新增 send_tool_call_message + 移除 request_tool_call 的工具注册

**Files:**
- Modify: `src/handlers/finance/message/send_message.rs` — 补齐 `neural` flag
- Modify: `src/handlers/finance/tool/request_tool_call.rs` — 去掉 `register_handler_tool`，保留 HTTP Handler
- New: `src/handlers/finance/tool/send_tool_call_message.rs` — 新增神经工具

### 背景

当前 `request_tool_call` 被注册为神经工具，但它的实现是**同步直接调用** `call_manual_tool_for_agent()`，这与 manual 模式工具的异步消息设计矛盾。

正确架构：
- **auto 工具**（神经工具 + 工具包工具）：rig 框架直接调用，同步返回
- **manual 工具**（外骨骼工具）：Agent 调用 `send_tool_call_message` 神经工具发 ToolCallRequest 消息 → 消费者异步执行 → ToolCallResult 回 Agent

因此：
- `request_tool_call` 保留为普通 HTTP Handler（去掉 `register_handler_tool`），供 HTTP API 或后续复杂架构使用
- 新增 `send_tool_call_message` 神经工具，封装 `message_domain.delivery().send_tool_call_request()`，发完消息立即返回

- [ ] **Step 1: 给 send_message 加 neural flag**

在 [src/handlers/finance/message/send_message.rs](file:///Users/aman/Technology/rust/ai_orz/src/handlers/finance/message/send_message.rs#L10-L15) 修改：

```rust
#[register_handler_tool(
    id = "send_message",
    name = "send_message",
    description = "Send a message to a user",
    params = "common::api::SendMessageParams",
    neural  // ← 新增
)]
```

- [ ] **Step 2: 从 request_tool_call 移除工具注册**

在 [src/handlers/finance/tool/request_tool_call.rs](file:///Users/aman/Technology/rust/ai_orz/src/handlers/finance/tool/request_tool_call.rs#L10-L16) 中，去掉 `#[register_handler_tool(...)]`，只保留 `#[generate_http_handler]`：

```rust
/// 请求工具调用（同步，HTTP API 专用）
///
/// 注意：此 Handler 不注册为 Agent 工具。
/// Agent 异步调用工具应使用 `dispatch_tool_call` 神经工具。
#[generate_http_handler]
pub async fn request_tool_call(
    ctx: RequestContext,
    params: RequestToolCallParams,
) -> Result<RequestToolCallResponse> {
    // ... 保持现有实现不变 ...
}
```

- [ ] **Step 3: 新增 send_tool_call_message 神经工具**

在 `src/handlers/finance/tool/send_tool_call_message.rs` 中创建：

```rust
//! Handler: 发送工具调用消息（异步，神经工具）

use crate::pkg::RequestContext;
use crate::service::domain::message::{self, SendToolCallRequestCommand};
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{SendToolCallMessageParams, SendToolCallMessageResponse};
use common::error::Result;

/// 发送工具调用消息（异步）
///
/// Agent 通过此工具发起 manual 工具的异步调用。
/// 消息发送后立即返回，工具执行结果通过 ToolCallResult 消息在下一轮 awaken 中送达。
#[register_handler_tool(
    id = "send_tool_call_message",
    name = "send_tool_call_message",
    description = "Send a tool call message (async)",
    params = "common::api::SendToolCallMessageParams",
    neural
)]
#[generate_http_handler]
pub async fn send_tool_call_message(
    ctx: RequestContext,
    params: SendToolCallMessageParams,
) -> Result<SendToolCallMessageResponse> {
    let agent_id = ctx
        .agent_id()
        .ok_or_else(|| common::error::err!(InvalidRequest, "当前请求缺少 Agent 上下文"))?
        .clone();

    let request_id = uuid::Uuid::now_v7().to_string();

    let cmd = SendToolCallRequestCommand {
        request_id: &request_id,
        tool_id: &params.tool_id,
        tool_name: &params.tool_name,
        from_agent_id: &agent_id,
        to_executor_id: "system",
        project_id: params.project_id.as_deref(),
        task_id: params.task_id.as_deref(),
        reply_to_id: None,
        args: params.params,
    };

    let message = message::domain().delivery().send_tool_call_request(ctx, cmd).await?;

    Ok(SendToolCallMessageResponse {
        request_id,
        message_id: message.po.id,
        status: "dispatched".to_string(),
    })
}
```

- [ ] **Step 4: 新增 DTO**

在 `common/src/api/` 中新增 `SendToolCallMessageParams` 和 `SendToolCallMessageResponse`。

- [ ] **Step 5: 验证编译**

Run: `cargo check`
Expected: PASS

- [ ] **Step 6: 运行测试**

Run: `cargo test`
Expected: PASS（554 个测试通过）

- [ ] **Step 7: Commit**

```bash
git add src/handlers/finance/message/send_message.rs src/handlers/finance/tool/request_tool_call.rs src/handlers/finance/tool/send_tool_call_message.rs common/src/api/
git commit -m "feat(tools): add send_tool_call_message neural tool, remove request_tool_call from agent tools"
```

---

### 4A-1b: 工具包机制

### Task 3: 项目管理工具包标记

**Files:**
- Modify: `src/handlers/project/task/mark_done.rs`
- Modify: `src/handlers/project/task/create_task.rs`
- Modify: `src/handlers/project/task/update_task.rs`
- Modify: `src/handlers/project/task/get_task.rs`
- Modify: `src/handlers/project/task/list_project_tasks.rs`
- Modify: `src/handlers/project/task/list_agent_tasks.rs`
- Modify: `src/handlers/project/task/update_task_status.rs`
- Modify: `src/handlers/project/project/create_project.rs`
- Modify: `src/handlers/project/project/update_project.rs`
- Modify: `src/handlers/project/project/get_project.rs`
- Modify: `src/handlers/project/project/list_projects.rs`
- Modify: `src/handlers/project/project/update_project_status.rs`

### 背景

所有项目/任务管理相关的工具都需要加 `tags = "project_management"` 标记，使其归入项目管理工具包。Agent 入职时安装该 tag 后即可免绑定调用这些工具。

- [ ] **Step 1: 给所有 12 个工具加 tags**

对每个工具的 `#[register_handler_tool(...)]` 宏调用，新增 `tags = "project_management"` 参数。

**完整工具清单**（每个都加 `tags = "project_management"`）：

| 文件 | 工具 ID |
|------|---------|
| `src/handlers/project/task/mark_done.rs` | `mark_done` |
| `src/handlers/project/task/create_task.rs` | `create_task` |
| `src/handlers/project/task/update_task.rs` | `update_task` |
| `src/handlers/project/task/get_task.rs` | `get_task` |
| `src/handlers/project/task/list_project_tasks.rs` | `list_project_tasks` |
| `src/handlers/project/task/list_agent_tasks.rs` | `list_agent_tasks` |
| `src/handlers/project/task/update_task_status.rs` | `update_task_status` |
| `src/handlers/project/project/create_project.rs` | `create_project` |
| `src/handlers/project/project/update_project.rs` | `update_project` |
| `src/handlers/project/project/get_project.rs` | `get_project` |
| `src/handlers/project/project/list_projects.rs` | `list_projects` |
| `src/handlers/project/project/update_project_status.rs` | `update_project_status` |

- [ ] **Step 2: 验证编译**

Run: `cargo check`
Expected: PASS

- [ ] **Step 3: 运行测试**

Run: `cargo test`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/handlers/project/
git commit -m "feat(tools): tag all project/task management tools with 'project_management'"
```

---

### Task 4: 免绑定校验扩展支持 installed_tags

**Files:**
- Modify: `src/service/domain/runtime/tool_execution.rs`

### 背景

当前 [call_manual_tool_for_agent](file:///Users/aman/Technology/rust/ai_orz/src/service/domain/runtime/tool_execution.rs#L74-L136) 的免绑定校验逻辑为：
1. 先在 agent 绑定工具中查找
2. 找不到？检查是否是神经工具（tags 含 "neural"）
3. 都不是 → 拒绝

需要扩展为三层校验：
1. 先在 agent 绑定工具中查找
2. 找不到？检查是否是神经工具（tags 含 "neural"）
3. 还找不到？检查是否属于已安装的工具包（tool 的 tags 与 agent 的 installed_tags 有交集）
4. 都不是 → 拒绝

**注意：** 此校验用于消费者 `handle_tool_call_request` 中通过 `call_manual_tool_for_agent` 执行工具时的权限检查。Agent 端通过 `dispatch_tool_call` 发送消息时不需要校验（消息本身就是意图表达，校验在执行端）。

- [ ] **Step 1: 修改 call_manual_tool_for_agent 方法**

在 [src/service/domain/runtime/tool_execution.rs](file:///Users/aman/Technology/rust/ai_orz/src/service/domain/runtime/tool_execution.rs#L74-L136) 中，修改 `None =>` 分支，扩展为三层校验。

- [ ] **Step 2: 实现 agent_config 获取辅助函数**

通过 `hr_domain.agent_manage().get_agent()` 加载 Agent 实体获取 `runtime_config.installed_tags`。

- [ ] **Step 3: 确保 RuntimeDomainImpl 有 hr_domain 依赖**

检查 [RuntimeDomainImpl](file:///Users/aman/Technology/rust/ai_orz/src/service/domain/runtime/mod.rs) 是否已注入 `hr_domain`。如果没有，需要添加。

Run: `cargo check`
Expected: 如果 hr_domain 依赖缺失会报错，按编译错误添加依赖

- [ ] **Step 4: 运行测试**

Run: `cargo test`
Expected: PASS

- [ ] **Step 5: 新增单元测试**

```rust
#[tokio::test]
async fn test_call_manual_tool_for_agent_with_installed_tag() {
    // 1. 创建 Agent，runtime_config 中安装 "project_management" tag
    // 2. 不绑定 project_management 工具包的任何工具
    // 3. 调用 call_manual_tool_for_agent 请求 mark_done 工具
    // 4. 验证：应该成功（因为已安装 project_management tag）
}

#[tokio::test]
async fn test_call_manual_tool_for_agent_without_installed_tag() {
    // 1. 创建 Agent，不安装任何 tag
    // 2. 调用 call_manual_tool_for_agent 请求 mark_done 工具
    // 3. 验证：应该失败（未绑定且未安装 tag）
}
```

Run: `cargo test tool_execution`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/service/domain/runtime/tool_execution.rs src/service/domain/runtime/tool_execution_test.rs
git commit -m "feat(runtime): extend tool binding check to support installed tool pack tags"
```

---

### Task 5: 唤醒时加载内置工具（神经 + 已安装工具包）

**Files:**
- Modify: `src/service/domain/runtime/awakening.rs`

### 背景

当前 [load_neural_tools](file:///Users/aman/Technology/rust/ai_orz/src/service/domain/runtime/awakening.rs#L158-L190) 只加载带 "neural" tag 的工具。需要扩展为 `load_builtin_tools`，同时加载：
1. 神经工具（tags 含 "neural"）— 所有 Agent 天生拥有
2. 已安装工具包工具（tags 与 agent.installed_tags 有交集）— 入职安装的工具包

- [ ] **Step 1: 重命名 load_neural_tools 为 load_builtin_tools**

- [ ] **Step 2: 更新 awaken 中的调用**

- [ ] **Step 3: 验证编译**

Run: `cargo check`
Expected: PASS

- [ ] **Step 4: 运行测试**

Run: `cargo test`
Expected: PASS

- [ ] **Step 5: 新增单元测试**

```rust
#[tokio::test]
async fn test_load_builtin_tools_includes_neural_and_installed_pack() {
    // 1. 创建 Agent，安装 "project_management" tag
    // 2. 调用 load_builtin_tools
    // 3. 验证：返回的工具包含 neural 工具 + project_management 工具包工具
    // 4. 验证：不包含未安装 tag 的外骨骼工具
}
```

Run: `cargo test awakening`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/service/domain/runtime/awakening.rs src/service/domain/runtime/awakening_test.rs
git commit -m "feat(runtime): load_builtin_tools loads neural tools + installed tool pack tools"
```

---

### 4A-1c: 入职 + API

### Task 6: Agent 入职自动安装 project_management 工具包

**Files:**
- Modify: `src/service/domain/hr/agent.rs`

### 背景

当 Agent 状态从 `PendingOnboard` → `Onboarded` 时，应自动安装 "project_management" 工具包 tag 到 runtime_config 中。

- [ ] **Step 1: 在 transition_status 中添加入职安装逻辑**

在 [src/service/domain/hr/agent.rs](file:///Users/aman/Technology/rust/ai_orz/src/service/domain/hr/agent.rs#L80-L123) 的 `transition_status` 方法中，在状态更新后、持久化前，添加入职安装逻辑。

- [ ] **Step 2: 验证编译**

Run: `cargo check`
Expected: PASS

- [ ] **Step 3: 运行现有测试**

Run: `cargo test agent_test`
Expected: PASS

- [ ] **Step 4: 新增单元测试**

```rust
#[tokio::test]
async fn test_onboard_installs_project_management_tag() {
    // 1. 创建 Agent（状态为 PendingOnboard）
    // 2. 调用 transition_status → Onboarded
    // 3. 验证：runtime_config.installed_tags 包含 "project_management"
}

#[tokio::test]
async fn test_non_onboard_transition_does_not_install_tag() {
    // 1. 创建 Agent（状态为 Interviewing）
    // 2. 调用 transition_status → PendingOnboard
    // 3. 验证：runtime_config.installed_tags 为空
}
```

Run: `cargo test agent_test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/service/domain/hr/agent.rs src/service/domain/hr/agent_test.rs
git commit -m "feat(hr): auto-install project_management tool pack on agent onboarding"
```

---

### Task 7: 工具包安装/卸载 API（Domain + Handler）

**Files:**
- Modify: `src/service/domain/hr/mod.rs`
- Modify: `src/service/domain/hr/agent.rs`
- New: `src/handlers/hr/agent/install_tool_pack.rs`
- New: `src/handlers/hr/agent/uninstall_tool_pack.rs`
- New: `src/handlers/hr/agent/list_installed_tool_packs.rs`

### 背景

除了入职时自动安装工具包，还需要提供显式的工具包管理 API，允许管理员：
- 给指定 Agent 安装某个 tag 的工具包
- 从指定 Agent 卸载某个 tag 的工具包
- 查询 Agent 已安装的工具包列表

实现方式：通过修改 `AgentRuntimeConfig.installed_tags`，调用 `agent_dal.update()` 持久化。

- [ ] **Step 1: 在 AgentManage trait 中新增方法**

在 [src/service/domain/hr/mod.rs](file:///Users/aman/Technology/rust/ai_orz/src/service/domain/hr/mod.rs#L103-L147) 的 `AgentManage` trait 中新增三个方法：

```rust
/// 安装工具包（按 tag）
async fn install_tool_pack(
    &self,
    ctx: RequestContext,
    agent_id: &str,
    tag: &str,
) -> Result<()>;

/// 卸载工具包（按 tag）
async fn uninstall_tool_pack(
    &self,
    ctx: RequestContext,
    agent_id: &str,
    tag: &str,
) -> Result<()>;

/// 列出已安装的工具包 tags
async fn list_installed_tool_packs(
    &self,
    ctx: RequestContext,
    agent_id: &str,
) -> Result<Vec<String>>;
```

- [ ] **Step 2: 在 HrDomainImpl 中实现方法**

在 [src/service/domain/hr/agent.rs](file:///Users/aman/Technology/rust/ai_orz/src/service/domain/hr/agent.rs) 中实现这三个方法。核心逻辑：

```rust
async fn install_tool_pack(
    &self,
    ctx: RequestContext,
    agent_id: &str,
    tag: &str,
) -> Result<()> {
    // 1. 加载 Agent
    let mut agent = self.agent_dal
        .find_by_id(ctx.clone(), agent_id)
        .await?
        .ok_or_else(|| common::error::Error::not_found("Agent not found"))?;
    
    // 2. 安装 tag（幂等：已安装则跳过）
    let mut config = agent.po.get_runtime_config();
    config.install_tag(tag);
    agent.po.set_runtime_config(&config);
    
    // 3. 持久化
    self.agent_dal.update(ctx, &agent).await?;
    
    log_info!(ctx, "install_tool_pack", "agent_id={}, tag={}", agent_id, tag);
    Ok(())
}
```

- [ ] **Step 3: 验证编译**

Run: `cargo check`
Expected: PASS

- [ ] **Step 4: 新增 Handler — install_tool_pack / uninstall_tool_pack / list_installed_tool_packs**

参考现有 Handler 风格创建三个新 Handler。

- [ ] **Step 5: 注册路由**

- `POST /api/hr/agents/{agent_id}/tool-packs/{tag}` — 安装工具包
- `DELETE /api/hr/agents/{agent_id}/tool-packs/{tag}` — 卸载工具包
- `GET /api/hr/agents/{agent_id}/tool-packs` — 列出已安装工具包

- [ ] **Step 6: 运行测试**

Run: `cargo test hr`
Expected: PASS

- [ ] **Step 7: 新增单元测试**

```rust
#[tokio::test]
async fn test_install_tool_pack() {
    // 1. 创建 Agent
    // 2. 调用 install_tool_pack("project_management")
    // 3. 验证：installed_tags 包含 "project_management"
    // 4. 再次调用 install_tool_pack（幂等测试）
    // 5. 验证：installed_tags 还是只有一个 "project_management"
}

#[tokio::test]
async fn test_uninstall_tool_pack() {
    // 1. 创建 Agent，安装一个 tag
    // 2. 调用 uninstall_tool_pack
    // 3. 验证：installed_tags 为空
}

#[tokio::test]
async fn test_list_installed_tool_packs() {
    // 1. 创建 Agent，安装多个 tag
    // 2. 调用 list_installed_tool_packs
    // 3. 验证：返回正确的 tag 列表
}
```

Run: `cargo test agent_test`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add src/service/domain/hr/ src/handlers/hr/
git commit -m "feat(hr): add tool pack install/uninstall/list API for agents"
```

---

### Task 8: 4A-1 最终验证

- [ ] **Step 1: 运行完整测试套件**

Run: `cargo test`
Expected: All tests PASS

- [ ] **Step 2: 运行 clippy**

Run: `cargo clippy -- -D warnings`
Expected: No warnings

- [ ] **Step 3: 端到端验证**

1. 创建 Agent → 入职 → 验证 installed_tags 包含 "project_management"
2. 给 Agent 发消息 → Agent 被唤醒 → 验证 Prompt 中包含神经工具 + project_management 工具包工具
3. Agent 调用 mark_done → 验证免绑定成功（因为已安装 project_management tag）
4. Agent 调用 send_tool_call_message → 验证 ToolCallRequest 消息入队 → 消费者执行 → ToolCallResult 回 Agent

---

## 4A-2: TaskAssignment 消息

### File Structure

| File | Responsibility | Change Type |
|------|---------------|-------------|
| `common/src/enums/message.rs` | 新增 TaskAssignment 消息类型 | Modify |
| `src/models/message.rs` | 新增 TaskAssignmentMessage payload 结构体 | Modify |
| `src/service/domain/message/mod.rs` | MessageDelivery trait 新增 send_task_assignment | Modify |
| `src/service/domain/message/delivery.rs` | 实现 send_task_assignment | Modify |
| `src/handlers/finance/message/send_task_assignment_message.rs` | 新增神经工具：发送任务分配消息 | New |
| `src/handlers/project/task/create_task.rs` | Handler 编排：创建任务后发 TaskAssignment 消息 | Modify |
| `src/service/domain/runtime/context_assembly.rs` | PromptBuilder 支持 TaskAssignment 提示语 | Modify |

---

### Task 9: 新增 MessageType::TaskAssignment + payload + 投递方法

**Files:**
- Modify: `common/src/enums/message.rs`
- Modify: `src/models/message.rs`
- Modify: `src/service/domain/message/mod.rs`
- Modify: `src/service/domain/message/delivery.rs`

### 背景

任务分配给 Agent 后，需要自动通知 Agent 开始执行。遵循"消息统一触发"原则，任务分配通过 `MessageType::TaskAssignment` 消息类型通知 Agent。

**职责划分：**
- **Project Domain**：只管数据持久化（创建任务、写入 assignee），不负责通知
- **Message Domain**：扩展 `TaskAssignment` 消息类型 + 投递方法，负责通知能力
- **Handler 层**：编排两个 Domain — 创建任务成功后，调用 Message Domain 发送 TaskAssignment 消息
- **Consumer**：`handle_agent_message` 天然处理（to_role=Agent），无需新增分支
- **Runtime**：PromptBuilder 根据 message_type 差异化提示语

**参考实现：** ToolCallResult 消息（MessageType::ToolCallResult = 6），有完整的消息类型定义 + payload 结构体 + Command + 投递方法实现 + 消费者处理链路。

- [ ] **Step 1: 新增 MessageType::TaskAssignment**

在 [common/src/enums/message.rs](file:///Users/aman/Technology/rust/ai_orz/common/src/enums/message.rs#L62-L82) 中新增：

```rust
/// TaskAssignment (任务分配通知，System/User → Agent)
TaskAssignment = 9,
```

同时更新 `From<i32>` 实现和其他映射。

- [ ] **Step 2: 新增 TaskAssignmentMessage payload 结构体**

在 [src/models/message.rs](file:///Users/aman/Technology/rust/ai_orz/src/models/message.rs) 中新增，参考 `ToolCallMessage` 的模式：

```rust
/// 任务分配消息内容
///
/// 存储在 message.content 中，对应 MessageType::TaskAssignment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAssignmentMessage {
    /// 任务 ID
    pub task_id: String,
    /// 任务标题
    pub task_title: String,
    /// 任务描述（可选）
    pub task_description: Option<String>,
    /// 关联项目 ID
    pub project_id: Option<String>,
    /// 分配者 ID
    pub from_id: String,
    /// 接收 Agent ID
    pub to_agent_id: String,
}
```

- [ ] **Step 3: Message Domain 新增 Command + trait 方法**

在 [src/service/domain/message/mod.rs](file:///Users/aman/Technology/rust/ai_orz/src/service/domain/message/mod.rs) 中新增：

```rust
/// 发送任务分配消息的命令参数
#[derive(Debug, Clone)]
pub struct SendTaskAssignmentCommand<'a> {
    pub task_id: &'a str,
    pub task_title: &'a str,
    pub task_description: Option<&'a str>,
    pub from_id: &'a str,
    pub from_role: MessageRole,
    pub to_agent_id: &'a str,
    pub project_id: Option<&'a str>,
}
```

在 `MessageDelivery` trait 中新增：

```rust
/// 发送任务分配消息
async fn send_task_assignment(
    &self,
    ctx: RequestContext,
    cmd: SendTaskAssignmentCommand<'_>,
) -> Result<Message>;
```

- [ ] **Step 4: 实现 send_task_assignment**

在 [src/service/domain/message/delivery.rs](file:///Users/aman/Technology/rust/ai_orz/src/service/domain/message/delivery.rs) 中实现，参考 `send_tool_call_result` 的模式：

1. 构造 TaskAssignmentMessage payload
2. 序列化为 JSON 作为 content
3. 构造 MessagePo（from_role=User/Agent, to_role=Agent, message_type=TaskAssignment）
4. 持久化

- [ ] **Step 5: 验证编译**

Run: `cargo check`
Expected: PASS

- [ ] **Step 6: 新增单元测试**

```rust
#[tokio::test]
async fn test_send_task_assignment() {
    // 1. 调用 send_task_assignment
    // 2. 验证：消息类型为 TaskAssignment
    // 3. 验证：to_role=Agent, from_role=User
    // 4. 验证：task_id、project_id 正确关联
    // 5. 验证：content 可正确反序列化为 TaskAssignmentMessage
}
```

Run: `cargo test delivery`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add common/src/enums/message.rs src/models/message.rs src/service/domain/message/
git commit -m "feat(message): add TaskAssignment message type and delivery method"
```

---

### Task 10: 新增 send_task_assignment_message 神经工具

**Files:**
- New: `src/handlers/finance/message/send_task_assignment_message.rs`

### 背景

主 Agent 分配任务给子 Agent 时，需要通过消息通知。和 `send_tool_call_message` 一样，这是一个封装了 Message Domain 投递方法的神经工具。

- [ ] **Step 1: 新增 send_task_assignment_message 神经工具**

```rust
//! Handler: 发送任务分配消息（神经工具）

use crate::pkg::RequestContext;
use crate::service::domain::message::{self, SendTaskAssignmentCommand};
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{SendTaskAssignmentMessageParams, SendTaskAssignmentMessageResponse};
use common::error::Result;

/// 发送任务分配消息
///
/// Agent 通过此工具给其他 Agent 分配任务。
/// 消息发送后立即返回，接收 Agent 在下一轮 awaken 中收到任务分配通知。
#[register_handler_tool(
    id = "send_task_assignment_message",
    name = "send_task_assignment_message",
    description = "Send a task assignment message to another agent",
    params = "common::api::SendTaskAssignmentMessageParams",
    neural
)]
#[generate_http_handler]
pub async fn send_task_assignment_message(
    ctx: RequestContext,
    params: SendTaskAssignmentMessageParams,
) -> Result<SendTaskAssignmentMessageResponse> {
    let from_id = ctx
        .agent_id()
        .map(|s| s.to_string())
        .unwrap_or_else(|| ctx.uid().to_string());

    let cmd = SendTaskAssignmentCommand {
        task_id: &params.task_id,
        task_title: &params.task_title,
        task_description: params.task_description.as_deref(),
        from_id: &from_id,
        from_role: common::enums::MessageRole::Agent,
        to_agent_id: &params.to_agent_id,
        project_id: params.project_id.as_deref(),
    };

    let message = message::domain().delivery().send_task_assignment(ctx, cmd).await?;

    Ok(SendTaskAssignmentMessageResponse {
        message_id: message.po.id,
    })
}
```

- [ ] **Step 2: 新增 DTO**

在 `common/src/api/` 中新增 `SendTaskAssignmentMessageParams` 和 `SendTaskAssignmentMessageResponse`。

- [ ] **Step 3: 验证编译 + 测试**

Run: `cargo check && cargo test`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/handlers/finance/message/send_task_assignment_message.rs common/src/api/
git commit -m "feat(tools): add send_task_assignment_message neural tool"
```

---

### Task 11: 任务创建 Handler 编排 + PromptBuilder 差异化

**Files:**
- Modify: `src/handlers/project/task/create_task.rs`
- Modify: `src/service/domain/runtime/context_assembly.rs`

### 背景

任务创建成功后，如果 assignee_type = Agent，Handler 层编排调用 Message Domain 发送 TaskAssignment 消息。

- [ ] **Step 1: 在 create_task Handler 中添加编排**

在任务创建成功后，如果分配给 Agent，调用 Message Domain 发送 TaskAssignment 消息：

```rust
// 创建任务成功后，如果分配给 Agent，自动发送任务分配消息
if task.po.assignee_type == AssigneeType::Agent {
    if let Some(assignee_id) = &task.po.assignee_id {
        message_domain.delivery().send_task_assignment(
            ctx.clone(),
            SendTaskAssignmentCommand {
                task_id: &task.po.id,
                task_title: &task.po.title,
                task_description: task.po.description.as_deref(),
                from_id: ctx.uid(),
                from_role: MessageRole::User,
                to_agent_id: assignee_id,
                project_id: task.po.project_id.as_deref(),
            },
        ).await?;
    }
}
```

- [ ] **Step 2: PromptBuilder 支持 TaskAssignment 差异化提示**

在 PromptBuilder 中，针对 TaskAssignment 类型的消息，使用专门的提示语前缀：

```
【任务分配通知】
你被分配了一个新任务：
  任务 ID: {task_id}
  任务标题: {task_title}
  ...

请开始执行任务。完成后使用 mark_done 工具标记完成。
```

- [ ] **Step 3: 验证编译 + 测试**

Run: `cargo check && cargo test`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/handlers/project/task/create_task.rs src/service/domain/runtime/context_assembly.rs
git commit -m "feat(project): auto-send TaskAssignment on task creation + prompt differentiation"
```

---

### Task 12: 4A-2 最终验证

- [ ] **Step 1: 运行完整测试套件**

Run: `cargo test`
Expected: All tests PASS

- [ ] **Step 2: 运行 clippy**

Run: `cargo clippy -- -D warnings`
Expected: No warnings

- [ ] **Step 3: 端到端验证**

1. 创建任务分配给 Agent → 验证 TaskAssignment 消息入队
2. Agent 被唤醒 → 验证 Prompt 中显示【任务分配通知】
3. 主 Agent 调用 send_task_assignment_message → 子 Agent 被唤醒

---

## 验收标准对照

### 4A-1 验收标准

| 验收标准 | 对应 Task | 状态 |
|---------|----------|------|
| AgentRuntimeConfig.installed_tags 字段完整实现 | Task 1 | [ ] |
| send_message 补齐 neural flag | Task 2 | [ ] |
| send_tool_call_message 神经工具新增 | Task 2 | [ ] |
| request_tool_call 从神经工具移除 | Task 2 | [ ] |
| project_management 工具包所有工具正确标记 tag | Task 3 | [ ] |
| 免绑定校验支持神经工具 + 已安装工具包 | Task 4 | [ ] |
| 唤醒时注入神经工具 + 已安装工具包工具 | Task 5 | [ ] |
| Agent 入职时自动安装 project_management 工具包 | Task 6 | [ ] |
| 工具包安装/卸载/查询 API 可用 | Task 7 | [ ] |
| 所有现有测试通过 | Task 8 | [ ] |

### 4A-2 验收标准

| 验收标准 | 对应 Task | 状态 |
|---------|----------|------|
| MessageType::TaskAssignment 定义 | Task 9 | [ ] |
| send_task_assignment 投递方法实现 | Task 9 | [ ] |
| send_task_assignment_message 神经工具 | Task 10 | [ ] |
| 任务创建后自动发送 TaskAssignment 消息 | Task 11 | [ ] |
| PromptBuilder 支持 TaskAssignment 差异化提示 | Task 11 | [ ] |
| 所有现有测试通过 | Task 12 | [ ] |

---

## 神经工具清单（4A-1 完成后）

| 工具 ID | 说明 | 分类 | 来源 |
|---------|------|------|------|
| `search_memory` | 关键词 + 向量语义混合搜索记忆 | 记忆 | 天生 |
| `query_memory` | 通用关系型查询记忆 | 记忆 | 天生 |
| `create_memory` | 创建新记忆（短期/长期） | 记忆 | 天生 |
| `update_memory` | 更新已有记忆 | 记忆 | 天生 |
| `delete_memory` | 删除记忆 | 记忆 | 天生 |
| `send_message` | 发送消息给用户 | 消息 | 天生 |
| `list_tools` | 列出可用工具 | 工具 | 天生 |
| `send_tool_call_message` | 发送工具调用消息（异步） | 工具 | 天生 |
| `send_task_assignment_message` | 发送任务分配消息 | 任务 | 天生（4A-2 新增） |

## 神经工具标记现状

| 工具 ID | 应为神经工具 | 当前标记 | 需修复 |
|---------|-------------|---------|--------|
| `search_memory` | ✅ 是 | ✅ 有 neural | 无需修改 |
| `query_memory` | ✅ 是 | ✅ 有 neural | 无需修改 |
| `create_memory` | ✅ 是 | ✅ 有 neural | 无需修改 |
| `update_memory` | ✅ 是 | ✅ 有 neural | 无需修改 |
| `delete_memory` | ✅ 是 | ✅ 有 neural | 无需修改 |
| `list_tools` | ✅ 是 | ✅ 有 neural | 无需修改 |
| `send_message` | ✅ 是 | ❌ 缺失 neural | **需补齐**（Task 2） |
| `send_tool_call_message` | ✅ 是（新增） | — | **新增**（Task 2） |
| `request_tool_call` | ❌ 否（同步 HTTP API） | 有 register_handler_tool | **需移除**（Task 2） |
| `mark_done` | ❌ 否（属于工具包） | 无 neural（正确） | 需加 `project_management` tag |

## 免绑定校验逻辑变更

```
Agent 调用 Manual 工具（消费者侧 call_manual_tool_for_agent）
    │
    ├── 先在 agent 绑定工具中查找
    │
    ├── 找不到？检查是否是神经工具（tags 含 "neural"）
    │       └─ 是 → 免绑定放行
    │
    ├── 还找不到？检查是否属于已安装的工具包
    │       └─ Agent 已安装该 tag → 免绑定放行
    │
    └── 都不是 → 拒绝：工具未绑定且不属于已安装工具包
```

## 唤醒时工具注入逻辑变更

```
load_builtin_tools(ctx, agent)
    │
    ├── 加载神经工具（tags 含 "neural"）
    │       └─ 所有 Agent 天生拥有
    │
    └── 加载已安装工具包工具
            └─ 查 Agent 已安装的 tags 列表
            └─ 按 tags 过滤所有启用工具
```

## 异步工具调用完整链路

```
Agent awaken（第 1 轮）
    │
    └── LLM 调用 send_tool_call_message 神经工具
            │
            └── message_domain.delivery().send_tool_call_request() → 消息入队
            └── 立即返回"已提交"
    │
    awaken 结束
    │
    ▼ Consumer 收到 ToolCallRequest 消息（to_role=System）
    │   └── handle_tool_call_request()
    │       ├── runtime_domain.tool_execution().call_manual_tool_for_agent()  ← 直接通过 Domain
    │       │   └── 三层免绑定校验：绑定 → 神经 → 已安装 tag
    │       └── message_domain.delivery().send_tool_call_result()  ← 结果回 Agent
    │
    ▼ Consumer 收到 ToolCallResult 消息（to_role=Agent）
        └── handle_agent_message() → runtime_domain.awakening().awaken()  ← Agent 第 2 轮思考
```

## 任务分配完整链路

```
用户/Agent 创建任务（create_task Handler / send_task_assignment_message 神经工具）
    │
    ├── project_domain.task_manage().create()  ← 同步，记录任务
    │
    └── message_domain.delivery().send_task_assignment()  ← 异步，消息入队
            │
            ▼ Consumer 收到 TaskAssignment 消息（to_role=Agent）
                └── handle_agent_message() → awaken()
                        │
                        └── PromptBuilder 显示【任务分配通知】
                        └── Agent 开始执行任务
                        └── 完成后调用 mark_done（project_management 工具包）
```

---

## 后续任务（P1/P2，本次不实现）

| # | 任务 | 说明 | 优先级 |
|---|------|------|--------|
| 4.A.9 | 子任务分解能力 | Agent 可以通过项目管理工具包创建子任务，形成任务树 | P1 |
| 4.A.10 | 任务进度追踪 | 百分比、当前步骤、执行历史 | P1 |
| 4.A.11 | 任务失败/重试机制 | 任务执行失败后自动重试或转人工 | P2 |
| 4.A.12 | 任务产物管理 | 执行结果、附件、中间产物 | P2 |
