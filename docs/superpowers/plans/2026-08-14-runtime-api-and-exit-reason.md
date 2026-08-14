# Runtime API + exit_reason 持久化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 暴露 Agent 运行时查询/取消/列表 HTTP 接口，并在统计事件中持久化 exit_reason，为前端运行时面板和统计分析铺路。

**Architecture:** 阶段 2 已完成 StateManager 层的 set/clear/cancel_thinking/get_think_runtime_snapshot。阶段 3 在此基础上：① 扩展 AgentAwakeEvent 新增 exit_reason 字段（DuckDB 持久化）；② 在 Cancelled 分支补充事件发布；③ 扩展 RuntimeDomain trait 新增 3 个方法；④ 实现 3 个 HTTP Handler + 路由注册。不扩展 SSE，前端通过轮询 runtime-status 获取实时状态。

**Tech Stack:** Rust + Axum + DuckDB + sqlx + common DTO

---

## File Structure

| 文件 | 职责 | 操作 |
|------|------|------|
| `common/src/api/runtime.rs` | Runtime 相关 DTO（Request/Response） | Create |
| `common/src/api/mod.rs` | 注册 runtime 模块 | Modify |
| `src/pkg/stats/agent_awake.rs` | AgentAwakeEvent 新增 exit_reason 字段 | Modify |
| `src/pkg/agent_runtime_state.rs` | StateManager 新增 list_runtime_agents 方法 | Modify |
| `src/service/domain/runtime/mod.rs` | RuntimeDomain trait 新增 3 个方法 + impl | Modify |
| `src/service/domain/runtime/awakening.rs` | Cancelled 分支补充事件发布 + exit_reason | Modify |
| `src/handlers/hr/agent/runtime_status.rs` | GET /agents/{id}/runtime-status | Create |
| `src/handlers/hr/agent/cancel_thinking.rs` | POST /agents/{id}/cancel-thinking | Create |
| `src/handlers/hr/agent/runtime_list.rs` | GET /agents/runtime-list | Create |
| `src/handlers/hr/agent/mod.rs` | 注册 3 个新 handler 模块 | Modify |
| `src/router.rs` | 注册 3 条路由 | Modify |

---

## Task 1: DTO 定义（common/src/api/runtime.rs）

**Files:**
- Create: `common/src/api/runtime.rs`
- Modify: `common/src/api/mod.rs`

- [ ] **Step 1: 创建 runtime.rs DTO 文件**

```rust
//! Runtime API 请求/响应 DTO
//!
//! Agent 运行时状态查询、取消思考、运行中 Agent 列表。

use serde::{Deserialize, Serialize};

/// GET /agents/{id}/runtime-status 响应
///
/// 包含 Agent 运行时状态 + 思考运行时快照（如有）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeStatusResponse {
    /// Agent ID
    pub agent_id: String,
    /// 运行时状态："idle" / "busy" / "resting"
    pub state: String,
    /// 当前处理的消息 ID（仅 Busy 时有值）
    pub current_message_id: Option<String>,
    /// 当前关联的任务 ID
    pub task_id: Option<String>,
    /// 当前关联的项目 ID
    pub project_id: Option<String>,
    /// 状态开始时间戳（毫秒）
    pub state_started_at: i64,
    /// 思考运行时快照（仅 Busy 时有值）
    pub think_runtime: Option<ThinkRuntimeInfo>,
}

/// 思考运行时信息（前端展示用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkRuntimeInfo {
    /// 当前 trace_id（日志检索用）
    pub trace_id: String,
    /// 场景："awaken" / "settle" / "summary" / "intent-analyze"
    pub scene: String,
    /// 当前轮次
    pub round: usize,
    /// 最大轮次
    pub max_rounds: usize,
    /// 累计输入 token
    pub tokens_input: u64,
    /// 累计输出 token
    pub tokens_output: u64,
    /// 累计总 token
    pub total_tokens: u64,
    /// 工具调用次数
    pub tool_call_count: usize,
    /// 思考状态："thinking" / "cancelled" / "finished"
    pub status: String,
    /// 思考开始时间戳（毫秒）
    pub started_at: i64,
    /// 最后更新时间戳（毫秒）
    pub last_updated_at: i64,
}

/// POST /agents/{id}/cancel-thinking 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelThinkingResponse {
    /// 是否成功取消（false 表示 Agent 当前未在思考）
    pub success: bool,
    /// 描述信息
    pub message: String,
}

/// GET /agents/runtime-list 请求参数
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RuntimeListRequest {
    /// 按状态过滤："busy" / "resting" / "idle"（不传则返回全部）
    #[serde(default)]
    pub state: Option<String>,
    /// 按任务 ID 过滤
    #[serde(default)]
    pub task_id: Option<String>,
    /// 按项目 ID 过滤
    #[serde(default)]
    pub project_id: Option<String>,
}

/// GET /agents/runtime-list 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeListResponse {
    /// 运行中 Agent 列表
    pub items: Vec<RuntimeStatusResponse>,
    /// 总数
    pub total: usize,
}
```

