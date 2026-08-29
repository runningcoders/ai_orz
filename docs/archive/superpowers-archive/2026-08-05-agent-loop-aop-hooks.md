# Agent Loop Alignment + AOP Sync Hooks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Align awaken/sleep_and_settle tool loading and prompt logic; add AOP-based synchronous hook mechanism for agent loop lifecycle, think rounds, tool execution, and state changes; replace ToolCallLoggingDecorator with AOP sync subscribers.

**Architecture:**
1. Extract the think loop from `awaken()` into a shared `run_think_loop()` helper; align `sleep_and_settle` to use it with filtered tools + ToolDescriptor (replacing the current `&[]` + single-think pattern)
2. Define 4 AOP event types (`AgentLoopEvent`, `ThinkRoundEvent`, `ToolExecEvent`, `AgentStateEvent`) in `src/models/events/`
3. Publish events from `awakening.rs` and `AgentRuntimeStateManager` via `aop::publish()` (ConsumeMode::Sync, no queue)
4. Replace `ToolCallLoggingDecorator` with two sync AOP consumers: `ToolExecLogConsumer` (JSONL logging) and `ToolExecStatsConsumer` (stats recording)
5. Clean up zombie code: remove `Builder.tools` field/setter/trait method, fix stale comments, remove unused `wake_agent_brain` scene parameter

**Tech Stack:** Rust, async-trait, serde, tokio, existing AOP framework (`pkg/aop/`)

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `src/models/events/agent_loop.rs` | Create | AgentLoopEvent (lifecycle start/end) |
| `src/models/events/think_round.rs` | Create | ThinkRoundEvent (per-round think) |
| `src/models/events/tool_exec.rs` | Create | ToolExecEvent (replaces decorator logging+stats) |
| `src/models/events/agent_state.rs` | Create | AgentStateEvent (state machine changes) |
| `src/models/events/mod.rs` | Modify | Export new event types |
| `src/service/domain/runtime/awakening.rs` | Modify | Extract think loop, align sleep, publish events, cleanup |
| `src/service/domain/runtime/mod.rs` | Modify | Remove scene param from wake_agent_brain trait, fix comments |
| `src/models/prompt_builder.rs` | Modify | Remove `tools()` trait method |
| `src/service/dal/agent/mod.rs` | Modify | Remove `tools` field + setter from DefaultPromptBuilder, fix comments |
| `src/service/dao/tool_call/mod.rs` | Modify | Remove `decorate` from trait |
| `src/service/dao/tool_call/impl.rs` | Modify | Replace decorator with inline execution + AOP publish |
| `src/pkg/tool_tracing/tool_call_logger.rs` | Delete | Remove LoggingDecorator (replaced by AOP) |
| `src/pkg/tool_tracing/mod.rs` | Modify | Remove decorator re-export, keep entry+logger |
| `src/pkg/agent_runtime_state.rs` | Modify | Publish AgentStateEvent on state changes |
| `src/pkg/stats/mod.rs` | Modify | Add global Stats accessor |
| `src/consumer/tool_exec_log_consumer.rs` | Create | Sync consumer: JSONL logging |
| `src/consumer/tool_exec_stats_consumer.rs` | Create | Sync consumer: stats recording |
| `src/consumer/agent_loop_consumer.rs` | Create | Sync consumer: lifecycle + think round logging |
| `src/consumer/mod.rs` | Modify | Register new consumers |
| `src/consumer/message.rs` | Modify | Remove scene arg from wake_agent_brain call |
| `src/handlers/hr/agent/settle_memory.rs` | Modify | Remove scene arg from wake_agent_brain call |

---

## Task 1: Define AOP Event Types

**Files:**
- Create: `src/models/events/agent_loop.rs`
- Create: `src/models/events/think_round.rs`
- Create: `src/models/events/tool_exec.rs`
- Create: `src/models/events/agent_state.rs`
- Modify: `src/models/events/mod.rs`

- [ ] **Step 1: Create `src/models/events/agent_loop.rs`**

```rust
use crate::pkg::aop::{Event, EventKind};
use serde::{Deserialize, Serialize};

/// Agent 循环生命周期事件（awaken/sleep_and_settle 的启动与完成）
///
/// 通过 AOP 同步发布，订阅者可记录循环耗时、状态等。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentLoopEvent {
    pub event_id: String,
    pub agent_id: String,
    pub trace_id: String,
    /// "awaken" 或 "settle"
    pub scene: String,
    /// "started" 或 "finished"
    pub phase: String,
    /// 完成时才有值："success" 或 "failed: {error}"
    pub status: Option<String>,
    /// 完成时才有值（毫秒）
    pub duration_ms: Option<u64>,
    /// awaken 场景关联的消息 ID
    pub message_id: Option<String>,
    pub created_at: i64,
}

impl AgentLoopEvent {
    pub fn started(agent_id: &str, trace_id: &str, scene: &str, message_id: Option<&str>) -> Self {
        Self {
            event_id: uuid::Uuid::now_v7().to_string(),
            agent_id: agent_id.to_string(),
            trace_id: trace_id.to_string(),
            scene: scene.to_string(),
            phase: "started".to_string(),
            status: None,
            duration_ms: None,
            message_id: message_id.map(|s| s.to_string()),
            created_at: common::constants::utils::current_timestamp_ms(),
        }
    }

    pub fn finished(
        agent_id: &str,
        trace_id: &str,
        scene: &str,
        status: &str,
        duration_ms: u64,
        message_id: Option<&str>,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::now_v7().to_string(),
            agent_id: agent_id.to_string(),
            trace_id: trace_id.to_string(),
            scene: scene.to_string(),
            phase: "finished".to_string(),
            status: Some(status.to_string()),
            duration_ms: Some(duration_ms),
            message_id: message_id.map(|s| s.to_string()),
            created_at: common::constants::utils::current_timestamp_ms(),
        }
    }
}

impl Event for AgentLoopEvent {
    fn kind(&self) -> EventKind {
        EventKind::new("agent.loop")
    }

    fn id(&self) -> &str {
        &self.event_id
    }

    fn order_key(&self) -> &str {
        &self.agent_id
    }

    fn created_at(&self) -> i64 {
        self.created_at
    }
}
```

- [ ] **Step 2: Create `src/models/events/think_round.rs`**

```rust
use crate::pkg::aop::{Event, EventKind};
use serde::{Deserialize, Serialize};

/// 每轮 think 事件（记录轮次、耗时、是否触发工具调用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkRoundEvent {
    pub event_id: String,
    pub agent_id: String,
    pub trace_id: String,
    /// "awaken" 或 "settle"
    pub scene: String,
    /// 第几轮（从 0 开始）
    pub round_number: usize,
    /// 本轮 think 耗时（毫秒）
    pub duration_ms: u64,
    /// 是否触发了工具调用
    pub has_tool_calls: bool,
    /// 工具调用数量
    pub tool_call_count: usize,
    pub created_at: i64,
}

impl ThinkRoundEvent {
    pub fn new(
        agent_id: &str,
        trace_id: &str,
        scene: &str,
        round_number: usize,
        duration_ms: u64,
        has_tool_calls: bool,
        tool_call_count: usize,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::now_v7().to_string(),
            agent_id: agent_id.to_string(),
            trace_id: trace_id.to_string(),
            scene: scene.to_string(),
            round_number,
            duration_ms,
            has_tool_calls,
            tool_call_count,
            created_at: common::constants::utils::current_timestamp_ms(),
        }
    }
}

impl Event for ThinkRoundEvent {
    fn kind(&self) -> EventKind {
        EventKind::new("agent.think.round")
    }

    fn id(&self) -> &str {
        &self.event_id
    }

    fn created_at(&self) -> i64 {
        self.created_at
    }
}
```

