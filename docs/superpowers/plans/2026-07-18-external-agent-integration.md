# 外部 Agent 接入实施计划 v5（Codex CLI + 通用 A2A Client）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 ai_orz 组织能注册并调用外部 Agent（先支持 Codex CLI 子进程包装，再支持通用 A2A HTTP Client），复用现有 awaken 链路统一执行。

**阶段划分：**
- **第一阶段（已完成 ✅）：Agent Runtime DAO 层**
  - 独立的 `dao/agent_runtime/` 文件夹，与 `dao/agent/` 平级
  - `AgentRuntimeDao` trait + `CodexRuntimeDao` + `A2aRuntimeDao`
  - trait 方法签名：`invoke(&self, ctx, agent: &AgentPo, prompt) -> Result<String>`
  - 16 个单元测试全部通过
- **第二阶段（已完成 ✅）：数据模型 + ExternalCortex**
  - AgentKind 枚举 + 数据库迁移
  - AgentRuntimeConfig 扩展 external_config + AgentPo 便捷方法
  - ExternalCortex 虚拟 Cortex 实现（备用）
- **第三阶段（已完成 ✅）：Brain 内部分发 + PromptBuilder trait**
  - **Brain 结构改造**（v5）：cortex 可空，新增 kind/agent_id/agent_name/runtime_config，内部分发
  - **BrainDal 统一入口**（v5）：think 根据 brain.kind 内部分发到 cortex 或 runtime
  - **PromptBuilder trait 抽象**：定义在 models 层，实现在各 Agent dal
  - **派生 Dal**：CodexAgentDal + A2aAgentDal 提供专属 PromptBuilder
  - **RuntimeDomain.awaken 统一调用**：只调 `brain_dal.think`，不再区分
- **第四阶段（已完成 ✅）：Handler + 测试**
  - 创建外部 Agent 的 HTTP handler
  - 单元测试验证
- **第五阶段（已完成 ✅）：前端页面支持**
  - Agent 列表页：类型徽章列 + 外部 Agent 创建入口（CLI/Remote 动态表单）
  - Agent 详情页：类型标签 + 运行时配置展示（CLI/Remote 分支渲染）
  - 前后端 API 类型扩展：`kind` + `external_config` 字段贯穿全链路

**Architecture (v5 更新):**

### 核心架构决策（v5）

1. **Brain 内部分发，上层统一入口**：
   - Brain 持有 `kind`（分发依据）+ `runtime_config`（运行时配置）+ `Option<Cortex>`（仅 Local 有）
   - `BrainDal.think()` 内部根据 `brain.kind` 分发：
     - Local → `cortex_dao.prompt()`
     - Cli → `execute_cli()`
     - Remote → `execute_a2a()`
   - 上层（RuntimeDomain.awaken）只调 `brain_dal.think(brain, prompt)`，完全统一

2. **外部 Agent 也装配 Brain，但 cortex 为 None**：
   - Brain 不仅是 cortex + memory，更是"思考执行环境"的统一抽象
   - 外部 Agent 通过 `Brain::new_external(kind, agent_id, agent_name, runtime_config, memories)` 构造
   - Local Agent 通过 `Brain::new_local(agent_id, agent_name, runtime_config, cortex, memories)` 构造

3. **wake_brain 接收 AgentPo**：
   - `BrainDal.wake_brain(ctx, agent: &AgentPo, memories, tools)` → 内部按 kind 构造 Brain
   - Local 分支：从 agent.model_provider_id 加载 provider，创建 cortex
   - 外部分支：直接用 runtime_config 构造，不创建 cortex

4. **runtime_config 存入 Brain**：
   - `AgentRuntimeConfig` 在 Brain 中存一份运行时副本
   - Local agent：runtime_config 用于 max_thinking_depth 等参数（当前暂未使用，预留）
   - 外部 agent：runtime_config.external_config 用于执行时读取配置

5. **PromptBuilder trait 抽象**：
   - trait 定义在 models 层，具体实现在各 Agent dal 中
   - 各 Agent dal 提供 `prompt_builder()` 方法返回对应 builder
   - RuntimeDomain 根据 agent.kind 路由到对应 dal 获取 builder

### 调用链路（v5）

```
Consumer 收到 TaskAssignment 消息
  ↓
RuntimeDomain.awaken(ctx, agent, message)
  │
  ├─ Step 1-3: 加载记忆、技能、工具等原始数据
  │
  ├─ Step 4: 获取 PromptBuilder（根据 agent.kind 路由）
  │    ├─ Local  → DefaultPromptBuilder
  │    ├─ Cli    → CliPromptBuilder
  │    └─ Remote → RemotePromptBuilder
  │    用 builder 组装 prompt
  │
  └─ Step 5: brain_dal.think(ctx, brain, prompt)  ← 统一入口，内部分发
       ├─ Local  → cortex_dao.prompt()
       ├─ Cli    → execute_cli()
       └─ Remote → execute_a2a()
```

**Tech Stack:** Rust + tokio（已有 `features=["full"]`，含 process）、reqwest 0.12（已有，用于 A2A HTTP）、sqlx（migration）、现有 ai_orz Domain/Consumer 层

---

## 范围说明

**本计划覆盖（P0 + P1）：**
- P0: 补全 brain 装配链路（Local agent 也受益）
- P0: Codex CLI 接入（Cli kind）
- P1: 通用 A2A HTTP Client 接入（Remote kind）
- 神经工具策略 L1：通过 PromptBuilder 注入工具描述/技能/短期记忆到外部 agent prompt

**不在本计划范围（后续独立计划）：**
- P2: A2A Server（让 ai_orz 组织本身暴露 A2A endpoint）
- P3: 跨组织路由（`OrganizationScope::Remote` 实际实现）
- L2 神经工具：解析外部 agent 输出中的工具调用意图并代理执行

---

## 调研结论（实施前必读）

### 调研 1：虚拟 Brain 可行性 ✅

- `CortexTrait` 是纯 trait（6 方法），不依赖 `ModelProvider`
- `BrainDal::think()` 读取 `brain.cortex.model_provider` **仅用于 enrich_ctx 和 debug 日志**，实际推理调用 `brain.cortex_trait()`
- `Brain::new(cortex, Vec::new())` 已是合法构造方式
- `ModelProvider` 是纯配置 struct，可用 dummy 值构造
- `Brain.memories` 从不被 think() 读取，记忆由 awaken() 独立加载注入 prompt

**结论**：可构造 `ExternalCortex` 实现 `CortexTrait`，搭配 dummy `ModelProvider`（id 为 `"external:{agent_id}"`）构造虚拟 Brain。

### 调研 2：AgentPo 字段 ✅

- `capabilities` 字段**已存在**（`src/models/agent.rs:273`），JSON string 存储 `Vec<String>`
- `runtime_config` 是 JSON string，解析为 `AgentRuntimeConfig`，无 `#[serde(flatten)]`
- **无** `metadata`/`extensions`/`extra` 字段
- agents 表从未被 ALTER 过

**结论**：复用 `capabilities` 字段；新增 `kind` 列；`AgentRuntimeConfig` 新增 `external_config` 字段（serde 向后兼容）。

### 调研 3：Brain 装配链路缺失 ⚠️

- `AgentDal::wake_brain` 和 `BrainDal::wake_brain` **都没有生产调用者**
- `consumer/message.rs:250` 和 `awakening.rs:96-99` 要求 `agent.brain` 必须是 Some
- 当前生产环境处理 agent 消息会直接报错"Agent 大脑未唤醒"

**结论**：本计划需要**补全 brain 装配链路**。补全方式：`HrDomain.get_agent()` 返回 agent 时按 kind 路由到对应 Dal 装配 brain。Local agent 走现有 `BrainDal.wake_brain()`，Cli/Remote agent 走派生 Dal 的 wake_brain()。

### 调研 4：神经工具策略

- 第一版（L1）：通过 PromptBuilder 注入神经工具描述、技能、短期记忆到外部 agent prompt
- 外部 agent 通过自然语言"看到"这些能力，但实际工具调用由外部 agent 自己的内部机制完成
- L2（后续）：派生 Dal 的 `parse_response()` 解析外部 agent 输出中的工具调用意图

---

## 文件结构

### 新建文件

| 文件 | 职责 |
|------|------|
| `common/src/enums/agent_kind.rs` | `AgentKind` 枚举（Local/Cli/Remote）✅ 已完成 |
| `migrations/20260719000000_add_kind_to_agents.sql` | agents 表新增 kind 列 ✅ 已完成 |
| `src/service/dao/cortex/external.rs` | `ExternalCortex` 实现 `CortexTrait` ✅ 已完成（注：v4 中此文件保留但不用于 Brain 装配，仅作为备用实现） |
| `src/service/dao/agent_runtime/mod.rs` | `AgentRuntimeDao` trait + 子模块声明 ✅ 已完成 |
| `src/service/dao/agent_runtime/codex.rs` | Codex CLI 执行逻辑 ✅ 已完成 |
| `src/service/dao/agent_runtime/a2a.rs` | A2A 执行逻辑 ✅ 已完成 |
| `src/models/prompt_builder.rs` | `PromptBuilder` trait 定义（纯抽象）🆕 v4 |
| `src/service/dal/agent_codex.rs` | `CodexAgentDal` 派生 Dal（管理操作 + 提供 CliPromptBuilder） |
| `src/service/dal/agent_a2a.rs` | `A2aAgentDal` 派生 Dal（管理操作 + 提供 RemotePromptBuilder） |
| `src/service/dal/prompt_builder_default.rs` | `DefaultPromptBuilder` 实现（Local agent 用）🆕 v4 |
| `common/src/api/external_agent.rs` | API 请求/响应类型 |
| `src/handlers/hr/agent/create_external_agent.rs` | `POST /api/v1/agents/external` HTTP handler |

### 修改文件

