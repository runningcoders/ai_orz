📦 归档标记（2026-08-16）：被 [docs/archive/plan-archive/Agent运行时上下文扩展.md](docs/archive/plan-archive/Agent运行时上下文扩展.md) 取代。保留原因：原始执行蓝图含逐步命令/检查清单，留作审计参考。生效方案：[docs/archive/plan-archive/Agent运行时上下文扩展.md](docs/archive/plan-archive/Agent运行时上下文扩展.md)

---

# AgentRuntimeInfo 业务上下文扩展 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 AgentRuntimeInfo 中补充 task_id / project_id 业务上下文字段，使前端能按任务/项目视角查看运行中 Agent。

**Architecture:** 扩展 `AgentRuntimeInfo` 结构体新增 `task_id` / `project_id` 字段，在 `set_busy` / `try_set_busy` 时一次性设置，`set_idle` 时清空，`set_resting` 时保留（沉淀仍在同一业务上下文中）。同步更新 API DTO 和 Handler 暴露这两个字段。

**Tech Stack:** Rust, Axum, DashMap, serde

**设计文档：** [docs/design/thinking_task_policy_engine_design.md](../../../docs/design/thinking_task_policy_engine_design.md) 第 103 行"业务上下文字段归属"决策

---

## File Structure

| 文件 | 职责 | 操作 |
|------|------|------|
| `src/pkg/agent_runtime_state.rs` | AgentRuntimeInfo 结构体 + StateManager 方法 | 修改 |
| `src/service/domain/runtime/awakening.rs:536` | awaken 中 set_busy 调用 | 修改 |
| `src/consumer/message.rs:153` | consumer 中 try_set_busy 调用 | 修改 |
| `common/src/api/agent.rs:160-201` | GetAgentResponse DTO | 修改 |
| `src/handlers/hr/agent/get_agent.rs:106-109` | 读取 runtime_info 构造响应 | 修改 |
| `src/handlers/hr/agent/update_agent_status.rs:86-89` | 同上 | 修改 |
| `common/src/api/agent_test.rs:38` | DTO 测试补字段 | 修改 |
| `src/service/dal/agent_test.rs:877` | set_busy 调用补参数 | 修改 |
| `tests/integration/agent_awaken_test.rs:158,728` | set_busy 调用补参数 | 修改 |

**无需修改（自动兼容）：**
- `src/service/domain/runtime/busy_guard.rs` — Drop 只调 set_idle，清空逻辑在 set_idle 内部
- `src/service/dal/agent/mod.rs` — inject_runtime_state 通过 get() 透传整个 AgentRuntimeInfo
- `UpdateAgentStatusResponse` — 是 `GetAgentResponse` 的类型别名（`common/src/api/agent.rs:343`）

---

## Task 1: 扩展 AgentRuntimeInfo + state 方法签名

**Files:**
- Modify: `src/pkg/agent_runtime_state.rs`

### Step 1: 写单元测试

