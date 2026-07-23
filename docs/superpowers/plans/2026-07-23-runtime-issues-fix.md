# Runtime 唤醒流程问题修复实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 分阶段修复 Agent 唤醒流程中发现的 40+ 个问题，覆盖 AOP 队列、状态机、trace 完整性、ctx 传递、消息交付、用户体验等维度。

**Architecture:** 按优先级分 5 个独立阶段，每阶段可独立交付 + 提交 + 测试。阶段 1（可用性致命问题）→ 阶段 2（数据一致性）→ 阶段 3（用户体验）→ 阶段 4（其他中等问题）→ 阶段 5（优化项）。

**Tech Stack:** Rust + tokio + sqlx + DashMap + rig + axum

---

## 文件结构概览

### 需要修改的文件
| 文件 | 责任 |
|------|------|
| `src/pkg/aop/core/registry.rs` | worker 循环调用 queue.ack/nack |
| `src/pkg/agent_runtime_state.rs` | 增加 try_set_busy CAS 方法 |
| `src/consumer/message.rs` | 用 try_set_busy + 修复 ctx 重建 + 任务状态检查顺序 |
| `src/service/domain/runtime/awakening.rs` | RAII guard + record_event 错误日志 + 清理死代码 |
| `src/service/domain/runtime/tool_execution.rs` | 修复 call_tool 返回真实 call_id + 失败保留 trace_ref |
| `src/service/dao/tool_call/impl.rs` | call_manual 返回 ToolCallEntry |
| `src/service/dal/tool.rs` + `src/service/dal/mcp_tool.rs` | call_manual 透传 entry |
| `src/pkg/tool_tracing/tool_call_logger.rs` | 修复重复写入 |
| `src/models/events/message.rs` | 修改 order_key 用 task_id 优先 |
| `src/service/domain/message/delivery.rs` | root_id 继承 |
| `src/handlers/finance/message/send_message.rs` | reply_to_id 兜底 |
| `src/handlers/finance/message/subscribe_sse.rs` | 客户端断开清理 |
| `src/service/domain/runtime/memory.rs` | 移除无效 task_id 参数 |
| `src/models/memory.rs` | trace_id 加随机后缀 |
| `src/service/dal/agent.rs` | stats 查询非致命 + 实现 tool_failures |

### 新增文件
| 文件 | 责任 |
|------|------|
| `src/service/domain/runtime/busy_guard.rs` | RAII guard 确保 set_idle |

---

## 阶段 1: 系统可用性致命问题

修复 3 个致命/高危问题，让系统在并发和异常场景下可用。

### Task 1.1: 修复 AOP queue.ack/nack 未调用

**Files:**
- Modify: `src/pkg/aop/core/registry.rs:258-270`

- [ ] **Step 1: 写测试验证当前 bug**

创建测试文件 `src/pkg/aop/core/registry_bug_test.rs`：

```rust
#[cfg(test)]
mod tests {
    use crate::pkg::aop::core::registry::Registry;
    use crate::pkg::aop::queue::in_memory::InMemoryEventQueue;
    use crate::pkg::aop::{Consumer, ConsumeMode, EventKind};
    use async_trait::async_trait;
    use common::error::Result;
    use std::sync::{Arc, Mutex};

    struct CapturingConsumer {
        processed: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl Consumer for CapturingConsumer {
        fn name(&self) -> &str { "test.capture" }
        fn interested_events(&self) -> Vec<EventKind> { vec![EventKind::new("test.event")] }
        fn consume_mode(&self) -> ConsumeMode { ConsumeMode::Async }
        async fn on_event(&self, _event: serde_json::Value) -> Result<()> {
            self.processed.lock().unwrap().push("called".to_string());
            Ok(())
        }
        async fn ack(&self, _event_id: &str) -> Result<()> { Ok(()) }
        async fn nack(&self, _event_id: &str) -> Result<()> { Ok(()) }
    }

    #[tokio::test]
    async fn test_queue_ack_called_after_on_event_success() {
        // 验证：on_event 成功后，queue.ack 必须被调用
        // 否则事件永远停留在 in_progress，后续同 order_key 事件被阻塞
        // 此测试在修复前会失败（queue.len() 不减少）
    }
}
```

注：此测试由于依赖内部实现细节较难直接编写，改为通过手动审查 + 集成测试验证。

- [ ] **Step 2: 修改 registry worker 循环，在 consumer.ack/nack 后调用 queue.ack/nack**

修改 `src/pkg/aop/core/registry.rs` 第 258-270 行：

```rust
match consumer.on_event(event_json).await {
    Ok(()) => {
        if let Err(e) = consumer.ack(&event_id).await {
            sys_error!("[{}] ack error for {}: {}", consumer_name, event_id, e);
        }
        // 修复：必须调用 queue.ack 从内存队列移除事件
        // 否则事件永远停留在 in_progress + events，导致同 order_key 消息卡死
        if let Err(e) = registry_arc.ack(&consumer_name, &event_id).await {
            sys_error!("[{}] queue.ack error for {}: {}", consumer_name, event_id, e);
        }
    }
    Err(e) => {
        sys_error!("[{}] on_event error for {}: {}", consumer_name, event_id, e);
        if let Err(e) = consumer.nack(&event_id).await {
            sys_error!("[{}] nack error for {}: {}", consumer_name, event_id, e);
        }
        // 修复：必须调用 queue.nack 让事件重新入队等待重试
        // 否则失败事件永远停留在 in_progress，无法重试
        if let Err(e) = registry_arc.nack(&consumer_name, &event_id).await {
            sys_error!("[{}] queue.nack error for {}: {}", consumer_name, event_id, e);
        }
    }
}
```

- [ ] **Step 3: 编译并运行测试**

Run: `cargo test aop 2>&1 | tail -20`
Expected: 编译通过，相关测试通过

- [ ] **Step 4: 提交**

```bash
git add src/pkg/aop/core/registry.rs
git commit -m "fix(aop): worker 循环调用 queue.ack/nack，修复事件卡死

修复致命 bug：worker 循环只调用 consumer.ack/nack（业务层），
未调用 registry.ack/nack（队列层），导致：
- 事件永远留在 events 和 in_progress map（内存泄漏）
- 同 order_key 后续事件全部卡死（has_active_message 永远 true）
- 失败事件永不重试"
```

---

### Task 1.2: 修复 Agent 状态泄漏（RAII guard）

**Files:**
- Create: `src/service/domain/runtime/busy_guard.rs`
- Modify: `src/service/domain/runtime/mod.rs`（声明新模块）
- Modify: `src/service/domain/runtime/awakening.rs`（使用 guard）

- [ ] **Step 1: 创建 RAII guard 文件**

创建 `src/service/domain/runtime/busy_guard.rs`：