| 文件 | 改动 |
|------|------|
| `common/src/enums/mod.rs` | 注册 `agent_kind` 模块 ✅ 已完成 |
| `src/models/agent.rs` | `AgentRuntimeConfig` 新增 `external_config`；`AgentPo` 新增 `kind` 字段 + 便捷方法 ✅ 已完成 |
| `src/models/mod.rs` | 注册 `prompt_builder` 模块 🆕 v4 |
| `src/service/dao/agent_runtime/mod.rs` | `AgentRuntimeDao` trait 方法签名改为接收 `&AgentPo` ✅ 已完成 |
| `src/service/dao/cortex/mod.rs` | 注册 `external` 模块 ✅ 已完成 |
| `src/service/dao/mod.rs` | 注册 `agent_runtime` 模块 ✅ 已完成 |
| `src/service/dal/brain.rs` | **新增 `invoke_external` 方法**，持有 `AgentRuntimeDao` 🆕 v4 |
| `src/service/dal/agent.rs` | 新增 `wake_agent_brain` 便捷方法（Local agent brain 装配） |
| `src/service/dal/mod.rs` | 注册 `agent_codex` + `agent_a2a` + `prompt_builder_default` 模块 🆕 v4 |
| `src/service/domain/runtime/awakening.rs` | **awaken 内部按 agent.kind 路由**：Local 走 think，外部走 invoke_external 🆕 v4 |
| `src/service/domain/runtime/context_assembly.rs` | `PromptBuilder` 改为实现 trait，保留链式 API 作为具体实现的额外方法 🆕 v4 |
| `src/service/domain/hr/agent.rs` | `get_agent` 对外部 agent 不装配 brain；`create_agent` 对外部 agent 跳过 model_provider 校验 🆕 v4 |
| `src/service/domain/hr/mod.rs` | `HrDomainImpl` 新增 `codex_agent_dal` + `a2a_agent_dal` 字段 🆕 v4 |
| `src/handlers/hr/agent/mod.rs` | 注册新 handler |
| `src/handlers/mod.rs` | 路由注册 |
| `src/main.rs` | 初始化派生 Dal，注入 HrDomain |

---

## Task 1: 新增 AgentKind 枚举

**Files:**
- Create: `common/src/enums/agent_kind.rs`
- Modify: `common/src/enums/mod.rs`

- [ ] **Step 1: 写 AgentKind 实现 + 测试**

Create `common/src/enums/agent_kind.rs`:

```rust
//! Agent 类型枚举：决定 Agent 的执行后端

use std::fmt;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
#[cfg(feature = "sqlx")]
use sqlx::Type;

/// Agent 类型：决定 Agent 的执行后端
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, JsonSchema)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum AgentKind {
    /// 本地 Agent（ai_orz 内部 Brain + Tools 执行）
    #[default]
    Local = 0,
    /// CLI Agent（子进程包装，如 Codex / Claude Code / Aider）
    Cli = 1,
    /// 远程 Agent（通过 A2A 协议调用的外部 Agent）
    Remote = 2,
}

impl AgentKind {
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }

    /// 是否为外部 Agent（需要外部执行器）
    pub fn is_external(&self) -> bool {
        matches!(self, AgentKind::Cli | AgentKind::Remote)
    }
}

impl From<i32> for AgentKind {
    fn from(v: i32) -> Self {
        match v {
            1 => AgentKind::Cli,
            2 => AgentKind::Remote,
            _ => AgentKind::Local,
        }
    }
}

impl fmt::Display for AgentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentKind::Local => write!(f, "local"),
            AgentKind::Cli => write!(f, "cli"),
            AgentKind::Remote => write!(f, "remote"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_local() {
        assert_eq!(AgentKind::default(), AgentKind::Local);
    }

    #[test]
    fn test_is_external() {
        assert!(!AgentKind::Local.is_external());
        assert!(AgentKind::Cli.is_external());
        assert!(AgentKind::Remote.is_external());
    }

    #[test]
    fn test_from_i32() {
        assert_eq!(AgentKind::from(0), AgentKind::Local);
        assert_eq!(AgentKind::from(1), AgentKind::Cli);
        assert_eq!(AgentKind::from(2), AgentKind::Remote);
        assert_eq!(AgentKind::from(99), AgentKind::Local);
    }

    #[test]
    fn test_to_i32() {
        assert_eq!(AgentKind::Local.to_i32(), 0);
        assert_eq!(AgentKind::Cli.to_i32(), 1);
        assert_eq!(AgentKind::Remote.to_i32(), 2);
    }
}
```

- [ ] **Step 2: 注册模块**

Modify `common/src/enums/mod.rs`，在已有 `pub mod agent;` 附近添加：

```rust
pub mod agent_kind;
pub use agent_kind::AgentKind;
```

- [ ] **Step 3: 运行测试验证通过**

Run: `cargo test -p common agent_kind -- --nocapture`
Expected: PASS（4 个测试全过）

- [ ] **Step 4: Commit**

```bash
git add common/src/enums/agent_kind.rs common/src/enums/mod.rs
git commit -m "feat: 新增 AgentKind 枚举（Local/Cli/Remote）"
```

---

## Task 2: 创建 migration（agents 表新增 kind 列）

**Files:**
- Create: `migrations/20260719000000_add_kind_to_agents.sql`

- [ ] **Step 1: 写 migration 文件**

Create `migrations/20260719000000_add_kind_to_agents.sql`:

```sql
-- Agent 表新增 kind 列：区分本地/CLI/远程 Agent
-- 0 = Local（默认，ai_orz 内部 Brain 执行）
-- 1 = Cli（子进程包装，如 Codex）
-- 2 = Remote（A2A 协议远程调用）
ALTER TABLE agents ADD COLUMN kind INTEGER NOT NULL DEFAULT 0;
```

- [ ] **Step 2: 验证 migration 可执行**

Run: `cargo build`
然后删除 DB 重新启动验证 migration 应用：

```bash
rm -f .ai_orz/ai_orz.db
cargo run &
sleep 5
kill %1
sqlite3 .ai_orz/ai_orz.db "PRAGMA table_info(agents);" | grep kind
```

Expected: 输出包含 `kind|INTEGER|1|0|0`（列存在且默认 0）

- [ ] **Step 3: Commit**

```bash
git add migrations/20260719000000_add_kind_to_agents.sql
git commit -m "feat: migration 新增 agents.kind 列"
```

---

## Task 3: 扩展 AgentRuntimeConfig + AgentPo 新增 kind 字段

**Files:**
- Modify: `src/models/agent.rs`

- [ ] **Step 1: 写失败测试**

在 `src/models/agent.rs` 测试模块中添加：

```rust
#[cfg(test)]
mod external_config_tests {
    use super::*;
    use common::enums::AgentKind;

    #[test]
    fn test_external_config_default_is_none() {
        let config = AgentRuntimeConfig::default();
        assert!(config.external_config.is_none());
    }

    #[test]
    fn test_external_config_cli_serialize_deserialize() {
        let mut config = AgentRuntimeConfig::default();
        config.external_config = Some(ExternalAgentConfig::Cli {
            command: "codex".to_string(),
            args: vec!["exec".to_string()],
            work_dir: "/tmp/codex-work".to_string(),
            env: vec![("CODEX_API_KEY".to_string(), "xxx".to_string())],
            timeout_secs: 300,
            prompt_template: None,
        });
        let json = serde_json::to_string(&config).unwrap();
        let decoded: AgentRuntimeConfig = serde_json::from_str(&json).unwrap();
        match decoded.external_config.unwrap() {
            ExternalAgentConfig::Cli { command, args, work_dir, env, timeout_secs, prompt_template } => {
                assert_eq!(command, "codex");
                assert_eq!(args, vec!["exec".to_string()]);
                assert_eq!(work_dir, "/tmp/codex-work");
                assert_eq!(env.len(), 1);
                assert_eq!(timeout_secs, 300);
                assert!(prompt_template.is_none());
            }
            _ => panic!("expected Cli variant"),
        }
    }

    #[test]
    fn test_external_config_remote_serialize_deserialize() {
        let mut config = AgentRuntimeConfig::default();
        config.external_config = Some(ExternalAgentConfig::Remote {
            endpoint: "https://other-agent.com".to_string(),
            agent_name: "remote-bot".to_string(),
            auth_token: Some("token123".to_string()),
            timeout_secs: 60,
        });
        let json = serde_json::to_string(&config).unwrap();
        let decoded: AgentRuntimeConfig = serde_json::from_str(&json).unwrap();
        match decoded.external_config.unwrap() {
            ExternalAgentConfig::Remote { endpoint, agent_name, auth_token, timeout_secs } => {
                assert_eq!(endpoint, "https://other-agent.com");
                assert_eq!(agent_name, "remote-bot");
                assert_eq!(auth_token, Some("token123".to_string()));
                assert_eq!(timeout_secs, 60);
            }
            _ => panic!("expected Remote variant"),
        }
    }

    #[test]
    fn test_external_config_backward_compatible() {
        let old_json = r#"{"max_thinking_depth":10,"thinking_interval_ms":0,"max_tool_calls_per_step":5,"enable_reflection":false,"require_user_confirm":true,"installed_tags":[],"installed_skill_packs":[]}"#;
        let config: AgentRuntimeConfig = serde_json::from_str(old_json).unwrap();
        assert!(config.external_config.is_none());
    }

    #[test]
    fn test_agent_po_new_defaults_kind_to_local() {
        let po = AgentPo::new(
            "TestBot".to_string(),
            vec!["coder".to_string()],
            "desc".to_string(),
            vec!["code".to_string()],
            "soul".to_string(),
            "provider-1".to_string(),
            "user-1".to_string(),
        );
        assert_eq!(po.kind, AgentKind::Local);
    }

    #[test]
    fn test_agent_po_kind_can_be_overridden() {
        let mut po = AgentPo::new(
            "CodexBot".to_string(),
            vec!["coder".to_string()],
            "desc".to_string(),
            vec!["code".to_string()],
            "soul".to_string(),
            String::new(),
            "user-1".to_string(),
        );
        po.kind = AgentKind::Cli;
        assert_eq!(po.kind, AgentKind::Cli);
    }
}
```