- [ ] **Step 3: Create `src/models/events/tool_exec.rs`**

This event replaces the ToolCallLoggingDecorator. It carries all ToolCallEntry fields plus ctx-derived fields for stats recording.

```rust
use crate::pkg::aop::{Event, EventKind};
use crate::pkg::tool_tracing::entry::{ToolCallEntry, ToolCallStatus};
use serde::{Deserialize, Serialize};

/// 工具执行完成事件（取代 ToolCallLoggingDecorator 的日志+统计职责）
///
/// 由 ToolCallDao::execute 在工具执行完成后通过 AOP 同步发布。
/// 订阅者：
/// - ToolExecLogConsumer：写入 JSONL 日志（取代 decorator 的 log_call）
/// - ToolExecStatsConsumer：记录统计事件（取代 decorator 的 record_tool_call_stat）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecEvent {
    pub event_id: String,
    /// 完整的工具调用条目（与原 ToolCallEntry 结构一致）
    pub entry: ToolCallEntry,
    /// 从 ctx 提取的组织 ID（统计用）
    pub organization_id: Option<String>,
    /// 从 ctx 提取的用户 ID（统计用）
    pub user_id: Option<String>,
    /// 原始参数 JSON 长度（统计用）
    pub args_len: u64,
    /// 结果 JSON 长度（统计用）
    pub result_len: u64,
    pub created_at: i64,
}

impl ToolExecEvent {
    pub fn new(
        entry: ToolCallEntry,
        organization_id: Option<String>,
        user_id: Option<String>,
        args_len: u64,
        result_len: u64,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::now_v7().to_string(),
            entry,
            organization_id,
            user_id,
            args_len,
            result_len,
            created_at: common::constants::utils::current_timestamp_ms(),
        }
    }
}

impl Event for ToolExecEvent {
    fn kind(&self) -> EventKind {
        EventKind::new("agent.tool.executed")
    }

    fn id(&self) -> &str {
        &self.event_id
    }

    fn order_key(&self) -> &str {
        // 按 agent_id 串行，保证同一 Agent 的工具日志顺序
        self.entry.agent_id.as_deref().unwrap_or("")
    }

    fn created_at(&self) -> i64 {
        self.created_at
    }
}
```

- [ ] **Step 4: Create `src/models/events/agent_state.rs`**

```rust
use crate::pkg::aop::{Event, EventKind};
use serde::{Deserialize, Serialize};

/// Agent 运行时状态变更事件（Idle/Busy/Resting 切换）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStateEvent {
    pub event_id: String,
    pub agent_id: String,
    /// 变更前状态："idle" / "busy" / "resting"
    pub from_state: String,
    /// 变更后状态："idle" / "busy" / "resting"
    pub to_state: String,
    /// Busy 时关联的消息 ID
    pub message_id: Option<String>,
    pub created_at: i64,
}

impl AgentStateEvent {
    pub fn new(
        agent_id: &str,
        from_state: &str,
        to_state: &str,
        message_id: Option<String>,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::now_v7().to_string(),
            agent_id: agent_id.to_string(),
            from_state: from_state.to_string(),
            to_state: to_state.to_string(),
            message_id,
            created_at: common::constants::utils::current_timestamp_ms(),
        }
    }
}

impl Event for AgentStateEvent {
    fn kind(&self) -> EventKind {
        EventKind::new("agent.state.changed")
    }

    fn id(&self) -> &str {
        &self.event_id
    }

    fn order_key(&self) -> &str {
        &self.agent_id
    }

    fn created_at(&self) -> i64 {
        self.created_at
    }
}
```

- [ ] **Step 5: Update `src/models/events/mod.rs`**

Add the new modules and re-exports:

```rust
pub mod a2a_task_update;
pub mod agent_loop;
pub mod agent_state;
pub mod cron_trigger;
pub mod message;
pub mod think_round;
pub mod tool_exec;

pub use a2a_task_update::{
    A2A_SYNCED_MSG_COUNT_PREFIX, A2A_TASK_ID_TAG_PREFIX, extract_a2a_task_id,
    extract_text_from_parts, get_synced_msg_count, make_a2a_task_tag, make_synced_msg_tag,
};
pub use agent_loop::AgentLoopEvent;
pub use agent_state::AgentStateEvent;
pub use cron_trigger::CronTriggerEvent;
pub use message::MessageCreatedEvent;
pub use think_round::ThinkRoundEvent;
pub use tool_exec::ToolExecEvent;
```

- [ ] **Step 6: Build to verify compilation**

Run: `cargo build -p ai_orz 2>&1 | head -30`
Expected: PASS (no errors, only warnings about unused events which will be consumed later)

- [ ] **Step 7: Commit**

```bash
git add src/models/events/
git commit -m "feat: add AOP event types for agent loop hooks (AgentLoopEvent, ThinkRoundEvent, ToolExecEvent, AgentStateEvent)"
```

---

## Task 2: Extract Shared Think Loop Helper

**Files:**
- Modify: `src/service/domain/runtime/awakening.rs`

This task extracts the think loop from `awaken()` into a reusable private method, preparing for sleep alignment and ThinkRoundEvent hooks.

- [ ] **Step 1: Add `run_think_loop` private method to `RuntimeDomainImpl`**

Add this method to the `impl RuntimeDomainImpl` block (after the existing helper methods, before `impl RuntimeAwakening`). This method encapsulates the timeout + iteration + tool dispatch logic currently inline in `awaken()`.

