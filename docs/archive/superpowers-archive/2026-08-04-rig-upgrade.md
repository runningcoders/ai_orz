# Rig 0.34 → 0.41 Upgrade Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade the `rig` dependency from 0.34.0 to 0.41.0, adapting all code to the new API (DynamicTool, AgentHook, crate split).

**Architecture:** rig 0.41 splits into `rig-core` (portable contracts) and `rig-agent` (classic runtime) behind a `rig` facade crate. The main breaking changes are: (1) `ToolDyn` trait is replaced by `DynamicTool` struct (closure-backed), (2) `PromptHook<M>` trait is replaced by `AgentHook` trait (non-generic, event-struct based), (3) `ToolError` enum is replaced by `ToolExecutionError` struct + `ToolErrorKind` enum, (4) Agent builder uses `.add_hook()` / `.dynamic_tools()` instead of `.hook()` / `.tools()`.

**Tech Stack:** Rust, rig 0.41 (rig-core + rig-agent facade), async-trait, serde_json

---

## API Migration Reference

This section is the authoritative reference for all tasks. Consult it whenever a mapping is needed.

### 1. Crate dependency

| Old (0.34) | New (0.41) |
|---|---|
| `rig-core = { version = "=0.34.0", features = ["all"] }` | `rig = { version = "0.41", features = ["all"] }` |

The `rig` facade re-exports everything. `rig::tool::DynamicTool`, `rig::agent::AgentHook`, `rig::tool::ToolExecutionError`, etc. are all accessible via the facade. The `agent` feature is on by default.

### 2. ToolDyn → DynamicTool

| Old (0.34) | New (0.41) |
|---|---|
| `Box<dyn ToolDyn>` (trait object) | `DynamicTool` (owned struct) |
| `impl ToolDyn for MyAdapter { fn name(&self) -> String; fn definition(&self, ...) -> ...; fn call(&self, args: String) -> ...; }` | `DynamicTool::new(name, description, parameters: Value, callback: F) -> DynamicTool` where `F: for<'a> Fn(&'a mut ToolContext, Value) -> Pin<Box<dyn Future<Output = Result<ToolOutput, ToolExecutionError>> + Send + 'a>> + WasmCompatSend + WasmCompatSync + 'static` |
| `Vec<Box<dyn ToolDyn>>` | `Vec<DynamicTool>` |

`DynamicTool` implements `Clone`. The callback receives `&mut ToolContext` and `Value` (parsed JSON), returns `Result<ToolOutput, ToolExecutionError>`.

### 3. ToolError → ToolExecutionError + ToolErrorKind

| Old (0.34) | New (0.41) |
|---|---|
| `rig::tool::ToolError::JsonError(e)` | `ToolExecutionError::new(ToolErrorKind::DeserializationError, e.to_string())` |
| `rig::tool::ToolError::ToolCallError(msg)` | `ToolExecutionError::new(ToolErrorKind::ToolCallError, msg)` |
| `rig::tool::ToolError` (as error type) | `rig::tool::ToolExecutionError` |

### 4. PromptHook<M> → AgentHook

| Old (0.34) | New (0.41) |
|---|---|
| `impl<M> PromptHook<M> for Hook where M: CompletionModel` | `impl AgentHook for Hook` (non-generic) |
| `use rig::agent::{HookAction, PromptHook, ToolCallHookAction};` | `use rig::agent::AgentHook;` + event/action types |
| `use rig::completion::{CompletionModel, CompletionResponse, Message};` | `use rig::completion::Message;` (CompletionModel no longer needed in hook bound) |

#### Method signature mapping:

**on_completion_call:**
```rust
// OLD
fn on_completion_call(&self, _prompt: &Message, history: &[Message]) -> impl Future<Output = HookAction> + WasmCompatSend
// NEW
fn on_completion_call(&self, _ctx: &HookContext, event: CompletionCallEvent<'_>) -> impl Future<Output = CompletionCallAction> + WasmCompatSend
// Access: event.prompt, event.history, event.turn
// Return: CompletionCallAction::continue_run()
```

**on_completion_response:**
```rust
// OLD
fn on_completion_response(&self, _prompt: &Message, response: &CompletionResponse<M::Response>) -> impl Future<Output = HookAction> + WasmCompatSend
// Access: response.usage (Usage struct with input_tokens, output_tokens, total_tokens)
// NEW
fn on_completion_response(&self, _ctx: &HookContext, event: CompletionResponseEvent<'_>) -> impl Future<Output = ObservationAction> + WasmCompatSend
// Access: event.usage (Usage struct with input_tokens, output_tokens, total_tokens)
// Return: ObservationAction::continue_run()
```

**on_tool_call:**
```rust
// OLD
fn on_tool_call(&self, tool_name: &str, tool_call_id: Option<String>, internal_call_id: &str, args: &str) -> impl Future<Output = ToolCallHookAction> + WasmCompatSend
// NEW
fn on_tool_call(&self, _ctx: &HookContext, event: ToolCall<'_>) -> impl Future<Output = ToolCallAction> + WasmCompatSend
// Access: event.tool_name, event.tool_call_id (Option<&str>), event.internal_call_id, event.args
// Return: ToolCallAction::run()
```

