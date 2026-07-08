# Agent Runtime State 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 Agent 引入纯内存运行时状态管理，区分空闲/休息/忙碌状态，支持查询时注入运行时信息，并在 Agent 忙碌时拒绝新消息。

**Architecture:** 纯内存状态机，全局单例管理，无 DB 持久化。服务重启后状态自然重置（Agent 自动休息）。业务链路通过 Project/Task/Message 表可追溯。消费端在消息入队前检查 Agent 是否忙碌，若忙碌则返回忙碌提示。

**Tech Stack:** Rust, DashMap (并发 HashMap), serde

---

## File Structure

| 文件 | 职责 |
|------|------|
| `common/src/enums/agent.rs` | 新增 `AgentRuntimeState` 枚举（前后端共享） |
| `src/pkg/agent_runtime_state.rs` | 内存状态管理器 + 全局单例 |
| `src/pkg/mod.rs` | 导出 agent_runtime_state 模块 |
| `src/service/domain/runtime/awakening.rs` | awaken 方法中更新状态 |
| `src/service/domain/runtime/mod.rs` | RuntimeDomain trait 增加状态查询能力 |
| `common/src/api/agent.rs` | 响应 DTO 增加运行时状态字段 |
| `src/models/agent.rs` | Agent 实体增加 `runtime_info` 字段 |
| `src/service/dal/agent.rs` | AgentDal.find_by_id 等方法中注入运行时状态 |
| `src/handlers/hr/agent/get_agent.rs` | 从实体读取运行时状态构造 DTO |
| `src/handlers/hr/agent/list_agents.rs` | 从实体读取运行时状态构造 DTO |
| `src/service/domain/message/mod.rs` | MessageDomain 增加检查 Agent 是否忙碌的方法 |
| `src/service/dal/message.rs` | 消息入队时检查 Agent 状态，若忙碌则拒绝 |

---

### Task 1: AgentRuntimeState 枚举定义

**Files:**
- Modify: `common/src/enums/agent.rs`
- Test: `common/src/enums/agent_test.rs` (如果不存在则创建)

**Context:**
现有 `common/src/enums/agent.rs` 包含 `AgentStatus`（生命周期状态）。新增 `AgentRuntimeState` 表示运行时状态，两者语义完全不同。

- [ ] **Step 1: 在 `AgentStatus` 之后添加 `AgentRuntimeState` 枚举**

```rust
/// Agent 运行时状态（纯内存，不持久化）
///
/// 服务重启后自动重置，业务链路通过 Project/Task/Message 表可追溯。
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, JsonSchema)]
pub enum AgentRuntimeState {
    /// 空闲，可以接受新消息
    #[default]
    Idle = 0,
    /// 休息中，不接受新消息
    /// 用于恢复精力、压缩上下文、构建知识突触等
    Resting = 1,
    /// 忙碌，正在处理消息
    Busy = 2,
}

impl AgentRuntimeState {
    pub fn from_i32(v: i32) -> Self {
        match v {
            1 => Self::Resting,
            2 => Self::Busy,
            _ => Self::Idle,
        }
    }

    pub fn to_i32(&self) -> i32 {
        *self as i32
    }

    /// 是否处于忙碌或休息状态（不可接受新消息）
    pub fn is_unavailable(&self) -> bool {
        matches!(self, Self::Busy | Self::Resting)
    }
}

impl From<i32> for AgentRuntimeState {
    fn from(v: i32) -> Self {
        Self::from_i32(v)
    }
}

impl From<i64> for AgentRuntimeState {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}
```