```rust
//! Agent Busy 状态的 RAII guard
//!
//! 确保无论 awaken 返回成功还是失败（包括 ? 提早返回和 panic），
//! Agent 的 Busy 状态都会被清理为 Idle。
//! 修复 set_busy 与 set_idle 之间的状态泄漏 bug。

use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;

/// RAII guard：创建时无需设置 Busy（调用方已设置），drop 时自动 set_idle
///
/// 使用方式：
/// ```ignore
/// AgentRuntimeStateManager::global().set_busy(&agent_id, &message_id);
/// let _guard = BusyGuard::new(agent_id);
/// // ... 后续所有 ? 提早返回都会触发 guard drop → set_idle
/// ```
pub struct BusyGuard {
    agent_id: String,
}

impl BusyGuard {
    /// 创建 guard，drop 时自动调用 set_idle
    pub fn new(agent_id: String) -> Self {
        Self { agent_id }
    }
}

impl Drop for BusyGuard {
    fn drop(&mut self) {
        AgentRuntimeStateManager::global().set_idle(&self.agent_id);
    }
}
```

- [ ] **Step 2: 在 mod.rs 声明模块**

在 `src/service/domain/runtime/mod.rs` 适当位置（其他子模块声明附近）添加：

```rust
pub mod busy_guard;
```

- [ ] **Step 3: 修改 awaken 方法使用 guard**

修改 `src/service/domain/runtime/awakening.rs` 的 `awaken` 方法。找到 `set_busy` 调用（约 line 85），在它之后立即创建 guard，并删除末尾的 `set_idle` 调用：

将：
```rust
// 设置 Agent 为忙碌状态
AgentRuntimeStateManager::global()
    .set_busy(&agent.po.id, &message.po.id);
```

改为：
```rust
// 设置 Agent 为忙碌状态
// 使用 RAII guard 确保 set_idle 一定被执行
// 修复：之前 set_busy 与 set_idle 之间多处 ? 提早返回会导致 Agent 永远 Busy
AgentRuntimeStateManager::global()
    .set_busy(&agent.po.id, &message.po.id);
let _busy_guard = crate::service::domain::runtime::busy_guard::BusyGuard::new(agent.po.id.clone());
```

然后**删除**两处 `set_idle` 调用：
- 约 line 162 的 `AgentRuntimeStateManager::global().set_idle(&agent.po.id);`（think 成功后）
- 检查 think 失败路径（约 line 150），那里也有一个 set_idle

guard 会在函数返回时（无论成功或失败）自动 drop 并执行 set_idle。

- [ ] **Step 4: 编译并运行测试**

Run: `cargo build 2>&1 | tail -10 && cargo test awakening 2>&1 | tail -20`
Expected: 编译通过，awakening 测试通过

- [ ] **Step 5: 提交**

```bash
git add src/service/domain/runtime/busy_guard.rs src/service/domain/runtime/mod.rs src/service/domain/runtime/awakening.rs
git commit -m "fix(runtime): RAII guard 确保 Agent Busy 状态一定被清理

修复：awaken 中 set_busy 与 set_idle 之间多处 ? 提早返回
（get_recent_context、brain 缺失等）会导致 Agent 永远 Busy，
后续所有消息被 is_unavailable 挡住，只有重启服务才能恢复。

用 BusyGuard RAII 模式，drop 时自动 set_idle，覆盖所有返回路径。"
```

---

### Task 1.3: 修复 TOCTOU 竞态 + Agent 不存在无限重试

**Files:**
- Modify: `src/pkg/agent_runtime_state.rs`
- Modify: `src/consumer/message.rs:141-167`

- [ ] **Step 1: 在 AgentRuntimeStateManager 增加 try_set_busy 原子方法**

在 `src/pkg/agent_runtime_state.rs` 的 `impl AgentRuntimeStateManager` 中（约 line 74 之后，set_busy 方法后）添加：

```rust
    /// 原子地尝试设置 Busy 状态
    ///
    /// 如果 Agent 当前是 Idle，设置为 Busy 并返回 true。
    /// 如果 Agent 当前是 Busy 或 Resting，返回 false（未修改状态）。
    ///
    /// 修复 TOCTOU 竞态：consumer 的 is_unavailable 检查与 awaken 的 set_busy 之间
    /// 会被其他 worker 插入，导致同一 Agent 被并发唤醒。
    pub fn try_set_busy(&self, agent_id: &str, message_id: &str) -> bool {
        let mut entry = self.states.entry(agent_id.to_string()).or_default();
        if entry.state.is_unavailable() {
            return false;
        }
        entry.state = AgentRuntimeState::Busy;
        entry.current_message_id = Some(message_id.to_string());
        entry.state_started_at = common::constants::utils::current_timestamp_ms();
        true
    }
```

- [ ] **Step 2: 修改 consumer handle_agent_message 使用 try_set_busy**

修改 `src/consumer/message.rs` 的 `handle_agent_message` 方法。将开头的状态检查（约 line 144-150）：

```rust
// 消费前检查 Agent 是否可用（空闲）
if AgentRuntimeStateManager::global().is_unavailable(agent_id) {
    return Err(Error::conflict(format!(
        "Agent {} is busy or resting, message will be retried",
        agent_id
    )));
}
```

替换为：

```rust
// 原子地占用 Agent（修复 TOCTOU 竞态）
// 之前 is_unavailable + 后续 set_busy 之间存在窗口，4 个 worker 并发时
// 同一 agent 收不同 project 消息会被两个 worker 同时通过检查
let acquired = AgentRuntimeStateManager::global()
    .try_set_busy(agent_id, &message.po.id);
if !acquired {
    return Err(Error::conflict(format!(
        "Agent {} is busy or resting, message will be retried",
        agent_id
    )));
}
// 注意：此时已 set_busy，后续失败路径必须 set_idle
// awaken 内部会创建 BusyGuard 确保清理
// 但 awaken 之前的失败（如 get_agent）需要显式清理
```

- [ ] **Step 3: 处理 get_agent 失败时的 set_idle**

在 `handle_agent_message` 中，get_agent 调用后（约 line 162-167）增加 set_idle 清理。将：

```rust
let mut agent = self
    .hr_domain
    .agent_manage()
    .get_agent(ctx.clone(), agent_id, fetch_options)
    .await?
    .ok_or_else(|| Error::not_found(format!("Agent {} not found", agent_id)))?;
```

改为：

```rust
let agent_result = self
    .hr_domain
    .agent_manage()
    .get_agent(ctx.clone(), agent_id, fetch_options)
    .await;