**on_tool_result:**
```rust
// OLD
fn on_tool_result(&self, tool_name: &str, tool_call_id: Option<String>, internal_call_id: &str, args: &str, result: &str) -> impl Future<Output = HookAction> + WasmCompatSend
// NEW
fn on_tool_result(&self, _ctx: &HookContext, event: ToolResultEvent<'_>) -> impl Future<Output = ToolResultAction> + WasmCompatSend
// Access: event.tool_name, event.tool_call_id, event.internal_call_id, event.args, event.presentation
// Return: ToolResultAction::keep()
```

### 5. Agent construction

| Old (0.34) | New (0.41) |
|---|---|
| `client.agent(model).hook(hook).build()` | `client.agent(model).add_hook(hook).build()` |
| `client.agent(model).hook(hook).tools(rig_tools).build()` | `client.agent(model).add_hook(hook).dynamic_tools(rig_tools).build()` |
| `Agent<ResponsesCompletionModel, RuntimeMonitoringHook>` | `Agent<ResponsesCompletionModel>` (no hook generic param) |

---

## File Structure

Files to modify, grouped by responsibility:

**Dependency config:**
- `Cargo.toml` (workspace) — upgrade rig version
- `common/Cargo.toml` — upgrade rig-core version

**Error conversion:**
- `common/src/error/types.rs` — ToolError → ToolExecutionError

**Tool adapter (core):**
- `src/models/tool.rs` — RigToolAdapter: ToolDyn impl → DynamicTool factory

**Monitoring hook:**
- `src/pkg/monitoring/rig_hook.rs` — PromptHook<M> → AgentHook

**Cortex layer (agent factory + providers):**
- `src/service/dao/cortex/mod.rs` — CortexDao trait signature
- `src/service/dao/cortex/rig.rs` — RigCortexDao implementation
- `src/service/dao/cortex/rig/openai.rs` — OpenAI provider
- `src/service/dao/cortex/rig/openai_compatible.rs` — OpenAI-compatible provider
- `src/service/dao/cortex/rig/ollama.rs` — Ollama provider
- `src/service/dao/cortex/rig_test.rs` — cortex tests

**ToolCall DAO layer:**
- `src/service/dao/tool_call/mod.rs` — ToolCallDao trait signature
- `src/service/dao/tool_call/impl.rs` — wrap_for_rig implementation
- `src/service/dao/tool_call/mcp.rs` — McpToolCallDao trait signature

**Handler adapter:**
- `src/pkg/tool_registry/handler_adapter/mod.rs` — ToolError usage

---

### Task 1: Update Cargo.toml dependencies

**Files:**
- Modify: `Cargo.toml` (workspace root, line 29)
- Modify: `common/Cargo.toml` (line 15)

- [x]**Step 1: Update workspace Cargo.toml**

In `Cargo.toml`, change line 29 from:

```toml
rig-core = { version = "=0.34.0", features = ["all"] }
```

to:

```toml
rig = { version = "0.41", features = ["all"] }
```

- [x]**Step 2: Update common/Cargo.toml**

In `common/Cargo.toml`, change line 15 from:

```toml
rig-core = { version = "0.34", optional = true }
```

to:

```toml
rig = { version = "0.41", optional = true, default-features = false }
```

Also update the feature name on line 29 from:

```toml
rig-integration = ["dep:rig-core"]
```

to:

```toml
rig-integration = ["dep:rig"]
```

- [x]**Step 3: Verify cargo resolves dependencies**

Run: `cargo update -p rig 2>&1 | head -20; cargo update -p rig-core 2>&1 | head -20`
Expected: cargo resolves the new rig version. May show "no matching package" errors for code compilation — that's expected, we'll fix code in subsequent tasks.

---

### Task 2: Update common error conversion (ToolError → ToolExecutionError)

**Files:**
- Modify: `common/src/error/types.rs:362-372`

- [x]**Step 1: Replace ToolError conversion**

In `common/src/error/types.rs`, replace the `From<rig::tool::ToolError>` impl (lines 362-372) with a `From<rig::tool::ToolExecutionError>` impl:

Replace:
```rust
/// Convert rig::tool::ToolError to our Error
#[cfg(feature = "rig-integration")]
impl From<rig::tool::ToolError> for Error {
    fn from(err: rig::tool::ToolError) -> Self {
        Error::new(
            crate::error::ErrorCode::ToolExecutionFailed,
            err.to_string(),
        )
        .with_source(err)
    }
}
```

With:
```rust
/// Convert rig::tool::ToolExecutionError to our Error
#[cfg(feature = "rig-integration")]
impl From<rig::tool::ToolExecutionError> for Error {
    fn from(err: rig::tool::ToolExecutionError) -> Self {
        Error::new(
            crate::error::ErrorCode::ToolExecutionFailed,
            err.to_string(),
        )
        .with_source(err)
    }
}
```