- [ ] **Step 2: 在 common/src/api/mod.rs 注册 runtime 模块**

在 `pub mod user;` 之后添加：

```rust
pub mod runtime;
```

在 re-exports 区域 `pub use user::*;` 之后添加：

```rust
pub use runtime::*;
```

- [ ] **Step 3: 验证编译**

Run: `cargo check -p common --message-format short`
Expected: 编译通过，无错误

- [ ] **Step 4: Commit**

```bash
git add common/src/api/runtime.rs common/src/api/mod.rs
git commit -m "feat: 新增 runtime API DTO（RuntimeStatusResponse/CancelThinkingResponse/RuntimeListRequest）"
```

---

## Task 2: AgentAwakeEvent 扩展 exit_reason 字段

**Files:**
- Modify: `src/pkg/stats/agent_awake.rs`

- [ ] **Step 1: 在 AgentAwakeEvent 结构体新增 exit_reason 字段**

在 `status: String` 字段之后添加：

```rust
    #[metric]
    pub exit_reason: String,
```

- [ ] **Step 2: 修改 new() 默认值**

在 `new()` 方法中添加：

```rust
            exit_reason: String::new(),
```

- [ ] **Step 3: 新增 with_exit_reason builder 方法**

在 `with_status` 方法之后添加：

```rust
    pub fn with_exit_reason(mut self, v: String) -> Self {
        self.exit_reason = v;
        self
    }
```

- [ ] **Step 4: 修改 create_table SQL（新增 exit_reason 列）**

将 create_table SQL 改为：

```sql
            CREATE TABLE IF NOT EXISTS agent_awake_events (
                id UUID PRIMARY KEY,
                timestamp BIGINT,
                agent_id VARCHAR,
                project_id VARCHAR,
                task_id VARCHAR,
                organization_id VARCHAR,
                user_id VARCHAR,
                message_id VARCHAR,
                call_count BIGINT,
                duration_ms BIGINT,
                status VARCHAR,
                exit_reason VARCHAR
            );
```

- [ ] **Step 5: 修改 insert_event SQL 和参数**

将 insert_event 的 SQL 改为：

```sql
            INSERT INTO agent_awake_events (
                id, timestamp, agent_id, project_id, task_id,
                organization_id, user_id, message_id,
                call_count, duration_ms, status, exit_reason
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);
```

参数数组末尾添加：

```rust
                &event.exit_reason as &dyn ToSql,
```

- [ ] **Step 6: 修改 bulk_insert_events SQL 和参数**

与 insert_event 相同的修改。

- [ ] **Step 7: 验证编译**

Run: `cargo check --message-format short`
Expected: 编译通过

- [ ] **Step 8: Commit**

```bash
git add src/pkg/stats/agent_awake.rs
git commit -m "feat: AgentAwakeEvent 新增 exit_reason 字段（DuckDB 持久化）"
```

---

## Task 3: Cancelled 分支补充事件发布 + exit_reason 记录

**Files:**
- Modify: `src/service/domain/runtime/awakening.rs`

- [ ] **Step 1: 在 awaken 的 Cancelled 分支补充事件发布**

找到 awaken 方法中的 `Ok(ThinkLoopResult::Cancelled { ... })` 分支，在 `return Ok(AwakeningResult { ... })` 之前添加：

```rust
                    // 记录取消的统计事件 + 循环完成事件
                    let duration_ms = start_time
                        .elapsed()
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);
                    if let Err(stats_err) = record_event!(
                        ctx.clone(),
                        AgentAwakeEvent {
                            agent_id: agent.po.id.clone(),
                            project_id: ctx.project_id().cloned(),
                            task_id: ctx.task_id().cloned(),
                            organization_id: ctx.organization_id.clone(),
                            user_id: Some(ctx.uid()),
                            message_id: Some(message.po.id.clone()),
                            call_count: 1,
                            duration_ms: duration_ms,
                            status: "cancelled".to_string(),
                            exit_reason: "cancelled".to_string(),
                        }
                    ) {
                        log_warn!(
                            &ctx,
                            "awaken",
                            "record_event failed on cancel path: {:?}",
                            stats_err
                        );
                    }
                    let _ = crate::pkg::aop::publish(AgentLoopEvent::finished(
                        &agent.po.id,
                        &trace_id,
                        "awaken",
                        "cancelled",
                        duration_ms,
                        Some(&message.po.id),
                    ))
                    .await;
```