在 `src/pkg/agent_runtime_state.rs` 末尾（第 173 行 `}` 之后）添加测试模块：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_busy_records_task_and_project() {
        let mgr = AgentRuntimeStateManager::new();
        mgr.set_busy("agent-1", "msg-1", Some("task-1"), Some("proj-1"));
        let info = mgr.get("agent-1").unwrap();
        assert_eq!(info.state, AgentRuntimeState::Busy);
        assert_eq!(info.current_message_id, Some("msg-1".to_string()));
        assert_eq!(info.task_id, Some("task-1".to_string()));
        assert_eq!(info.project_id, Some("proj-1".to_string()));
    }

    #[test]
    fn test_set_busy_with_none_context() {
        let mgr = AgentRuntimeStateManager::new();
        mgr.set_busy("agent-1", "msg-1", None, None);
        let info = mgr.get("agent-1").unwrap();
        assert_eq!(info.task_id, None);
        assert_eq!(info.project_id, None);
    }

    #[test]
    fn test_try_set_busy_records_task_and_project() {
        let mgr = AgentRuntimeStateManager::new();
        let acquired = mgr.try_set_busy("agent-1", "msg-1", Some("task-1"), Some("proj-1"));
        assert!(acquired);
        let info = mgr.get("agent-1").unwrap();
        assert_eq!(info.task_id, Some("task-1".to_string()));
        assert_eq!(info.project_id, Some("proj-1".to_string()));
    }

    #[test]
    fn test_set_idle_clears_context() {
        let mgr = AgentRuntimeStateManager::new();
        mgr.set_busy("agent-1", "msg-1", Some("task-1"), Some("proj-1"));
        mgr.set_idle("agent-1");
        let info = mgr.get("agent-1").unwrap();
        assert_eq!(info.state, AgentRuntimeState::Idle);
        assert_eq!(info.current_message_id, None);
        assert_eq!(info.task_id, None);
        assert_eq!(info.project_id, None);
    }

    #[test]
    fn test_set_resting_preserves_task_and_project() {
        let mgr = AgentRuntimeStateManager::new();
        mgr.set_busy("agent-1", "msg-1", Some("task-1"), Some("proj-1"));
        mgr.set_resting("agent-1");
        let info = mgr.get("agent-1").unwrap();
        assert_eq!(info.state, AgentRuntimeState::Resting);
        // 沉淀场景：清空 message_id，但保留 task_id / project_id（同一业务上下文）
        assert_eq!(info.current_message_id, None);
        assert_eq!(info.task_id, Some("task-1".to_string()));
        assert_eq!(info.project_id, Some("proj-1".to_string()));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ai_orz --lib pkg::agent_runtime_state::tests -- --nocapture 2>&1 | head -20`
Expected: 编译失败（`task_id` / `project_id` 字段不存在，`set_busy` / `try_set_busy` 签名不匹配）

- [ ] **Step 3: 扩展 AgentRuntimeInfo 结构体**

在 `src/pkg/agent_runtime_state.rs` 中修改 `AgentRuntimeInfo` 结构体（第 12-19 行）：

```rust
/// Agent 运行时信息（内存中）
#[derive(Debug, Clone)]
pub struct AgentRuntimeInfo {
    pub state: AgentRuntimeState,
    /// 当前处理的消息 ID（仅 Busy 时有效）
    pub current_message_id: Option<String>,
    /// 状态开始时间戳（毫秒）
    pub state_started_at: i64,
    /// 当前关联的任务 ID（set_busy 时设置，set_idle 时清空，set_resting 时保留）
    pub task_id: Option<String>,
    /// 当前关联的项目 ID（同上）
    pub project_id: Option<String>,
}
```

修改 `Default` 实现（第 21-29 行）：

```rust
impl Default for AgentRuntimeInfo {
    fn default() -> Self {
        Self {
            state: AgentRuntimeState::Idle,
            current_message_id: None,
            state_started_at: 0,
            task_id: None,
            project_id: None,
        }
    }
}
```

- [ ] **Step 4: 修改 set_idle 清空新字段**

在 `set_idle` 方法中（第 52-60 行），在 `entry.current_message_id = None;` 之后添加：

```rust
    pub fn set_idle(&self, agent_id: &str) {
        let from_state = self.get_state(agent_id);
        let mut entry = self.states.entry(agent_id.to_string()).or_default();
        entry.state = AgentRuntimeState::Idle;
        entry.current_message_id = None;
        entry.task_id = None;
        entry.project_id = None;
        entry.state_started_at = common::constants::utils::current_timestamp_ms();
        drop(entry); // 释放 dashmap 借用
        self.notify_state_change(agent_id, state_str(from_state), "idle", None);
    }
```

- [ ] **Step 5: 修改 set_resting 保留 task_id / project_id**

在 `set_resting` 方法中（第 63-71 行），**不添加** task_id / project_id 的清空代码（保留原值）。方法体不变：

```rust
    pub fn set_resting(&self, agent_id: &str) {
        let from_state = self.get_state(agent_id);
        let mut entry = self.states.entry(agent_id.to_string()).or_default();
        entry.state = AgentRuntimeState::Resting;
        entry.current_message_id = None;
        // 注意：task_id / project_id 保留不清空
        // 沉淀（sleep_and_settle）在 awaken 的 Busy 期间触发，仍在同一业务上下文中
        entry.state_started_at = common::constants::utils::current_timestamp_ms();
        drop(entry);
        self.notify_state_change(agent_id, state_str(from_state), "resting", None);
    }
```

- [ ] **Step 6: 修改 set_busy 签名**

将 `set_busy` 方法（第 74-83 行）替换为：

```rust
    /// 设置 Agent 为忙碌状态
    ///
    /// task_id / project_id 为业务上下文，整个 Busy 期间不变，
    /// 用于前端按任务/项目视角过滤运行中 Agent。
    pub fn set_busy(
        &self,
        agent_id: &str,
        message_id: &str,
        task_id: Option<&str>,
        project_id: Option<&str>,
    ) {
        let from_state = self.get_state(agent_id);
        let msg_id = message_id.to_string();
        let mut entry = self.states.entry(agent_id.to_string()).or_default();
        entry.state = AgentRuntimeState::Busy;
        entry.current_message_id = Some(msg_id.clone());
        entry.task_id = task_id.map(|s| s.to_string());
        entry.project_id = project_id.map(|s| s.to_string());
        entry.state_started_at = common::constants::utils::current_timestamp_ms();
        drop(entry);
        self.notify_state_change(agent_id, state_str(from_state), "busy", Some(msg_id));
    }
```

- [ ] **Step 7: 修改 try_set_busy 签名**

将 `try_set_busy` 方法（第 92-107 行）替换为：

```rust
    /// 原子地尝试设置 Busy 状态
    ///
    /// 如果 Agent 当前是 Idle，设置为 Busy 并返回 true。
    /// 如果 Agent 当前是 Busy 或 Resting，返回 false（未修改状态）。
    ///
    /// 修复 TOCTOU 竞态：consumer 的 is_unavailable 检查与 awaken 的 set_busy 之间
    /// 会被其他 worker 插入，导致同一 Agent 被并发唤醒。
    pub fn try_set_busy(
        &self,
        agent_id: &str,
        message_id: &str,
        task_id: Option<&str>,
        project_id: Option<&str>,
    ) -> bool {
        let from_state;
        let msg_id = message_id.to_string();
        {
            let mut entry = self.states.entry(agent_id.to_string()).or_default();
            if entry.state.is_unavailable() {
                return false;
            }
            from_state = entry.state;
            entry.state = AgentRuntimeState::Busy;
            entry.current_message_id = Some(msg_id.clone());
            entry.task_id = task_id.map(|s| s.to_string());
            entry.project_id = project_id.map(|s| s.to_string());
            entry.state_started_at = common::constants::utils::current_timestamp_ms();
        }
        self.notify_state_change(agent_id, state_str(from_state), "busy", Some(msg_id));
        true
    }
```

- [ ] **Step 8: Run test to verify it passes**

Run: `cargo test -p ai_orz --lib pkg::agent_runtime_state::tests -- --nocapture 2>&1 | head -20`
Expected: 5 个测试全部 PASS

- [ ] **Step 9: Commit**

```bash
git add src/pkg/agent_runtime_state.rs
git commit -m "feat: extend AgentRuntimeInfo with task_id/project_id business context"
```

---

## Task 2: 更新生产代码调用方

**Files:**
- Modify: `src/service/domain/runtime/awakening.rs:536`
- Modify: `src/consumer/message.rs:153`

### Step 1: 更新 awaken 中的 set_busy 调用

在 `src/service/domain/runtime/awakening.rs` 第 536 行，将：

```rust
        AgentRuntimeStateManager::global().set_busy(&agent.po.id, &message.po.id);
```

替换为：

```rust
        AgentRuntimeStateManager::global().set_busy(
            &agent.po.id,
            &message.po.id,
            message.po.task_id.as_deref(),
            message.po.project_id.as_deref(),
        );
```

- [ ] **Step 2: 更新 consumer 中的 try_set_busy 调用**

在 `src/consumer/message.rs` 第 153 行，将：

```rust
        let acquired = AgentRuntimeStateManager::global().try_set_busy(agent_id, &message.po.id);
```

替换为：

```rust
        let acquired = AgentRuntimeStateManager::global().try_set_busy(
            agent_id,
            &message.po.id,
            message.po.task_id.as_deref(),
            message.po.project_id.as_deref(),
        );
```

- [ ] **Step 3: Run cargo check**

Run: `cargo check --message-format short 2>&1 | tail -5`
Expected: 编译通过（测试代码会报错，下一步处理）

注意：此步骤可能会有测试编译错误（set_busy / try_set_busy 签名变更），这是预期的。

- [ ] **Step 4: Commit**

```bash
git add src/service/domain/runtime/awakening.rs src/consumer/message.rs
git commit -m "feat: pass task_id/project_id to set_busy/try_set_busy from message context"
```

---

## Task 3: 扩展 API DTO + Handler

**Files:**
- Modify: `common/src/api/agent.rs:160-201` (GetAgentResponse)
- Modify: `src/handlers/hr/agent/get_agent.rs:106-109,144-145`
- Modify: `src/handlers/hr/agent/update_agent_status.rs:86-89,134-135`
- Modify: `common/src/api/agent_test.rs:38`

### Step 1: 扩展 GetAgentResponse DTO

在 `common/src/api/agent.rs` 的 `GetAgentResponse` 结构体中（第 192 行 `current_message_id` 字段之后），添加两个新字段：

```rust
    /// 当前处理的消息 ID（仅忙碌时有效）
    pub current_message_id: Option<String>,
    /// 当前关联的任务 ID（仅忙碌时有效）
    pub current_task_id: Option<String>,
    /// 当前关联的项目 ID（仅忙碌时有效）
    pub current_project_id: Option<String>,
    /// 已绑定的工具 ID 列表
    pub tools: Vec<String>,
```

- [ ] **Step 2: 更新 get_agent.rs handler**

在 `src/handlers/hr/agent/get_agent.rs` 第 106-109 行，将：

```rust
    let (runtime_state, current_message_id) = match &agent.runtime_info {
        Some(info) => (info.state as i32, info.current_message_id.clone()),
        None => (AgentRuntimeState::Idle as i32, None),
    };
```

替换为：

```rust
    let (runtime_state, current_message_id, current_task_id, current_project_id) =
        match &agent.runtime_info {
            Some(info) => (
                info.state as i32,
                info.current_message_id.clone(),
                info.task_id.clone(),
                info.project_id.clone(),
            ),
            None => (AgentRuntimeState::Idle as i32, None, None, None),
        };
```

在同一文件的 `Ok(GetAgentResponse { ... })` 构造中（第 144-145 行），在 `current_message_id,` 之后添加：

```rust
        runtime_state,
        current_message_id,
        current_task_id,
        current_project_id,
        tools,
```

- [ ] **Step 3: 更新 update_agent_status.rs handler**

在 `src/handlers/hr/agent/update_agent_status.rs` 第 86-89 行，将：

```rust
    let (runtime_state, current_message_id) = match &agent.runtime_info {
        Some(info) => (info.state as i32, info.current_message_id.clone()),
        None => (AgentRuntimeState::Idle as i32, None),
    };
```

替换为：

```rust
    let (runtime_state, current_message_id, current_task_id, current_project_id) =
        match &agent.runtime_info {
            Some(info) => (
                info.state as i32,
                info.current_message_id.clone(),
                info.task_id.clone(),
                info.project_id.clone(),
            ),
            None => (AgentRuntimeState::Idle as i32, None, None, None),
        };
```

在同一文件的 `Ok(UpdateAgentStatusResponse { ... })` 构造中（第 134-135 行），在 `current_message_id,` 之后添加：

```rust
        runtime_state,
        current_message_id,
        current_task_id,
        current_project_id,
        tools,
```

- [ ] **Step 4: 更新 DTO 测试**

在 `common/src/api/agent_test.rs` 第 38 行（`current_message_id: None,` 之后），添加：

```rust
        runtime_state: 0,
        current_message_id: None,
        current_task_id: None,
        current_project_id: None,
        tools: vec![],
```

- [ ] **Step 5: Run cargo check**

Run: `cargo check --message-format short 2>&1 | tail -5`
Expected: 编译通过（仅测试代码可能还有 set_busy 签名不匹配的错误，下一步处理）

- [ ] **Step 6: Commit**

```bash
git add common/src/api/agent.rs common/src/api/agent_test.rs \
        src/handlers/hr/agent/get_agent.rs src/handlers/hr/agent/update_agent_status.rs
git commit -m "feat: expose current_task_id/current_project_id in GetAgentResponse"
```

---

## Task 4: 更新测试代码中的 set_busy 调用

**Files:**
- Modify: `src/service/dal/agent_test.rs:877`
- Modify: `tests/integration/agent_awaken_test.rs:158,728`

### Step 1: 更新 agent_test.rs

在 `src/service/dal/agent_test.rs` 第 877 行，将：

```rust
    manager.set_busy(agent_busy.id(), "msg-test-1");
```

替换为：

```rust
    manager.set_busy(agent_busy.id(), "msg-test-1", None, None);
```

- [ ] **Step 2: 更新 agent_awaken_test.rs 第一处**

在 `tests/integration/agent_awaken_test.rs` 第 158 行，将：

```rust
    runtime_state.set_busy(&agent_id, &message_id);
```

替换为：

```rust
    runtime_state.set_busy(&agent_id, &message_id, None, None);
```

- [ ] **Step 3: 更新 agent_awaken_test.rs 第二处**

在 `tests/integration/agent_awaken_test.rs` 第 728 行，将：

```rust
    runtime_state.set_busy(&agent_id, &message.po.id);
```

替换为：

```rust
    runtime_state.set_busy(&agent_id, &message.po.id, None, None);
```

- [ ] **Step 4: Run cargo check**

Run: `cargo check --message-format short 2>&1 | tail -5`
Expected: 编译完全通过，无错误

- [ ] **Step 5: Run tests**

Run: `cargo test -p ai_orz --lib pkg::agent_runtime_state::tests -- --nocapture 2>&1 | head -20`
Expected: 5 个单元测试全部 PASS

Run: `cargo test -p ai_orz --lib service::dal::agent_test -- --nocapture 2>&1 | head -20`
Expected: 测试通过

- [ ] **Step 6: Commit**

```bash
git add src/service/dal/agent_test.rs tests/integration/agent_awaken_test.rs
git commit -m "test: update set_busy calls for new task_id/project_id signature"
```

---

## Verification

- [ ] **Final check: cargo clippy**

Run: `cargo clippy --message-format short 2>&1 | tail -10`
Expected: 无 warnings

- [ ] **Final check: 所有测试**

Run: `cargo test -p ai_orz --lib 2>&1 | tail -10`
Expected: 全部通过