- [x]**Step 2: Verify common crate compiles**

Run: `cargo check -p common --features rig-integration 2>&1 | head -30`
Expected: `common` crate compiles successfully (no rig::tool::ToolError references remain).

---

### Task 3: Refactor RigToolAdapter (ToolDyn → DynamicTool factory)

**Files:**
- Modify: `src/models/tool.rs:1-135`

This is the core change. `RigToolAdapter` currently implements the `ToolDyn` trait. In rig 0.41, `ToolDyn` no longer exists — it's replaced by the `DynamicTool` struct which is created from a closure. So `RigToolAdapter` becomes a factory that produces `DynamicTool` instances.

- [x]**Step 1: Update imports in tool.rs**

In `src/models/tool.rs`, replace line 9:

```rust
use rig::tool::{ToolDyn, ToolError};
```

with:

```rust
use rig::tool::{DynamicTool, ToolContext, ToolExecutionError, ToolErrorKind, ToolOutput};
```

- [x]**Step 2: Rewrite RigToolAdapter as a DynamicTool factory**

Replace the entire `RigToolAdapter` struct and its `ToolDyn` impl (lines 58-135) with:

```rust
/// Rig 适配层 - 将我们的 CoreTool trait 转换为 Rig 的 DynamicTool
///
/// 用于 auto 模式，让 Rig 可以直接调用我们的工具
/// Rig 调用接口不传递 RequestContext，所以需要创建时持有
pub struct RigToolAdapter {
    ctx: RequestContext,
    inner: Box<dyn CoreTool>,
}

impl RigToolAdapter {
    pub fn new(ctx: RequestContext, inner: Box<dyn CoreTool>) -> Self {
        Self { ctx, inner }
    }

    /// Consume the adapter and produce a `DynamicTool` for rig 0.41+.
    ///
    /// The closure captures `ctx` and `inner` by move, so each `DynamicTool`
    /// owns its own copy of the RequestContext and CoreTool.
    pub fn into_dynamic_tool(self) -> DynamicTool {
        let name = self.inner.po().name.clone();
        let description = self.inner.po().description.clone();
        let parameters = self
            .inner
            .po()
            .parameters_schema
            .clone()
            .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}}));
        let ctx = self.ctx;
        let inner = self.inner;

        DynamicTool::new(
            name,
            description,
            parameters,
            move |_tool_ctx: &mut ToolContext, args: serde_json::Value| {
                let ctx = ctx.clone();
                let inner = inner.clone();
                Box::pin(async move {
                    let result = inner.call(ctx, args).await;
                    match result {
                        Ok(v) => Ok(ToolOutput::json(v)),
                        Err(e) => Err(ToolExecutionError::new(
                            ToolErrorKind::ToolCallError,
                            e.to_string(),
                        )),
                    }
                })
            },
        )
    }
}
```

Key changes:
- No more `impl ToolDyn for RigToolAdapter` — instead `into_dynamic_tool()` consumes self and returns a `DynamicTool`.
- The closure captures `ctx` (cloned per call) and `inner` (cloned via `dyn_clone` per call).
- `ToolOutput::json(v)` constructs the output from a `serde_json::Value` (the `IntoToolOutput` blanket impl would also work via `ToolOutput::text(serde_json::to_string(&v)?)`, but `json()` preserves structure).
- Error path uses `ToolExecutionError::new(ToolErrorKind::ToolCallError, message)`.

- [x]**Step 3: Verify tool.rs has no remaining ToolDyn references**

Run: `grep -n "ToolDyn" src/models/tool.rs`
Expected: no matches.

---

### Task 4: Migrate RuntimeMonitoringHook (PromptHook → AgentHook)

**Files:**
- Modify: `src/pkg/monitoring/rig_hook.rs` (entire file)

- [x]**Step 1: Rewrite rig_hook.rs with new imports and AgentHook impl**

Replace the entire content of `src/pkg/monitoring/rig_hook.rs` with:

```rust
//! Runtime Monitoring - 运行时监控框架
//!
//! 负责对 LLM 推理运行时进行监控，支持：
//! - Token 使用统计（自动写入 Stats）
//! - 调用日志记录
//! - 耗时监控
//! - 工具调用审计

use crate::pkg::request_context::RequestContext;
use crate::pkg::stats::ModelCallEvent;
use rig::agent::{
    AgentHook, CompletionCallAction, CompletionCallEvent, CompletionResponseEvent,
    HookContext, ObservationAction, ToolCall, ToolCallAction, ToolResultAction, ToolResultEvent,
};
use tracing::{debug, info, warn};

/// Runtime Monitoring Hook
/// 接入 rig 0.41 的 AgentHook 机制，实现运行时监控
///
/// 在 `on_completion_response` 中自动将 token 用量写入 Stats，
/// tags 从 `RequestContext` 提取（agent_id / task_id / project_id /
/// model_provider_id / model_name / organization_id / user_id），
/// metrics 包含 tokens_input / tokens_output / total_tokens。
#[derive(Clone)]
pub struct RuntimeMonitoringHook {
    ctx: RequestContext,
}

impl RuntimeMonitoringHook {
    pub fn new(ctx: RequestContext) -> Self {
        Self { ctx }
    }
}

impl AgentHook for RuntimeMonitoringHook {
    /// Called before the prompt is sent to the model
    fn on_completion_call(
        &self,
        _ctx: &HookContext,
        event: CompletionCallEvent<'_>,
    ) -> impl futures_util::Future<Output = CompletionCallAction> + rig::wasm_compat::WasmCompatSend
    {
        debug!(
            log_id = self.ctx.log_id,
            user_id = ?self.ctx.user_id,
            organization_id = ?self.ctx.organization_id,
            history_len = event.history.len(),
            "Starting completion call"
        );
        async { CompletionCallAction::continue_run() }
    }

    /// Called after the prompt is sent to the model and a response is received.
    ///
    /// 自动记录 token 用量到 Stats，便于后续按 agent / project / task /
    /// model_provider 等维度聚合统计。
    fn on_completion_response(
        &self,
        _ctx: &HookContext,
        event: CompletionResponseEvent<'_>,
    ) -> impl futures_util::Future<Output = ObservationAction> + rig::wasm_compat::WasmCompatSend
    {
        let usage = event.usage;
        info!(
            log_id = self.ctx.log_id,
            user_id = ?self.ctx.user_id,
            organization_id = ?self.ctx.organization_id,
            input_tokens = usage.input_tokens,
            output_tokens = usage.output_tokens,
            total_tokens = usage.total_tokens,
            "Completion response received - Token usage recorded"
        );

        let ctx = self.ctx.clone();
        let agent_id = self.ctx.agent_id().cloned();
        let project_id = self.ctx.project_id().cloned();
        let task_id = self.ctx.task_id().cloned();
        let model_provider_id = self.ctx.model_provider_id().cloned();
        let model_name = self.ctx.model_name().cloned();
        let organization_id = self.ctx.organization_id().cloned();
        let user_id = self.ctx.user_id().cloned();

        async move {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            let event = ModelCallEvent::new(timestamp)
                .with_agent_id(agent_id)
                .with_project_id(project_id)
                .with_task_id(task_id)
                .with_model_provider_id(model_provider_id)
                .with_model_name(model_name)
                .with_organization_id(organization_id)
                .with_user_id(user_id)
                .with_tokens_input(usage.input_tokens)
                .with_tokens_output(usage.output_tokens)
                .with_total_tokens(usage.total_tokens);

            if let Err(e) = ctx.stats().record(ctx.clone(), event).await {
                warn!(
                    log_id = ctx.log_id,
                    error = %e,
                    "Failed to record stats event for completion response"
                );
            }
            ObservationAction::continue_run()
        }
    }

    /// Called before a tool call is executed.
    fn on_tool_call(
        &self,
        _ctx: &HookContext,
        event: ToolCall<'_>,
    ) -> impl futures_util::Future<Output = ToolCallAction> + rig::wasm_compat::WasmCompatSend
    {
        debug!(
            log_id = self.ctx.log_id,
            user_id = ?self.ctx.user_id,
            tool_name = event.tool_name,
            tool_call_id = ?event.tool_call_id,
            internal_call_id = %event.internal_call_id,
            args_length = event.args.len(),
            "Tool call starting"
        );
        async { ToolCallAction::run() }
    }

    /// Called after a tool call has been executed.
    ///
    /// 工具调用统计已在 ToolCallLoggingDecorator 中统一记录，
    /// 此处仅保留日志记录用于调试。
    fn on_tool_result(
        &self,
        _ctx: &HookContext,
        event: ToolResultEvent<'_>,
    ) -> impl futures_util::Future<Output = ToolResultAction> + rig::wasm_compat::WasmCompatSend
    {
        debug!(
            log_id = self.ctx.log_id,
            tool_name = event.tool_name,
            tool_call_id = ?event.tool_call_id,
            internal_call_id = %event.internal_call_id,
            args_length = event.args.len(),
            "Tool call completed"
        );

        async { ToolResultAction::keep() }
    }
}
```

Key changes:
- `impl<M> PromptHook<M>` → `impl AgentHook` (non-generic)
- Old `HookAction::cont()` → new `CompletionCallAction::continue_run()` / `ObservationAction::continue_run()`
- Old `ToolCallHookAction::cont()` → new `ToolCallAction::run()`
- Old `HookAction::cont()` (for tool_result) → new `ToolResultAction::keep()`
- Method params changed from individual fields to event structs: `ToolCall<'_>`, `ToolResultEvent<'_>`, `CompletionCallEvent<'_>`, `CompletionResponseEvent<'_>`
- `Box::pin(async { ... })` → `async { ... }` (the trait uses `impl Future`, not `Pin<Box<...>>`)
- Removed `CompletionModel` and `CompletionResponse` from imports (no longer needed)
- `event.usage` still has `input_tokens`, `output_tokens`, `total_tokens` fields