- [ ] **Step 2: 在 awaken 成功路径补充 exit_reason**

找到 awaken 方法中成功路径的 `record_event!(... AgentAwakeEvent { ... status: "success".to_string() ... })`，添加 `exit_reason` 字段。

exit_reason 的值来自已有的 `exit_reason` 变量（第 1013 行定义），需要将其小写化：

```rust
                        exit_reason: exit_reason.to_lowercase(),
```

- [ ] **Step 3: 在 awaken 失败路径补充 exit_reason**

找到 awaken 方法中失败路径的 `record_event!(... AgentAwakeEvent { ... status: format!("failed: {}", e) ... })`，添加：

```rust
                            exit_reason: "error".to_string(),
```

- [ ] **Step 4: 验证编译**

Run: `cargo check --message-format short`
Expected: 编译通过

- [ ] **Step 5: Commit**

```bash
git add src/service/domain/runtime/awakening.rs
git commit -m "feat: Cancelled 分支补充 AgentAwakeEvent + AgentLoopEvent 发布，记录 exit_reason"
```

---

## Task 4: 扩展 RuntimeDomain trait + StateManager

**Files:**
- Modify: `src/pkg/agent_runtime_state.rs`
- Modify: `src/service/domain/runtime/mod.rs`

- [ ] **Step 1: 在 StateManager 新增 list_runtime_agents 方法**

在 `get_all_states` 方法之后添加：

```rust
    /// 查询运行中 Agent 列表（带过滤参数）
    ///
    /// 过滤参数均为 Option，None 表示不过滤。
    /// state_filter: "busy" / "resting" / "idle"（None 返回全部）
    pub fn list_runtime_agents(
        &self,
        state_filter: Option<&str>,
        task_id_filter: Option<&str>,
        project_id_filter: Option<&str>,
    ) -> Vec<(String, AgentRuntimeInfo)> {
        self.states
            .iter()
            .filter(|entry| {
                let info = entry.value();
                // 状态过滤
                if let Some(state) = state_filter {
                    let info_state = match info.state {
                        AgentRuntimeState::Idle => "idle",
                        AgentRuntimeState::Busy => "busy",
                        AgentRuntimeState::Resting => "resting",
                    };
                    if info_state != state {
                        return false;
                    }
                }
                // 任务 ID 过滤
                if let Some(tid) = task_id_filter {
                    if info.task_id.as_deref() != Some(tid) {
                        return false;
                    }
                }
                // 项目 ID 过滤
                if let Some(pid) = project_id_filter {
                    if info.project_id.as_deref() != Some(pid) {
                        return false;
                    }
                }
                true
            })
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }
```

- [ ] **Step 2: 在 RuntimeDomain trait 新增 3 个方法**

在 `fn is_agent_unavailable(&self, agent_id: &str) -> bool;` 之后添加：

```rust
    /// 取消 Agent 思考（触发 cancel_flag）
    ///
    /// 返回 true 表示成功取消（Agent 正在思考），
    /// 返回 false 表示 Agent 当前未在思考。
    fn cancel_thinking(&self, agent_id: &str) -> bool;

    /// 查询 Agent 运行时状态 + 思考运行时快照
    ///
    /// 返回 (state, current_message_id, task_id, project_id, state_started_at, think_runtime_snapshot)
    fn get_runtime_status(
        &self,
        agent_id: &str,
    ) -> (
        AgentRuntimeState,
        Option<String>,
        Option<String>,
        Option<String>,
        i64,
        Option<crate::pkg::agent_runtime_state::ThinkRuntimeSnapshot>,
    );

    /// 查询运行中 Agent 列表（带过滤参数）
    fn list_runtime_agents(
        &self,
        state_filter: Option<&str>,
        task_id_filter: Option<&str>,
        project_id_filter: Option<&str>,
    ) -> Vec<(String, crate::pkg::agent_runtime_state::AgentRuntimeInfo)>;
```

- [ ] **Step 3: 在 RuntimeDomainImpl 实现这 3 个方法**

在 `fn is_agent_unavailable(...)` 的 impl 之后添加：