- [ ] **Step 2: 运行 common crate 编译检查**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo check -p common`
Expected: PASS

- [ ] **Step 3: 提交**

```bash
git add common/src/enums/agent.rs
git commit -m "feat: add AgentRuntimeState enum for in-memory runtime state tracking"
```

---

### Task 2: 内存状态管理器

**Files:**
- Create: `src/pkg/agent_runtime_state.rs`
- Modify: `src/pkg/mod.rs`

**Context:**
`src/pkg/` 是后端公共工具包，已有 `tool_tracing`, `stats`, `request_context` 等模块。新建 `agent_runtime_state.rs` 存放全局状态管理器。

- [ ] **Step 1: 创建状态管理器文件**

Create: `src/pkg/agent_runtime_state.rs`

```rust
//! Agent 运行时状态管理器
//!
//! 纯内存状态，全局单例，不持久化。
//! 服务重启后状态自动重置（Agent 相当于自动休息）。

use crate::models::agent::Agent;
use common::enums::AgentRuntimeState;
use dashmap::DashMap;
use std::sync::Arc;

/// Agent 运行时信息（内存中）
#[derive(Debug, Clone)]
pub struct AgentRuntimeInfo {
    pub state: AgentRuntimeState,
    /// 当前处理的消息 ID（仅 Busy 时有效）
    pub current_message_id: Option<String>,
    /// 状态开始时间戳（毫秒）
    pub state_started_at: i64,
}

impl Default for AgentRuntimeInfo {
    fn default() -> Self {
        Self {
            state: AgentRuntimeState::Idle,
            current_message_id: None,
            state_started_at: 0,
        }
    }
}

/// Agent 运行时状态管理器（全局单例）
pub struct AgentRuntimeStateManager {
    states: DashMap<String, AgentRuntimeInfo>,
}

impl AgentRuntimeStateManager {
    /// 创建新的管理器实例（用于测试）
    pub fn new() -> Self {
        Self {
            states: DashMap::new(),
        }
    }

    /// 获取全局单例
    pub fn global() -> Arc<Self> {
        use std::sync::OnceLock;
        static INSTANCE: OnceLock<Arc<AgentRuntimeStateManager>> = OnceLock::new();
        INSTANCE
            .get_or_init(|| Arc::new(Self::new()))
            .clone()
    }

    /// 设置 Agent 为空闲状态
    pub fn set_idle(&self, agent_id: &str) {
        let mut entry = self.states.entry(agent_id.to_string()).or_default();
        entry.state = AgentRuntimeState::Idle;
        entry.current_message_id = None;
        entry.state_started_at = crate::pkg::utils::current_timestamp_ms();
    }

    /// 设置 Agent 为休息状态
    pub fn set_resting(&self, agent_id: &str) {
        let mut entry = self.states.entry(agent_id.to_string()).or_default();
        entry.state = AgentRuntimeState::Resting;
        entry.current_message_id = None;
        entry.state_started_at = crate::pkg::utils::current_timestamp_ms();
    }

    /// 设置 Agent 为忙碌状态
    pub fn set_busy(&self, agent_id: &str, message_id: &str) {
        let mut entry = self.states.entry(agent_id.to_string()).or_default();
        entry.state = AgentRuntimeState::Busy;
        entry.current_message_id = Some(message_id.to_string());
        entry.state_started_at = crate::pkg::utils::current_timestamp_ms();
    }

    /// 获取 Agent 运行时信息
    pub fn get(&self, agent_id: &str) -> Option<AgentRuntimeInfo> {
        self.states.get(agent_id).map(|v| v.clone())
    }

    /// 获取 Agent 运行时状态（不存在则返回 Idle）
    pub fn get_state(&self, agent_id: &str) -> AgentRuntimeState {
        self.get(agent_id)
            .map(|info| info.state)
            .unwrap_or(AgentRuntimeState::Idle)
    }

    /// Agent 是否不可用（忙碌或休息）
    pub fn is_unavailable(&self, agent_id: &str) -> bool {
        self.get_state(agent_id).is_unavailable()
    }

    /// 获取所有 Agent 的运行时状态（用于列表查询）
    pub fn get_all_states(&self) -> Vec<(String, AgentRuntimeInfo)> {
        self.states
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }
}