- [x]**Step 2: Verify no PromptHook references remain**

Run: `grep -rn "PromptHook\|HookAction\|ToolCallHookAction" src/pkg/monitoring/`
Expected: no matches.

---

### Task 5: Update CortexDao trait + RigCortexDao (Vec<DynamicTool>)

**Files:**
- Modify: `src/service/dao/cortex/mod.rs:9,27`
- Modify: `src/service/dao/cortex/rig.rs:8,44`

- [x]**Step 1: Update CortexDao trait signature in mod.rs**

In `src/service/dao/cortex/mod.rs`:

Change line 9 from:
```rust
use ::rig::tool::ToolDyn;
```
to:
```rust
use rig::tool::DynamicTool;
```

Change line 27 (inside `create_cortex_trait` signature) from:
```rust
        rig_tools: Vec<Box<dyn ToolDyn>>,
```
to:
```rust
        rig_tools: Vec<DynamicTool>,
```

- [x]**Step 2: Update RigCortexDao implementation in rig.rs**

In `src/service/dao/cortex/rig.rs`:

Change line 8 from:
```rust
use rig::tool::ToolDyn;
```
to:
```rust
use rig::tool::DynamicTool;
```

Change line 44 (inside `create_cortex_trait` method signature) from:
```rust
        rig_tools: Vec<Box<dyn ToolDyn>>,
```
to:
```rust
        rig_tools: Vec<DynamicTool>,
```

- [x]**Step 3: Verify no ToolDyn references in cortex layer**

Run: `grep -rn "ToolDyn" src/service/dao/cortex/`
Expected: no matches.

---

### Task 6: Update provider implementations (openai, openai_compatible, ollama)

**Files:**
- Modify: `src/service/dao/cortex/rig/openai.rs`
- Modify: `src/service/dao/cortex/rig/openai_compatible.rs`
- Modify: `src/service/dao/cortex/rig/ollama.rs`

All three providers have identical patterns. The changes for each are:
1. `use rig::tool::ToolDyn;` → `use rig::tool::DynamicTool;`
2. `rig_tools: Vec<Box<dyn ToolDyn>>` → `rig_tools: Vec<DynamicTool>`
3. `Agent<ResponsesCompletionModel, RuntimeMonitoringHook>` → `Agent<ResponsesCompletionModel>`
4. `.hook(hook)` → `.add_hook(hook)`
5. `.tools(rig_tools)` → `.dynamic_tools(rig_tools)`

- [x]**Step 1: Update openai.rs**

In `src/service/dao/cortex/rig/openai.rs`:

Change imports (lines 9-15) — replace:
```rust
use rig::agent::Agent;
use rig::completion::Prompt;
use rig::embeddings::EmbeddingModel;
use rig::prelude::*;
use rig::providers::openai;
use rig::providers::openai::responses_api::ResponsesCompletionModel;
use rig::tool::ToolDyn;
```
with:
```rust
use rig::agent::Agent;
use rig::completion::Prompt;
use rig::embeddings::EmbeddingModel;
use rig::prelude::*;
use rig::providers::openai;
use rig::providers::openai::responses_api::ResponsesCompletionModel;
use rig::tool::DynamicTool;
```

Change the struct field type (line 24) from:
```rust
    agent: Agent<ResponsesCompletionModel, RuntimeMonitoringHook>,
```
to:
```rust
    agent: Agent<ResponsesCompletionModel>,
```

Change the `new` method signature (line 34) from:
```rust
        rig_tools: Vec<Box<dyn ToolDyn>>,
```
to:
```rust
        rig_tools: Vec<DynamicTool>,
```

Change the agent construction (lines 50-58) from:
```rust
        let hook = RuntimeMonitoringHook::new(ctx.clone());
        let agent = if rig_tools.is_empty() {
            client.agent(model.clone()).hook(hook).build()
        } else {
            client
                .agent(model.clone())
                .hook(hook)
                .tools(rig_tools)
                .build()
        };
```
to:
```rust
        let hook = RuntimeMonitoringHook::new(ctx.clone());
        let agent = if rig_tools.is_empty() {
            client.agent(model.clone()).add_hook(hook).build()
        } else {
            client
                .agent(model.clone())
                .add_hook(hook)
                .dynamic_tools(rig_tools)
                .build()
        };
```

- [x]**Step 2: Update openai_compatible.rs**

Apply the exact same set of changes to `src/service/dao/cortex/rig/openai_compatible.rs`:
- Replace `use rig::tool::ToolDyn;` with `use rig::tool::DynamicTool;`
- Replace `Agent<ResponsesCompletionModel, RuntimeMonitoringHook>` with `Agent<ResponsesCompletionModel>`
- Replace `rig_tools: Vec<Box<dyn ToolDyn>>` with `rig_tools: Vec<DynamicTool>`
- Replace `.hook(hook)` with `.add_hook(hook)`
- Replace `.tools(rig_tools)` with `.dynamic_tools(rig_tools)`