```rust
    fn cancel_thinking(&self, agent_id: &str) -> bool {
        crate::pkg::agent_runtime_state::AgentRuntimeStateManager::global()
            .cancel_thinking(agent_id)
    }

    fn get_runtime_status(
        &self,
        agent_id: &str,
    ) -> (
        AgentRuntimeState,
        Option<String>,
        Option<String>,
        Option<String>,
        i64,
        Option<crate::pkg::agent_runtime_state::ThinkRuntimeSnapshot>,
    ) {
        let mgr = crate::pkg::agent_runtime_state::AgentRuntimeStateManager::global();
        let info = mgr.get(agent_id);
        match info {
            Some(info) => (
                info.state,
                info.current_message_id,
                info.task_id,
                info.project_id,
                info.state_started_at,
                info.think_runtime.as_ref().map(|tr| tr.snapshot()),
            ),
            None => (
                AgentRuntimeState::Idle,
                None,
                None,
                None,
                0,
                None,
            ),
        }
    }

    fn list_runtime_agents(
        &self,
        state_filter: Option<&str>,
        task_id_filter: Option<&str>,
        project_id_filter: Option<&str>,
    ) -> Vec<(String, crate::pkg::agent_runtime_state::AgentRuntimeInfo)> {
        crate::pkg::agent_runtime_state::AgentRuntimeStateManager::global()
            .list_runtime_agents(state_filter, task_id_filter, project_id_filter)
    }
```

- [ ] **Step 4: 验证编译**

Run: `cargo check --message-format short`
Expected: 编译通过

- [ ] **Step 5: Commit**

```bash
git add src/pkg/agent_runtime_state.rs src/service/domain/runtime/mod.rs
git commit -m "feat: StateManager 新增 list_runtime_agents + RuntimeDomain trait 新增 cancel/status/list 方法"
```

---

## Task 5: 3 个 Handler 实现 + 路由注册

**Files:**
- Create: `src/handlers/hr/agent/runtime_status.rs`
- Create: `src/handlers/hr/agent/cancel_thinking.rs`
- Create: `src/handlers/hr/agent/runtime_list.rs`
- Modify: `src/handlers/hr/agent/mod.rs`
- Modify: `src/router.rs`

- [ ] **Step 1: 创建 runtime_status.rs handler**

```rust
//! GET /agents/{id}/runtime-status
//! 查询 Agent 运行时状态 + 思考运行时快照

use axum::extract::Path;
use axum::response::Json;
use common::api::{ApiResponse, RuntimeStatusResponse, ThinkRuntimeInfo};

use crate::pkg::request_context::RequestContext;
use crate::service::domain::runtime::RuntimeDomain;

pub async fn runtime_status_handler(
    ctx: RequestContext,
    Path(agent_id): Path<String>,
) -> Json<ApiResponse<RuntimeStatusResponse>> {
    let runtime = crate::service::domain::runtime::domain();
    let (state, current_message_id, task_id, project_id, state_started_at, think_runtime) =
        runtime.get_runtime_status(&agent_id);

    let state_str = match state {
        common::enums::AgentRuntimeState::Idle => "idle",
        common::enums::AgentRuntimeState::Busy => "busy",
        common::enums::AgentRuntimeState::Resting => "resting",
    };

    let think_runtime_info = think_runtime.map(|snap| ThinkRuntimeInfo {
        trace_id: snap.trace_id,
        scene: snap.scene.as_str().to_string(),
        round: snap.round,
        max_rounds: snap.max_rounds,
        tokens_input: snap.tokens_input,
        tokens_output: snap.tokens_output,
        total_tokens: snap.total_tokens,
        tool_call_count: snap.tool_call_count,
        status: match snap.status {
            crate::pkg::agent_runtime_state::ThinkStatus::Thinking => "thinking",
            crate::pkg::agent_runtime_state::ThinkStatus::Cancelled => "cancelled",
            crate::pkg::agent_runtime_state::ThinkStatus::Finished => "finished",
        }
        .to_string(),
        started_at: snap.started_at,
        last_updated_at: snap.last_updated_at,
    });

    Json(ApiResponse::success(RuntimeStatusResponse {
        agent_id,
        state: state_str.to_string(),
        current_message_id,
        task_id,
        project_id,
        state_started_at,
        think_runtime: think_runtime_info,
    }))
}
```

- [ ] **Step 2: 创建 cancel_thinking.rs handler**