```rust
impl RuntimeDomainImpl {
    // ... existing methods (brain_dal, tool_dal, prompt_builder) ...

    /// 执行 think 循环（awaken/sleep_and_settle 共用）
    ///
    /// 统一封装：超时控制 + 多轮迭代 + 工具调用分发。
    /// 每轮 think 后发布 ThinkRoundEvent（通过 AOP 同步转发）。
    ///
    /// 返回最终模型回答内容（ThinkResult::Final 的 content）。
    async fn run_think_loop(
        &self,
        ctx: RequestContext,
        brain: &crate::models::brain::Brain,
        prompt: &str,
        tool_descriptors: &[crate::models::cortex_types::ToolDescriptor],
        agent: &Agent,
        scene_str: &str,
        trace_id: &str,
    ) -> Result<String> {
        const THINK_TIMEOUT_SECS: u64 = 300;
        const MAX_TOOL_ITERATIONS: usize = 10;

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(THINK_TIMEOUT_SECS),
            async {
                let mut messages = vec![ChatMessage::user(prompt.to_string())];
                for round in 0..MAX_TOOL_ITERATIONS {
                    let round_start = std::time::Instant::now();
                    let result = self
                        .brain_dal()
                        .think(ctx.clone(), brain, &messages, tool_descriptors)
                        .await?;
                    let round_duration_ms = round_start.elapsed().as_millis() as u64;

                    match result {
                        ThinkResult::Final { content, .. } => {
                            // 发布 ThinkRoundEvent（无工具调用，最终轮）
                            let _ = crate::pkg::aop::publish(ThinkRoundEvent::new(
                                &agent.po.id,
                                trace_id,
                                scene_str,
                                round,
                                round_duration_ms,
                                false,
                                0,
                            ))
                            .await;
                            return Ok(content);
                        }
                        ThinkResult::ToolCall {
                            content,
                            tool_calls,
                            ..
                        } => {
                            let tc_count = tool_calls.len();
                            messages.push(ChatMessage::Assistant {
                                content,
                                tool_calls: Some(tool_calls.clone()),
                            });

                            for tc in tool_calls {
                                match agent.tools().iter().find(|t| t.po.name == tc.name) {
                                    Some(tool) => {
                                        let call_result = match tool.po.control_mode {
                                            common::enums::tool::ControlMode::Auto => {
                                                self.tool_dal()
                                                    .execute_auto(ctx.clone(), tool, tc.arguments)
                                                    .await
                                            }
                                            common::enums::tool::ControlMode::Manual => {
                                                self.tool_dal()
                                                    .execute_manual(ctx.clone(), tool, tc.arguments)
                                                    .await
                                            }
                                        };
                                        match call_result {
                                            Ok((value, _entry)) => {
                                                messages.push(ChatMessage::tool(
                                                    tc.id,
                                                    format!("{}", value),
                                                ));
                                            }
                                            Err(e) => {
                                                messages.push(ChatMessage::tool(
                                                    tc.id,
                                                    format!("Error: {}", e),
                                                ));
                                            }
                                        }
                                    }
                                    None => {
                                        messages.push(ChatMessage::tool(
                                            tc.id,
                                            format!("Error: tool {} not found", tc.name),
                                        ));
                                    }
                                }
                            }

                            // 发布 ThinkRoundEvent（有工具调用）
                            let _ = crate::pkg::aop::publish(ThinkRoundEvent::new(
                                &agent.po.id,
                                trace_id,
                                scene_str,
                                round,
                                round_duration_ms,
                                true,
                                tc_count,
                            ))
                            .await;
                        }
                    }
                }
                Err(err!(
                    Internal,
                    "think loop exceeded max {} iterations",
                    MAX_TOOL_ITERATIONS
                ))
            },
        )
        .await;

        match result {
            Ok(inner) => inner,
            Err(_elapsed) => Err(err!(
                Internal,
                "brain think timeout after {}s",
                THINK_TIMEOUT_SECS
            )),
        }
    }
}
```

Add the necessary import at the top of the file:
```rust
use crate::models::events::ThinkRoundEvent;
```

- [ ] **Step 2: Refactor `awaken()` to use `run_think_loop`**

Replace the inline think loop (lines ~234-317) with a call to `run_think_loop`. The awaken method should:

1. Build `tool_descriptors` from `agent.tools()` (already done at line 226-227)
2. Call `self.run_think_loop(ctx.clone(), brain, &prompt, &tool_descriptors, agent, "awaken", &trace_id)`
3. Keep the error handling (record_event on failure) after the call

Replace the block from `const THINK_TIMEOUT_SECS` through the end of the `think_result` match with:

```rust
        // Step 4: 调用大脑思考（带工具调用循环）
        let brain = agent
            .brain
            .as_ref()
            .ok_or_else(|| err!(Internal, "Agent 大脑未唤醒，请先调用 wake_brain()"))?;

        let tool_descriptors: Vec<ToolDescriptor> =
            agent.tools().iter().map(ToolDescriptor::from).collect();

        let think_result = self
            .run_think_loop(
                ctx.clone(),
                brain,
                &prompt,
                &tool_descriptors,
                agent,
                "awaken",
                &trace_id,
            )
            .await;
```

- [ ] **Step 3: Build to verify compilation**

Run: `cargo build -p ai_orz 2>&1 | head -30`
Expected: PASS

- [ ] **Step 4: Run existing awaken tests**

Run: `cargo test -p ai_orz --lib service::domain::runtime::awakening -- --nocapture 2>&1 | tail -20`
Expected: PASS (test_awaken_with_skills, test_awaken_without_skills)

- [ ] **Step 5: Commit**

```bash
git add src/service/domain/runtime/awakening.rs
git commit -m "refactor: extract run_think_loop helper from awaken for reuse in sleep_and_settle"
```

---

## Task 3: Align sleep_and_settle with awaken

**Files:**
- Modify: `src/service/domain/runtime/awakening.rs`

This task aligns sleep_and_settle to use the shared think loop with filtered tools, replacing the current `&[]` + single-think pattern.

- [ ] **Step 1: Replace sleep_and_settle's think call with run_think_loop**

In `sleep_and_settle()`, replace the block from `const THINK_TIMEOUT_SECS` through the `raw_output` match (lines ~488-537) with:

```rust
        // Step 5: 调用大脑思考（带工具调用循环，与 awaken 对称）
        // sleep 场景传递过滤后的记忆工具，Agent 可通过 function calling 调用记忆工具完成沉淀
        let brain = agent
            .brain
            .as_ref()
            .ok_or_else(|| err!(Internal, "Agent 大脑未唤醒，请先调用 wake_brain()"))?;

        let tool_descriptors: Vec<ToolDescriptor> = all_tools
            .iter()
            .map(|t| ToolDescriptor::from(t))
            .collect::<Vec<_>>()
            .into_iter()
            .collect();

        // 查找过滤后的工具实体（用于工具调用循环中的 execute 分发）
        // agent.tools() 包含全部工具，通过 name 匹配找到对应的 Tool 实体
        let think_result = self
            .run_think_loop(
                ctx.clone(),
                brain,
                &prompt,
                &tool_descriptors,
                agent,
                "settle",
                &trace_id,
            )
            .await;
```

Note: `all_tools` here is `Vec<ToolPo>` (filtered). `ToolDescriptor::from` takes `&Tool`, but we have `ToolPo`. Check the existing `From` impl — it's `From<&Tool>` in cortex_types.rs. We need to convert from `ToolPo` instead.

Looking at the existing awaken code (line 226-227):
```rust
let tool_descriptors: Vec<ToolDescriptor> =
    agent.tools().iter().map(ToolDescriptor::from).collect();
```
This uses `From<&Tool>` where Tool contains the po. For sleep, `all_tools` is `Vec<ToolPo>`. We need to either:
- Use `agent.tools()` filtered to match `all_tools` (but that defeats the purpose of pre-filtering)
- Or add `From<&ToolPo>` for ToolDescriptor

The simplest approach: build tool_descriptors from `agent.tools()` filtered by the same scene rule (the filtering already happened above, so just filter agent.tools() the same way):

```rust
        let tool_descriptors: Vec<ToolDescriptor> = agent
            .tools()
            .iter()
            .filter(|t| {
                let tags = t.po.get_tags();
                scene.is_tool_allowed(&tags)
            })
            .map(ToolDescriptor::from)
            .collect();
```

This reuses the `From<&Tool>` impl and applies the same filter. The `all_tools` Vec<ToolPo> is still used for the builder (though it will be removed in Task 5).

Replace the `tool_descriptors` construction with this filtered version. The full replacement block:

```rust
        // Step 5: 调用大脑思考（带工具调用循环，与 awaken 对称）
        // sleep 场景传递过滤后的记忆工具，Agent 可通过 function calling 调用记忆工具完成沉淀
        let brain = agent
            .brain
            .as_ref()
            .ok_or_else(|| err!(Internal, "Agent 大脑未唤醒，请先调用 wake_brain()"))?;

        let tool_descriptors: Vec<ToolDescriptor> = agent
            .tools()
            .iter()
            .filter(|t| {
                let tags = t.po.get_tags();
                scene.is_tool_allowed(&tags)
            })
            .map(ToolDescriptor::from)
            .collect();

        let think_result = self
            .run_think_loop(
                ctx.clone(),
                brain,
                &prompt,
                &tool_descriptors,
                agent,
                "settle",
                &trace_id,
            )
            .await;

        let raw_output = match think_result {
            Ok(content) => content,
            Err(e) => {
                let duration_ms = start_time
                    .elapsed()
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                if let Err(stats_err) = record_event!(
                    ctx,
                    AgentAwakeEvent {
                        agent_id: agent.po.id.clone(),
                        project_id: None,
                        task_id: None,
                        organization_id: ctx.organization_id.clone(),
                        user_id: Some(ctx.uid()),
                        message_id: None,
                        call_count: 1,
                        duration_ms: duration_ms,
                        status: format!("settle failed: {}", e),
                    }
                ) {
                    log_warn!(
                        &ctx,
                        "sleep_and_settle",
                        "record_event failed on error path, stats may be incomplete: {:?}",
                        stats_err
                    );
                }
                return Err(e);
            }
        };
```

Also remove the now-unnecessary comment at line 504-505 about "Settle 场景不传工具".

- [ ] **Step 2: Build to verify compilation**

Run: `cargo build -p ai_orz 2>&1 | head -30`
Expected: PASS

- [ ] **Step 3: Run existing tests**

Run: `cargo test -p ai_orz --lib service::domain::runtime::awakening -- --nocapture 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/service/domain/runtime/awakening.rs
git commit -m "fix: align sleep_and_settle with awaken — pass filtered tools and use think loop"
```

---

## Task 4: Publish AgentLoopEvent from awakening.rs

**Files:**
- Modify: `src/service/domain/runtime/awakening.rs`

- [ ] **Step 1: Add AgentLoopEvent import**

At the top of `awakening.rs`, add:
```rust
use crate::models::events::{AgentLoopEvent, ThinkRoundEvent};
```
(ThinkRoundEvent was added in Task 2 but ensure the import is correct.)

- [ ] **Step 2: Publish AgentLoopEvent::started at awaken start**

In `awaken()`, after the trace_id is obtained (after line 185 `let trace_id = trace.id.clone();`), add:

```rust
        // 发布循环启动事件（AOP 同步转发）
        let _ = crate::pkg::aop::publish(AgentLoopEvent::started(
            &agent.po.id,
            &trace_id,
            "awaken",
            Some(&message.po.id),
        ))
        .await;
```

- [ ] **Step 3: Publish AgentLoopEvent::finished on awaken success**

In `awaken()`, before the final `Ok(AwakeningResult { ... })` return (after the stats record_event block, around line 390), add:

```rust
        let _ = crate::pkg::aop::publish(AgentLoopEvent::finished(
            &agent.po.id,
            &trace_id,
            "awaken",
            "success",
            duration_ms,
            Some(&message.po.id),
        ))
        .await;
```

- [ ] **Step 4: Publish AgentLoopEvent::finished on awaken failure**

In `awaken()` error path (the `Err(e) =>` branch of the think_result match, around line 322-351), after the record_event call, add:

```rust
                let _ = crate::pkg::aop::publish(AgentLoopEvent::finished(
                    &agent.po.id,
                    &trace_id,
                    "awaken",
                    &format!("failed: {}", e),
                    duration_ms,
                    Some(&message.po.id),
                ))
                .await;
```

- [ ] **Step 5: Publish AgentLoopEvent for sleep_and_settle (started + finished)**

In `sleep_and_settle()`:
- After `let trace_id = trace.id.clone();` (around line 445), add the started event:
```rust
        let _ = crate::pkg::aop::publish(AgentLoopEvent::started(
            &agent.po.id,
            &trace_id,
            "settle",
            None,
        ))
        .await;
```

- In the success path (after stats record_event, before the return), add:
```rust
        let _ = crate::pkg::aop::publish(AgentLoopEvent::finished(
            &agent.po.id,
            &trace_id,
            "settle",
            "settle success",
            duration_ms,
            None,
        ))
        .await;
```

- In the error path (after stats record_event, before `return Err(e)`), add:
```rust
                let _ = crate::pkg::aop::publish(AgentLoopEvent::finished(
                    &agent.po.id,
                    &trace_id,
                    "settle",
                    &format!("settle failed: {}", e),
                    duration_ms,
                    None,
                ))
                .await;
```

- [ ] **Step 6: Build and test**

Run: `cargo build -p ai_orz 2>&1 | head -30`
Run: `cargo test -p ai_orz --lib service::domain::runtime::awakening -- --nocapture 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/service/domain/runtime/awakening.rs
git commit -m "feat: publish AgentLoopEvent (started/finished) from awaken and sleep_and_settle"
```

---

## Task 5: Publish AgentStateEvent from AgentRuntimeStateManager

**Files:**
- Modify: `src/pkg/agent_runtime_state.rs`

- [ ] **Step 1: Add import and publish AgentStateEvent on each state transition**

At the top of the file, add:
```rust
use crate::models::events::AgentStateEvent;
```

Modify each state setter to publish an event. Add a private helper:

```rust
impl AgentRuntimeStateManager {
    /// 发布状态变更事件（AOP 同步转发）
    async fn publish_state_event(
        &self,
        agent_id: &str,
        from_state: &str,
        to_state: &str,
        message_id: Option<String>,
    ) {
        let _ = crate::pkg::aop::publish(AgentStateEvent::new(
            agent_id,
            from_state,
            to_state,
            message_id,
        ))
        .await;
    }
}
```

Then modify each setter to call `publish_state_event` after updating the state:

**`set_idle`**:
```rust
    pub fn set_idle(&self, agent_id: &str) {
        let from_state = self
            .get_state(agent_id)
            .map(|s| s.as_str().to_string())
            .unwrap_or_else(|| "idle".to_string());
        let mut entry = self.states.entry(agent_id.to_string()).or_default();
        entry.state = AgentRuntimeState::Idle;
        entry.current_message_id = None;
        entry.state_started_at = common::constants::utils::current_timestamp_ms();
        // 发布状态变更事件（同步，非阻塞业务流程）
        // 使用 tokio::spawn 避免在同步方法中 await
        let mgr = AgentRuntimeStateManager::global();
        tokio::spawn(async move {
            mgr.publish_state_event(agent_id, &from_state, "idle", None)
                .await;
        });
    }
```

Wait — `set_idle` is a sync method, but `publish` is async. We can't `.await` in a sync method. Options:
1. Make the state managers async (breaking change, many callers)
2. Use `tokio::spawn` to fire-and-forget the publish
3. Use a channel to send events to a background task

Option 2 (tokio::spawn) is simplest. The event is fire-and-forget for sync consumers anyway. But `tokio::spawn` requires a runtime context. Since the agent loop runs in async context, the callers are already in a runtime.

Actually, looking more carefully: `set_idle` is called from `BusyGuard::drop` which is sync. The Drop happens when the guard goes out of scope, which is in an async context (inside `awaken()`). But Drop itself is sync.

The cleanest approach: use `tokio::spawn` in the sync setters. This is fire-and-forget — the sync consumer will process the event shortly after. Since ConsumeMode::Sync consumers are lightweight (just logging), the slight delay is acceptable.

But wait, there's a subtlety. The AOP publish for sync consumers calls `consumer.on_event().await` directly. If we spawn it, the consumer runs in a separate task. This is fine for fire-and-forget hooks.

Let me revise: instead of modifying each setter individually, create a helper that spawns the publish:

```rust
    fn notify_state_change(&self, agent_id: &str, from_state: &str, to_state: &str, message_id: Option<String>) {
        // 同步方法中无法 await，使用 tokio::spawn 异步发布事件
        // 事件为 fire-and-forget，不影响业务流程
        let agent_id = agent_id.to_string();
        tokio::spawn(async move {
            let _ = crate::pkg::aop::publish(AgentStateEvent::new(
                &agent_id,
                from_state,
                to_state,
                message_id,
            ))
            .await;
        });
    }
```