impl Default for AgentRuntimeStateManager {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 2: 在 `src/pkg/mod.rs` 中导出模块**

Modify: `src/pkg/mod.rs`

在已有模块导出之后添加：

```rust
pub mod agent_runtime_state;
```

- [ ] **Step 3: 编译检查**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo check --lib 2>&1 | grep "^error" | head -10`
Expected: 无错误（可能需要处理 utils 模块路径）

注意：`crate::pkg::utils::current_timestamp_ms()` 如果不存在，改用 `common::constants::utils::current_timestamp_ms()`。

- [ ] **Step 4: 提交**

```bash
git add src/pkg/agent_runtime_state.rs src/pkg/mod.rs
git commit -m "feat: add AgentRuntimeStateManager in-memory state tracker"
```

---

### Task 3: 在 awaken 中更新状态

**Files:**
- Modify: `src/service/domain/runtime/awakening.rs`

**Context:**
`awaken` 是 Agent 被唤醒执行的核心方法。需要在开始处理时设置 Busy，完成时设置 Idle。

- [ ] **Step 1: 在 awaken 方法开头设置 Busy**

Modify: `src/service/domain/runtime/awakening.rs`

在 `let start_time = ...` 之后、`let ctx = enrich_ctx!` 之前添加：

```rust
use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;

// ... 现有代码 ...

async fn awaken(...) -> Result<AwakeningResult> {
    let start_time = std::time::SystemTime::now();

    // 设置 Agent 为忙碌状态
    AgentRuntimeStateManager::global()
        .set_busy(&agent.po.id, &message.po.id);

    // 补充 Agent 上下文到 ctx
    let ctx = enrich_ctx!(&ctx, agent);
    
    // ... 现有代码（步骤 1-5）...
```

- [ ] **Step 2: 在 awaken 方法成功返回前设置 Idle**

在 `Ok(AwakeningResult { ... })` 之前添加：

```rust
    // Step 9: 设置 Agent 为空闲状态
    AgentRuntimeStateManager::global()
        .set_idle(&agent.po.id);

    // Step 10: 返回结果
    Ok(AwakeningResult {
        agent_id: agent.po.id.clone(),
        trace_ids: vec![trace_id],
        raw_input: prompt,
        raw_output,
    })
```

- [ ] **Step 3: 在出错时也设置 Idle（使用 RAII guard 或 catch_unwind 兜底）**

由于 Rust async 函数中 Drop 的复杂性，最简单的方式是在结果处理处统一设置 Idle：

```rust
    // 替换原来的直接返回
    let result = self.brain_dal().think(ctx.clone(), brain, &prompt).await;
    
    // 无论成功失败，最后都设置为 Idle
    AgentRuntimeStateManager::global()
        .set_idle(&agent.po.id);
    
    let raw_output = result?;
    
    // ... 后续步骤 ...
```

具体实现：将 `let raw_output = self.brain_dal().think(...).await?;` 改成先捕获 Result，设置 Idle，再 `?` 展开。

- [ ] **Step 4: 编译检查**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo check --lib 2>&1 | grep "^error" | head -10`
Expected: 无错误

- [ ] **Step 5: 提交**

```bash
git add src/service/domain/runtime/awakening.rs
git commit -m "feat: update Agent runtime state in awaken lifecycle"
```

---

### Task 4: RuntimeDomain 暴露状态查询接口

**Files:**
- Modify: `src/service/domain/runtime/mod.rs`

**Context:**
需要在 RuntimeDomain trait 中增加查询 Agent 运行时状态的能力，供上层调用。

- [ ] **Step 1: 在 RuntimeDomain trait 中增加状态查询方法**

在 `trait RuntimeDomain` 中（`fn tool_execution` 之后）添加：

```rust
    /// 查询 Agent 运行时状态
    fn agent_runtime_state(&self, agent_id: &str) -> AgentRuntimeState;
    
    /// Agent 是否处于不可用状态（忙碌或休息）
    fn is_agent_unavailable(&self, agent_id: &str) -> bool;
```

注意：这些不是 async 方法，因为只是查内存，无 IO。

- [ ] **Step 2: 在 RuntimeDomainImpl 中实现这些方法**

```rust
impl RuntimeDomain for RuntimeDomainImpl {
    // ... 现有实现 ...
    
    fn agent_runtime_state(&self, agent_id: &str) -> AgentRuntimeState {
        crate::pkg::agent_runtime_state::AgentRuntimeStateManager::global()
            .get_state(agent_id)
    }
    
    fn is_agent_unavailable(&self, agent_id: &str) -> bool {
        crate::pkg::agent_runtime_state::AgentRuntimeStateManager::global()
            .is_unavailable(agent_id)
    }
}
```

- [ ] **Step 3: 编译检查**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo check --lib`
Expected: PASS

- [ ] **Step 4: 提交**

```bash
git add src/service/domain/runtime/mod.rs
git commit -m "feat: expose agent runtime state query on RuntimeDomain"
```

---

### Task 5: 响应 DTO 增加运行时状态字段

**Files:**
- Modify: `common/src/api/agent.rs`
- Modify: `common/src/api/agent_test.rs`（如果存在）

**Context:**
在 `GetAgentResponse` 和 `AgentListItem` 中增加 `runtime_state` 字段，让前端能看到 Agent 当前状态。

- [ ] **Step 1: 在 `GetAgentResponse` 中增加运行时状态字段**

Modify: `common/src/api/agent.rs`

```rust
use crate::enums::{AgentStatus, AgentRuntimeState};
```

在 `GetAgentResponse` 中，在 `updated_at` 字段后添加：

```rust
    /// 运行时状态（内存状态，服务重启后重置）
    pub runtime_state: i32,
    /// 当前处理的消息 ID（仅忙碌时有效）
    pub current_message_id: Option<String>,
```

- [ ] **Step 2: 在 `AgentListItem` 中增加运行时状态字段**

在 `AgentListItem` 中，在 `created_at` 字段后添加：

```rust
    /// 运行时状态（内存状态）
    pub runtime_state: i32,
```

- [ ] **Step 3: 编译检查**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo check -p common`
Expected: PASS

- [ ] **Step 4: 提交**

```bash
git add common/src/api/agent.rs
git commit -m "feat: add runtime_state to Agent response DTOs"
```

---

### Task 6: Agent 实体增加 runtime_info 字段

**Files:**
- Modify: `src/models/agent.rs`

**Context:**
在 Agent 业务实体中增加 `runtime_info` 字段，由 DAL 层注入，Handler 层只需读取。

- [ ] **Step 1: 在 Agent 结构体中增加 runtime_info 字段**

Modify: `src/models/agent.rs`

在 `pub struct Agent` 中，在 `pub tools` 字段后添加：

```rust
pub struct Agent {
    /// 底层持久化对象
    pub po: AgentPo,
    /// 装配好的 Brain（推理执行实体）
    pub brain: Option<Brain>,
    /// 绑定的工具列表
    pub tools: Vec<Tool>,
    /// 运行时状态信息（由 DAL 层从内存注入）
    ///
    /// None 表示未注入（如刚创建还未查询）
    pub runtime_info: Option<crate::pkg::agent_runtime_state::AgentRuntimeInfo>,
}
```

- [ ] **Step 2: 更新 Debug 和 Clone 实现**

在 `impl fmt::Debug for Agent` 中添加：

```rust
impl fmt::Debug for Agent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Agent")
            .field("po", &self.po)
            .field("brain", &self.brain.is_some())
            .field("tools", &self.tools.len())
            .field("runtime_info", &self.runtime_info)
            .finish()
    }
}
```

Clone 实现保持不变（`AgentRuntimeInfo` 已经是 Clone）。

- [ ] **Step 3: 更新 Agent::new() 方法**

在创建 Agent 时默认 runtime_info 为 None：

```rust
pub fn new(...) -> Self {
    Self {
        po,
        brain: None,
        tools: vec![],
        runtime_info: None,
    }
}
```

- [ ] **Step 4: 编译检查**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo check --lib 2>&1 | grep "^error" | head -10`
Expected: 无错误

- [ ] **Step 5: 提交**

```bash
git add src/models/agent.rs
git commit -m "feat: add runtime_info field to Agent entity"
```

---

### Task 7: AgentDal 注入运行时状态

**Files:**
- Modify: `src/service/dal/agent.rs`

**Context:**
DAL 层负责组装 Agent 实体。在 `find_by_id` 等方法返回 Agent 之前，从内存状态管理器获取运行时信息并注入。

- [ ] **Step 1: 在 AgentDalImpl 中注入运行时状态**

找到 `find_by_id` 和 `list_by_organization` 等返回 Agent 的方法。

在返回 Agent 之前，注入运行时状态：

```rust
use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;

impl AgentDalImpl {
    /// 注入运行时状态到 Agent 实体
    fn inject_runtime_state(agent: Agent) -> Agent {
        let runtime_info = AgentRuntimeStateManager::global()
            .get(&agent.po.id);
        Agent {
            runtime_info,
            ..agent
        }
    }
}
```

在各个方法中使用：

```rust
async fn find_by_id(&self, ctx: RequestContext, id: String) -> Result<Option<Agent>> {
    // ... 原有查询逻辑 ...
    let agent = ...; // 组装好的 Agent
    
    // 注入运行时状态
    Ok(Some(Self::inject_runtime_state(agent)))
}

async fn list_by_organization(&self, ctx: RequestContext, org_id: String) -> Result<Vec<Agent>> {
    // ... 原有查询逻辑 ...
    let agents = ...; // 组装好的 Agent 列表
    
    // 注入运行时状态
    Ok(agents.into_iter().map(Self::inject_runtime_state).collect())
}
```

- [ ] **Step 2: 编译检查**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo check --lib 2>&1 | grep "^error" | head -10`
Expected: 无错误

- [ ] **Step 3: 提交**

```bash
git add src/service/dal/agent.rs
git commit -m "feat: inject runtime state from memory in AgentDal"
```

---

### Task 8: Handler 从实体读取运行时状态

**Files:**
- Modify: `src/handlers/hr/agent/get_agent.rs`
- Modify: `src/handlers/hr/agent/list_agents.rs`

**Context:**
Handler 层不再直接调用状态管理器，而是从 Agent 实体的 `runtime_info` 字段读取，然后构造 DTO。

- [ ] **Step 1: 修改 get_agent handler**

Modify: `src/handlers/hr/agent/get_agent.rs`

删除 `use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;`（如果之前添加了）

改为从 agent.runtime_info 读取：

```rust
use common::enums::AgentRuntimeState;

// ... 在构造 GetAgentResponse 时 ...

    let (runtime_state, current_message_id) = match &agent.runtime_info {
        Some(info) => (info.state as i32, info.current_message_id.clone()),
        None => (AgentRuntimeState::Idle as i32, None),
    };

    Ok(GetAgentResponse {
        // ... 原有字段 ...
        status: agent.po.status as i32,
        created_at: agent.po.created_at,
        updated_at: agent.po.updated_at,
        runtime_state,
        current_message_id,
    })
```

- [ ] **Step 2: 修改 list_agents handler**

Modify: `src/handlers/hr/agent/list_agents.rs`

删除 `use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;`（如果之前添加了）

改为从 agent.runtime_info 读取：

```rust
use common::enums::AgentRuntimeState;

// ... 在 map 中 ...

    .map(|agent| {
        let runtime_state = match &agent.runtime_info {
            Some(info) => info.state as i32,
            None => AgentRuntimeState::Idle as i32,
        };
        
        AgentListItem {
            // ... 原有字段 ...
            status: agent.po.status as i32,
            created_at: agent.po.created_at,
            runtime_state,
        }
    })
```

- [ ] **Step 3: 编译检查**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo check --lib 2>&1 | grep "^error" | head -10`
Expected: 无错误

- [ ] **Step 4: 提交**

```bash
git add src/handlers/hr/agent/get_agent.rs src/handlers/hr/agent/list_agents.rs
git commit -m "feat: read runtime state from Agent entity in handlers"
```

---

### Task 9: 消息入队时检查 Agent 忙碌状态

**Files:**
- Modify: `src/service/domain/message/mod.rs`（或 `src/service/domain/message/delivery.rs`）

**Context:**
需要找到消息入队的方法，在入队前检查目标 Agent 是否忙碌。如果忙碌，返回业务错误提示。

先确认消息入队方法的准确位置：

```bash
grep -n "fn send\|fn enqueue\|fn deliver" src/service/domain/message/*.rs
```

假设入队方法是 `send_message` 或 `enqueue_message`，在方法开头添加：

- [ ] **Step 1: 找到消息入队方法**

Run: `grep -n "async fn send\|async fn enqueue" src/service/domain/message/*.rs`
Expected: 找到目标方法位置

- [ ] **Step 2: 在入队方法中增加忙碌检查**

```rust
use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;
use common::enums::AgentRuntimeState;

// 在消息入队方法中，检查目标 Agent 是否忙碌
if let Some(agent_id) = &command.agent_id {
    if AgentRuntimeStateManager::global().is_unavailable(agent_id) {
        let state = AgentRuntimeStateManager::global().get_state(agent_id);
        let state_name = match state {
            AgentRuntimeState::Busy => "忙碌中",
            AgentRuntimeState::Resting => "休息中",
            _ => "不可用",
        };
        return Err(common::error::Error::business(
            format!("Agent 当前{}，无法处理新消息", state_name)
        ));
    }
}
```

注意：具体的参数名（`command.agent_id`）需要根据实际入队方法的签名调整。

- [ ] **Step 3: 编译检查**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo check --lib`
Expected: PASS

- [ ] **Step 4: 提交**

```bash
git add src/service/domain/message/*.rs
git commit -m "feat: reject message enqueue when target Agent is busy or resting"
```

---

### Task 10: 运行全部测试

- [ ] **Step 1: 运行全部测试**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo test --lib 2>&1 | grep -E "(FAILED|passed|failed|test result)" | tail -10`
Expected: 全部测试通过（或已知失败的数量不变）

- [ ] **Step 2: 如有测试失败则修复**

常见的需要修复的点：
1. Agent API DTO 新增字段后，测试中构造 DTO 的地方需要同步更新
2. `common/src/api/agent_test.rs` 中的断言可能需要更新
3. Agent 实体新增字段后，测试中构造 Agent 的地方需要更新

- [ ] **Step 3: 最终提交**

```bash
git add -A
git commit -m "feat: Agent runtime state management complete

- AgentRuntimeState enum: Idle / Resting / Busy
- In-memory state manager with global singleton
- State updated in awaken lifecycle (Busy -> Idle)
- runtime_info field added to Agent entity
- DAL injects runtime state from memory
- Handlers read from entity, not from state manager
- Messages rejected when target Agent is unavailable"
```

---

## Self-Review Checklist

**1. Spec coverage:**
- ✅ 枚举定义在 common 中 — Task 1
- ✅ 状态机在后端（纯内存）— Task 2
- ✅ Agent 实体增加运行时信息字段 — Task 6
- ✅ DAL 层注入运行时状态 — Task 7
- ✅ Handler 从实体读取 — Task 8
- ✅ Agent 忙碌时拒绝新消息 — Task 9
- ✅ 区分空闲和休息状态 — Task 1 (Idle vs Resting)

**2. Placeholder scan:**
- ✅ 无 "TBD", "TODO", "implement later"
- ✅ 每个步骤都有具体代码
- ✅ 每个步骤都有运行命令和期望输出

**3. Type consistency:**
- ✅ `AgentRuntimeState` 在 common 中定义，前后端共享
- ✅ `AgentRuntimeInfo` 在 backend pkg 中定义，只在 Agent 实体中使用
- ✅ 状态值统一用 `i32` 传输（序列化为 JSON 数字）
- ✅ `AgentRuntimeStateManager::global()` 在 awaken 和 DAL 中引用一致
- ✅ Handler 层不直接访问状态管理器，符合分层规范