```rust
//! POST /agents/{id}/cancel-thinking
//! 取消 Agent 正在进行的思考

use axum::extract::Path;
use axum::response::Json;
use common::api::{ApiResponse, CancelThinkingResponse};

use crate::pkg::request_context::RequestContext;
use crate::service::domain::runtime::RuntimeDomain;

pub async fn cancel_thinking_handler(
    ctx: RequestContext,
    Path(agent_id): Path<String>,
) -> Json<ApiResponse<CancelThinkingResponse>> {
    let runtime = crate::service::domain::runtime::domain();
    let success = runtime.cancel_thinking(&agent_id);

    let message = if success {
        "已发送取消信号，Agent 将在当前轮次完成后退出思考".to_string()
    } else {
        "Agent 当前未在思考，无需取消".to_string()
    };

    log_info!(
        &ctx,
        "cancel_thinking",
        "agent_id={}, success={}",
        agent_id,
        success
    );

    Json(ApiResponse::success(CancelThinkingResponse { success, message }))
}
```

- [ ] **Step 3: 创建 runtime_list.rs handler**

```rust
//! GET /agents/runtime-list
//! 查询运行中 Agent 列表（带过滤参数）

use axum::extract::Query;
use axum::response::Json;
use common::api::{
    ApiResponse, RuntimeListRequest, RuntimeListResponse, RuntimeStatusResponse, ThinkRuntimeInfo,
};

use crate::pkg::request_context::RequestContext;
use crate::service::domain::runtime::RuntimeDomain;

pub async fn runtime_list_handler(
    ctx: RequestContext,
    Query(req): Query<RuntimeListRequest>,
) -> Json<ApiResponse<RuntimeListResponse>> {
    let runtime = crate::service::domain::runtime::domain();
    let agents = runtime.list_runtime_agents(
        req.state.as_deref(),
        req.task_id.as_deref(),
        req.project_id.as_deref(),
    );

    let items: Vec<RuntimeStatusResponse> = agents
        .into_iter()
        .map(|(agent_id, info)| {
            let state_str = match info.state {
                common::enums::AgentRuntimeState::Idle => "idle",
                common::enums::AgentRuntimeState::Busy => "busy",
                common::enums::AgentRuntimeState::Resting => "resting",
            };
            let think_runtime = info.think_runtime.as_ref().map(|tr| {
                let snap = tr.snapshot();
                ThinkRuntimeInfo {
                    trace_id: snap.trace_id,
                    scene: snap.scene.as_str().to_string(),
                    round: snap.round,
                    max_rounds: snap.max_rounds,
                    tokens_input: snap.tokens_input,
                    tokens_output: snap.tokens_output,
                    total_tokens: snap.total_tokens,
                    tool_call_count: snap.tool_call_count,
                    status: match snap.status {
                        crate::pkg::agent_runtime_state::ThinkStatus::Thinking => "thinking",
                        crate::pkg::agent_runtime_state::ThinkStatus::Cancelled => "cancelled",
                        crate::pkg::agent_runtime_state::ThinkStatus::Finished => "finished",
                    }
                    .to_string(),
                    started_at: snap.started_at,
                    last_updated_at: snap.last_updated_at,
                }
            });
            RuntimeStatusResponse {
                agent_id,
                state: state_str.to_string(),
                current_message_id: info.current_message_id,
                task_id: info.task_id,
                project_id: info.project_id,
                state_started_at: info.state_started_at,
                think_runtime,
            }
        })
        .collect();

    let total = items.len();
    log_info!(&ctx, "runtime_list", "returned {} agents", total);

    Json(ApiResponse::success(RuntimeListResponse { items, total }))
}
```

- [ ] **Step 4: 在 hr/agent/mod.rs 注册 3 个新模块**

在 `pub mod update_memory;` 之后添加：

```rust
pub mod runtime_status;
pub mod cancel_thinking;
pub mod runtime_list;
```

在 re-exports 区域 `pub use update_memory::update_memory_handler;` 之后添加：

```rust
pub use cancel_thinking::cancel_thinking_handler;
pub use runtime_list::runtime_list_handler;
pub use runtime_status::runtime_status_handler;
```

- [ ] **Step 5: 在 router.rs 注册 3 条路由**

在 `hr_routes()` 中，`.route("/agents/{id}", get(handlers::hr::agent::get_agent_handler))` 之前添加：

```rust
        .route(
            "/agents/runtime-list",
            get(handlers::hr::agent::runtime_list_handler),
        )
```

在 `/agents/{id}/status` 路由之后添加：