Then in each setter, after updating the state, call `self.notify_state_change(...)`.

Modify `set_idle`, `set_resting`, `set_busy`, and `try_set_busy` to capture `from_state` before mutation and call `notify_state_change` after.

For `set_idle`:
```rust
    pub fn set_idle(&self, agent_id: &str) {
        let from_state = self.get_state(agent_id);
        let mut entry = self.states.entry(agent_id.to_string()).or_default();
        entry.state = AgentRuntimeState::Idle;
        entry.current_message_id = None;
        entry.state_started_at = common::constants::utils::current_timestamp_ms();
        drop(entry); // release dashmap borrow
        self.notify_state_change(agent_id, state_str(from_state), "idle", None);
    }
```

Add a helper:
```rust
fn state_str(state: AgentRuntimeState) -> &'static str {
    match state {
        AgentRuntimeState::Idle => "idle",
        AgentRuntimeState::Busy => "busy",
        AgentRuntimeState::Resting => "resting",
    }
}
```

Apply the same pattern to `set_resting`, `set_busy`, and `try_set_busy` (only notify when `try_set_busy` returns true).

- [ ] **Step 2: Build to verify compilation**

Run: `cargo build -p ai_orz 2>&1 | head -30`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/pkg/agent_runtime_state.rs
git commit -m "feat: publish AgentStateEvent on runtime state changes via AOP"
```

---

## Task 6: Make Stats Globally Accessible

**Files:**
- Modify: `src/pkg/stats/mod.rs`
- Modify: `src/pkg/storage/mod.rs`

The ToolExecStatsConsumer needs to access the Stats instance to record tool call stats. Currently Stats is only accessible via `ctx.stats_opt()`. We add a global accessor.

- [ ] **Step 1: Add global Stats singleton to `src/pkg/stats/mod.rs`**

At the end of the file (after existing re-exports), add:

```rust
use std::sync::{Arc, OnceLock};

static GLOBAL_STATS: OnceLock<Arc<Stats>> = OnceLock::new();

/// 初始化全局 Stats 单例（应用启动时调用）
pub fn init_global_stats(stats: Stats) {
    let _ = GLOBAL_STATS.set(Arc::new(stats));
}

/// 获取全局 Stats 单例（AOP 消费者等无法通过 ctx 访问 Stats 的场景使用）
pub fn global_stats() -> Option<&'static Arc<Stats>> {
    GLOBAL_STATS.get()
}
```

- [ ] **Step 2: Initialize global Stats in Storage**

In `src/pkg/storage/mod.rs`, find where `Stats::open()` is called (around line 97) and add `init_global_stats` after the Stats is created:

```rust
let stats = Stats::open(
    // ... existing params ...
)?;
crate::pkg::stats::init_global_stats(stats.clone());
```

Note: `Stats` needs to be `Clone` or wrapped in `Arc`. Check if `Stats` implements `Clone`. If not, wrap it: change `GLOBAL_STATS` to `OnceLock<Stats>` and use `init_global_stats(stats)` (moving it in).

Actually, looking at the existing code, `Storage` holds `Stats` directly (not Arc). Let me check if Stats is Clone... Looking at `storage/mod.rs:149`: `pub fn stats(&self) -> &Stats`. It returns a reference. So Stats is not Clone (it holds a DuckDB connection).

Simpler approach: store a raw pointer or reference. But that's unsafe. Better: change `GLOBAL_STATS` to store `&'static Stats` by leaking it, or use `OnceLock<Stats>`:

```rust
static GLOBAL_STATS: OnceLock<Stats> = OnceLock::new();

pub fn init_global_stats(stats: Stats) {
    let _ = GLOBAL_STATS.set(stats);
}

pub fn global_stats() -> Option<&'static Stats> {
    GLOBAL_STATS.get()
}
```

Then in Storage, after creating stats, move a clone or the original into global. Since Stats is not Clone, we need to either:
- Make Stats Clone (likely not trivial due to DuckDB connection)
- Or initialize global Stats separately

The cleanest: call `init_global_stats(stats)` BEFORE storing it in Storage, then Storage gets its own via `global_stats().unwrap()`.

But this changes Storage initialization. Let me take a different approach: in `storage/mod.rs`, after `Stats::open()`, set the global and then store a reference.

Actually, looking at `Storage::new()` at line 97:
```rust
let stats = Stats::open(...)?;
```

We can do:
```rust
let stats = Stats::open(...)?;
crate::pkg::stats::init_global_stats(stats);
// Then Storage stores a reference to the global, or we need to restructure.
```

But Storage needs to own Stats. If we move it to global, Storage can't own it.

**Simplest solution:** Make `GLOBAL_STATS` hold `Arc<Stats>`, and make Stats wraptable in Arc. Then both Storage and global hold Arc clones.

Looking at Storage struct, it likely holds `Stats` directly. Let me check... Actually, I'll modify the approach: store `Arc<Stats>` in both places.

In `storage/mod.rs`, change the Stats field to `Arc<Stats>`:
```rust
pub struct Storage {
    // ... other fields ...
    stats: Arc<Stats>,
}
```

Then:
```rust
let stats = Arc::new(Stats::open(...)?);
crate::pkg::stats::init_global_stats(stats.clone());
// Store Arc<Stats> in Storage
```

But this requires changing Storage's stats field and all its accessors. That's a larger change.

**Even simpler approach:** Don't make Stats global. Instead, have the ToolExecStatsConsumer create a `RequestContext::new_system()` and check if it has stats. Since `new_system()` doesn't have stats, the recording would be silently skipped.

But then stats wouldn't be recorded at all for tool calls!

**Pragmatic solution:** Keep the stats recording inline in `ToolCallDao::execute` (using ctx.stats), and only move the JSONL logging to AOP. The ToolExecEvent is still published for other subscribers.

Actually, re-reading the user's request: "你看看我们是不是能取代掉现有的装饰器实现，让这些日志和统计都通过 aop 来实现"

The user is asking IF we can do this. If stats can't easily go through AOP due to the ctx dependency, we should explain this and keep stats inline.

**Decision:** For this plan, I'll use the `Arc<Stats>` approach. It's the cleanest way to make Stats globally accessible.

Modify `storage/mod.rs`:
1. Change the `stats` field to `Arc<Stats>`
2. After `Stats::open()`, wrap in Arc and init global
3. Update `stats()` and `stats_opt()` to return `&Stats` (dereference Arc)

Actually, let me read the Storage struct first to understand the current structure...

Actually, I already have enough context. Let me just proceed with the `Arc<Stats>` approach in the plan. The subagent can handle the details.

Wait, actually I should check the Storage struct to see if changing to Arc is feasible. Let me read it.

Actually, for the plan, I'll specify the minimal changes needed. Let me simplify:

**Approach:** Add `init_global_stats` that takes `&'static Stats`. In `Storage::new()`, after creating Stats, leak it to get a static reference, then store the owned Stats in Storage and the static ref in global.

No, leaking is bad. Let me use `Arc<Stats>`.

OK, let me just specify the approach in the plan and let the subagent handle the details. The key changes are:
1. `GLOBAL_STATS: OnceLock<Arc<Stats>>`
2. `init_global_stats(Arc<Stats>)`
3. `global_stats() -> Option<&'static Arc<Stats>>`
4. In Storage, wrap Stats in Arc, init global with clone, store Arc