- [ ] **Step 2: 运行测试验证失败**

Run: `cargo test --lib external_config_tests`
Expected: FAIL（`ExternalAgentConfig` 未定义、`po.kind` 字段不存在）

- [ ] **Step 3: 实现 ExternalAgentConfig + 扩展 AgentRuntimeConfig + AgentPo**

在 `src/models/agent.rs` 中，`AgentRuntimeConfig` struct 定义之前，新增：

```rust
/// 外部 Agent 执行配置（仅 AgentKind::Cli / Remote 时使用）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "executor", rename_all = "snake_case")]
pub enum ExternalAgentConfig {
    /// CLI 子进程执行器配置
    Cli {
        /// 启动命令（如 "codex"、"claude"、"aider"）
        command: String,
        /// 命令参数
        args: Vec<String>,
        /// 工作目录（绝对路径）
        work_dir: String,
        /// 环境变量（key, value 列表）
        env: Vec<(String, String)>,
        /// 超时时间（秒），0 表示不超时
        timeout_secs: u64,
        /// 自定义 prompt 模板（None 用默认模板）
        prompt_template: Option<String>,
    },
    /// A2A 远程执行器配置
    Remote {
        /// A2A Server 的 base URL（如 "https://other-agent.com"）
        endpoint: String,
        /// 目标 Agent Card 中的 name（用于定位 agent）
        agent_name: String,
        /// 认证 token（Bearer）
        auth_token: Option<String>,
        /// 超时时间（秒），0 表示不超时
        timeout_secs: u64,
    },
}
```

修改 `AgentRuntimeConfig` struct，在 `installed_skill_packs` 字段后新增：

```rust
    /// 外部 Agent 执行配置（仅 Cli/Remote kind 时使用）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_config: Option<ExternalAgentConfig>,
```

修改 `impl Default for AgentRuntimeConfig`，在 `installed_skill_packs: Vec::new(),` 后新增：

```rust
            external_config: None,
```

修改 `AgentPo` struct，在 `status: AgentStatus` 字段后新增：

```rust
    /// Agent 类型（Local/Cli/Remote）
    pub kind: common::enums::AgentKind,
```

修改 `AgentPo::new()` 方法（约 line 308-333），在返回的 `Self { ... }` 中新增 `kind: common::enums::AgentKind::Local,`：

```rust
    pub fn new(
        name: String,
        roles: Vec<String>,
        description: String,
        capabilities: Vec<String>,
        soul: String,
        model_provider_id: String,
        creator: String,
    ) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            name,
            role: serde_json::to_string(&roles).unwrap_or_default(),
            description,
            capabilities: serde_json::to_string(&capabilities).unwrap_or_default(),
            soul,
            model_provider_id,
            runtime_config: "{}".to_string(),
            status: AgentStatus::Interviewing,
            kind: common::enums::AgentKind::Local,
            created_by: creator,
            modified_by: creator,
            created_at: now,
            updated_at: now,
        }
    }
```

- [ ] **Step 4: 更新 DAO 层 SQL 查询（包含 kind 列）**

Run: `grep -rn "INSERT INTO agents\|SELECT.*FROM agents\|UPDATE agents" src/service/dao/agent/`

对 `src/service/dao/agent/sqlite.rs` 中所有 SQL：
- INSERT 语句：新增 `kind` 列名和占位符
- SELECT 语句：新增 `kind` 列
- UPDATE 语句：新增 `kind` 设置子句

示例（SQLite INSERT，约 line 67）：

```rust
// 修改前
let sql = r#"INSERT INTO agents (id, name, role, description, soul, capabilities, runtime_config, model_provider_id, status, created_by, modified_by, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#;

// 修改后
let sql = r#"INSERT INTO agents (id, name, role, description, soul, capabilities, runtime_config, model_provider_id, status, kind, created_by, modified_by, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#;
```

bind 调用新增 `agent_po.kind.to_i32()`。

SELECT 语句（约 line 96, 114, 210）新增 `kind` 列，FromRow 解析时新增 `kind: row.kind`（注意：sqlx Type 派生需要 `AgentKind` 实现 `Type<Sqlite>`，已在 Step 1 通过 `#[cfg_attr(feature = "sqlx", derive(Type))]` 处理）。

UPDATE 语句（约 line 318）新增 `kind = ?` 设置子句。

DuckDB DAO 文件（`src/service/dao/agent/duckdb.rs` 如果存在）同样处理。

- [ ] **Step 5: 运行测试验证通过**

Run: `cargo test --lib external_config_tests`
Expected: PASS（6 个测试全过）

Run: `cargo build`
Expected: 编译通过

- [ ] **Step 6: Commit**

```bash
git add src/models/agent.rs src/service/dao/agent/
git commit -m "feat: AgentRuntimeConfig 新增 external_config，AgentPo 新增 kind 字段"
```

---

## Task 4: AgentRuntimeDao trait + CodexRuntimeDao 实现

**Files:**
- Create: `src/service/dao/agent/runtime/mod.rs`
- Create: `src/service/dao/agent/runtime/codex.rs`
- Modify: `src/service/dao/agent/mod.rs`

- [ ] **Step 1: 写 AgentRuntimeDao trait**

Create `src/service/dao/agent/runtime/mod.rs`:

```rust
//! Agent Runtime DAO：外部 Agent 执行层抽象
//!
//! 仅负责"执行 prompt 返回原始输出"，不处理消息转换。
//! 消息体转换（build_prompt / parse_response）由各自派生 Dal 内聚处理。

pub mod codex;
pub mod a2a;

use async_trait::async_trait;
use common::error::Result;
use crate::pkg::RequestContext;

/// 外部 Agent 执行器 trait
#[async_trait]
pub trait AgentRuntimeDao: Send + Sync {
    /// 执行 prompt，返回原始输出
    ///
    /// 输入：完整 prompt 字符串（由派生 Dal 的 build_prompt 构造）
    /// 输出：外部 agent 的原始输出（CLI stdout 或 A2A task artifact text）
    async fn invoke(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        prompt: &str,
    ) -> Result<String>;
}
```

- [ ] **Step 2: 写 CodexRuntimeDao 实现 + 测试**

Create `src/service/dao/agent/runtime/codex.rs`:

```rust
//! Codex CLI Runtime DAO：通过子进程 stdin/stdout 调用 Codex 等 CLI agent
//!
//! 工作流程：
//! 1. 拉起子进程（command + args），设置 work_dir / env
//! 2. 通过 stdin 传入完整 prompt
//! 3. 收集 stdout 作为输出
//! 4. 带超时控制

use std::time::Duration;
use async_trait::async_trait;
use tokio::process::Command;

use common::error::{err, Result};
use crate::pkg::RequestContext;

use super::AgentRuntimeDao;

/// Codex CLI Runtime DAO
pub struct CodexRuntimeDao;

impl CodexRuntimeDao {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl AgentRuntimeDao for CodexRuntimeDao {
    async fn invoke(
        &self,
        _ctx: RequestContext,
        agent_id: &str,
        prompt: &str,
    ) -> Result<String> {
        // 注：实际配置（command/args/work_dir/env/timeout）由 CodexAgentDal 从 agent.po.runtime_config
        // 解析后，通过 Self 的扩展方法或重构为持有配置的实例来使用。
        // 这里 invoke 仅做执行；配置解析在 CodexAgentDal.wake_brain 时一次性完成并装配到 ExternalCortex。
        //
        // 但由于 RuntimeDao 是无状态 trait，我们把配置作为 ExternalCortex 的字段持有，
        // ExternalCortex.prompt() 内部调用具体执行逻辑（见 Task 5）。
        //
        // 因此这个 trait 实际上由 ExternalCortex 直接完成执行，
        // RuntimeDao 提供的是"执行入口"的抽象，便于测试 mock。
        //
        // 这里 invoke 不会被实际调用（ExternalCortex 直接执行），
        // 但保留 trait 以备未来需要把执行层独立化的场景。
        Err(err!(Internal, "CodexRuntimeDao.invoke should not be called directly; ExternalCortex handles execution"))
    }
}

/// 实际执行 Codex CLI 子进程的逻辑（由 ExternalCortex 调用）
pub async fn execute_cli(
    agent_id: &str,
    command: &str,
    args: &[String],
    work_dir: &str,
    env: &[(String, String)],
    timeout_secs: u64,
    prompt: &str,
) -> Result<String> {
    let mut cmd = Command::new(command);
    cmd.args(args)
       .current_dir(work_dir)
       .stdin(std::process::Stdio::piped())
       .stdout(std::process::Stdio::piped())
       .stderr(std::process::Stdio::piped());

    for (k, v) in env {
        cmd.env(k, v);
    }

    let mut child = cmd.spawn().map_err(|e: std::io::Error|
        err!(Internal, "Failed to spawn CLI agent '{}' for agent {}: {}", command, agent_id, e)
    )?;

    // 通过 stdin 传入 prompt
    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        let _ = stdin.write_all(prompt.as_bytes()).await;
        // stdin drop 自动关闭
    }

    // 等待退出（带超时）
    let wait_fut = child.wait_with_output();
    let output = if timeout_secs > 0 {
        tokio::time::timeout(Duration::from_secs(timeout_secs), wait_fut)
            .await
            .map_err(|_| err!(Internal, "CLI agent '{}' timed out after {}s for agent {}", command, timeout_secs, agent_id))?
            .map_err(|e: std::io::Error| err!(Internal, "CLI agent wait failed for agent {}: {}", agent_id, e))?
    } else {
        wait_fut.await.map_err(|e: std::io::Error| err!(Internal, "CLI agent wait failed for agent {}: {}", agent_id, e))?
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(err!(Internal, "CLI agent '{}' exited with {} for agent {}, stderr: {}", command, output.status, agent_id, stderr));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_cli_with_cat() {
        // cat 命令读 stdin 写 stdout，模拟 agent 处理 prompt
        let result = execute_cli(
            "test-agent",
            "cat",
            &[],
            "/tmp",
            &[],
            10,
            "hello from test",
        ).await.unwrap();
        assert!(result.contains("hello from test"));
    }

    #[tokio::test]
    async fn test_execute_cli_timeout() {
        let result = execute_cli(
            "test-agent",
            "sleep",
            &["10".to_string()],
            "/tmp",
            &[],
            1,
            "hello",
        ).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("timed out"), "expected timeout, got: {}", err_msg);
    }

    #[tokio::test]
    async fn test_execute_cli_command_not_found() {
        let result = execute_cli(
            "test-agent",
            "this-command-does-not-exist-xyz",
            &[],
            "/tmp",
            &[],
            5,
            "hello",
        ).await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 3: 注册 runtime 子模块**

在 `src/service/dao/agent/mod.rs` 末尾添加：

```rust
pub mod runtime;
```

- [ ] **Step 4: 运行测试验证通过**

Run: `cargo test --lib runtime::codex::tests`
Expected: PASS（3 个测试全过：cat、timeout、command not found）

- [ ] **Step 5: Commit**

```bash
git add src/service/dao/agent/runtime/ src/service/dao/agent/mod.rs
git commit -m "feat: 新增 AgentRuntimeDao trait + CodexRuntimeDao 执行逻辑"
```

---

## Task 5: ExternalCortex 实现 CortexTrait

**Files:**
- Create: `src/models/external_cortex.rs`
- Modify: `src/models/mod.rs`

- [ ] **Step 1: 写 ExternalCortex + 测试**

Create `src/models/external_cortex.rs`:

```rust
//! ExternalCortex: 为外部 Agent（Cli/Remote）实现的虚拟 Cortex
//!
//! 实现 CortexTrait，但不依赖任何 ModelProvider。
//! prompt() 时根据 ExternalAgentConfig 调用对应的执行后端：
//! - Cli: 调用 crate::service::dao::agent::runtime::codex::execute_cli
//! - Remote: 调用 crate::service::dao::agent::runtime::a2a::execute_a2a（Task 6 添加）