```rust
        .route(
            "/agents/{id}/runtime-status",
            get(handlers::hr::agent::runtime_status_handler),
        )
        .route(
            "/agents/{id}/cancel-thinking",
            post(handlers::hr::agent::cancel_thinking_handler),
        )
```

- [ ] **Step 6: 验证编译 + clippy**

Run: `cargo clippy --all-targets --message-format short`
Expected: 零警告通过

- [ ] **Step 7: Commit**

```bash
git add src/handlers/hr/agent/runtime_status.rs src/handlers/hr/agent/cancel_thinking.rs src/handlers/hr/agent/runtime_list.rs src/handlers/hr/agent/mod.rs src/router.rs
git commit -m "feat: 新增 runtime-status/cancel-thinking/runtime-list 3 个 HTTP 接口"
```

---

## Task 6: 单元测试

**Files:**
- Modify: `src/pkg/agent_runtime_state.rs`（在现有测试模块中追加）
- Modify: `src/service/domain/runtime/awakening.rs`（如有需要）

- [ ] **Step 1: 在 agent_runtime_state.rs 追加 list_runtime_agents 测试**

在测试模块末尾 `}` 之前添加：

```rust
    #[test]
    fn test_list_runtime_agents_no_filter() {
        let mgr = AgentRuntimeStateManager::new();
        mgr.set_busy("agent-1", "msg-1", Some("task-1"), Some("proj-1"));
        mgr.set_busy("agent-2", "msg-2", Some("task-2"), None);
        mgr.set_idle("agent-3");

        let list = mgr.list_runtime_agents(None, None, None);
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn test_list_runtime_agents_filter_by_state() {
        let mgr = AgentRuntimeStateManager::new();
        mgr.set_busy("agent-1", "msg-1", None, None);
        mgr.set_idle("agent-2");

        let busy = mgr.list_runtime_agents(Some("busy"), None, None);
        assert_eq!(busy.len(), 1);
        assert_eq!(busy[0].0, "agent-1");

        let idle = mgr.list_runtime_agents(Some("idle"), None, None);
        assert_eq!(idle.len(), 1);
        assert_eq!(idle[0].0, "agent-2");
    }

    #[test]
    fn test_list_runtime_agents_filter_by_task_id() {
        let mgr = AgentRuntimeStateManager::new();
        mgr.set_busy("agent-1", "msg-1", Some("task-1"), None);
        mgr.set_busy("agent-2", "msg-2", Some("task-2"), None);

        let filtered = mgr.list_runtime_agents(None, Some("task-1"), None);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].0, "agent-1");
    }

    #[test]
    fn test_list_runtime_agents_filter_by_project_id() {
        let mgr = AgentRuntimeStateManager::new();
        mgr.set_busy("agent-1", "msg-1", None, Some("proj-1"));
        mgr.set_busy("agent-2", "msg-2", None, Some("proj-2"));

        let filtered = mgr.list_runtime_agents(None, None, Some("proj-1"));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].0, "agent-1");
    }

    #[test]
    fn test_list_runtime_agents_combined_filter() {
        let mgr = AgentRuntimeStateManager::new();
        mgr.set_busy("agent-1", "msg-1", Some("task-1"), Some("proj-1"));
        mgr.set_busy("agent-2", "msg-2", Some("task-1"), Some("proj-2"));
        mgr.set_busy("agent-3", "msg-3", Some("task-2"), Some("proj-1"));

        // state=busy + task-1 + proj-1 → 只有 agent-1
        let filtered = mgr.list_runtime_agents(Some("busy"), Some("task-1"), Some("proj-1"));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].0, "agent-1");
    }
```

- [ ] **Step 2: 验证 clippy**

Run: `cargo clippy --all-targets --message-format short`
Expected: 零警告通过

- [ ] **Step 3: Commit**

```bash
git add src/pkg/agent_runtime_state.rs
git commit -m "test: 新增 list_runtime_agents 过滤测试（state/task_id/project_id 组合）"
```

---

## Verification

- [ ] `cargo check --all-targets` 通过
- [ ] `cargo clippy --all-targets -- -D warnings` 零警告
- [ ] 3 个接口路由注册正确：GET /agents/{id}/runtime-status、POST /agents/{id}/cancel-thinking、GET /agents/runtime-list
- [ ] AgentAwakeEvent.exit_reason 在 success/cancelled/error 三条路径均正确记录
- [ ] Cancelled 分支发布 AgentLoopEvent（status="cancelled"）
- [ ] runtime-list 支持 state/task_id/project_id 三个过滤参数