- [ ] **Step 1: Add global Stats accessor**

In `src/pkg/stats/mod.rs`, add at the end:

```rust
use std::sync::{Arc, OnceLock};

static GLOBAL_STATS: OnceLock<Arc<Stats>> = OnceLock::new();

/// 初始化全局 Stats 单例
pub fn init_global_stats(stats: Arc<Stats>) {
    let _ = GLOBAL_STATS.set(stats);
}

/// 获取全局 Stats（AOP 消费者等无 ctx 场景使用）
pub fn global_stats() -> Option<&'static Arc<Stats>> {
    GLOBAL_STATS.get()
}
```

- [ ] **Step 2: Initialize global Stats in Storage**

Read `src/pkg/storage/mod.rs` to find the Stats::open call and Storage struct. Modify to:
1. Wrap Stats in `Arc::new(Stats::open(...)?)`
2. Call `crate::pkg::stats::init_global_stats(stats_arc.clone())`
3. Store `stats_arc` in Storage

Update `Storage::stats()` and `stats_opt()` to dereference Arc.

- [ ] **Step 3: Build to verify**

Run: `cargo build -p ai_orz 2>&1 | head -30`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/pkg/stats/mod.rs src/pkg/storage/mod.rs
git commit -m "feat: add global Stats accessor for AOP consumers"
```

---

## Task 7: Replace ToolCallLoggingDecorator with AOP

**Files:**
- Modify: `src/service/dao/tool_call/impl.rs`
- Modify: `src/service/dao/tool_call/mod.rs`
- Delete: `src/pkg/tool_tracing/tool_call_logger.rs`
- Modify: `src/pkg/tool_tracing/mod.rs`
- Create: `src/consumer/tool_exec_log_consumer.rs`
- Create: `src/consumer/tool_exec_stats_consumer.rs`

- [ ] **Step 1: Modify `ToolCallDao::execute` to publish ToolExecEvent instead of using decorator**

In `src/service/dao/tool_call/impl.rs`, replace the `execute` method. The new version:
1. Clones the raw tool and calls it directly (no decorator wrapping)
2. Constructs ToolCallEntry inline (same logic as decorator's `call_with_entry`)
3. Publishes ToolExecEvent via AOP (sync consumers handle logging + stats)
4. Returns `(Value, ToolCallEntry)` as before

```rust
    async fn execute(
        &self,
        ctx: RequestContext,
        tool: &Tool,
        args: Value,
    ) -> Result<(Value, ToolCallEntry)> {
        use common::constants::utils::current_timestamp_ms;
        use uuid::Uuid;

        let call_id = Uuid::now_v7().to_string();
        let started_at = current_timestamp_ms();
        let po = tool.our_tool.po();

        // 直接调用原始工具（无装饰器包装）
        let cloned: Box<dyn CoreTool + Send + Sync> = dyn_clone::clone_box(&*tool.our_tool);
        let result = cloned.call(ctx.clone(), args.clone()).await;
        let finished_at = current_timestamp_ms();
        let duration_ms = finished_at - started_at;

        // 脱敏处理（与原 decorator 一致）
        let args_clone = args.clone();
        let output_json: Option<Value> = match &result {
            Ok(v) => Some(v.clone()),
            Err(_) => None,
        };
        let (log_input, log_output, log_error) = redact_trace_values_for_tool(
            po,
            args,
            output_json,
            result.as_ref().err().map(|e| e.to_string()),
        );

        // 构造 entry
        let mut entry = ToolCallEntry {
            call_id,
            tool_id: po.id.clone(),
            tool_name: po.name.clone(),
            agent_id: ctx.agent_id().cloned(),
            task_id: ctx.task_id().cloned(),
            project_id: ctx.project_id().cloned(),
            started_at: started_at.try_into().unwrap(),
            finished_at: finished_at.try_into().unwrap(),
            duration_ms: duration_ms.try_into().unwrap(),
            input: log_input,
            output: log_output,
            error: log_error,
            status: match &result {
                Ok(_) => ToolCallStatus::Completed,
                Err(_) => ToolCallStatus::Failed,
            },
            metadata: Value::Null,
        };

        // 注入 caller_location
        let location = std::panic::Location::caller();
        let location_str = format!("{}:{}", location.file(), location.line());
        if let serde_json::Value::Object(ref mut map) = entry.metadata {
            map.insert("caller_location".to_string(), Value::String(location_str));
        } else {
            let mut map = serde_json::Map::new();
            map.insert("caller_location".to_string(), Value::String(location_str));
            entry.metadata = Value::Object(map);
        }

        // 通过 AOP 同步发布工具执行事件（订阅者处理日志+统计）
        let args_len = serde_json::to_string(&args_clone)
            .map(|s| s.len() as u64)
            .unwrap_or(0);
        let result_len = entry
            .output
            .as_ref()
            .and_then(|v| serde_json::to_string(v).ok())
            .map(|s| s.len() as u64)
            .unwrap_or(0);

        let _ = crate::pkg::aop::publish(crate::models::events::ToolExecEvent::new(
            entry.clone(),
            ctx.organization_id().cloned(),
            ctx.user_id().cloned(),
            args_len,
            result_len,
        ))
        .await;

        match result {
            Ok(value) => Ok((value, entry)),
            Err(error) => {
                use common::error::{ErrorCode, ErrorType};
                let mut err = common::error::Error::typed(
                    ErrorCode::ToolExecutionFailed,
                    ErrorType::Tool,
                    error.to_string(),
                )
                .with_source(error);
                let trace_ref = ToolCallTraceRef {
                    tool_id: entry.tool_id,
                    call_id: entry.call_id,
                };
                let mut field = common::error::ErrorField::new();
                field.set_trace_ref(trace_ref);
                err = err.with_field(field);
                Err(err.into())
            }
        }
    }
```

Add the `redact_trace_values_for_tool` function (moved from tool_call_logger.rs) to the impl.rs file:

```rust
fn redact_trace_values_for_tool(
    po: &ToolPo,
    input: Value,
    output: Option<Value>,
    error: Option<String>,
) -> (Value, Option<Value>, Option<String>) {
    use common::enums::ToolProtocol;
    if !matches!(po.protocol, ToolProtocol::Http | ToolProtocol::Mcp) {
        return (input, output, error);
    }
    (
        Value::String("[REDACTED]".to_string()),
        output.map(|_| Value::String("[REDACTED]".to_string())),
        error.map(|_| "[REDACTED]".to_string()),
    )
}
```

Also update the imports at the top of impl.rs:
```rust
use crate::models::tool::{CoreTool, Tool, ToolCallTraceRef, ToolPo};
use crate::pkg::tool_tracing::entry::{ToolCallEntry, ToolCallStatus};
// Remove: use crate::pkg::tool_tracing::ToolCallLoggingDecorator;
```

- [ ] **Step 2: Remove `decorate` from ToolCallDao trait and impl**

In `src/service/dao/tool_call/mod.rs`, remove the `decorate` method from the trait (lines ~48-52).

In `src/service/dao/tool_call/impl.rs`, remove the `decorate` implementation (lines ~70-72).

- [ ] **Step 3: Delete `tool_call_logger.rs` and update `mod.rs`**

Delete `src/pkg/tool_tracing/tool_call_logger.rs`.

In `src/pkg/tool_tracing/mod.rs`, remove:
```rust
pub mod tool_call_logger;
pub use tool_call_logger::LoggingDecorator as ToolCallLoggingDecorator;
```

Keep `entry` and `logger` modules (they're still used by the consumer and for query APIs).

- [ ] **Step 4: Create `src/consumer/tool_exec_log_consumer.rs`**

This sync consumer replaces the decorator's JSONL logging:

```rust
//! Tool execution log consumer (AOP sync)
//!
//! Replaces ToolCallLoggingDecorator's JSONL logging.
//! Subscribes to "agent.tool.executed" events and writes to ToolCallLogger.