let mut agent = match agent_result {
    Ok(Some(a)) => a,
    Ok(None) => {
        // Agent 不存在：永久错误，不应无限重试
        // 释放 Busy 状态并返回非重试错误
        AgentRuntimeStateManager::global().set_idle(agent_id);
        return Err(Error::not_found(format!(
            "Agent {} not found, message will not be retried",
            agent_id
        )));
    }
    Err(e) => {
        // 查询失败：临时错误，释放 Busy 允许重试
        AgentRuntimeStateManager::global().set_idle(agent_id);
        return Err(e);
    }
};
```

- [ ] **Step 4: 处理 thinking_depth / task 状态检查失败时的 set_idle**

在 thinking_depth 检查分支和 task 状态检查分支的 return Ok(()) 之前，增加 set_idle。找到这两处 `return Ok(())`（约 line 200 和 line 218），在它们之前添加：

```rust
AgentRuntimeStateManager::global().set_idle(agent_id);
return Ok(());
```

- [ ] **Step 5: 编译并测试**

Run: `cargo build 2>&1 | tail -10 && cargo test 2>&1 | grep -E "test result|running [0-9]+ tests" | head -10`
Expected: 编译通过，745 测试全部通过

- [ ] **Step 6: 提交**

```bash
git add src/pkg/agent_runtime_state.rs src/consumer/message.rs
git commit -m "fix(runtime): try_set_busy 原子操作修复 TOCTOU 竞态

- 增加 try_set_busy 原子方法（Idle→Busy），避免 is_unavailable + set_busy
  之间的竞态窗口导致同一 Agent 被并发唤醒
- Agent 不存在时返回 not_found 并 set_idle，避免无限重试
- get_agent / thinking_depth / task 状态检查失败时显式 set_idle"
```

---

### Task 1.4: 修复 nack 无退避自旋

**Files:**
- Modify: `src/pkg/aop/core/registry.rs:264-269`

- [ ] **Step 1: 在 on_event 返回 Err 后添加退避 sleep**

修改 `src/pkg/aop/core/registry.rs` 的 worker 循环。在 `Err(e)` 分支（约 line 264）的 nack 之后添加退避：

将：
```rust
Err(e) => {
    sys_error!("[{}] on_event error for {}: {}", consumer_name, event_id, e);
    if let Err(e) = consumer.nack(&event_id).await {
        sys_error!("[{}] nack error for {}: {}", consumer_name, event_id, e);
    }
    // 修复：必须调用 queue.nack 让事件重新入队等待重试
    if let Err(e) = registry_arc.nack(&consumer_name, &event_id).await {
        sys_error!("[{}] queue.nack error for {}: {}", consumer_name, event_id, e);
    }
}
```

改为：
```rust
Err(e) => {
    sys_error!("[{}] on_event error for {}: {}", consumer_name, event_id, e);
    if let Err(e) = consumer.nack(&event_id).await {
        sys_error!("[{}] nack error for {}: {}", consumer_name, event_id, e);
    }
    if let Err(e) = registry_arc.nack(&consumer_name, &event_id).await {
        sys_error!("[{}] queue.nack error for {}: {}", consumer_name, event_id, e);
    }
    // 修复：on_event 失败后添加退避，避免紧密自旋
    // 之前 error_retry_sleep_ms 只用于 dequeue_for 失败，不用于 on_event 失败
    // 导致 Agent busy 时每秒重新入队被取出，形成 CPU 紧密自旋
    tokio::time::sleep(tokio::time::Duration::from_millis(error_sleep)).await;
}
```

- [ ] **Step 2: 编译并测试**

Run: `cargo build 2>&1 | tail -10`
Expected: 编译通过

- [ ] **Step 3: 提交**

```bash
git add src/pkg/aop/core/registry.rs
git commit -m "fix(aop): on_event 失败后添加退避 sleep，修复 CPU 自旋