- [x]**Step 3: Update ollama.rs**

Apply the exact same set of changes to `src/service/dao/cortex/rig/ollama.rs`:
- Replace `use rig::tool::ToolDyn;` with `use rig::tool::DynamicTool;`
- Replace `Agent<ResponsesCompletionModel, RuntimeMonitoringHook>` with `Agent<ResponsesCompletionModel>`
- Replace `rig_tools: Vec<Box<dyn ToolDyn>>` with `rig_tools: Vec<DynamicTool>`
- Replace `.hook(hook)` with `.add_hook(hook)`
- Replace `.tools(rig_tools)` with `.dynamic_tools(rig_tools)`

- [x]**Step 4: Verify no ToolDyn references in providers**

Run: `grep -rn "ToolDyn" src/service/dao/cortex/rig/openai.rs src/service/dao/cortex/rig/openai_compatible.rs src/service/dao/cortex/rig/ollama.rs`
Expected: no matches.

---

### Task 7: Update ToolCallDao trait + implementations

**Files:**
- Modify: `src/service/dao/tool_call/mod.rs:11,33-34`
- Modify: `src/service/dao/tool_call/impl.rs:3,11,72-96`
- Modify: `src/service/dao/tool_call/mcp.rs:16,88-89`

- [x]**Step 1: Update ToolCallDao trait in mod.rs**

In `src/service/dao/tool_call/mod.rs`:

Change line 11 from:
```rust
use anyhow::Result;
```
Keep this line, but add `DynamicTool` import. After line 11, the `use` block currently has no rig import (it uses `rig::tool::ToolDyn` inline in the trait). Update the trait method signature (lines 33-34) from:
```rust
    fn wrap_for_rig(&self, tools: &[Tool], ctx: RequestContext)
        -> Vec<Box<dyn rig::tool::ToolDyn>>;
```
to:
```rust
    fn wrap_for_rig(&self, tools: &[Tool], ctx: RequestContext) -> Vec<rig::tool::DynamicTool>;
```

- [x]**Step 2: Update ToolCallDaoImpl in impl.rs**

In `src/service/dao/tool_call/impl.rs`:

Change line 3 from:
```rust
use crate::models::tool::{CoreTool, RigToolAdapter, Tool, ToolCallTraceRef, ToolPo};
```
(keep this — no change needed for the import)

Change line 11 from:
```rust
use rig::tool::ToolDyn;
```
to:
```rust
use rig::tool::DynamicTool;
```

Change the `wrap_for_rig` method signature and body (lines 72-96) from:
```rust
    fn wrap_for_rig(&self, tools: &[Tool], ctx: RequestContext) -> Vec<Box<dyn ToolDyn>> {
        let mut rig_tools = Vec::new();

        for tool in tools {
            // Only include tools that are Auto mode (automatic invocation by Rig)
            if tool.po.control_mode != ControlMode::Auto {
                continue;
            }

            // Clone the core tool (we need our own copy for wrapping)
            let cloned: Box<dyn CoreTool + Send + Sync> = dyn_clone::clone_box(&*tool.our_tool);

            // Wrap with logging decorator to capture logs
            let decorated = ToolCallLoggingDecorator::new(cloned);
            let decorated_box: Box<dyn CoreTool + Send + Sync> = Box::new(decorated);

            // Adapt to Rig's ToolDyn interface
            let rig_adapter = RigToolAdapter::new(ctx.clone(), decorated_box);
            let rig_adapter_box: Box<dyn ToolDyn> = Box::new(rig_adapter);

            rig_tools.push(rig_adapter_box);
        }

        rig_tools
    }
```
to:
```rust
    fn wrap_for_rig(&self, tools: &[Tool], ctx: RequestContext) -> Vec<DynamicTool> {
        let mut rig_tools = Vec::new();

        for tool in tools {
            // Only include tools that are Auto mode (automatic invocation by Rig)
            if tool.po.control_mode != ControlMode::Auto {
                continue;
            }

            // Clone the core tool (we need our own copy for wrapping)
            let cloned: Box<dyn CoreTool + Send + Sync> = dyn_clone::clone_box(&*tool.our_tool);

            // Wrap with logging decorator to capture logs
            let decorated = ToolCallLoggingDecorator::new(cloned);
            let decorated_box: Box<dyn CoreTool> = Box::new(decorated);

            // Adapt to Rig's DynamicTool interface (rig 0.41+)
            let rig_adapter = RigToolAdapter::new(ctx.clone(), decorated_box);
            let dynamic_tool = rig_adapter.into_dynamic_tool();

            rig_tools.push(dynamic_tool);
        }

        rig_tools
    }
```