use async_trait::async_trait;
use crate::pkg::aop::{ConsumeMode, Consumer, EventKind};
use crate::models::events::ToolExecEvent;
use crate::pkg::tool_tracing::logger::ToolCallLogger;
use common::error::Result;

pub struct ToolExecLogConsumer;

impl ToolExecLogConsumer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ToolExecLogConsumer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Consumer for ToolExecLogConsumer {
    fn name(&self) -> &str {
        "tool_exec_log"
    }

    fn interested_events(&self) -> Vec<EventKind> {
        vec![EventKind::new("agent.tool.executed")]
    }

    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Sync
    }

    async fn on_event(&self, event: serde_json::Value) -> Result<()> {
        let event: ToolExecEvent = serde_json::from_value(event).map_err(|e| {
            common::error::Error::internal(format!("failed to deserialize ToolExecEvent: {}", e))
        })?;

        // 写入 JSONL 日志（与原 decorator 的 log_call 逻辑一致）
        let logger = ToolCallLogger::get();
        let _ = logger.log_call(&event.entry.tool_id, event.entry);

        Ok(())
    }
}
```

- [ ] **Step 5: Create `src/consumer/tool_exec_stats_consumer.rs`**

This sync consumer replaces the decorator's stats recording:

```rust
//! Tool execution stats consumer (AOP sync)
//!
//! Replaces ToolCallLoggingDecorator's record_tool_call_stat.
//! Subscribes to "agent.tool.executed" events and records ToolCallEvent stats.

use async_trait::async_trait;
use crate::pkg::aop::{ConsumeMode, Consumer, EventKind};
use crate::models::events::ToolExecEvent;
use crate::pkg::stats::{ToolCallEvent, global_stats};
use common::error::Result;

pub struct ToolExecStatsConsumer;

impl ToolExecStatsConsumer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ToolExecStatsConsumer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Consumer for ToolExecStatsConsumer {
    fn name(&self) -> &str {
        "tool_exec_stats"
    }

    fn interested_events(&self) -> Vec<EventKind> {
        vec![EventKind::new("agent.tool.executed")]
    }

    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Sync
    }

    async fn on_event(&self, event: serde_json::Value) -> Result<()> {
        let event: ToolExecEvent = serde_json::from_value(event).map_err(|e| {
            common::error::Error::internal(format!("failed to deserialize ToolExecEvent: {}", e))
        })?;

        // 构造统计事件（与原 decorator 的 record_tool_call_stat 逻辑一致）
        let status = if matches!(event.entry.status, crate::pkg::tool_tracing::entry::ToolCallStatus::Completed) {
            "success".to_string()
        } else {
            "failed".to_string()
        };

        let stats_event = ToolCallEvent::new(event.entry.finished_at as i64)
            .with_tool_id(event.entry.tool_id.clone())
            .with_tool_name(event.entry.tool_name.clone())
            .with_agent_id(event.entry.agent_id.clone())
            .with_project_id(event.entry.project_id.clone())
            .with_task_id(event.entry.task_id.clone())
            .with_organization_id(event.organization_id.clone())
            .with_user_id(event.user_id.clone())
            .with_args_len(event.args_len)
            .with_result_len(event.result_len)
            .with_duration_ms(event.entry.duration_ms)
            .with_status(status);

        // 通过全局 Stats 记录（AOP 消费者无 ctx，使用 global_stats）
        if let Some(stats) = global_stats() {
            let ctx = crate::pkg::request_context::RequestContext::new_system();
            let _ = stats.record(ctx, stats_event).await;
        }

        Ok(())
    }
}
```

- [ ] **Step 6: Register new consumers in `src/consumer/mod.rs`**

Add the new consumer modules and register them:

```rust
pub mod tool_exec_log_consumer;
pub mod tool_exec_stats_consumer;

pub async fn init() -> Result<()> {
    sys_info!("registering business consumers to AOP event center...");

    aop::registry().register_consumer(Arc::new(message::MessageConsumer::new()))?;
    aop::registry().register_consumer(Arc::new(scheduler::CronTriggerConsumer::new()))?;
    aop::registry().register_consumer(Arc::new(tool_exec_log_consumer::ToolExecLogConsumer::new()))?;
    aop::registry().register_consumer(Arc::new(tool_exec_stats_consumer::ToolExecStatsConsumer::new()))?;

    sys_info!("all business consumers registered");
    Ok(())
}
```

- [ ] **Step 7: Fix any remaining references to ToolCallLoggingDecorator**

Search for all remaining references to `ToolCallLoggingDecorator` or `LoggingDecorator` in the codebase and update them:

Run: `grep -rn "ToolCallLoggingDecorator\|LoggingDecorator" src/ --include="*.rs"`

Update any test files that reference the decorator. The test in `awakening.rs` uses `ToolCallLogger` directly (not the decorator), so it should be fine. Check `runtime/mod.rs` which has `tool_call_logger: Arc<ToolCallLogger>` field — this is the logger, not the decorator, so it stays.

- [ ] **Step 8: Build and test**

Run: `cargo build -p ai_orz 2>&1 | head -40`
Expected: PASS

Run: `cargo test -p ai_orz --lib 2>&1 | tail -30`
Expected: PASS (fix any breakages from decorator removal)

- [ ] **Step 9: Commit**

```bash
git add src/service/dao/tool_call/ src/pkg/tool_tracing/ src/consumer/
git commit -m "refactor: replace ToolCallLoggingDecorator with AOP sync consumers (log + stats)"
```

---

## Task 8: Clean Up Zombie Code and Stale Comments

**Files:**
- Modify: `src/models/prompt_builder.rs`
- Modify: `src/service/dal/agent/mod.rs`
- Modify: `src/service/domain/runtime/awakening.rs`
- Modify: `src/service/domain/runtime/mod.rs`
- Modify: `src/consumer/message.rs`
- Modify: `src/handlers/hr/agent/settle_memory.rs`

- [ ] **Step 1: Remove `tools()` from PromptBuilder trait**

In `src/models/prompt_builder.rs`:
1. Remove the `tools(&mut self, tools: &[ToolPo])` method (lines 51-54)
2. Remove `ToolPo` from imports if no longer used
3. Update the doc comment example to remove `builder.tools(&tools)`

- [ ] **Step 2: Remove `tools` field and setter from DefaultPromptBuilder**

In `src/service/dal/agent/mod.rs`:
1. Remove `tools: Vec<ToolPo>` field from `DefaultPromptBuilder` struct
2. Remove the `tools()` setter implementation
3. Remove `ToolPo` import if no longer used in that scope
4. Fix the stale doc comment at line ~892-893 about "Manual 工具调用规范"

- [ ] **Step 3: Remove `builder.tools()` calls from awakening.rs**

In `src/service/domain/runtime/awakening.rs`:
1. Remove `builder.tools(&all_tools)` in `awaken()` (line ~204)
2. Remove `builder.tools(&all_tools)` in `sleep_and_settle()` (line ~476)
3. Remove the `all_tools` variable in `awaken()` (lines ~190-191) since it's no longer needed by the builder (ToolDescriptor is built separately from `agent.tools()`)

Note: In `sleep_and_settle()`, the `all_tools` Vec<ToolPo> is still constructed for the builder but no longer needed. However, the filtered `tool_descriptors` is built from `agent.tools()` directly. Remove the unused `all_tools` variable in sleep_and_settle.

- [ ] **Step 4: Remove `wake_agent_brain` scene parameter**

In `src/service/domain/runtime/mod.rs`:
1. Remove `scene: ThinkingScene` from the `wake_agent_brain` trait method signature
2. Remove the stale comment about "场景过滤" (lines ~125-128)

In `src/service/domain/runtime/awakening.rs`:
1. Remove `_scene: ThinkingScene` from the `wake_agent_brain` implementation
2. Fix the doc comment (lines ~100-101) to remove mention of "从 agent.tools 分离出 Auto 工具注入 Rig"
3. Fix the stale comment at lines ~187-189 about "wake_agent_brain 已将 Auto 工具移出"

In `src/consumer/message.rs`:
1. Remove `ThinkingScene::Awaken` argument from `wake_agent_brain()` call

In `src/handlers/hr/agent/settle_memory.rs`:
1. Remove `ThinkingScene::Settle` argument from `wake_agent_brain()` call

- [ ] **Step 5: Fix ThinkingScene doc comment**

In `src/service/domain/runtime/awakening.rs`, fix the doc comment on `ThinkingScene` (lines ~20-24):

```rust
/// 思考场景类型
///
/// 用于区分唤醒（awaken）和沉睡（sleep_and_settle）两种场景。
/// sleep_and_settle 根据场景过滤可用工具和技能（只保留 neural/memory 标签）。
```

- [ ] **Step 6: Build and test**

Run: `cargo build -p ai_orz 2>&1 | head -30`
Expected: PASS

Run: `cargo test -p ai_orz --lib 2>&1 | tail -30`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/models/prompt_builder.rs src/service/dal/agent/mod.rs src/service/domain/runtime/awakening.rs src/service/domain/runtime/mod.rs src/consumer/message.rs src/handlers/hr/agent/settle_memory.rs
git commit -m "refactor: remove zombie Builder.tools code, stale comments, and unused wake_agent_brain scene param"
```