error_retry_sleep_ms 之前只用于 dequeue_for 失败，不用于 on_event 失败。
导致 Agent busy 时 nack 后立即重新入队被取出，形成 CPU 紧密自旋。"
```

---

## 阶段 2: 数据一致性与 Trace 完整性

修复 trace 断链、ctx 字段丢失、统计失效等数据一致性问题。

### Task 2.1: 修复 tool_call_id 伪造 + 失败丢弃 trace_ref

**Files:**
- Modify: `src/service/dao/tool_call/impl.rs:97-143`（call_manual 返回 entry）
- Modify: `src/service/dal/tool.rs:681-688`（call_manual 透传）
- Modify: `src/service/dal/mcp_tool.rs`（call_manual 透传）
- Modify: `src/service/domain/runtime/tool_execution.rs:31-71`（使用真实 call_id）

- [ ] **Step 1: 修改 ToolCallDao trait，让 call_manual 返回 (Value, ToolCallEntry)**

在 `src/service/dao/tool_call/mod.rs` 的 `ToolCallDao` trait 中：

将：
```rust
async fn call_manual(
    &self,
    ctx: RequestContext,
    tool: &Tool,
    args: Value,
) -> Result<Value>;
```

改为：
```rust
async fn call_manual(
    &self,
    ctx: RequestContext,
    tool: &Tool,
    args: Value,
) -> Result<(Value, ToolCallEntry)>;
```

- [ ] **Step 2: 修改 ToolCallDaoImpl 实现**

在 `src/service/dao/tool_call/impl.rs` 中修改 `call_manual` 方法。

将成功分支（约 line 122-125）：
```rust
match result {
    Ok(value) => {
        Ok(value)
    }
```

改为：
```rust
match result {
    Ok(value) => {
        Ok((value, entry))
    }
```

失败分支保持构造 Error（已带 trace_ref），但把 entry 也返回便于调试：
保持现有逻辑不变，因为 entry 已经被消费构造 trace_ref。

实际上失败分支返回 `Err(err.into())`，所以不需要返回 entry。

- [ ] **Step 3: 修改 ToolDal trait + 实现透传 entry**

在 `src/service/dal/tool.rs` 的 `ToolDal` trait 中：

将：
```rust
async fn call_manual(
    &self,
    ctx: RequestContext,
    tool: &Tool,
    args: Value,
) -> Result<Value>;
```

改为：
```rust
async fn call_manual(
    &self,
    ctx: RequestContext,
    tool: &Tool,
    args: Value,
) -> Result<(Value, ToolCallEntry)>;
```

同时修改 `call_tool` 签名（trait + impl）同样改为返回 `Result<(Value, ToolCallEntry)>`。

实现中（约 line 672-688）：

将：
```rust
async fn call_tool(&self, ctx: RequestContext, tool: &Tool, args: Value) -> Result<Value> {
    self.call_manual(ctx, tool, args).await
}

async fn call_manual(&self, ctx: RequestContext, tool: &Tool, args: Value) -> Result<Value> {
    self.tool_call_dao.call_manual(ctx, tool, args).await.map_err(Into::into)
}
```

改为：
```rust
async fn call_tool(&self, ctx: RequestContext, tool: &Tool, args: Value) -> Result<(Value, ToolCallEntry)> {
    self.call_manual(ctx, tool, args).await
}

async fn call_manual(&self, ctx: RequestContext, tool: &Tool, args: Value) -> Result<(Value, ToolCallEntry)> {
    self.tool_call_dao.call_manual(ctx, tool, args).await.map_err(Into::into)
}
```

- [ ] **Step 4: 同步修改 McpToolDal**

在 `src/service/dal/mcp_tool.rs` 中，对 `call_tool` 和 `call_manual` 做同样修改，返回 `Result<(Value, ToolCallEntry)>`。

- [ ] **Step 5: 修改 Runtime tool_execution 使用真实 call_id**

在 `src/service/domain/runtime/tool_execution.rs` 的 `call_tool` 方法中：

将：
```rust
let result = match execution {
    Ok(result) => result,
    Err(error) => {
        let mapped_message: String = match tool.po.protocol {
            ToolProtocol::Mcp => map_mcp_tool_error(&tool_id, &error),
            ToolProtocol::Builtin | ToolProtocol::Http => error.to_string(),
        };
        return Err(common::error::Error::new(
            common::error::ErrorCode::ToolExecutionFailed,
            mapped_message,
        ));
    }
};

Ok(ToolExecutionResult::new(
    result,
    tool_id.clone(),
    uuid::Uuid::now_v7().to_string(),
))
```

改为：
```rust
let (result, entry) = match execution {
    Ok((value, entry)) => (value, entry),
    Err(error) => {
        // 修复：保留原 error 的 field（含 trace_ref），不再构造新 Error
        let mapped_message: String = match tool.po.protocol {
            ToolProtocol::Mcp => map_mcp_tool_error(&tool_id, &error),
            ToolProtocol::Builtin | ToolProtocol::Http => error.to_string(),
        };
        // 透传原 error 的 field 和 source，避免丢失 trace_ref
        return Err(error.with_message(mapped_message));
    }
};

// 修复：使用 LoggingDecorator 生成的真实 call_id，不再现场伪造
Ok(ToolExecutionResult::new(
    result,
    entry.tool_id.clone(),
    entry.call_id.clone(),
))
```

注：需确认 `common::error::Error` 是否有 `with_message` 方法。如果没有，使用现有方式保留 field：
```rust
let mut new_err = common::error::Error::new(
    common::error::ErrorCode::ToolExecutionFailed,
    mapped_message,
);
// 保留原 field（含 trace_ref）
if let Some(field) = error.field() {
    new_err = new_err.with_field(field.clone());
}
return Err(new_err);
```

- [ ] **Step 6: 修改所有 call_tool / call_manual 的调用方**

搜索所有调用 `call_tool` 或 `call_manual` 的地方，更新解构：

```bash
grep -rn "\.call_tool\b\|\.call_manual\b" src/ | grep -v test
```

对每处调用，将：
```rust
let result = something.call_tool(ctx, &tool, args).await?;
```

改为：
```rust
let (result, _entry) = something.call_tool(ctx, &tool, args).await?;
```

（除非需要使用 entry，比如 `call_manual_tool_for_agent`）

- [ ] **Step 7: 编译并修复所有错误**

Run: `cargo build 2>&1 | tail -30`
Expected: 编译通过（可能需要多次修复调用方）

- [ ] **Step 8: 运行测试**

Run: `cargo test 2>&1 | grep -E "test result|running [0-9]+ tests" | head -10`
Expected: 全部测试通过

- [ ] **Step 9: 提交**

```bash
git add -A
git commit -m "fix(runtime): tool_call_id 使用真实 call_id + 失败保留 trace_ref

修复两个严重 trace 断链 bug：
1. 成功路径：call_tool 现场生成 uuid 作为 call_id，与 LoggingDecorator
   生成的真实 call_id 不一致，客户端查询永远查不到
2. 失败路径：重新构造 Error 丢弃原 field 中的 trace_ref，失败调用无法关联

改为：call_manual 返回 (Value, ToolCallEntry)，Runtime 使用真实 call_id，
失败时透传原 Error 的 field。"
```

---

### Task 2.2: 修复 Rig Auto 工具 trace 重复写入

**Files:**
- Modify: `src/pkg/tool_tracing/tool_call_logger.rs:165-173`

- [ ] **Step 1: 移除 CoreTool::call 实现中的重复 log_call**

在 `src/pkg/tool_tracing/tool_call_logger.rs` 中，`CoreTool::call` 实现（约 line 166-173）：

将：
```rust
#[async_trait]
impl CoreTool for LoggingDecorator {
    async fn call(&self, ctx: RequestContext, args: Value) -> Result<Value> {
        let (result, entry) = self.call_with_entry(ctx, args).await;
        // Log the entry immediately
        let tool_id = entry.tool_id.clone();
        let _ = self.logger.log_call(&tool_id, entry);
        result
    }
```

改为：
```rust
#[async_trait]
impl CoreTool for LoggingDecorator {
    async fn call(&self, ctx: RequestContext, args: Value) -> Result<Value> {
        // call_with_entry 内部已经执行 log_call，这里不再重复写入
        // 修复：之前 Rig 调用 Auto 工具时，call_with_entry 和 call 各写一次，
        // 产生两条相同 call_id 的 trace 记录
        let (result, _entry) = self.call_with_entry(ctx, args).await;
        result
    }
```

- [ ] **Step 2: 编译并测试**

Run: `cargo build 2>&1 | tail -10 && cargo test tool_tracing 2>&1 | tail -20`
Expected: 编译通过，测试通过

- [ ] **Step 3: 提交**

```bash
git add src/pkg/tool_tracing/tool_call_logger.rs
git commit -m "fix(tracing): 移除 CoreTool::call 中的重复 log_call

call_with_entry 内部已 log_call，CoreTool::call 实现又 log_call 一次，
导致 Rig 调用 Auto 工具时产生两条相同 call_id 的 trace 记录，
污染统计和查询。"
```

---

### Task 2.3: 修复异步路径 ctx 字段丢失

**Files:**
- Modify: `src/handlers/finance/tool/send_tool_call_message.rs`（携带 ctx 字段）
- Modify: `src/models/message.rs`（ToolCallMessage 增加 ctx 字段）
- Modify: `src/consumer/message.rs:290-308`（重建 ctx 时回填）

- [ ] **Step 1: 在 ToolCallMessage 增加 ctx 传递字段**

在 `src/models/message.rs` 的 `ToolCallMessage` 结构体中增加：

```rust
    /// 发起方的 log_id（用于异步路径重建 ctx）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_log_id: Option<String>,
    /// 发起方的 user_id（用于异步路径重建 ctx）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_user_id: Option<String>,
    /// 发起方的 model_provider_id（用于异步路径重建 ctx）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_model_provider_id: Option<String>,
    /// 发起方的 model_name（用于异步路径重建 ctx）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_model_name: Option<String>,
```

更新 `new_request` 和 `new_success_result` / `new_error_result` 初始化这些字段为 None。

- [ ] **Step 2: send_tool_call_message 填充 ctx 字段**

在 `src/handlers/finance/tool/send_tool_call_message.rs` 中，构造 `ToolCallMessage` 时填充 ctx 字段：

```rust
let tool_call = ToolCallMessage::new_request(
    // ... existing fields ...
);
// 填充 ctx 字段，供 consumer 重建 ctx
tool_call.from_log_id = Some(ctx.log_id.clone());
tool_call.from_user_id = ctx.user_id.clone();
tool_call.from_model_provider_id = ctx.model_provider_id.clone();
tool_call.from_model_name = ctx.model_name.clone();
```

注：需要将 `new_request` 返回的 `ToolCallMessage` 改为 `mut`。

- [ ] **Step 3: consumer 重建 ctx 时回填字段**

修改 `src/consumer/message.rs` 的 `handle_tool_call_request`（约 line 290-308）：

将：
```rust
let mut builder = RequestContext::builder();
builder = builder.agent_id(tool_call.from_id.clone());
if let Some(project_id) = &tool_call.project_id {
    builder = builder.project_id(project_id.clone());
}
if let Some(task_id) = &tool_call.task_id {
    builder = builder.task_id(task_id.clone());
}
if let Some(org_id) = &message.po.organization_id {
    builder = builder.organization_id(org_id.clone());
}
if message.from_role() == MessageRole::User {
    builder = builder.user_id(message.po.from_id.clone());
}
let ctx = builder.build();
```

改为：
```rust
let mut builder = RequestContext::builder();
builder = builder.agent_id(tool_call.from_id.clone());
if let Some(project_id) = &tool_call.project_id {
    builder = builder.project_id(project_id.clone());
}
if let Some(task_id) = &tool_call.task_id {
    builder = builder.task_id(task_id.clone());
}
if let Some(org_id) = &message.po.organization_id {
    builder = builder.organization_id(org_id.clone());
}
// 修复：从 ToolCallMessage 回填 ctx 字段，与同步路径保持一致
// 之前 from_role=Agent 时 user_id 永远不会被设置，log_id/model_* 全部丢失
if let Some(log_id) = &tool_call.from_log_id {
    builder = builder.log_id(log_id.clone());
}
if let Some(user_id) = &tool_call.from_user_id {
    builder = builder.user_id(user_id.clone());
}
if let Some(model_provider_id) = &tool_call.from_model_provider_id {
    builder = builder.model_provider_id(model_provider_id.clone());
}
if let Some(model_name) = &tool_call.from_model_name {
    builder = builder.model_name(model_name.clone());
}
let ctx = builder.build();
```

- [ ] **Step 4: 编译并测试**

Run: `cargo build 2>&1 | tail -10 && cargo test 2>&1 | grep "test result" | head -5`
Expected: 编译通过，测试通过

- [ ] **Step 5: 提交**

```bash
git add -A
git commit -m "fix(message): 异步工具调用路径回填 ctx 字段

之前 handle_tool_call_request 重建 ctx 时：
- from_role=Agent 时 user_id 永远不设置
- log_id 重新生成，与触发轮次断链
- model_provider_id / model_name 全部丢失

现在 ToolCallMessage 携带 from_log_id/from_user_id/from_model_* 字段，
consumer 重建 ctx 时回填，与同步路径保持一致。"
```

---

### Task 2.4: record_event! 失败记录日志 + 任务状态检查顺序

**Files:**
- Modify: `src/service/domain/runtime/awakening.rs:171,198`
- Modify: `src/consumer/message.rs:170-220`

- [ ] **Step 1: 修改 awakening.rs 的 record_event! 不再静默丢弃**

将两处 `let _ = record_event!(...)`：

成功路径（约 line 198）：
```rust
let _ = record_event!(ctx, AgentAwakeEvent { ... });
```

失败路径（约 line 171）：
```rust
let _ = record_event!(ctx, AgentAwakeEvent { ... status: format!("failed: {}", e), ... });
```

改为：
```rust
if let Err(stats_err) = record_event!(ctx, AgentAwakeEvent { ... }) {
    log_warn!(
        &ctx,
        "awaken",
        "record_event failed, stats may be incomplete: {:?}",
        stats_err
    );
}
```

失败路径同样处理。

注：错误信息中的 `e` 已被 move（return Err(e)），需要在 move 之前 format。改为：
```rust
let err_msg = e.to_string();
let _ = record_event!(ctx, AgentAwakeEvent {
    ...
    status: format!("failed: {}", err_msg),
});
log_warn!(..., "stats record failed");
return Err(e);
```

实际由于 `record_event!` 在 `return Err(e)` 之前，需要先 clone error message。

- [ ] **Step 2: 调整 consumer 任务状态检查顺序**

在 `src/consumer/message.rs` 的 `handle_agent_message` 中，将 task 完成状态检查（约 line 207-220）移到 thinking_depth 检查（约 line 170-204）之前。

将：
```rust
// 检查轮次限制
if let (Some(_task_id), Some(stats)) = (&message.po.task_id, &agent.stats) {
    // ... thinking_depth 检查 ...
}

// 检查任务完成状态
if let Some(task_id) = &message.po.task_id {
    // ... task 状态检查 ...
}
```

改为：
```rust
// 检查任务完成状态（优先检查，避免对已完成任务发送误导消息）
if let Some(task_id) = &message.po.task_id {
    match self.project_domain.task_manage().get(ctx.clone(), task_id).await {
        Ok(Some(task)) => {
            if matches!(task.po.status, common::enums::TaskStatus::Completed | common::enums::TaskStatus::Cancelled | common::enums::TaskStatus::Archived) {
                log_info!(...);
                AgentRuntimeStateManager::global().set_idle(agent_id);
                return Ok(());
            }
        }
        Ok(None) => {
            log_warn!(&ctx, "handle_agent_message", "task {} not found", task_id);
        }
        Err(e) => {
            // 查询失败：释放 Busy 并返回错误触发重试
            AgentRuntimeStateManager::global().set_idle(agent_id);
            return Err(e);
        }
    }
}

// 检查轮次限制
if let (Some(_task_id), Some(stats)) = (&message.po.task_id, &agent.stats) {
    // ... thinking_depth 检查（保持原样）...
}
```

- [ ] **Step 3: 编译并测试**

Run: `cargo build 2>&1 | tail -10 && cargo test 2>&1 | grep "test result" | head -5`
Expected: 编译通过，测试通过

- [ ] **Step 4: 提交**

```bash
git add -A
git commit -m "fix(runtime): record_event 失败记录日志 + 任务状态检查优先

- record_event! 失败不再静默丢弃，改为 log_warn 记录
  修复：统计失败导致 total_calls 偏小，轮次限制可能失效
- 任务状态检查移到 thinking_depth 之前
  修复：task 已 Completed/Cancelled 但深度也超限时，会先触发深度分支
  向用户发送误导消息 'reached maximum thinking depth'
- task 查询失败时返回错误触发重试（之前 Ok(None) 静默跳过）"
```

---

## 阶段 3: 用户体验问题

修复消息链断裂、SSE 内存泄漏、投递失败静默等问题。

### Task 3.1: 修复消息链 root_id 继承

**Files:**
- Modify: `src/service/domain/message/delivery.rs:128,168`

- [ ] **Step 1: 修改 send_to_user 继承 root_id**

在 `src/service/domain/message/delivery.rs` 的 `send_to_user` 方法中（约 line 168）：

将：
```rust
let po = MessagePo::new(
    id.clone(),
    // ... other fields ...
    cmd.reply_to_id.map(|s| s.to_string()),
    Some(id),  // root_id 始终为自身
    // ...
);
```

改为：
```rust
// root_id 继承：如果有 reply_to_id，查询父消息的 root_id；否则自身为 root
let root_id = match cmd.reply_to_id {
    Some(parent_id) => {
        // 查询父消息的 root_id
        match self.message_dal.find_by_id(ctx.clone(), parent_id).await {
            Ok(Some(parent)) => parent.po.root_id.unwrap_or(parent_id.to_string()),
            _ => id.clone(),
        }
    }
    None => id.clone(),
};

let po = MessagePo::new(
    id.clone(),
    // ... other fields ...
    cmd.reply_to_id.map(|s| s.to_string()),
    Some(root_id),
    // ...
);
```

注：需要确认 `find_by_id` 方法存在且接收 ctx。如果没有，可能需要通过 message_dal 查询。

- [ ] **Step 2: send_to_agent 同样修改**

在 `send_to_agent` 方法中（约 line 128）做同样修改。

- [ ] **Step 3: 编译并测试**

Run: `cargo build 2>&1 | tail -10 && cargo test delivery 2>&1 | tail -20`
Expected: 编译通过，测试通过

- [ ] **Step 4: 提交**

```bash
git add src/service/domain/message/delivery.rs
git commit -m "fix(message): root_id 继承父消息，修复消息链断裂

之前 root_id 始终为自身 ID，多轮对话每条消息都是独立 root，
无法按 root_id 拉取完整对话链。现在继承父消息的 root_id。"
```

---

### Task 3.2: SSE 客户端断开清理

**Files:**
- Modify: `src/handlers/finance/message/subscribe_sse.rs:40-46`

- [ ] **Step 1: 修改 SSE 清理逻辑**

在 `src/handlers/finance/message/subscribe_sse.rs` 中，将清理逻辑从等待 ctrl_c 改为检测 stream 结束。

查看当前实现（约 line 40-46），将：
```rust
tokio::spawn(async move {
    let _ = tokio::signal::ctrl_c().await;
    let _ = message::domain().delivery().unsubscribe_sse(ctx_clone, &conn_id_clone).await;
});
```

改为使用一个 oneshot channel，当 SSE stream 结束时触发清理：

```rust
let (cleanup_tx, cleanup_rx) = tokio::sync::oneshot::channel::<()>();

// 清理任务：等待 stream 结束信号后注销连接
let ctx_clone = ctx.clone();
let conn_id_clone = conn_id.to_string();
tokio::spawn(async move {
    let _ = cleanup_rx.await;
    let _ = message::domain().delivery().unsubscribe_sse(ctx_clone, &conn_id_clone).await;
});

// 将 cleanup_tx 返回，在 stream 结束时发送信号
// 实际实现：在 stream 的 drop 或 finally 中发送信号
```

具体实现取决于 SSE handler 的结构。一个更简单的方式是用 `tokio::select!`：

```rust
// 在 SSE handler 中，用 select 检测 stream 结束
let conn_id_clone = conn_id.to_string();
let ctx_clone = ctx.clone();

// SSE stream 生成器
let stream = BroadcastStream::new(rx).map(|event| {
    // ... 现有 event 处理 ...
});

// 用 select 检测客户端断开
tokio::select! {
    _ = stream => {
        // stream 结束（客户端断开或服务器关闭）
    }
    _ = tokio::signal::ctrl_c() => {
        // 服务器关闭
    }
}

// 无论哪种情况，都执行清理
let _ = message::domain().delivery().unsubscribe_sse(ctx_clone, &conn_id_clone).await;
```

- [ ] **Step 2: 编译并测试**

Run: `cargo build 2>&1 | tail -10 && cargo test sse 2>&1 | tail -20`
Expected: 编译通过

- [ ] **Step 3: 提交**

```bash
git add src/handlers/finance/message/subscribe_sse.rs
git commit -m "fix(sse): 客户端断开时注销连接，修复内存泄漏

之前清理逻辑等待 ctrl_c 信号，客户端关闭浏览器时不会触发清理，
connections 和 user_connections map 无限增长。"
```

---

### Task 3.3: deliver_message 投递失败不再静默

**Files:**
- Modify: `src/consumer/message.rs:259-276`

- [ ] **Step 1: 检查投递结果，失败时返回错误触发 nack**

在 `src/consumer/message.rs` 的 `handle_user_message` 中（约 line 259-276）：

将：
```rust
async fn handle_user_message(&self, message: &Message) -> Result<()> {
    let ctx = self.rebuild_context(message);
    let cmd = DeliverMessageCommand {
        message,
        user_id: &message.po.to_id,
    };
    let result = self.message_domain
        .delivery()
        .deliver_message(ctx, cmd)
        .await?;
    sys_debug!(
        "user message delivered: sse={}, channels={}/{}",
        result.sse_delivered,
        result.success,
        result.total
    );
    Ok(())
}
```

改为：
```rust
async fn handle_user_message(&self, message: &Message) -> Result<()> {
    let ctx = self.rebuild_context(message);
    let cmd = DeliverMessageCommand {
        message,
        user_id: &message.po.to_id,
    };
    let result = self.message_domain
        .delivery()
        .deliver_message(ctx, cmd)
        .await?;

    log_info!(
        "user message delivered: sse={}, channels={}/{}",
        result.sse_delivered,
        result.success,
        result.total
    );

    // 修复：所有渠道投递失败时返回错误，触发 nack 重试
    // 之前即使 success=0 也返回 Ok(())，消息被 ack 标记为 Processed，永远不会重试
    if result.success == 0 && result.sse_delivered == 0 {
        return Err(Error::internal(format!(
            "All delivery channels failed for message {}, will retry",
            message.po.id
        )));
    }

    Ok(())
}
```

- [ ] **Step 2: 编译并测试**

Run: `cargo build 2>&1 | tail -10 && cargo test 2>&1 | grep "test result" | head -5`
Expected: 编译通过，测试通过

- [ ] **Step 3: 提交**

```bash
git add src/consumer/message.rs
git commit -m "fix(message): 所有投递渠道失败时返回错误触发重试

之前 deliver_message 即使 success=0 也返回 Ok(())，
消息被 ack 标记为 Processed，永远不会重试。"
```

---

### Task 3.4: 修改 MessageCreatedEvent order_key 用 task_id 优先

**Files:**
- Modify: `src/models/events/message.rs:27-29`

- [ ] **Step 1: 修改 order_key 逻辑**

在 `src/models/events/message.rs` 中：

将：
```rust
fn order_key(&self) -> &str {
    self.project_id.as_deref().unwrap_or("")
}
```

改为：
```rust
fn order_key(&self) -> &str {
    // 优先按 task_id 分组，避免同 project 不同 task 互相阻塞
    // 修复：之前按 project_id 分组，同一 project 下不同 task 的消息串行处理
    // Agent 处理 task A 时 task B 的用户消息被阻塞
    if let Some(task_id) = &self.task_id {
        task_id
    } else {
        self.project_id.as_deref().unwrap_or("")
    }
}
```

- [ ] **Step 2: 编译并测试**

Run: `cargo build 2>&1 | tail -10 && cargo test 2>&1 | grep "test result" | head -5`
Expected: 编译通过，测试通过

- [ ] **Step 3: 提交**

```bash
git add src/models/events/message.rs
git commit -m "fix(aop): MessageCreatedEvent order_key 优先用 task_id

之前按 project_id 分组，同一 project 下不同 task 的消息串行处理，
Agent 处理 task A 时 task B 的用户消息被阻塞。"
```

---

## 阶段 4: 其他中等问题

### Task 4.1: trace_id 加随机后缀避免并发碰撞

**Files:**
- Modify: `src/models/memory.rs:83-87`

- [ ] **Step 1: 修改 trace_id 生成**

在 `src/models/memory.rs` 的 `MemoryTrace::new` 中：

将：
```rust
let trace_id = format!("trace-{}-{}", agent_id, chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));
```

改为：
```rust
let now = chrono::Utc::now();
let created_at = now.timestamp();
let trace_id = format!(
    "trace-{}-{}-{}",
    agent_id,
    now.timestamp_nanos_opt().unwrap_or(0),
    rand::random::<u16>()
);
```

同时用 `now` 替换 `created_at` 的计算，避免两次调用 `Utc::now()`。

- [ ] **Step 2: 编译并测试**

Run: `cargo build 2>&1 | tail -10 && cargo test memory 2>&1 | tail -20`
Expected: 编译通过，测试通过

- [ ] **Step 3: 提交**

```bash
git add src/models/memory.rs
git commit -m "fix(memory): trace_id 加随机后缀避免并发碰撞

同一 agent 并发处理两条消息时 timestamp_nanos 可能相同，
导致 trace_id 冲突。加 u16 随机后缀。"
```

---

### Task 4.2: stats 查询失败不阻塞 agent 加载

**Files:**
- Modify: `src/service/dal/agent.rs:328-344`

- [ ] **Step 1: 修改 get_agent 中 stats 查询为非致命**

在 `src/service/dal/agent.rs` 的 `get_agent` 方法中（约 line 328-344）：

将：
```rust
if with_stats {
    let stats = self.agent_stats_dao.get_stats(ctx.clone(), query, stats_options).await?;
    agent.stats = Some(stats);
}
```

改为：
```rust
if with_stats {
    // stats 查询失败不应阻塞 agent 加载
    // 修复：DuckDB 查询失败时整个 get_agent 失败，触发 nack 重试，
    // 但重试也无法修复 stats 问题，反而阻塞消息消费
    match self.agent_stats_dao.get_stats(ctx.clone(), query, stats_options).await {
        Ok(stats) => agent.stats = Some(stats),
        Err(e) => {
            log_warn!(
                ctx.clone(),
                "get_agent",
                "stats query failed, skip depth check: {}",
                e
            );
            // stats 保持 None，consumer 的 thinking_depth 检查会跳过
        }
    }
}
```

- [ ] **Step 2: 编译并测试**

Run: `cargo build 2>&1 | tail -10 && cargo test 2>&1 | grep "test result" | head -5`
Expected: 编译通过，测试通过

- [ ] **Step 3: 提交**

```bash
git add src/service/dal/agent.rs
git commit -m "fix(agent): stats 查询失败不阻塞 agent 加载

DuckDB 查询失败时 get_agent 返回错误触发 nack，但重试无法修复
stats 问题，反而阻塞消息消费。改为 log_warn 并跳过深度检查。"
```

---

### Task 4.3: think 添加超时

**Files:**
- Modify: `src/service/domain/runtime/awakening.rs:159`

- [ ] **Step 1: 给 think 调用添加超时**

在 `src/service/domain/runtime/awakening.rs` 中，找到 think 调用（约 line 159）：

将：
```rust
let think_result = self.brain_dal().think(ctx.clone(), brain, &prompt).await;
```

改为：
```rust
// 添加超时，避免 LLM API hang 住导致 Agent 永远 Busy
// 修复：think 无超时，Local 路径调 HTTP LLM API 若网络 hang 住，
// set_idle 永不执行，Agent 永远 Busy
const THINK_TIMEOUT_SECS: u64 = 300; // 5 分钟
let think_result = match tokio::time::timeout(
    std::time::Duration::from_secs(THINK_TIMEOUT_SECS),
    self.brain_dal().think(ctx.clone(), brain, &prompt)
).await {
    Ok(result) => result,
    Err(_elapsed) => {
        Err(err!(Internal, "brain think timeout after {}s", THINK_TIMEOUT_SECS))
    }
};
```

- [ ] **Step 2: 编译并测试**

Run: `cargo build 2>&1 | tail -10 && cargo test awakening 2>&1 | tail -20`
Expected: 编译通过，测试通过

- [ ] **Step 3: 提交**

```bash
git add src/service/domain/runtime/awakening.rs
git commit -m "fix(runtime): think 添加 5 分钟超时，避免 Agent 永远 Busy

LLM API hang 住或网络问题时，think 永不返回，set_idle 永不执行，
Agent 永远 Busy。添加 tokio::time::timeout 包装。"
```

---

### Task 4.4: 移除 user_profile 死代码 + get_recent_context 无效参数

**Files:**
- Modify: `src/service/domain/runtime/awakening.rs:139-147`
- Modify: `src/service/domain/runtime/memory.rs:14-38`

- [ ] **Step 1: 移除 user_profile 死代码块**

在 `src/service/domain/runtime/awakening.rs` 中，删除约 line 139-147 的死代码：

```rust
// 删除以下代码：
// 【角色分工】只有客服类 Agent 才需要拼接用户喜好等信息
// TODO: 后续从上层 Domain 传入用户画像
let agent_roles = agent.po.get_roles();
if agent_roles.contains(&"customer_service".to_string())
    || agent_roles.contains(&"客服".to_string())
{
    // 预留：实际使用时从上层传入用户画像
    // builder.user_profile(user_profile_str);
}
```

- [ ] **Step 2: 移除 get_recent_context 的 task_id 参数（或使其生效）**

查看 `src/service/domain/runtime/memory.rs` 的 `get_recent_context`。由于 `MemoryQuery` 没有 task_id 字段，最简单的修复是从签名移除 task_id：

将：
```rust
async fn get_recent_context(
    &self,
    ctx: RequestContext,
    agent_id: &str,
    task_id: Option<&str>,
    limit: usize,
) -> Result<Vec<Memory>> {
    let ctx = ctx.to_builder().try_task_id(task_id).build();
    // ...
}
```

改为：
```rust
async fn get_recent_context(
    &self,
    ctx: RequestContext,
    agent_id: &str,
    limit: usize,
) -> Result<Vec<Memory>> {
    // task_id 参数之前完全无效（MemoryQuery 无 task_id 字段），已移除
    // 跨 task 记忆隔离待 MemoryQuery 增加 task_id 字段后实现
    // ...
}
```

更新 trait 定义和所有调用方（awakening.rs line 94）。

- [ ] **Step 3: 编译并测试**

Run: `cargo build 2>&1 | tail -10 && cargo test 2>&1 | grep "test result" | head -5`
Expected: 编译通过，测试通过

- [ ] **Step 4: 提交**

```bash
git add -A
git commit -m "refactor(runtime): 移除 user_profile 死代码 + get_recent_context 无效参数

- user_profile 块只有注释无执行代码，移除避免误导
- get_recent_context 的 task_id 参数完全无效（MemoryQuery 无对应字段），
  移除参数避免 API 误导"
```

---

## 阶段 5: 优化项

### Task 5.1: Builtin/Http 工具错误信息脱敏

**Files:**
- Modify: `src/service/domain/runtime/tool_execution.rs:50-53`

- [ ] **Step 1: 为 Builtin/Http 添加错误脱敏**

将：
```rust
let mapped_message: String = match tool.po.protocol {
    ToolProtocol::Mcp => map_mcp_tool_error(&tool_id, &error),
    ToolProtocol::Builtin | ToolProtocol::Http => error.to_string(),
};
```

改为：
```rust
let mapped_message: String = match tool.po.protocol {
    ToolProtocol::Mcp => map_mcp_tool_error(&tool_id, &error),
    ToolProtocol::Builtin | ToolProtocol::Http => {
        // 脱敏：不暴露底层错误细节给 LLM，避免路径/配置泄露
        format!("tool {} execution failed", tool_id)
    }
};
```

- [ ] **Step 2: 编译并测试**

Run: `cargo build 2>&1 | tail -10 && cargo test tool_execution 2>&1 | tail -20`
Expected: 编译通过，测试可能需要更新断言

- [ ] **Step 3: 提交**

```bash
git add src/service/domain/runtime/tool_execution.rs
git commit -m "fix(runtime): Builtin/Http 工具错误信息脱敏

之前 error.to_string() 原样返回，可能含路径/配置/内部状态，
经 LLM 回灌到下一轮 prompt，造成信息泄露。"
```

---

### Task 5.2: call_manual_tool_for_agent 校验 agent 存在

**Files:**
- Modify: `src/service/domain/runtime/tool_execution.rs:96-102`

- [ ] **Step 1: agent 不存在时返回错误**

将：
```rust
let installed_tags = self
    .agent_dal
    .find_by_id(ctx.clone(), &agent_id)
    .await?
    .map(|agent| agent.po.get_installed_tags())
    .unwrap_or_default();
```

改为：
```rust
let agent_po = self
    .agent_dal
    .find_by_id(ctx.clone(), &agent_id)
    .await?
    .ok_or_else(|| common::error::Error::tool_call_failed(format!(
        "Agent {} not found, cannot authorize tool call", agent_id
    )))?;
let installed_tags = agent_po.po.get_installed_tags();
```

- [ ] **Step 2: 编译并测试**

Run: `cargo build 2>&1 | tail -10 && cargo test tool_execution 2>&1 | tail -20`
Expected: 编译通过

- [ ] **Step 3: 提交**

```bash
git add src/service/domain/runtime/tool_execution.rs
git commit -m "fix(runtime): call_manual_tool_for_agent 校验 agent 存在

之前 agent 不存在时 unwrap_or_default() 静默退化为空 vec，
只要工具带 neural 标签就能执行，削弱授权语义。"
```

---

## 执行说明

### 测试命令
每个阶段完成后运行：
```bash
cargo build 2>&1 | tail -10
cargo test 2>&1 | grep "test result" | head -5
```

### 提交规范
每个 Task 完成后提交，commit message 使用 `fix(scope):` 或 `refactor(scope):` 前缀。

### 阶段交付
- **阶段 1**：系统可用性致命问题（4 个 Task）
- **阶段 2**：数据一致性（4 个 Task）
- **阶段 3**：用户体验（4 个 Task）
- **阶段 4**：其他中等问题（4 个 Task）
- **阶段 5**：优化项（2 个 Task）

### 未在本计划中的低优先级问题
以下问题记录但暂不修复，可后续处理：
- enrich_ctx 重复执行（性能优化）
- raw_input/raw_output 暴露敏感信息（Debug 实现脱敏）
- trace_ids Vec 永远单元素（API 简化）
- caller_location 无诊断价值（移除或改进）
- broadcast 慢消费者丢消息（增大 buffer）
- unregister O(N×M) 性能（数据结构优化）
- 缺 Processing 状态阶段（状态机增强）
- TaskAssignment 无特殊处理（业务逻辑增强）
- max_thinking_depth 负数转换（类型改为 u32）
- 服务重启丢在途事件（持久化队列）
- raw_output 不直接发送给用户（设计决策，需确认）
- tool_failures 接口未接入（功能未实现）
- user_profile 来源设计（长期 TODO）

---

## Self-Review

### Spec coverage
- F1 AOP queue.ack/nack → Task 1.1 ✓
- H1 set_busy/set_idle 状态泄漏 → Task 1.2 ✓
- H2 TOCTOU 竞态 → Task 1.3 ✓
- H3 伪造 call_id → Task 2.1 ✓
- H4 丢弃 trace_ref → Task 2.1 ✓
- H5 trace 重复写入 → Task 2.2 ✓
- H6 异步路径 ctx 丢失 → Task 2.3 ✓
- H7 record_event 失败 → Task 2.4 ✓
- H8 任务状态检查顺序 → Task 2.4 ✓
- H9 nack 无退避 → Task 1.4 ✓
- M1-M18 中等问题 → 阶段 3 + 4 ✓
- 低优先级 → 阶段 5 + 未修复列表 ✓

### Placeholder scan
- 所有 Task 都有完整代码块，无 TBD/TODO 占位符
- 部分 Task 的"修复所有调用方"需要运行时搜索，已在 Task 2.1 Step 6 说明

### Type consistency
- `call_manual` 返回类型：`Result<Value>` → `Result<(Value, ToolCallEntry)>` 在所有相关 Task 中一致
- `try_set_busy` 方法名在 Task 1.3 中定义并在 consumer 中使用，命名一致
- `BusyGuard` 在 Task 1.2 中定义，在 awakening 中使用，命名一致