use anyhow::Result as AnyhowResult;
use async_trait::async_trait;
use common::enums::ModelCapability;
use dyn_clone::DynClone;

use crate::models::agent::ExternalAgentConfig;
use crate::models::brain::CortexTrait;

/// 外部 Agent 的虚拟 Cortex
#[derive(Clone)]
pub struct ExternalCortex {
    /// Agent ID（用于日志和错误信息）
    pub agent_id: String,
    /// 外部执行配置
    pub config: ExternalAgentConfig,
}

impl ExternalCortex {
    pub fn new(agent_id: String, config: ExternalAgentConfig) -> Self {
        Self { agent_id, config }
    }
}

#[async_trait]
impl CortexTrait for ExternalCortex {
    fn capability(&self) -> ModelCapability {
        // 外部 agent 假定为 Agent 能力（可对话可执行）
        ModelCapability::Agent
    }

    fn model_provider_id(&self) -> &str {
        // 返回虚拟 provider id（不对应真实记录）
        // AgentDal::wake_brain 会读取此值同步到 agent.po.model_provider_id
        "external"
    }

    fn model_name(&self) -> &str {
        match &self.config {
            ExternalAgentConfig::Cli { command, .. } => command,
            ExternalAgentConfig::Remote { agent_name, .. } => agent_name,
        }
    }

    async fn prompt(&self, prompt: &str) -> AnyhowResult<String> {
        match &self.config {
            ExternalAgentConfig::Cli { command, args, work_dir, env, timeout_secs, .. } => {
                crate::service::dao::agent::runtime::codex::execute_cli(
                    &self.agent_id,
                    command,
                    args,
                    work_dir,
                    env,
                    *timeout_secs,
                    prompt,
                )
                .await
                .map_err(|e| anyhow::anyhow!("Codex CLI execution failed: {}", e))
            }
            ExternalAgentConfig::Remote { endpoint, agent_name, auth_token, timeout_secs } => {
                crate::service::dao::agent::runtime::a2a::execute_a2a(
                    &self.agent_id,
                    endpoint,
                    agent_name,
                    auth_token,
                    *timeout_secs,
                    prompt,
                )
                .await
                .map_err(|e| anyhow::anyhow!("A2A execution failed: {}", e))
            }
        }
    }

    async fn embeddings(&self, _texts: &[String]) -> AnyhowResult<Vec<Vec<f32>>> {
        // 外部 agent 不支持 embeddings
        Err(anyhow::anyhow!("ExternalCortex does not support embeddings"))
    }

    fn support_tools(&self) -> bool {
        // 外部 agent 有自己的工具系统，不通过 rig 注入
        false
    }
}