Key changes:
- Return type `Vec<Box<dyn ToolDyn>>` → `Vec<DynamicTool>`
- `RigToolAdapter::new(ctx, decorated_box)` returns `RigToolAdapter`, then call `.into_dynamic_tool()` to get `DynamicTool`
- No more `Box<dyn ToolDyn>` boxing — `DynamicTool` is an owned struct
- `decorated_box` type changed from `Box<dyn CoreTool + Send + Sync>` to `Box<dyn CoreTool>` (since `RigToolAdapter::new` takes `Box<dyn CoreTool>` which is `Box<dyn CoreTool + Send + Sync>` via the trait def — keep `Box<dyn CoreTool + Send + Sync>` if the compiler requires it; the trait already bounds `Send + Sync`)

Note: If the compiler complains about `Box<dyn CoreTool + Send + Sync>` vs `Box<dyn CoreTool>`, keep the original `Box<dyn CoreTool + Send + Sync>` type annotation — `RigToolAdapter::new` accepts `Box<dyn CoreTool>` which is the same as `Box<dyn CoreTool + Send + Sync>` since the trait already includes those bounds.

- [x]**Step 3: Update McpToolCallDao in mcp.rs**

In `src/service/dao/tool_call/mcp.rs`:

Change line 16 from:
```rust
use rig::tool::ToolDyn;
```
to:
```rust
use rig::tool::DynamicTool;
```

Change line 88-89 (the `wrap_for_rig` signature in the `impl ToolCallDao for McpToolCallDaoImpl` block) from:
```rust
    fn wrap_for_rig(&self, tools: &[Tool], ctx: RequestContext) -> Vec<Box<dyn ToolDyn>> {
        self.base.wrap_for_rig(tools, ctx)
    }
```
to:
```rust
    fn wrap_for_rig(&self, tools: &[Tool], ctx: RequestContext) -> Vec<DynamicTool> {
        self.base.wrap_for_rig(tools, ctx)
    }
```

- [x]**Step 4: Verify no ToolDyn references in tool_call**

Run: `grep -rn "ToolDyn" src/service/dao/tool_call/`
Expected: no matches.

---

### Task 8: Update handler_adapter (ToolError usage)

**Files:**
- Modify: `src/pkg/tool_registry/handler_adapter/mod.rs:21,181-188,258-260`

The `handler_adapter` uses `rig::tool::ToolError` for error conversion. In rig 0.41, `ToolError` is replaced by `ToolExecutionError`.

- [x]**Step 1: Update imports**

In `src/pkg/tool_registry/handler_adapter/mod.rs`, change line 21 from:
```rust
use rig::tool::ToolError;
```
to:
```rust
use rig::tool::{ToolErrorKind, ToolExecutionError};
```

- [x]**Step 2: Update error conversion in HandlerToolAdapter::call**

In the `impl CoreTool for HandlerToolAdapter<Params>` block, replace lines 181-188 from:
```rust
    async fn call(&self, ctx: RequestContext, args: Value) -> Result<Value> {
        // Parse JSON args to Params type
        let params: Params = match serde_json::from_value(args) {
            Ok(p) => p,
            Err(e) => {
                return Err(ToolError::JsonError(e).into());
            }
        };

        match self.inner.call(ctx, params).await {
            Ok(result) => Ok(result),
            Err(app_error) => Err(ToolError::ToolCallError(Box::new(app_error)).into()),
        }
    }
```
to:
```rust
    async fn call(&self, ctx: RequestContext, args: Value) -> Result<Value> {
        // Parse JSON args to Params type
        let params: Params = match serde_json::from_value(args) {
            Ok(p) => p,
            Err(e) => {
                return Err(ToolExecutionError::new(
                    ToolErrorKind::DeserializationError,
                    e.to_string(),
                )
                .into());
            }
        };

        match self.inner.call(ctx, params).await {
            Ok(result) => Ok(result),
            Err(app_error) => Err(ToolExecutionError::new(
                ToolErrorKind::ToolCallError,
                app_error.to_string(),
            )
            .into()),
        }
    }
```

- [x]**Step 3: Update app_error_to_tool_error helper**

Replace lines 258-260 from:
```rust
pub fn app_error_to_tool_error(e: common::error::Error) -> ToolError {
    ToolError::ToolCallError(e.to_string().into())
}
```
to:
```rust
pub fn app_error_to_tool_error(e: common::error::Error) -> ToolExecutionError {
    ToolExecutionError::new(ToolErrorKind::ToolCallError, e.to_string())
}
```

- [x]**Step 4: Verify no ToolError references (only ToolExecutionError)**

Run: `grep -n "ToolError" src/pkg/tool_registry/handler_adapter/mod.rs`
Expected: only `ToolErrorKind` and `ToolExecutionError` references, no bare `ToolError`.

---

### Task 9: Update cortex tests

**Files:**
- Modify: `src/service/dao/cortex/rig_test.rs`

- [x]**Step 1: Update test imports and type annotations**

In `src/service/dao/cortex/rig_test.rs`, change line 5 from:
```rust
use ::rig::tool::ToolDyn;
```
to:
```rust
use rig::tool::DynamicTool;
```