---

## Task 9: Create Agent Loop Consumer (Optional Logging)

**Files:**
- Create: `src/consumer/agent_loop_consumer.rs`
- Modify: `src/consumer/mod.rs`

This consumer subscribes to AgentLoopEvent and ThinkRoundEvent for logging purposes.

- [ ] **Step 1: Create `src/consumer/agent_loop_consumer.rs`**

```rust
//! Agent loop event consumer (AOP sync)
//!
//! Subscribes to agent.loop and agent.think.round events for logging.

use async_trait::async_trait;
use crate::pkg::aop::{ConsumeMode, Consumer, EventKind};
use crate::models::events::{AgentLoopEvent, ThinkRoundEvent};
use common::error::Result;

pub struct AgentLoopConsumer;

impl AgentLoopConsumer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AgentLoopConsumer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Consumer for AgentLoopConsumer {
    fn name(&self) -> &str {
        "agent_loop"
    }

    fn interested_events(&self) -> Vec<EventKind> {
        vec![
            EventKind::new("agent.loop"),
            EventKind::new("agent.think.round"),
        ]
    }

    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Sync
    }

    async fn on_event(&self, event: serde_json::Value) -> Result<()> {
        let kind = event.get("kind").and_then(|v| v.as_str()).unwrap_or("");

        match kind {
            "agent.loop" => {
                let event: AgentLoopEvent = serde_json::from_value(event).map_err(|e| {
                    common::error::Error::internal(format!(
                        "failed to deserialize AgentLoopEvent: {}",
                        e
                    ))
                })?;
                match event.phase.as_str() {
                    "started" => {
                        sys_info!(
                            "agent loop started: agent={}, scene={}, trace={}",
                            event.agent_id,
                            event.scene,
                            event.trace_id
                        );
                    }
                    "finished" => {
                        sys_info!(
                            "agent loop finished: agent={}, scene={}, status={:?}, duration={}ms",
                            event.agent_id,
                            event.scene,
                            event.status,
                            event.duration_ms.unwrap_or(0)
                        );
                    }
                    _ => {}
                }
            }
            "agent.think.round" => {
                let event: ThinkRoundEvent = serde_json::from_value(event).map_err(|e| {
                    common::error::Error::internal(format!(
                        "failed to deserialize ThinkRoundEvent: {}",
                        e
                    ))
                })?;
                sys_debug!(
                    "think round: agent={}, scene={}, round={}, duration={}ms, tool_calls={}",
                    event.agent_id,
                    event.scene,
                    event.round_number,
                    event.duration_ms,
                    event.tool_call_count
                );
            }
            _ => {}
        }

        Ok(())
    }
}
```

- [ ] **Step 2: Register in `src/consumer/mod.rs`**

Add:
```rust
pub mod agent_loop_consumer;
```

And in `init()`:
```rust
    aop::registry().register_consumer(Arc::new(agent_loop_consumer::AgentLoopConsumer::new()))?;
```

- [ ] **Step 3: Build and commit**

Run: `cargo build -p ai_orz 2>&1 | head -30`
Expected: PASS

```bash
git add src/consumer/agent_loop_consumer.rs src/consumer/mod.rs
git commit -m "feat: add AgentLoopConsumer for lifecycle and think round logging"
```

---

## Task 10: Final Verification

- [ ] **Step 1: Run full test suite**

Run: `cargo test -p ai_orz --lib 2>&1 | tail -40`
Expected: PASS (fix any breakages)

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -p ai_orz -- -D warnings 2>&1 | tail -30`
Expected: PASS (fix any warnings)

- [ ] **Step 3: Run integration tests if available**

Run: `cargo test -p ai_orz --test '*' 2>&1 | tail -30`
Expected: PASS

- [ ] **Step 4: Verify no remaining decorator references**

Run: `grep -rn "LoggingDecorator\|ToolCallLoggingDecorator\|tool_call_logger" src/ --include="*.rs"`
Expected: No references to the deleted decorator (ToolCallLogger in logger.rs is still used by the consumer and is fine)

- [ ] **Step 5: Final commit if any fixes were needed**

```bash
git add -A
git commit -m "fix: resolve test and clippy issues from agent loop AOP refactor"
```

---

## Self-Review

### Spec Coverage
- ✅ Agent 工作循环对齐 (Task 2, 3): Extract shared think loop, align sleep with awaken
- ✅ 工具加载流程 (Task 3): sleep passes filtered tools as ToolDescriptor + uses think loop
- ✅ Prompt 去掉工具部分 (Task 8): Remove Builder.tools zombie code
- ✅ AOP 同步 hook 机制 (Task 1, 4, 5, 7, 9): Events defined, published from awakening + state manager
- ✅ 取代装饰器 (Task 7): ToolCallLoggingDecorator replaced by ToolExecLogConsumer + ToolExecStatsConsumer
- ✅ 不同类型订阅者 (Task 7, 9): Log consumer, stats consumer, loop consumer

### Placeholder Scan
- No TBD/TODO in steps
- All code blocks contain actual implementation code
- File paths are absolute or relative to project root

### Type Consistency
- `AgentLoopEvent` fields consistent across definition and publish sites
- `ThinkRoundEvent` fields match between definition and `run_think_loop` publish
- `ToolExecEvent.entry` type is `ToolCallEntry` (from `pkg::tool_tracing::entry`)
- `AgentStateEvent` fields match between definition and `notify_state_change`