dyn_clone::clone_trait_object!(CortexTrait);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_external_cortex_model_name_for_cli() {
        let cortex = ExternalCortex::new(
            "test-agent".to_string(),
            ExternalAgentConfig::Cli {
                command: "codex".to_string(),
                args: vec![],
                work_dir: "/tmp".to_string(),
                env: vec![],
                timeout_secs: 60,
                prompt_template: None,
            },
        );
        assert_eq!(cortex.model_name(), "codex");
        assert_eq!(cortex.model_provider_id(), "external");
        assert_eq!(cortex.capability(), ModelCapability::Agent);
        assert!(!cortex.support_tools());
    }

    #[test]
    fn test_external_cortex_model_name_for_remote() {
        let cortex = ExternalCortex::new(
            "test-agent".to_string(),
            ExternalAgentConfig::Remote {
                endpoint: "https://other.com".to_string(),
                agent_name: "remote-bot".to_string(),
                auth_token: None,
                timeout_secs: 60,
            },
        );
        assert_eq!(cortex.model_name(), "remote-bot");
    }

    #[tokio::test]
    async fn test_external_cortex_prompt_cli_with_cat() {
        let cortex = ExternalCortex::new(
            "test-agent".to_string(),
            ExternalAgentConfig::Cli {
                command: "cat".to_string(),
                args: vec![],
                work_dir: "/tmp".to_string(),
                env: vec![],
                timeout_secs: 10,
                prompt_template: None,
            },
        );
        let result = cortex.prompt("hello world").await.unwrap();
        assert!(result.contains("hello world"));
    }

    #[tokio::test]
    async fn test_external_cortex_embeddings_unsupported() {
        let cortex = ExternalCortex::new(
            "test-agent".to_string(),
            ExternalAgentConfig::Cli {
                command: "cat".to_string(),
                args: vec![],
                work_dir: "/tmp".to_string(),
                env: vec![],
                timeout_secs: 10,
                prompt_template: None,
            },
        );
        let result = cortex.embeddings(&["test".to_string()]).await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: 注册模块**

在 `src/models/mod.rs` 添加：

```rust
pub mod external_cortex;
```

- [ ] **Step 3: 运行测试验证通过**

Run: `cargo test --lib external_cortex::tests`
Expected: PASS（4 个测试全过）

- [ ] **Step 4: Commit**

```bash
git add src/models/external_cortex.rs src/models/mod.rs
git commit -m "feat: 实现 ExternalCortex（虚拟 Brain 的 CortexTrait 实现）"
```

---

## Task 6: A2aRuntimeDao 实现

**Files:**
- Create: `src/service/dao/agent/runtime/a2a.rs`

- [ ] **Step 1: 实现 execute_a2a 函数**

Create `src/service/dao/agent/runtime/a2a.rs`:

```rust
//! A2A Runtime DAO：通过 HTTP JSON-RPC 调用支持 A2A 协议的远程 Agent
//!
//! 实现核心 A2A 方法 tasks/send（同步等待结果）。
//! Agent Card 发现（GET /.well-known/agent.json）暂未实现，agent_name 由配置直接指定。

use std::time::Duration;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

use common::error::{err, Result};

/// JSON-RPC 2.0 请求
#[derive(Serialize)]
struct JsonRpcRequest<'a, T> {
    jsonrpc: &'a str,
    id: u64,
    method: &'a str,
    params: T,
}

/// JSON-RPC 2.0 响应
#[derive(Deserialize)]
struct JsonRpcResponse<T> {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    #[allow(dead_code)]
    id: Option<u64>,
    result: Option<T>,
    error: Option<JsonRpcError>,
}

#[derive(Deserialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

/// A2A Task 对象（仅提取需要的字段）
#[derive(Debug, Deserialize)]
struct A2aTask {
    #[allow(dead_code)]
    id: String,
    #[allow(dead_code)]
    status: A2aTaskStatus,
    artifacts: Vec<A2aArtifact>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum A2aTaskStatus {
    Submitted,
    Working,
    InputRequired,
    Completed,
    Failed,
    Canceled,
}

#[derive(Debug, Deserialize)]
struct A2aArtifact {
    parts: Vec<A2aPart>,
}

#[derive(Debug, Deserialize)]
struct A2aPart {
    text: Option<String>,
}

/// tasks/send 请求参数
#[derive(Serialize)]
struct SendTaskParams {
    agent_id: Option<String>,
    message: A2aMessage,
}

#[derive(Serialize)]
struct A2aMessage {
    role: String,
    parts: Vec<A2aMessagePart>,
}

#[derive(Serialize)]
struct A2aMessagePart {
    #[serde(rename = "type")]
    part_type: String,
    text: String,
}

/// 执行 A2A tasks/send 调用
pub async fn execute_a2a(
    agent_id: &str,
    endpoint: &str,
    target_agent_name: &str,
    auth_token: &Option<String>,
    timeout_secs: u64,
    prompt: &str,
) -> Result<String> {
    let params = SendTaskParams {
        agent_id: Some(target_agent_name.to_string()),
        message: A2aMessage {
            role: "user".to_string(),
            parts: vec![A2aMessagePart {
                part_type: "text".to_string(),
                text: prompt.to_string(),
            }],
        },
    };

    let req = JsonRpcRequest {
        jsonrpc: "2.0",
        id: 1,
        method: "tasks/send",
        params,
    };

    let url = format!("{}/a2a", endpoint.trim_end_matches('/'));
    let headers = build_headers(auth_token)?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(if timeout_secs > 0 { timeout_secs } else { 300 }))
        .build()
        .map_err(|e| err!(Internal, "Failed to build HTTP client for agent {}: {}", agent_id, e))?;

    let send_fut = client.post(&url).headers(headers).json(&req).send();
    let resp = if timeout_secs > 0 {
        tokio::time::timeout(Duration::from_secs(timeout_secs), send_fut)
            .await
            .map_err(|_| err!(Internal, "A2A request timed out after {}s for agent {}", timeout_secs, agent_id))?
            .map_err(|e| err!(Internal, "A2A request failed for agent {}: {}", agent_id, e))?
    } else {
        send_fut.await.map_err(|e| err!(Internal, "A2A request failed for agent {}: {}", agent_id, e))?
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(err!(Internal, "A2A server returned {} for agent {}: {}", status, agent_id, body));
    }

    let rpc_resp: JsonRpcResponse<A2aTask> = resp.json().await
        .map_err(|e| err!(Internal, "Failed to parse A2A response for agent {}: {}", agent_id, e))?;

    if let Some(err_obj) = rpc_resp.error {
        return Err(err!(Internal, "A2A JSON-RPC error {} for agent {}: {}", err_obj.code, agent_id, err_obj.message));
    }

    let task = rpc_resp.result.ok_or_else(|| err!(Internal, "A2A response missing result for agent {}", agent_id))?;

    // 提取所有 artifact 的 text parts
    let output: String = task.artifacts
        .iter()
        .flat_map(|a| a.parts.iter().filter_map(|p| p.text.clone()))
        .collect::<Vec<_>>()
        .join("\n");

    Ok(output)
}

fn build_headers(auth_token: &Option<String>) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    if let Some(token) = auth_token {
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token))
                .map_err(|e| err!(Internal, "Invalid auth token: {}", e))?,
        );
    }
    Ok(headers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_a2a_invalid_endpoint_returns_error() {
        let result = execute_a2a(
            "test-agent",
            "http://127.0.0.1:1",  // 不可达端口
            "remote-bot",
            &None,
            2,
            "hello",
        ).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_a2a_timeout() {
        // 启动一个本地 HTTP 服务但故意不响应，触发超时
        // 这里简化测试：用不可达端口 + 短超时
        let result = execute_a2a(
            "test-agent",
            "http://127.0.0.1:1",
            "remote-bot",
            &None,
            1,
            "hello",
        ).await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: 运行测试验证通过**

Run: `cargo test --lib runtime::a2a::tests`
Expected: PASS（2 个测试全过：invalid endpoint、timeout）

- [ ] **Step 3: Commit**

```bash
git add src/service/dao/agent/runtime/a2a.rs
git commit -m "feat: 实现 A2aRuntimeDao（HTTP JSON-RPC 调用远程 A2A Agent）"
```

---

## Task 7: PromptBuilder trait 抽象 + DefaultPromptBuilder 实现

**设计原则（v4）：**
- trait 定义在 models 层（纯抽象，不含具体实现）
- 具体实现在各 Agent dal 中
- 各 Agent dal 提供 `prompt_builder()` 方法，仅完成对 builder 的构造和返回
- 工厂方法在 RuntimeDomain 中，上层根据 agent.kind 选择不同的 dal 生成相应的 builder

**Files:**
- Create: `src/models/prompt_builder.rs`
- Create: `src/service/dal/prompt_builder_default.rs`
- Modify: `src/models/mod.rs`
- Modify: `src/service/dal/mod.rs`
- Modify: `src/service/domain/runtime/context_assembly.rs`（现有 PromptBuilder 重构为 DefaultPromptBuilder）

---

### Step 1: 定义 PromptBuilder trait

Create `src/models/prompt_builder.rs`:

```rust
//! PromptBuilder trait - Prompt 组装器抽象
//!
//! 【定位】纯抽象 trait，定义 prompt 组装的标准接口。
//! 不同类型的 Agent（Local/Cli/Remote）可以提供各自的实现，
//! 通过各自的 Agent Dal 构造并返回。
//!
//! trait 方法采用 &mut self 风格（非链式返回 Self），
//! 以便支持 trait object（Box<dyn PromptBuilder>）。

use crate::models::agent::Agent;
use crate::models::memory::Memory;
use crate::models::message::Message;
use crate::models::skill::SkillPo;
use crate::models::user::UserPo;

/// Prompt 组装器 trait
///
/// 各 Agent Dal 提供具体实现，通过 `prompt_builder()` 方法返回。
/// RuntimeDomain 根据 agent.kind 选择对应的 Dal 生成 builder。
pub trait PromptBuilder: Send {
    /// 设置当前思考的 Trace ID（模型输出时可引用此 ID）
    fn current_trace_id(&mut self, trace_id: &str);

    /// 设置关联的 Trace ID 列表
    fn trace_ids(&mut self, trace_ids: &[String]);

    /// 设置 Agent 人设（System Prompt）
    fn system_prompt(&mut self, agent: &Agent);

    /// 设置历史对话记忆
    fn history(&mut self, memories: &[Memory]);

    /// 设置当前用户消息
    fn current_message(&mut self, message: &Message);

    /// 设置 Agent 可用技能
    fn agent_skills(&mut self, skills: &[SkillPo]);

    /// 设置 Agent 绑定的 Manual 工具
    fn bound_tools(&mut self, agent: &Agent);

    /// 设置工具失败统计
    fn tool_failures(&mut self, failures: &[(String, u64)]);

    /// 设置用户画像
    fn user_profile(&mut self, user: &UserPo);

    /// 构建最终 Prompt 字符串（消费 self）
    fn build(self) -> String;
}
```

### Step 2: 实现 DefaultPromptBuilder

将 `src/service/domain/runtime/context_assembly.rs` 中现有的 `PromptBuilder` struct 重命名为 `DefaultPromptBuilder`，并实现 `PromptBuilder` trait。

具体改动：
- struct `PromptBuilder` → `DefaultPromptBuilder`
- 所有方法从 `fn xxx(mut self, ...) -> Self` 改为 `fn xxx(&mut self, ...)`
- `build(self)` 保持消费 self
- 新增 `impl PromptBuilder for DefaultPromptBuilder`
- 保留链式 API 作为 DefaultPromptBuilder 的额外方法（不在 trait 中），方便测试和内部使用
- 保留便捷函数 `build_conversation_prompt`（内部使用 DefaultPromptBuilder）

Create `src/service/dal/prompt_builder_default.rs`（或直接在 context_assembly.rs 中实现，根据项目惯例决定）。

### Step 3: 注册模块

在 `src/models/mod.rs` 添加：
```rust
pub mod prompt_builder;
```

在 `src/service/dal/mod.rs` 添加（如独立文件）：
```rust
pub mod prompt_builder_default;
```

### Step 4: 运行测试验证

Run: `cargo test --lib context_assembly`
Expected: 现有测试全部通过（DefaultPromptBuilder 行为与原 PromptBuilder 一致）

### Step 5: Commit

```bash
git add src/models/prompt_builder.rs src/service/dal/prompt_builder_default.rs \
        src/models/mod.rs src/service/dal/mod.rs \
        src/service/domain/runtime/context_assembly.rs
git commit -m "feat: PromptBuilder 抽象为 trait + DefaultPromptBuilder 实现"
```

---

## Task 8: BrainDal 新增 invoke_external 方法

**设计原则（v4）：**
- BrainDal 作为统一调度入口，新增 `invoke_external` 方法
- 持有 `AgentRuntimeDao`（通过 Arc 注入），调用其 `invoke` 方法
- 与 `think` 共享审计日志、token 统计等逻辑
- BrainDal 只负责最终运行时调度，不参与管理操作

**Files:**
- Modify: `src/service/dal/brain.rs`

---

### Step 1: BrainDal trait 新增 invoke_external 方法

在 `BrainDal` trait 中新增：

```rust
    /// 调用外部 Agent 执行 prompt（Cli/Remote kind 专用）
    ///
    /// 与 think() 对应，但走 AgentRuntimeDao 而非 CortexDao。
    /// 外部 Agent 不需要 Brain，直接通过 AgentRuntimeDao 调用。
    /// 共享审计日志、统计等统一逻辑。
    async fn invoke_external(
        &self,
        ctx: RequestContext,
        agent: &AgentPo,
        prompt: &str,
    ) -> Result<String>;
```

### Step 2: BrainDalImpl 持有 AgentRuntimeDao

修改 `BrainDalImpl` struct：

```rust
struct BrainDalImpl {
    cortex_dao: Arc<dyn CortexDao + Send + Sync>,
    tool_call_dao: Arc<dyn ToolCallDao + Send + Sync>,
    /// 外部 Agent 执行 DAO（v4 新增）
    /// 根据 agent.kind 在 invoke_external 中选择对应的实现
    codex_runtime_dao: Arc<crate::service::dao::agent_runtime::codex::CodexRuntimeDao>,
    a2a_runtime_dao: Arc<crate::service::dao::agent_runtime::a2a::A2aRuntimeDao>,
}
```

或者更灵活的方式：持有 `Vec<Box<dyn AgentRuntimeDao>>` 或根据 agent 配置动态创建。

**推荐简化方案**：不持有具体 DAO 实例，而是在 invoke_external 中根据 agent 的 external_config 直接调用对应的 execute 函数：

```rust
async fn invoke_external(
    &self,
    ctx: RequestContext,
    agent: &AgentPo,
    prompt: &str,
) -> Result<String> {
    let ctx = enrich_ctx!(&ctx, &*agent);
    let start = std::time::Instant::now();

    log_debug!(
        ctx.clone(),
        "brain_invoke_external",
        "External agent invoke start, agent_id={}, kind={}",
        agent.po.id, agent.po.kind,
    );

    // 根据 agent 配置选择执行后端
    let config = agent.get_external_config().ok_or_else(|| {
        err!(Internal, "Agent {} is external but external_config is missing", agent.po.id)
    })?;

    let result = match config {
        ExternalAgentConfig::Cli { command, args, work_dir, env, timeout_secs, prompt_template } => {
            // 应用 prompt 模板
            let final_prompt = apply_prompt_template(&prompt_template, prompt);
            crate::service::dao::agent_runtime::codex::execute_cli(
                &agent.po.id, &command, &args, &work_dir, &env, timeout_secs, &final_prompt,
            ).await
        }
        ExternalAgentConfig::Remote { endpoint, agent_name, auth_token, timeout_secs } => {
            crate::service::dao::agent_runtime::a2a::execute_a2a(
                &agent.po.id, &endpoint, &agent_name, &auth_token, timeout_secs, prompt,
            ).await
        }
    };

    log_debug!(
        ctx.clone(),
        "brain_invoke_external_complete",
        "External agent invoke completed, agent_id={}, elapsed={:?}",
        agent.po.id, start.elapsed(),
    );

    result
}
```

### Step 3: 更新 init/new 函数

修改 `brain::init()` 和 `brain::new()` 以支持新的依赖（如果选择持有 DAO 实例的方案）。

### Step 4: 运行编译验证

Run: `cargo build`
Expected: 编译通过

### Step 5: Commit

```bash
git add src/service/dal/brain.rs
git commit -m "feat: BrainDal 新增 invoke_external 方法（外部 Agent 调度入口）"
```

---

## Task 9: 派生 Dal（CodexAgentDal + A2aAgentDal 拆分） ✅ 已完成

**实施结果（v5）：**
- CodexAgentDal 和 A2aAgentDal 通过委托模式持有 `Arc<dyn AgentDal>`
- 所有管理方法委托 self.base.xxx()
- AgentDal trait 提供 prompt_builder() 默认实现返回 DefaultPromptBuilder
- **设计原则**：每类 Agent Dal 配套自己的 PromptBuilder；没有专属 builder 时复用 trait 默认方法提供的 DefaultPromptBuilder，不引入笼统的"外部 builder"。未来实现 CliPromptBuilder/RemotePromptBuilder 时在对应 Dal 中重写 prompt_builder()
- 测试验证：brain 3/3, agent 108/108, context_assembly 3/3 通过

**设计原则（v4 更新）：**
- Dal 作为具体业务层，不同外部 Agent 类型独立 Dal
- **职责变化**：派生 Dal 不再负责 brain 装配（v4 中外部 Agent 不装配 brain）
- **新职责**：管理操作（CRUD 委托）+ 提供专属 PromptBuilder + 信息转换方法
- `CodexAgentDal`：处理 Cli kind，提供 CliPromptBuilder
- `A2aAgentDal`：处理 Remote kind，提供 RemotePromptBuilder

**Files:**
- Create: `src/service/dal/agent_codex.rs`
- Create: `src/service/dal/agent_a2a.rs`
- Modify: `src/service/dal/mod.rs`

---

### Step 1: CodexAgentDal 实现

Create `src/service/dal/agent_codex.rs`:

```rust
//! CodexAgentDal：CLI 类型外部 Agent 的派生 Dal
//!
//! 职责（v4）：
//! - 组合基础 AgentDal（CRUD 委托）
//! - 提供 CliPromptBuilder（Cli kind 专属的 prompt 组装逻辑）
//! - 未来可扩展：CLI 独有的消息体转换、输出解析等

use std::sync::Arc;

use crate::models::agent::Agent;
use crate::models::prompt_builder::PromptBuilder;
use crate::service::dal::agent::AgentDal;
use crate::service::dal::prompt_builder_default::DefaultPromptBuilder;

/// Codex / CLI Agent Dal
pub struct CodexAgentDal {
    /// 基础 AgentDal（CRUD 委托给 base）
    pub base: Arc<dyn AgentDal>,
}

impl CodexAgentDal {
    pub fn new(base: Arc<dyn AgentDal>) -> Self {
        Self { base }
    }

    /// 返回 Cli kind 专属的 PromptBuilder
    ///
    /// 第一版直接复用 DefaultPromptBuilder，
    /// 后续可根据 CLI Agent 特性定制（如不同的 prompt 模板、简化记忆注入等）
    pub fn prompt_builder(&self) -> Box<dyn PromptBuilder> {
        // 第一版：复用 DefaultPromptBuilder
        // TODO: 后续可替换为 CliPromptBuilder，提供 CLI 特化的 prompt 组装
        Box::new(DefaultPromptBuilder::new())
    }
}
```

### Step 2: A2aAgentDal 实现

Create `src/service/dal/agent_a2a.rs`:

```rust
//! A2aAgentDal：Remote 类型外部 Agent 的派生 Dal
//!
//! 职责（v4）：
//! - 组合基础 AgentDal（CRUD 委托）
//! - 提供 RemotePromptBuilder（Remote kind 专属的 prompt 组装逻辑）
//! - 未来可扩展：A2A 协议独有的消息格式转换、任务状态跟踪等

use std::sync::Arc;

use crate::models::agent::Agent;
use crate::models::prompt_builder::PromptBuilder;
use crate::service::dal::agent::AgentDal;
use crate::service::dal::prompt_builder_default::DefaultPromptBuilder;

/// A2A / Remote Agent Dal
pub struct A2aAgentDal {
    /// 基础 AgentDal（CRUD 委托给 base）
    pub base: Arc<dyn AgentDal>,
}

impl A2aAgentDal {
    pub fn new(base: Arc<dyn AgentDal>) -> Self {
        Self { base }
    }

    /// 返回 Remote kind 专属的 PromptBuilder
    pub fn prompt_builder(&self) -> Box<dyn PromptBuilder> {
        // 第一版：复用 DefaultPromptBuilder
        // TODO: 后续可替换为 RemotePromptBuilder，提供 A2A 特化的 prompt 组装
        Box::new(DefaultPromptBuilder::new())
    }
}
```

### Step 3: 注册模块

在 `src/service/dal/mod.rs` 添加：
```rust
pub mod agent_codex;
pub mod agent_a2a;
pub mod prompt_builder_default;
```

### Step 4: 运行编译验证

Run: `cargo build && cargo test --lib agent_codex --lib agent_a2a`
Expected: 编译通过，测试通过

### Step 5: Commit

```bash
git add src/service/dal/agent_codex.rs src/service/dal/agent_a2a.rs src/service/dal/mod.rs
git commit -m "feat: 拆分派生 Dal - CodexAgentDal + A2aAgentDal（提供专属 PromptBuilder）"
```

---

## Task 10: HrDomain 集成 + RuntimeDomain awaken 路由 ✅ 已完成（v5 调整）

**实施结果（v5 方案调整）：**
- v5 方案下 Brain 内部分发，不再需要 invoke_external 方法，统一走 BrainDal.think
- PromptBuilder trait 新增 builtin_tools 方法（注入神经工具+已安装工具包）
- RuntimeDomainImpl 持有派生 Dal（codex_agent_dal, a2a_agent_dal），提供 prompt_builder(agent) 工厂方法按 kind 路由
- awakening.rs Step 4 使用工厂方法替代硬编码 DefaultPromptBuilder::new()
- create_agent 按 kind 跳过 model_provider_id 校验（外部 Agent 不需要本地 provider）
- HrDomain 不需要持有派生 Dal（prompt_builder 在 RuntimeDomain 中使用）
- 测试验证：awakening 11/11, hr 26/26, runtime 69/69 通过

**设计原则（v4）：**
- HrDomain 持有派生 Dal，提供 prompt_builder 工厂方法
- RuntimeDomain.awaken 内部按 agent.kind 路由：
  - Local → BrainDal.think(ctx, brain, prompt)
  - 外部 → BrainDal.invoke_external(ctx, agent, prompt)
- 外部 Agent 不装配 brain，agent.brain 保持 None
- HrDomain.get_agent 对外部 agent 不调用 brain 装配

**Files:**
- Modify: `src/service/domain/hr/mod.rs`
- Modify: `src/service/domain/hr/agent.rs`
- Modify: `src/service/domain/runtime/awakening.rs`
- Modify: `src/service/domain/runtime/mod.rs`（新增 prompt_builder 工厂方法）

---

### Step 1: 修改 HrDomainImpl 持有派生 Dal

在 `src/service/domain/hr/mod.rs` 中修改 `HrDomainImpl` struct：

```rust
struct HrDomainImpl {
    agent_dal: Arc<dyn AgentDal>,
    /// CLI 类型外部 Agent Dal（提供 CliPromptBuilder）
    codex_agent_dal: Arc<CodexAgentDal>,
    /// A2A 远程类型外部 Agent Dal（提供 RemotePromptBuilder）
    a2a_agent_dal: Arc<A2aAgentDal>,
    tool_dal: Arc<dyn ToolDal>,
    skill_dal: Arc<dyn SkillDal>,
}
```

修改 `init()`、`new()`、`HrDomainImpl::new()` 相应地传入派生 Dal。

### Step 2: HrDomain 提供 prompt_builder 工厂方法

在 HrDomain trait 或 RuntimeDomain 中新增方法：

```rust
    /// 根据 agent.kind 返回对应的 PromptBuilder
    ///
    /// 工厂方法：上层调用此方法获取 builder，组装 prompt 后交给 BrainDal 调度
    fn prompt_builder(&self, agent: &Agent) -> Box<dyn PromptBuilder>;
```

实现中按 kind 路由：

```rust
fn prompt_builder(&self, agent: &Agent) -> Box<dyn PromptBuilder> {
    match agent.po.kind {
        AgentKind::Local => self.agent_dal.prompt_builder(),
        AgentKind::Cli => self.codex_agent_dal.prompt_builder(),
        AgentKind::Remote => self.a2a_agent_dal.prompt_builder(),
    }
}
```

注：AgentDal 也需要新增 `prompt_builder()` 方法返回 DefaultPromptBuilder。

### Step 3: 修改 awakening.rs 按 kind 路由

在 `src/service/domain/runtime/awakening.rs` 的 `awaken` 方法中，修改 Step 5：

```rust
        // Step 5: 根据 agent.kind 路由到不同的执行入口
        use common::enums::AgentKind;
        let raw_output = match agent.po.kind {
            AgentKind::Local => {
                // Local agent: 走 BrainDal.think（需要 brain 已装配）
                let brain = agent
                    .brain
                    .as_ref()
                    .ok_or_else(|| err!(Internal, "Local Agent 大脑未唤醒，请先调用 wake_brain()"))?;
                self.brain_dal().think(ctx.clone(), brain, &prompt).await?
            }
            AgentKind::Cli | AgentKind::Remote => {
                // 外部 agent: 走 BrainDal.invoke_external（不需要 brain）
                self.brain_dal().invoke_external(ctx.clone(), &agent.po, &prompt).await?
            }
        };
```

### Step 4: 修改 get_agent 对外部 agent 不装配 brain

在 `src/service/domain/hr/agent.rs` 的 `get_agent` 中：

```rust
    async fn get_agent(&self, ctx: RequestContext, id: &str, options: AgentFetchOptions) -> Result<Option<Agent>> {
        let agent_opt = self.agent_dal.get_agent(ctx.clone(), id, options).await?;

        if let Some(agent) = agent_opt.as_ref() {
            // v4: 外部 Agent 不装配 brain
            // Local agent 的 brain 装配在 awaken 时按需进行（或保持原有 wake_agent_brain 逻辑）
        }

        Ok(agent_opt)
    }
```

### Step 5: create_agent 对外部 agent 跳过 model_provider_id 校验

```rust
    async fn create_agent(&self, ctx: RequestContext, agent: &Agent) -> Result<()> {
        // 外部 agent 跳过 model_provider_id 校验
        if !agent.po.kind.is_external() {
            if agent.po.model_provider_id.is_empty() {
                return Err(Error::bad_request("model_provider_id is required for Local agent"));
            }
        }
        // ... 其余校验和创建逻辑
    }
```

### Step 6: 运行编译验证

Run: `cargo build`
Expected: 编译通过

Run: `cargo test --lib hr::`
Expected: 现有测试全部通过（可能需要调整部分测试以适配新架构）

### Step 7: Commit

```bash
git add src/service/domain/hr/mod.rs src/service/domain/hr/agent.rs \
        src/service/domain/runtime/awakening.rs src/service/domain/runtime/mod.rs
git commit -m "feat: HrDomain 集成派生 Dal + RuntimeDomain awaken 按 kind 路由"
```

---

## Task 11: 补全 Local agent 的 brain 装配链路 ✅ 已完成（v5 调整）

**实施结果（v5 方案调整）：**
- v5 方案下外部 Agent 也装配 Brain（用 new_external，cortex 为 None），统一走 think 入口
- RuntimeAwakening trait 新增 wake_agent_brain(ctx, agent: &mut Agent) 方法（幂等装配）
- 实现：Local agent 加载 builtin tools 构造带 Cortex 的 Brain；External agent 构造虚拟 Brain
- consumer/message.rs 中 agent.brain.is_none() 时自动调用 wake_agent_brain（替代原报错逻辑）
- 测试 test_agent_no_brain_returns_internal 更新为 test_agent_no_brain_auto_wakes 验证自动装配
- 测试验证：consumer 23/23, runtime 69/69, hr 26/26 通过

**Files:**
- Modify: `src/service/domain/hr/agent.rs`（get_agent 中 Local 分支）
- Modify: `src/service/dal/agent.rs`（新增 wake_agent_brain 便捷方法）

### Step 1: 在 AgentDal trait 新增 wake_agent_brain 便捷方法

在 `src/service/dal/agent.rs` 的 `AgentDal` trait 中新增：

```rust
    /// 便捷方法：从 agent.po.model_provider_id 加载 provider，
    /// 加载 memories 和 tools，构造 Brain 并赋值给 agent
    ///
    /// 这是 Local agent 的 brain 装配入口。
    /// 内部调用 BrainDal.wake_brain 构造 Brain，再赋值给 agent。
    async fn wake_agent_brain(&self, ctx: RequestContext, agent: &mut Agent) -> Result<()>;
```

在 `AgentDalImpl` 中实现（加载 provider、tools，调用 BrainDal.wake_brain）。

### Step 2: 修改 HrDomain.get_agent 的 Local 分支调用 wake_agent_brain

```rust
        if let Some(agent) = agent_opt.as_mut() {
            use common::enums::AgentKind;
            match agent.po.kind {
                AgentKind::Local => {
                    // Local agent: 装配真实 Brain
                    self.agent_dal.wake_agent_brain(ctx.clone(), agent).await?;
                }
                AgentKind::Cli | AgentKind::Remote => {
                    // 外部 agent: 不装配 brain（v4 决策）
                }
            }
        }
```

### Step 3: 运行编译和测试

Run: `cargo build && cargo test`
Expected: 编译通过，所有现有测试通过

### Step 4: Commit

```bash
git add src/service/dal/agent.rs src/service/domain/hr/agent.rs
git commit -m "feat: 补全 Local agent 的 brain 装配链路（wake_agent_brain）"
```

---

## Task 12: HTTP API - 创建外部 Agent ✅ 已完成

**实施结果：**
- 新增 `common/src/api/external_agent.rs`（CreateExternalAgentRequest/Response）
- 新增 `src/handlers/hr/agent/create_external_agent.rs`（按 kind 构造 ExternalAgentConfig + 设置 kind + external_config，调用通用 create_agent）
- 注册路由 `POST /api/v1/hr/agents/external`
- hr 26/26 测试通过

**设计原则：**
- Handler 对应用户行为，天然具体；Domain 层保持通用抽象
- Handler 内部构造好 `Agent`（设置 `kind` + `external_config`），直接调用通用 `create_agent`
- Domain 层不新增 `create_external_agent` 语法糖方法

**Files:**
- Create: `common/src/api/external_agent.rs`
- Modify: `common/src/api/mod.rs`
- Create: `src/handlers/hr/agent/create_external_agent.rs`
- Modify: `src/handlers/hr/agent/mod.rs`
- Modify: `src/handlers/mod.rs` 或路由注册文件

---

### Step 1: 定义 API 类型

Create `common/src/api/external_agent.rs`:

```rust
//! 外部 Agent API 类型

use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateExternalAgentParams {
    /// Agent 名称
    pub name: String,
    /// 角色标签
    #[serde(default)]
    pub roles: Vec<String>,
    /// 描述
    #[serde(default)]
    pub description: String,
    /// 能力描述（已存在字段，复用）
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// 灵魂/性格设定
    #[serde(default)]
    pub soul: String,
    /// Agent 类型：cli / remote
    pub kind: String,

    // CLI 配置（kind=cli 时必填）
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub work_dir: Option<String>,
    pub env: Option<Vec<(String, String)>>,
    pub timeout_secs: Option<u64>,
    pub prompt_template: Option<String>,

    // Remote 配置（kind=remote 时必填）
    pub endpoint: Option<String>,
    pub agent_name: Option<String>,
    pub auth_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateExternalAgentResponse {
    pub agent_id: String,
}
```

在 `common/src/api/mod.rs` 注册：

```rust
pub mod external_agent;
```

---

### Step 2: 写 HTTP handler

**Handler 直接调用 Domain 层通用的 `create_agent` 方法**，在 Handler 内完成参数解析、kind 设置、external_config 构造等具体用户行为逻辑。

Create `src/handlers/hr/agent/create_external_agent.rs`:

```rust
//! Handler: POST /api/v1/agents/external - 创建外部 Agent（Cli/Remote）
//!
//! 这是一个用户行为导向的 Handler，负责将 HTTP 请求参数转换为通用的 Agent 实体，
//! 然后调用 Domain 层通用的 create_agent 方法完成创建。
//!
//! Domain 层不提供 create_external_agent 等同作用语法糖，
//! 用户使用差异通过不同的 Handler 处理。

use crate::pkg::RequestContext;
use crate::service::domain::hr::{self, AgentManage};
use ai_orz_macros::generate_http_handler;
use common::api::{CreateExternalAgentParams, CreateExternalAgentResponse};
use common::error::{Error, Result};
use common::enums::AgentKind;
use crate::models::agent::{Agent, AgentPo, ExternalAgentConfig, AgentRuntimeConfig};

#[generate_http_handler]
pub async fn create_external_agent(
    ctx: RequestContext,
    params: CreateExternalAgentParams,
) -> Result<CreateExternalAgentResponse> {
    let kind = match params.kind.as_str() {
        "cli" => AgentKind::Cli,
        "remote" => AgentKind::Remote,
        other => return Err(Error::bad_request(format!(
            "Invalid kind '{}', expected 'cli' or 'remote'", other
        ))),
    };

    let external_config = match kind {
        AgentKind::Cli => {
            let command = params.command.as_ref()
                .ok_or_else(|| Error::bad_request("command is required for cli kind"))?;
            let work_dir = params.work_dir.as_ref()
                .ok_or_else(|| Error::bad_request("work_dir is required for cli kind"))?;
            ExternalAgentConfig::Cli {
                command: command.clone(),
                args: params.args.clone().unwrap_or_default(),
                work_dir: work_dir.clone(),
                env: params.env.clone().unwrap_or_default(),
                timeout_secs: params.timeout_secs.unwrap_or(300),
                prompt_template: params.prompt_template.clone(),
            }
        }
        AgentKind::Remote => {
            let endpoint = params.endpoint.as_ref()
                .ok_or_else(|| Error::bad_request("endpoint is required for remote kind"))?;
            let agent_name = params.agent_name.as_ref()
                .ok_or_else(|| Error::bad_request("agent_name is required for remote kind"))?;
            ExternalAgentConfig::Remote {
                endpoint: endpoint.clone(),
                agent_name: agent_name.clone(),
                auth_token: params.auth_token.clone(),
                timeout_secs: params.timeout_secs.unwrap_or(300),
            }
        }
        _ => unreachable!(),
    };

    // 构造 AgentPo，设置 kind 和 external_config
    let mut po = AgentPo::new(
        params.name,
        params.roles,
        params.description,
        params.capabilities,
        params.soul,
        String::new(), // 外部 agent 不需要 model_provider_id
        ctx.uid(),
    );
    po.kind = kind;

    let mut runtime_config: AgentRuntimeConfig = po.get_runtime_config();
    runtime_config.external_config = Some(external_config);
    po.set_runtime_config(&runtime_config);

    let agent = Agent::from_po(po);

    // 直接调用通用 create_agent（Domain 层方法），不调用语法糖方法
    hr::domain().agent_manage()
        .create_agent(ctx, &agent)
        .await?;

    Ok(CreateExternalAgentResponse {
        agent_id: agent.po.id.clone(),
    })
}
```

---

### Step 3: 注册 handler 模块与路由

在 `src/handlers/hr/agent/mod.rs` 添加：

```rust
pub mod create_external_agent;
```

在 `src/handlers/mod.rs` 或路由注册主文件中（grep `create_agent` 找现有路由位置），添加：

```rust
.route(
    "/api/v1/agents/external",
    axum::routing::post(handlers::hr::agent::create_external_agent::create_external_agent),
)
```

---

### Step 4: 运行编译验证

Run: `cargo build`
Expected: 编译通过

---

### Step 5: Commit

```bash
git add common/src/api/external_agent.rs common/src/api/mod.rs \
        src/handlers/hr/agent/create_external_agent.rs \
        src/handlers/hr/agent/mod.rs src/handlers/mod.rs
git commit -m "feat: 新增 POST /api/v1/agents/external HTTP handler"
```

---

## Task 13: 单元测试验证 ✅ 已完成

**实施结果：**
- 在 `context_assembly.rs` 添加 3 个 PromptBuilder trait 行为测试（builtin_tools/bound_tools/chained calls）
- 创建 `agent_codex_test.rs`（5 个测试：委托 create/find_by_id/rebuild_vectors + 默认 builder 复用 + 重复 build）
- 创建 `agent_a2a_test.rs`（3 个测试：委托 create/get_agent + 默认 builder 复用）
- 14 个新测试全部通过，回归测试 120/120 通过

**策略（v4 更新）：** 先通过单元测试完成后端验证，不做端到端集成测试。聚焦各模块独立验证：
- AgentKind 枚举（Task 1 已覆盖）✅
- ExternalAgentConfig 序列化/反序列化（Task 3 已覆盖）✅
- Codex CLI 执行逻辑（Task 4 已覆盖）✅
- A2A HTTP 执行逻辑（Task 6 已覆盖）✅
- ExternalCortex（Task 5 已覆盖）✅
- **新增 v4**：PromptBuilder trait + DefaultPromptBuilder（Task 7）
- **新增 v4**：BrainDal.invoke_external（Task 8）
- **新增 v4**：派生 Dal 提供 PromptBuilder（Task 9）
- **新增 v4**：RuntimeDomain awaken 按 kind 路由（Task 10）
- **新增 v4**：Local agent brain 装配链路（Task 11）

**Files:**
- Modify: `src/service/dal/prompt_builder_default.rs`（补充 PromptBuilder 测试）
- Modify: `src/service/dal/agent_codex.rs`（补充 prompt_builder 测试）
- Modify: `src/service/dal/agent_a2a.rs`（补充 prompt_builder 测试）
- Modify: `src/service/domain/runtime/awakening.rs` 或 test 文件（补充路由逻辑测试）
- Modify: `src/service/domain/hr/agent_test.rs`（补充路由逻辑测试）

---

### Step 1: PromptBuilder 测试补充

在 `src/service/dal/prompt_builder_default.rs` 的测试模块中验证：
- DefaultPromptBuilder 实现了 PromptBuilder trait
- 各 setter 方法正确累积状态
- build() 生成预期的 prompt 格式
- 与原 PromptBuilder 行为一致（回归测试）

---

### Step 2: 派生 Dal 的 prompt_builder 测试

在 `src/service/dal/agent_codex.rs` 和 `src/service/dal/agent_a2a.rs` 中验证：
- `prompt_builder()` 返回的 builder 可以正确组装 prompt
- 返回的 builder 类型正确（第一版是 DefaultPromptBuilder，后续可特化）

---

### Step 3: RuntimeDomain 路由逻辑单元测试

在 `src/service/domain/runtime/` 的测试中验证 awaken 按 AgentKind 路由：
- Local kind 调用 BrainDal.think
- Cli/Remote kind 调用 BrainDal.invoke_external
- 外部 Agent 不需要 brain 装配也能执行

---

### Step 4: 运行全部单元测试

Run: `cargo test`
Expected: 全部 PASS（重点关注：prompt_builder、agent_codex、agent_a2a、awakening、hr::agent_test）

---

### Step 5: Commit

```bash
git add src/service/dal/prompt_builder_default.rs src/service/dal/agent_codex.rs \
        src/service/dal/agent_a2a.rs src/service/domain/runtime/ \
        src/service/domain/hr/agent_test.rs
git commit -m "test: 补充 v4 架构相关单元测试"
```

---

## Task 14: 文档更新 ✅ 已完成

**实施结果：**
- 创建 `docs/external_agent_design.md` 简洁设计文档
- 涵盖：AgentKind 分类、三层架构、Brain 装配链路、PromptBuilder 工厂方法、HTTP API、关键约束、相关文件
- 遵循用户偏好：简洁聚焦定位与用法，非详细规范

**Files:**
- Create: `docs/external_agent_design.md`

- [ ] **Step 1: 写设计文档**

Create `docs/external_agent_design.md`，内容包括：
- 背景与目标
- AgentKind 分类（Local/Cli/Remote）
- 三层架构：
  - DAO 层：AgentRuntimeDao（`dao/agent/runtime/` 下，Codex + A2A 两种实现）
  - DAL 层：CodexAgentDal + A2aAgentDal（拆分独立，各自封装独有业务逻辑）
  - Domain 层：通用 `create_agent`，按 kind 路由装配 brain
- Brain 装配链路补全（Local + 外部 agent 统一装配）
- ExternalCortex 工作原理
- CodexRuntimeDao（子进程 stdin/stdout）
- A2aRuntimeDao（JSON-RPC over HTTP）
- Domain 层设计原则：通用抽象方法，Handler 层处理用户行为差异
- 复用 awaken 链路（短期记忆、技能注入、Trace、统计）
- 数据库 schema 变更
- HTTP API
- 配置示例（Codex、Claude Code、Aider、通用 A2A）
- 神经工具策略 L1/L2
- 未来扩展（A2A Server、跨组织路由）

文档结构参考 `docs/feishu_p2p_design.md` 或 `docs/mcp_tool_design.md`。

- [ ] **Step 2: Commit**

```bash
git add docs/external_agent_design.md
git commit -m "docs: 外部 Agent 接入设计文档"
```

---

## 自检清单（v4）

完成所有任务后，运行以下验证：

- [ ] `cargo build` 编译通过
- [ ] `cargo test` 全部测试通过（重点关注：agent_kind、external_config_tests、runtime::codex、runtime::a2a、external_cortex、prompt_builder、agent_codex、agent_a2a、awakening、hr::agent_test）
- [ ] `cargo run` 启动成功
- [ ] 单元测试验证：
  - [ ] AgentKind 枚举（4 个测试）✅
  - [ ] ExternalAgentConfig 序列化/反序列化 + 向后兼容（6 个测试）✅
  - [ ] Codex CLI 执行逻辑（3 个测试）✅
  - [ ] A2A HTTP 执行逻辑（2 个测试）✅
  - [ ] ExternalCortex 虚拟 Cortex（7 个测试）✅
  - [ ] **PromptBuilder trait + DefaultPromptBuilder（至少 3 个测试）** 🆕 v4
  - [ ] **BrainDal.invoke_external（至少 2 个测试）** 🆕 v4
  - [ ] **CodexAgentDal prompt_builder（至少 1 个测试）** 🆕 v4
  - [ ] **A2aAgentDal prompt_builder（至少 1 个测试）** 🆕 v4
  - [ ] **RuntimeDomain awaken 按 kind 路由（至少 2 个测试）** 🆕 v4
  - [ ] HrDomain 按 kind 路由逻辑（至少 1 个测试）
- [ ] 手动冒烟测试（可选，非必须）：通过 `POST /api/v1/agents/external` 创建一个 `cat` 命令模拟的 Cli agent，验证创建成功且 kind=Cli

---

## 未来工作（不在本计划范围）

### A2A Server（P2）
让 ai_orz 组织本身作为 A2A Server 被外部调用：
- 暴露 `GET /.well-known/agent.json` Agent Card
- 实现 `POST /a2a` JSON-RPC endpoint
- A2A Task ↔ ai_orz Task+Message 映射

### 跨组织路由（P3）
实现 `OrganizationScope::Remote` 的实际逻辑：
- 跨组织 Agent Card 发现
- 跨组织任务路由
- 组织间认证

### 神经工具策略 L2
- 派生 Dal 的 `parse_response()` 解析外部 agent 输出中的工具调用意图
- 由 ai_orz 代理执行神经工具调用
- 让外部 agent 也能调用 ai_orz 的记忆、协作等能力