Then replace all occurrences of `Vec<Box<dyn ToolDyn>>` with `Vec<DynamicTool>`. There are 6 such occurrences (one per test function, lines 31, 60, 88, 116, 144, 172). Each looks like:

```rust
    let rig_tools: Vec<Box<dyn ToolDyn>> = vec![];
```

Replace each with:

```rust
    let rig_tools: Vec<DynamicTool> = vec![];
```

You can use a find-and-replace-all for this pattern since it's identical in all 6 test functions.

- [x]**Step 2: Verify no ToolDyn in tests**

Run: `grep -n "ToolDyn" src/service/dao/cortex/rig_test.rs`
Expected: no matches.

---

### Task 10: Full build + clippy + test verification

**Files:**
- None (verification only)

This is the critical verification task. All previous tasks must be complete before running this.

- [x]**Step 1: Full compilation check**

Run: `cargo check 2>&1 | tail -30`
Expected: zero errors. If there are errors, fix them before proceeding.

Common issues to watch for:
- `ToolError` still referenced somewhere — search and replace with `ToolExecutionError`/`ToolErrorKind`
- `ToolDyn` still referenced somewhere — search and replace with `DynamicTool`
- `PromptHook` still referenced somewhere — search and replace with `AgentHook`
- `.hook(` still used instead of `.add_hook(`
- `.tools(` still used instead of `.dynamic_tools(`
- `Agent<M, H>` type annotation still has hook generic — remove the hook generic param
- `Box::pin(async { ... })` in hook methods — rig 0.41 uses `impl Future` return, so `async { ... }` works directly. However, if the future captures borrowed data, `Box::pin` may still be needed. Check the compiler errors.

- [x]**Step 2: Search for any remaining old API references**

Run: `grep -rn "ToolDyn\|PromptHook\|HookAction\|ToolCallHookAction\|\.hook(\|\.tools(rig" src/ | grep -v "add_hook\|dynamic_tools"`
Expected: no matches. If matches found, fix them.

- [x]**Step 3: Run clippy**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -30`
Expected: zero warnings. Fix any warnings.

- [x]**Step 4: Run cortex tests**

Run: `cargo test -p ai_orz --lib service::dao::cortex::rig_test 2>&1 | tail -20`
Expected: all 6 tests pass (test_create_openai_cortex, test_create_deepseek_cortex, test_create_qwen_cortex, test_create_doubao_cortex, test_create_ollama_cortex, test_create_openai_compatible_custom_base_url).

- [x]**Step 5: Run format check**

Run: `cargo fmt --all -- --check 2>&1 | tail -10`
Expected: no formatting issues. If issues, run `cargo fmt --all` to fix.

- [x]**Step 6: Commit all changes**

Run:
```bash
git add -A
git commit -m "$(cat <<'EOF'
chore: upgrade rig from 0.34 to 0.41

Migrate to rig 0.41 API:
- ToolDyn trait → DynamicTool struct (closure-backed)
- PromptHook<M> trait → AgentHook trait (non-generic, event structs)
- ToolError enum → ToolExecutionError struct + ToolErrorKind enum
- Agent builder: .hook() → .add_hook(), .tools() → .dynamic_tools()
- Agent<M, H> → Agent<M> (no hook generic param)
- rig-core dep → rig facade crate
EOF
)"
```

---

## Self-Review Notes

**Spec coverage:** All files identified in the "File Structure" section have corresponding tasks. The `doubao_vision.rs` and `fastembed.rs` cortex providers don't use `ToolDyn` (they're embedding-only, no tools) and don't need changes — verified via grep showing no `ToolDyn` usage in those files.

**Placeholder scan:** No placeholders. All code blocks contain complete, concrete code.

**Type consistency:**
- `DynamicTool` used consistently across all tasks
- `ToolExecutionError::new(ToolErrorKind::Variant, message)` signature consistent
- `AgentHook` method signatures match the docs.rs 0.41 reference
- `into_dynamic_tool()` method name consistent between Task 3 (definition) and Task 7 (usage)

**Risk areas:**
1. The `ToolOutput::json(v)` method — if it doesn't exist or has a different signature, use `ToolOutput::text(serde_json::to_string(&v).unwrap_or_default())` as fallback. Check docs.rs/rig/0.41.0/rig/tool/struct.ToolOutput.html for the `json` method.
2. The `Usage` struct fields (`input_tokens`, `output_tokens`, `total_tokens`) — these are core rig types unlikely to change, but verify if compilation fails.
3. `WasmCompatSend` bound on hook futures — the plan uses `rig::wasm_compat::WasmCompatSend` which matches the trait definition. If the bound isn't satisfied, the compiler will guide you.
4. The `DynamicTool` callback signature uses `for<'a> Fn(...)` — the closure we provide must match this HRTB. The `move |_tool_ctx: &mut ToolContext, args: Value| { ... Box::pin(async move { ... }) }` pattern should satisfy this.
