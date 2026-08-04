# Remove rig Dependency - Self-built Cortex Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the `rig` crate dependency by building a self-contained cortex DAO layer that directly calls OpenAI Chat Completions API via `reqwest`, with an explicit tool-calling loop at the awakening layer (cortex DAO only "thinks" and emits tool-call requests; the upper layer executes tools and feeds results back).

**Architecture (扁平化):** 直接的 `BrainDal → CortexDao` 调用关系，不再有 `Cortex` 实体或 `CortexTrait` 抽象层。(1) **CortexDao (思考层抽象)** — 按 `provider_type` 分发的统一 trait，接收 `&ModelProviderPo` + prompt + tools，调用 LLM via Chat Completions API，返回 `ThinkResult`。`BrainDal` 根据 `ModelProvider.provider_type` 在 `CortexDaoRegistry` 中选择对应实现（`OpenAiCompatibleCortexDao` / `FastEmbedCortexDao` / `ExternalCortexDao`）。CortexDao never generates `tool_call_id` or executes tools. (2) **Brain/awakening (execution layer)** — owns the explicit tool-calling loop, generates `tool_call_id`, executes `CoreTool` (Auto + Manual unified), appends results back into the prompt, controls max rounds. (3) **Stats/monitoring** — token usage extracted from the raw HTTP response body in cortex DAO, recorded via `ctx.stats().record()` — no rig hook needed. All providers (OpenAI/DeepSeek/Qwen/Doubao/Ollama/Custom) uniformly use `/chat/completions` (not `/responses`), fixing a latent compatibility risk. Provider config（`base_url`/`api_key`/`model_name` 等）由 `ModelProviderPo` 携带，`CortexDao` 实现内部读取，provider 配置优先于默认值。

**Tech Stack:** Rust, reqwest (already a dependency), serde_json, async-trait, tokio, existing `CoreTool` trait, existing `ToolCallLoggingDecorator`, existing `StatsCollector`.

---

## Architecture Overview

```
┌──────────────────────────────────────────────────────────────┐
│ Execution layer (awakening.rs)                                │
│ - Owns Vec<ToolDescriptor> (dynamic, can grow mid-loop)       │
│ - Calls brain.think(ctx, brain, prompt, &tools)              │
│   → ThinkResult（brain 自带 model_provider，无需传 provider）  │
│ - On ToolCall: generate tool_call_id (uuid v7)                │
│   → execute CoreTool via ToolCallLoggingDecorator             │
│   → append "{tool_name, args, result}" to prompt              │
│   → loop back to brain.think()                                │
│ - max_tool_rounds limit (default 10)                          │
└────────────┬─────────────────────────────────────────────────┘
             │ brain.think(ctx, brain, prompt, &tools)
             │   → 从 brain.model_provider.as_ref() 读取 provider
             │   → cortex_registry().get(provider.provider_type)
             │   → dao.think(ctx, provider, prompt, &tools)
             ▼
┌──────────────────────────────────────────────────────────────┐
│ Brain (持有 model_provider)                                   │
│ - Brain.model_provider: Option<ModelProviderPo>               │
│   - Local agent: Some(provider) — 由 wake_agent_brain 装配    │
│     （通过 agent 的 model provider 配置进入运行时 brain）       │
│   - Cli/Remote agent: None — 外部 agent 不需要 provider        │
│ - Brain 装配方法（new_local/new_external）接收并保存 provider   │
└──────────────────────────────────────────────────────────────┘
             │
             ▼
┌──────────────────────────────────────────────────────────────┐
│ Thinking layer (BrainDal → CortexDao)                         │
│ - BrainDal 根据 brain.model_provider.provider_type 在         │
│   CortexDaoRegistry 中选择对应 CortexDao 实现                │
│ - CortexDao.think(ctx, &ModelProviderPo, prompt, &tools)      │
│ - CortexDao 实现（OpenAiCompatible/FastEmbed/External）       │
│   直接从 ModelProviderPo 读取 base_url/api_key/model_name     │
│   （provider 配置优先于默认值）                                │
│ - POST /chat/completions via reqwest                          │
│ - Parse response:                                            │
│   - tool_calls present → ThinkResult::ToolCall(name, args)   │
│   - no tool_calls → ThinkResult::Final(text)                 │
│ - Extract usage → ctx.stats().record(ModelCallEvent)          │
│ - Does NOT generate tool_call_id (upper layer's job)          │
│ - 不再有 Cortex 实体 / CortexTrait 抽象层                     │
└──────────────────────────────────────────────────────────────┘
```

**CortexDaoRegistry 按 provider_type 分发:**
```
ProviderType::OpenAI | DeepSeek | Qwen | Doubao | DoubaoVision | Ollama | Custom
    → OpenAiCompatibleCortexDao (单例，共享 HTTP 连接池)
ProviderType::FastEmbed
    → FastEmbedCortexDao (单例，本地 fastembed crate)
AgentKind::Cli | Remote
    → ExternalCortexDao (单例，CLI 子进程 / A2A 协议；不依赖 provider_type)
```

**Key insight:** Removing rig's black-box loop means the tool list becomes **dynamic** — the agent can call a search tool to discover more tools, append them to the list, and use them in the next think() call. This was impossible with rig's "tools fixed at Agent construction" model. 同时去掉 `Cortex` 实体和 `CortexTrait` 抽象后，调用链从 `Brain → Cortex(trait对象) → 具体实现` 扁平化为 `BrainDal → CortexDao(具体实现)`。**Brain 直接持有 `model_provider: Option<ModelProviderPo>`**（Local agent 为 `Some`，外部 agent 为 `None`），`think()` 时不再作为参数传入，而是从 `brain.model_provider` 读取后传给 `CortexDao` 方法，provider 配置（base_url/api_key/model_name 等）的解析职责统一收敛到 DAO 实现内部。

**向量化架构（双层入口）：** 向量化能力通过两个层级暴露：(1) **cortex 包级函数** `embed_entity(ctx, provider, entity)` / `embed_text_for_search(ctx, provider, text)`（`src/service/dao/cortex/mod.rs`）— 纯协议层，本质是一个路由函数：根据 `provider.provider_type` 从 `CortexDaoRegistry` 路由到可用的 cortex dao 具体实现并完成向量化。不依赖任何 DAL，不查 DB，调用方负责获取 provider。DAL 层不再重复定义 `try_build_vector_params_*` helper，而是由各 DAL 方法（`upsert_vector_index` / `search` 等）内部先查 provider，再直接调 cortex 包级函数（见 Task 6b）。(2) **BrainDal embed 入口** `embed_entity(ctx, entity)` / `embed_text_for_search(ctx, text)`（`src/service/dal/brain.rs`）— domain 层入口，内部查默认 Embedding Provider 后调 cortex 包级函数，返回 `Option<VectorIndexParams>`（`None` 表示降级跳过）。Domain 层向量化场景直接复用 BrainDal 入口，无需自行注入 `model_provider_dao`。**ToolDescriptor 从 Tool 直接派生**：业务代码层面直接传递 `Tool`，通过 `impl From<&Tool> for ToolDescriptor`（Task 1）派生，cortex dao 层再转换为下游协议需要的 tool 信息结构。

---

## API Migration Reference

### CortexDao trait 升级（取代 CortexTrait + Cortex 实体）

旧的 `CortexTrait` trait + `Cortex` 实体（持有 `ModelProvider + Box<dyn CortexTrait>`）已被删除。新的 `CortexDao` trait 直接定义 `think()` + `embed()`，所有方法接收 `&ModelProviderPo`。不再有 `create_cortex_trait()` 工厂方法，也不再在调用前预构造 Cortex 对象。

| 旧 API | 新 API |
|---|---|
| `CortexTrait::prompt(&self, prompt: &str) -> Result<String>` | (删除 trait) |
| `CortexTrait::prompt(&self, prompt, tools) -> Result<ThinkResult>` | (删除 trait) |
| `Cortex` struct 持有 `ModelProvider + Box<dyn CortexTrait>` | (删除 struct) |
| `Brain.cortex: Option<Cortex>` 字段 | (删除字段) |
| `CortexDao::create_cortex_trait(ctx, provider, rig_tools) -> Box<dyn CortexTrait>` | (删除方法) |
| `CortexDao::prompt(ctx, cortex: &dyn CortexTrait, prompt) -> Result<String>` | `CortexDao::think(ctx, provider: &ModelProviderPo, prompt, tools) -> Result<ThinkResult>` |
| `CortexDao::embed_text_raw(ctx, cortex, text) -> Vec<f32>` | `CortexDao::embed_text(ctx, provider, text) -> Vec<f32>`（trait 默认实现） |
| `CortexDao::embed_entity(ctx, cortex, entity) -> VectorIndexParams` | `CortexDao::embed_entity(ctx, provider, entity) -> VectorIndexParams`（trait 默认实现） |
| `CortexDao::embed_text_for_search(ctx, cortex, text) -> VectorIndexParams` | `CortexDao::embed_text_for_search(ctx, provider, text) -> VectorIndexParams`（trait 默认实现） |
| rig `AgentHook::on_completion_response` extracts usage | cortex DAO parses `response.usage` directly from HTTP body |
| `Vec<DynamicTool>` injected at Cortex construction | `Vec<ToolDescriptor>` passed per `think()` call |

### New types

```rust
pub enum ThinkResult {
    Final(String),
    ToolCall(ToolCallRequest),
}

pub struct ToolCallRequest {
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,  // JSON Schema
}

// 从业务 Tool 直接派生（Task 1 定义）
impl From<&crate::models::tool::Tool> for ToolDescriptor { ... }
```

### New package-level functions (cortex 协议层)

```rust
// src/service/dao/cortex/mod.rs — 包级函数，不依赖任何 DAL
pub async fn embed_entity(
    ctx: RequestContext,
    provider: &ModelProviderPo,
    entity: &dyn Vectorizable,
) -> Result<VectorIndexParams>;

pub async fn embed_text_for_search(
    ctx: RequestContext,
    provider: &ModelProviderPo,
    text: &str,
) -> Result<VectorIndexParams>;
```

> 调用方负责获取 provider。DAL 层 helper（Task 6b）和 BrainDal 入口（Task 4 Step 3）都调用这两个包级函数。

### Brain / BrainDal signature change

| Old | New |
|---|---|
| `brain.think(ctx, brain, prompt) -> Result<String>` | `brain.think(ctx, brain, prompt, tools) -> Result<ThinkResult>`（provider 从 `brain.model_provider` 读取，不再作为参数传入） |
| `brain.think(ctx, brain, prompt, tools, provider) -> Result<ThinkResult>` | `brain.think(ctx, brain, prompt, tools) -> Result<ThinkResult>`（去掉 provider 参数） |
| `Brain { ..., cortex: Option<Cortex> }` | `Brain { ..., model_provider: Option<ModelProviderPo> }`（Brain 持有 provider，删除 cortex 字段） |
| `Brain::new_local(..., cortex)` | `Brain::new_local(..., model_provider: ModelProviderPo)`（接收并保存 model_provider） |
| (新增) `Brain::new_external(kind, ...)` | `Brain::new_external(kind, ...)`（外部 agent 不接收 model_provider，`model_provider: None`） |
| `cortex_dao.prompt(ctx, cortex, prompt, tools)` | `cortex_dao.think(ctx, provider, prompt, tools)`（provider 由 BrainDal 从 brain 读取后传入） |
| (新增) `BrainDal::embed_entity(ctx, entity) -> Result<Option<VectorIndexParams>>` | domain 层入口，内部查默认 provider + 调 `cortex::embed_entity()` 包级函数，`None` 表示降级 |
| (新增) `BrainDal::embed_text_for_search(ctx, text) -> Result<Option<VectorIndexParams>>` | domain 层入口，内部查默认 provider + 调 `cortex::embed_text_for_search()` 包级函数 |
| DAL 层 `try_build_vector_params_*` helper（各自调 `cortex_dao.create_cortex_trait + embed_entity`） | **删除 helper**，各 DAL 方法（`upsert_vector_index` / `search` 等）内部直接调 `cortex::embed_entity()` / `embed_text_for_search()` 包级函数；DAL 仍自查 provider（`model_provider_dao.get_default_embedding_provider()`），`cortex_dao` 字段可移除（详见 Task 6b） |

### Provider unification

All providers now use `POST {base_url}/chat/completions`，`base_url` 由 `ModelProviderPo.base_url` 携带（优先于默认值，由 `OpenAiCompatibleCortexDao` 内部的 `resolve_base_url(provider)` 解析）：
- OpenAI: 默认 `https://api.openai.com/v1`
- DeepSeek: 默认 `https://api.deepseek.com`
- Qwen: 默认 `https://dashscope.aliyuncs.com/compatible-mode/v1`
- Doubao / DoubaoVision: 默认 `https://ark.cn-beijing.volces.com/api/v3`
- Ollama: 默认 `http://localhost:11434/v1`
- Custom: 仅使用 provider 提供的 base_url（无默认值）

All use the same Chat Completions request/response format. The only difference is the base_url（provider 配置优先于默认值）。

---

## File Structure

**New files (Phase 1 — core implementation):**
- `src/models/cortex_types.rs` — `ThinkResult`, `ToolCallRequest`, `ToolDescriptor` + `impl From<&Tool> for ToolDescriptor`（从业务 Tool 直接派生）
- `src/service/dao/cortex/native/mod.rs` — `CortexDaoRegistry`（按 `provider_type` 分发到 `OpenAiCompatibleCortexDao` / `FastEmbedCortexDao` / `ExternalCortexDao`），暴露 `cortex_registry()` 单例访问 + `init()`
- `src/service/dao/cortex/native/openai.rs` — `OpenAiCompatibleCortexDao`（单例，处理 OpenAI/DeepSeek/Qwen/Doubao/DoubaoVision/Ollama/Custom）；持有共享 `reqwest::Client`（连接池），不再持有 `api_key`/`base_url`/`model_name`/`ctx`/`provider_type` 字段（这些全部从 `&ModelProviderPo` 在方法内读取）；`embed()` 根据 `provider.provider_type` 分支（`DoubaoVision` 走 `/embeddings/multimodal`）；包含 `resolve_base_url(provider)` 函数
- `src/service/dao/cortex/native/http.rs` — shared HTTP helpers (build request, parse response, extract usage)；包含 `call_embeddings_multimodal`（集成自 `rig/doubao_vision.rs`，支持豆包 Vision 的 `/embeddings/multimodal` endpoint）

**Modified files (Phase 1 — interface upgrade):**
- `src/models/brain.rs` — 删除 `CortexTrait` trait、`Cortex` 实体；`Brain` struct 去掉 `cortex: Option<Cortex>` 字段，新增 `model_provider: Option<ModelProviderPo>` 字段；`Brain::new_local(...)` 接收 `model_provider: ModelProviderPo` 参数并保存；新增 `Brain::new_external(kind, ...)` 方法（外部 agent 不接收 provider，`model_provider: None`）
- `src/service/dao/cortex/mod.rs` — `CortexDao` trait 重写：删除 `create_cortex_trait()`，新增 `think(ctx, provider, prompt, tools)` + `embed(ctx, provider, texts)`（+ 默认实现的 `embed_text` / `embed_entity` / `embed_text_for_search`）；删除 `mod rig;` `pub use self::rig::*`；**新增包级函数 `embed_entity(ctx, provider, entity)` / `embed_text_for_search(ctx, provider, text)`**（不依赖任何 DAL，内部从 registry 获取 DAO）
- `src/service/dao/cortex/external.rs` — `ExternalCortex` 改名为 `ExternalCortexDao`，实现新 `CortexDao` trait（`think()` 返回 `ThinkResult::Final`，`embed()` 返回 `Err`）
- `src/service/dao/cortex/fastembed.rs` — `FastEmbedCortex` 改名为 `FastEmbedCortexDao`，实现新 `CortexDao` trait（`think()` 返回 `Err`，`embed()` 走本地 fastembed crate）
- `src/service/dal/brain.rs` — `BrainDal::think()` 签名升级（**去掉 `provider` 参数**，从 `brain.model_provider.as_ref()` 读取 provider），按 `brain.kind` 分支：`Local` 走 `cortex_registry().get(provider.provider_type).think(...)`，`Cli`/`Remote` 走 `execute_cli`/`execute_a2a`；`think_without_memory()` 同步升级；**新增 `embed_entity(ctx, entity)` / `embed_text_for_search(ctx, text)` 方法**（domain 层入口，内部查默认 provider + 调 cortex 包级函数，返回 `Option<VectorIndexParams>`）
- `src/service/domain/runtime/awakening.rs` — 实现显式工具调用循环；`awaken()` 调用 `brain.think()` 不传 provider（brain 自带）
- `src/models/agent.rs` (or `mod.rs`) — re-export new cortex types

**Modified files (Phase 1 — callers of think/prompt):**
- `src/service/domain/runtime/awakening.rs` — `awaken()` calls `think()` with tools（不传 provider，从 brain 读取），loops on `ThinkResult::ToolCall`；**工具列表构建改用 `map(ToolDescriptor::from)`**（从业务 Tool 直接派生）
- `src/service/domain/runtime/awakening.rs` — `wake_agent_brain()` 不再调用 `create_cortex_trait()`；加载 agent + 加载 ModelProvider，传给 `Brain::new_local(..., model_provider)` 注入 brain；外部 agent 走 `Brain::new_external(...)`（不传 provider）
- `src/service/consumer/message.rs` — no change (calls `awaken()`, not `think()` directly)

**Modified files (Phase 1 — DAL 层向量化统一改造, Task 6b):**
- 所有包含 `try_build_vector_params_for_entity` / `try_build_vector_params_for_search` 的 DAL 文件（6 个：agent / tool / memory / message / task / project）— **删除 helper 函数**，改造各 DAL 方法（`upsert_vector_index` / `search` 等）内部：先自查 provider（`model_provider_dao.get_default_embedding_provider()`），再直接调 cortex 包级函数 `crate::service::dao::cortex::embed_entity()` / `embed_text_for_search()`；移除不再使用的 `cortex_dao` 字段（agent.rs 的 `upsert_vector_index` 原本就内联了逻辑，需把其中 `cortex_dao.create_cortex_trait + embed_entity` 改为调包级函数）

**Files to delete (Phase 2 — cleanup):**
- `src/service/dao/cortex/rig/` — entire directory (openai.rs, openai_compatible.rs, ollama.rs, doubao_vision.rs, fastembed.rs)；`doubao_vision.rs` 的多模态 embedding 功能已集成到 `native/http.rs` 的 `call_embeddings_multimodal`，独立文件不再需要；`fastembed.rs` 已被新的 `src/service/dao/cortex/fastembed.rs`（`FastEmbedCortexDao`）取代
- `src/service/dao/cortex/rig.rs` — old RigCortexDao
- `src/service/dao/cortex/rig_test.rs` — old tests (replaced by native tests)
- `src/pkg/monitoring/rig_hook.rs` — RuntimeMonitoringHook (no longer needed)
- `src/models/tool.rs` — `RigToolAdapter` struct + `into_dynamic_tool()` method
- `patches/rig-fastembed/` — stub crate (no longer needed)

**Modified files (Phase 2 — cleanup):**
- `Cargo.toml` — remove `rig` dependency, remove `[patch.crates-io]` section
- `common/Cargo.toml` — remove `rig` dependency, remove `rig-integration` feature
- `common/src/error/types.rs` — remove `From<rig::tool::ToolExecutionError>` impl
- `src/models/tool.rs` — remove `RigToolAdapter`, remove rig imports
- `src/pkg/tool_registry/handler_adapter/mod.rs` — replace `ToolErrorKind`/`ToolExecutionError` with `common::error::Error`
- `src/pkg/tool_registry/mcp.rs` — replace `ToolErrorKind`/`ToolExecutionError` with `common::error::Error`
- `src/pkg/tool_tracing/tests.rs` — replace `ToolErrorKind`/`ToolExecutionError`
- `src/service/dao/tool_call/mod.rs` — remove `wrap_for_rig` from trait
- `src/service/dao/tool_call/impl.rs` — remove `wrap_for_rig` implementation
- `src/service/dao/tool_call/mcp.rs` — remove `wrap_for_rig` implementation
- `src/service/dal/tool.rs` — remove `wrap_for_rig` proxy method
- `src/service/dal/tool_test.rs` — remove `wrap_for_rig` test
- `src/service/dal/agent_test.rs`, `project_test.rs`, `skill_test.rs`, `task_test.rs`, `memory_test.rs`, `message_test.rs` — remove `DynamicTool` imports (no longer needed for cortex construction)
- `src/service/domain/runtime/tool_execution_test.rs` — replace `ToolErrorKind`/`ToolExecutionError`
- `src/service/domain/message/delivery_test.rs` — replace `ToolErrorKind`/`ToolExecutionError`
- `src/lib.rs` (or main entry) — change `cortex::rig::init()` to `cortex::native::init()`

---

### Task 1: Define new cortex types (ThinkResult, ToolDescriptor, ToolCallRequest)

**Files:**
- Create: `src/models/cortex_types.rs`
- Modify: `src/models/mod.rs` (add module declaration)

- [ ] **Step 1: Create `src/models/cortex_types.rs`**

```rust
//! Cortex 类型定义 - 思考层与执行层之间的契约类型
//!
//! CortexDao（思考层）通过 ThinkResult 向上层（执行层）表达思考结果：
//! - Final：思考完成，返回最终回答
//! - ToolCall：要求调用工具（cortex DAO 不执行工具，只表达意图）

use serde::{Deserialize, Serialize};

/// 思考结果
///
/// 由 CortexDao::think() 返回，表达 LLM 的思考结果：
/// - Final：模型给出了最终回答，思考结束
/// - ToolCall：模型要求调用某个工具，执行层负责执行后把结果拼回 prompt 再次调用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ThinkResult {
    /// 最终回答（思考结束）
    Final(String),
    /// 要求调用工具（想用工具但还没动手）
    ToolCall(ToolCallRequest),
}

/// 工具调用请求
///
/// 只包含"用哪个工具 + 什么参数"，不包含 tool_call_id（由上层生成）。
/// 这与人的思考方式一致：大脑决定"我要用搜索工具搜 X"，身体负责执行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRequest {
    /// 工具名（对应 CoreTool.po().name）
    pub tool_name: String,
    /// 参数（JSON）
    pub arguments: serde_json::Value,
}

/// 工具描述
///
/// CortexDao 感知的最小单元，用于传递给 LLM 的 tools 字段。
/// 由 CoreTool 转换而来，每次 think() 调用时动态传递（支持动态工具列表）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    /// 工具名称
    pub name: String,
    /// 工具描述
    pub description: String,
    /// 参数 JSON Schema
    pub parameters: serde_json::Value,
}

impl ToolDescriptor {
    /// 转换为 OpenAI Chat Completions API 的 tool 对象
    ///
    /// 格式：
    /// ```json
    /// {
    ///   "type": "function",
    ///   "function": {
    ///     "name": "...",
    ///     "description": "...",
    ///     "parameters": { ... }
    ///   }
    /// }
    /// ```
    pub fn to_openai_tool(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters,
            }
        })
    }
}

/// 从业务层 `Tool` 结构体直接派生 `ToolDescriptor`
///
/// 业务代码层面直接传递我们的 `Tool`，在 cortex dao 中再转换为
/// 下游协议需要的 tool 信息结构。这样 awakening / domain 层无需
/// 手动从 `ToolPo` 字段构建 `ToolDescriptor`，统一通过 `From` trait 派生。
impl From<&crate::models::tool::Tool> for ToolDescriptor {
    fn from(tool: &crate::models::tool::Tool) -> Self {
        ToolDescriptor {
            name: tool.po.name.clone(),
            description: tool.po.description.clone(),
            parameters: tool.po.parameters_schema.clone().unwrap_or_else(|| {
                serde_json::json!({"type": "object", "properties": {}})
            }),
        }
    }
}
```

> **关键变化：** 新增 `impl From<&Tool> for ToolDescriptor`，业务层直接传递 `Tool`，由 cortex dao 层通过 `From` trait 派生 `ToolDescriptor`。`Tool` struct 持有 `po: ToolPo`（含 `name` / `description` / `parameters_schema`），派生时若 `parameters_schema` 为 `None` 则使用默认空 JSON Schema。

- [ ] **Step 2: Add module declaration in `src/models/mod.rs`**

Find the existing module list in `src/models/mod.rs` and add:

```rust
pub mod cortex_types;
```

Also add a re-export at the top of the file (alongside other `pub use` statements):

```rust
pub use cortex_types::{ThinkResult, ToolCallRequest, ToolDescriptor};
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check --message-format=short 2>&1`
Expected: zero errors related to the new file.

---

### Task 2: 删除 CortexTrait + Cortex 实体，升级 CortexDao trait

**Files:**
- Modify: `src/models/brain.rs` — 删除 `CortexTrait` trait + `Cortex` 实体；`Brain` 去掉 `cortex` 字段，新增 `model_provider: Option<ModelProviderPo>` 字段；`Brain::new_local(...)` 接收 `model_provider` 参数并保存；新增 `Brain::new_external(kind, ...)` 方法（外部 agent 不传 provider）
- Modify: `src/service/dao/cortex/mod.rs` — 重写 `CortexDao` trait（删除 `create_cortex_trait` / `prompt` / `embed_text_raw` / `embed_entity` / `embed_text_for_search`，新增 `think` + `embed`，并提供 `embed_text` / `embed_entity` / `embed_text_for_search` 的默认实现）

- [ ] **Step 1: 删除 `CortexTrait` trait + `Cortex` 实体 + `Brain.cortex` 字段，新增 `Brain.model_provider` 字段**

In `src/models/brain.rs`:

1. 删除整个 `CortexTrait` trait 定义（包含 `prompt()`、`embeddings()`、`capability()`、`model_provider_id()`、`model_name()`、`support_tools()` 等方法）。
2. 删除整个 `Cortex` 实体（持有 `ModelProvider + Box<dyn CortexTrait>` 的 struct）及其 `impl` 块、`cortex()` 访问器。
3. 在 `Brain` struct 中去掉 `cortex: Option<Cortex>` 字段，**新增 `pub model_provider: Option<ModelProviderPo>` 字段**（Local agent 为 `Some`，外部 agent 为 `None`）。
4. 更新 `Brain::new_local(...)` 构造方法，**新增 `model_provider: ModelProviderPo` 参数**并保存为 `Some(model_provider)`；其他字段保持不变。
5. **新增 `Brain::new_external(kind, ...)` 构造方法**，接收 `kind: AgentKind`（`Cli` 或 `Remote`）等参数，`model_provider: None`。
6. 删除 `use common::enums::ModelCapability;` 中仅被 `CortexTrait`/`Cortex` 用到的导入（如果 `AgentKind` 仍在用则保留 `AgentKind`）。
7. 删除 `dyn_clone::DynClone` 相关导入（仅 `CortexTrait` 用到）。
8. 新增 `use crate::models::model_provider::ModelProviderPo;` 导入。

新的 `Brain` struct 应该形如：

```rust
use crate::models::model_provider::ModelProviderPo;
use crate::models::memory::Memory;
use crate::models::runtime_config::AgentRuntimeConfig;
use common::enums::AgentKind;

pub struct Brain {
    pub kind: AgentKind,
    pub agent_id: String,
    pub agent_name: String,
    pub runtime_config: AgentRuntimeConfig,
    pub memories: Vec<Memory>,
    /// Local agent 的模型配置（通过 agent 的 model provider 配置进入运行时 brain）
    /// Cli/Remote agent 可能为 None
    pub model_provider: Option<ModelProviderPo>,
}
```

`Brain::new_local(...)` 接收 `model_provider` 参数并保存：

```rust
impl Brain {
    pub fn new_local(
        agent_id: String,
        agent_name: String,
        runtime_config: AgentRuntimeConfig,
        memories: Vec<Memory>,
        model_provider: ModelProviderPo,  // 新增参数
    ) -> Self {
        Self {
            kind: AgentKind::Local,
            agent_id,
            agent_name,
            runtime_config,
            memories,
            model_provider: Some(model_provider),
        }
    }

    /// 外部 agent（CLI 子进程 / A2A 协议）的 Brain 装配方法
    ///
    /// 外部 agent 不需要 model_provider（自行管理 LLM 调用），保存为 None
    pub fn new_external(
        kind: AgentKind,  // AgentKind::Cli 或 AgentKind::Remote
        agent_id: String,
        agent_name: String,
        runtime_config: AgentRuntimeConfig,
        memories: Vec<Memory>,
        // 不接收 model_provider（外部 agent 不需要）
    ) -> Self {
        debug_assert!(
            matches!(kind, AgentKind::Cli | AgentKind::Remote),
            "new_external 仅用于 Cli/Remote agent"
        );
        Self {
            kind,
            agent_id,
            agent_name,
            runtime_config,
            memories,
            model_provider: None,
        }
    }
}
```

> **关键变化：**
> - Brain 直接持有 `model_provider: Option<ModelProviderPo>`，相当于通过 agent 的 model provider 配置进入了运行时 brain
> - `Brain::new_local(...)` 接收 `model_provider: ModelProviderPo` 参数并保存为 `Some(...)`
> - 新增 `Brain::new_external(kind, ...)` 方法，外部 agent 不传 provider，`model_provider: None`
> - 后续 `think()` 时从 `brain.model_provider.as_ref()` 读取 provider，无需在 `think()` 参数中传入

- [ ] **Step 2: 重写 `CortexDao` trait**

In `src/service/dao/cortex/mod.rs`，删除旧的 `CortexDao` trait（含 `create_cortex_trait` / `prompt` / `embed_text_raw` / `embed_entity` / `embed_text_for_search`），替换为：

```rust
use crate::models::cortex_types::{ThinkResult, ToolDescriptor};
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::request_context::RequestContext;

/// Cortex DAO - 模型调用的统一抽象
///
/// 不同 provider_type 有不同的具体实现（OpenAI兼容 / FastEmbed / External）。
/// BrainDal 根据 ModelProvider.provider_type 在 CortexDaoRegistry 中选择对应实现。
/// provider 中的配置（base_url, api_key, model_name 等）优先于默认值，
/// 由具体实现内部解析（如 OpenAiCompatibleCortexDao::resolve_base_url）。
#[async_trait::async_trait]
pub trait CortexDao: Send + Sync {
    /// 思考：接收 prompt + 工具列表，返回最终回答或工具调用请求
    ///
    /// - `provider`: 模型提供商配置（base_url/api_key/model_name 等从中读取）
    /// - `prompt`: 完整提示词（含系统提示、历史、当前消息等）
    /// - `tools`: 本次可用的工具列表（动态）
    async fn think(
        &self,
        ctx: RequestContext,
        provider: &ModelProviderPo,
        prompt: &str,
        tools: &[ToolDescriptor],
    ) -> Result<ThinkResult>;

    /// 向量化：批量生成文本向量
    async fn embed(
        &self,
        ctx: RequestContext,
        provider: &ModelProviderPo,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>>;

    /// 向量化单条文本（便捷方法）
    async fn embed_text(
        &self,
        ctx: RequestContext,
        provider: &ModelProviderPo,
        text: &str,
    ) -> Result<Vec<f32>> {
        let vectors = self
            .embed(ctx, provider, std::slice::from_ref(&text.to_string()))
            .await?;
        Ok(vectors.into_iter().next().unwrap_or_default())
    }

    /// 向量化实体（提取文本 + 计算哈希 + 调用 embed）
    async fn embed_entity(
        &self,
        ctx: RequestContext,
        provider: &ModelProviderPo,
        entity: &dyn crate::models::vector::Vectorizable,
    ) -> Result<crate::models::vector::VectorIndexParams> {
        let text = entity.vectorize_text();
        let vector = self.embed_text(ctx.clone(), provider, &text).await?;
        Ok(crate::models::vector::VectorIndexParams {
            vector,
            content_hash: entity.vector_content_hash(),
            model_provider_id: provider.id.clone(),
            embedding_model: provider.model_name.clone(),
            expire_at: entity.vector_expire_at(),
        })
    }

    /// 向量化搜索关键词
    async fn embed_text_for_search(
        &self,
        ctx: RequestContext,
        provider: &ModelProviderPo,
        text: &str,
    ) -> Result<crate::models::vector::VectorIndexParams> {
        let vector = self.embed_text(ctx, provider, text).await?;
        let content_hash = sha256::digest(text);
        Ok(crate::models::vector::VectorIndexParams {
            vector,
            content_hash,
            model_provider_id: provider.id.clone(),
            embedding_model: provider.model_name.clone(),
            expire_at: None,
        })
    }
}
```

同时在 `mod.rs` 中删除 `mod rig;` / `pub use self::rig::{RigCortexDao, dao, init};` 等旧导出（rig 模块在 Task 9 整体删除，本步骤先注释或删除 `pub use`，使编译错误集中在 `rig.rs` 内部）。`pub mod native;` 在 Task 3 中添加。

Key changes:
- 删除 `create_cortex_trait()` 工厂方法 — 不再预构造 Cortex 对象
- `prompt(ctx, cortex: &dyn CortexTrait, prompt, tools)` → `think(ctx, provider: &ModelProviderPo, prompt, tools)`
- `embed_text_raw(ctx, cortex, text)` → `embed_text(ctx, provider, text)`（trait 默认实现，调用 `embed()`）
- `embed_entity` / `embed_text_for_search` 改为 trait 默认实现，从 `provider.id` / `provider.model_name` 读取元数据（不再从 `cortex.model_provider_id()` / `cortex.model_name()` 读取）
- 删除 `mod rig;` 的 `pub use` 导出

- [ ] **Step 2b: 新增 cortex 包级函数 `embed_entity` / `embed_text_for_search`**

在 `src/service/dao/cortex/mod.rs` 中，紧接 `CortexDao` trait 定义之后，新增两个**包级函数**（不是 trait 方法）。这些函数不依赖任何 DAL，纯协议层：根据 `provider.provider_type` 从 `CortexDaoRegistry` 获取对应 DAO，执行向量化，组装 `VectorIndexParams`。

```rust
use crate::models::vector::{VectorIndexParams, Vectorizable};
use crate::pkg::request_context::RequestContext;
use crate::models::model_provider::ModelProviderPo;

/// 向量化实体（包级函数，不依赖任何 DAL）
///
/// 根据 `provider.provider_type` 选择对应 CortexDao，执行向量化。
/// 调用方负责获取 provider（如从 `model_provider_dao` 查询）。
///
/// 与 `CortexDao::embed_entity` trait 默认实现的区别：
/// - trait 方法需要先持有 `Arc<dyn CortexDao>`（调用方自行从 registry 获取）
/// - 包级函数内部自动从 registry 获取 DAO，调用方只需传 provider + entity
///
/// 适用场景：
/// - DAL 层向量化调用点（注入了 `model_provider_dao`，查到 provider 后调用本函数）
/// - BrainDal 的 `embed_entity` 入口（内部查默认 provider 后调用本函数）
pub async fn embed_entity(
    ctx: RequestContext,
    provider: &ModelProviderPo,
    entity: &dyn Vectorizable,
) -> anyhow::Result<VectorIndexParams> {
    let dao = crate::service::dao::cortex::native::cortex_registry()
        .get(provider.provider_type);
    dao.embed_entity(ctx, provider, entity).await
}

/// 向量化搜索关键词（包级函数，不依赖任何 DAL）
///
/// 与 `embed_entity` 类似，根据 `provider.provider_type` 选择 DAO，
/// 对纯文本进行向量化（无实体元信息，`expire_at: None`）。
pub async fn embed_text_for_search(
    ctx: RequestContext,
    provider: &ModelProviderPo,
    text: &str,
) -> anyhow::Result<VectorIndexParams> {
    let dao = crate::service::dao::cortex::native::cortex_registry()
        .get(provider.provider_type);
    dao.embed_text_for_search(ctx, provider, text).await
}
```

> **关键设计：**
> - 这两个是**包级函数**（`pub async fn`），不是 `CortexDao` trait 方法。放在 `src/service/dao/cortex/mod.rs` 中。
> - 内部通过 `cortex_registry().get(provider.provider_type)` 获取 DAO，再调用 trait 默认实现的 `embed_entity` / `embed_text_for_search`。
> - **不依赖任何 DAL**（不注入 `model_provider_dao` 等），纯协议层。调用方负责获取 provider。
> - DAL 层不再定义 `try_build_vector_params_*` helper，各 DAL 方法（`upsert_vector_index` / `search` 等）内部自查 provider 后直接调用本函数完成向量化（见 Task 6b）。
> - BrainDal 的 `embed_entity` / `embed_text_for_search` 入口也可调用本函数（内部多了一步查默认 provider）。
> - 签名与 `CortexDao` trait 默认实现保持一致（`ctx, provider, entity/text`），确保调用方在包级函数和 trait 方法之间可无缝切换。

> **Note:** 包级函数依赖 `cortex_registry()` 单例，因此必须在 `cortex::native::init()` 调用后才能使用（Task 8 修改初始化点）。Phase 1 期间 DAL 层调用点暂保持现状（Task 6b 统一改造），本步骤仅定义函数。

- [ ] **Step 3: Verify compilation errors (expected — 下游调用方尚未更新)**

Run: `cargo check --message-format=short 2>&1`
Expected: errors in `cortex/rig.rs`、`cortex/external.rs`、`cortex/rig/doubao_vision.rs`、`cortex/rig/fastembed.rs`、`dal/brain.rs`、`awakening.rs`（签名不匹配 / 引用了已删除的 `Cortex` / `CortexTrait`）。这些错误将在后续 Task 中修复。

---

### Task 3: Implement native CortexDao (shared HTTP helpers + OpenAiCompatibleCortexDao + CortexDaoRegistry)

**Files:**
- Create: `src/service/dao/cortex/native/mod.rs` — `CortexDaoRegistry`（按 `provider_type` 分发）
- Create: `src/service/dao/cortex/native/http.rs` — shared HTTP helpers
- Create: `src/service/dao/cortex/native/openai.rs` — `OpenAiCompatibleCortexDao`

- [ ] **Step 1: Create `src/service/dao/cortex/native/http.rs` — shared HTTP helpers**

```rust
//! Native cortex DAO HTTP helpers — 共享的 Chat Completions API 调用逻辑
//!
//! 所有 provider（OpenAI/DeepSeek/Qwen/Doubao/DoubaoVision/Ollama/Custom）统一走
//! POST {base_url}/chat/completions，使用 OpenAI Chat Completions 协议。
//! 这些函数接收 base_url/api_key/model 作为参数（由 CortexDao 实现从
//! `&ModelProviderPo` 中解析后传入），自身不持有任何 per-call 状态。

use anyhow::{Result, anyhow};
use reqwest::Client;
use serde_json::{Value, json};

use crate::models::cortex_types::{ThinkResult, ToolCallRequest, ToolDescriptor};
use crate::pkg::request_context::RequestContext;
use crate::pkg::stats::ModelCallEvent;

/// Chat Completions API 请求
pub struct ChatCompletionsRequest {
    pub model: String,
    pub messages: Vec<Value>,
    pub tools: Vec<Value>,
    pub temperature: Option<f32>,
}

/// Chat Completions API 响应解析结果
pub struct ChatCompletionsResponse {
    pub result: ThinkResult,
    pub usage: Option<TokenUsage>,
}

/// Token 用量（从 response body 提取）
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

/// 调用 Chat Completions API 并解析响应
///
/// - `http`: reqwest client（可复用连接池）
/// - `base_url`: provider base URL（如 `https://api.openai.com/v1`）
/// - `api_key`: API 密钥
/// - `request`: 请求参数
/// - `ctx`: 用于 stats 记录
pub async fn call_chat_completions(
    http: &Client,
    base_url: &str,
    api_key: &str,
    request: &ChatCompletionsRequest,
    ctx: &RequestContext,
) -> Result<ChatCompletionsResponse> {
    // 构造请求 body
    let mut body = json!({
        "model": request.model,
        "messages": request.messages,
    });

    if !request.tools.is_empty() {
        body["tools"] = json!(request.tools);
    }

    if let Some(temp) = request.temperature {
        body["temperature"] = json!(temp);
    }

    // 构造 URL
    let url = format!(
        "{}/chat/completions",
        base_url.trim_end_matches('/')
    );

    // 发送请求
    let resp = http
        .post(&url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| anyhow!("Chat Completions request failed: {e}"))?;

    let status = resp.status();
    let response_body: Value = resp
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse Chat Completions response (status {status}): {e}"))?;

    if !status.is_success() {
        let error_msg = response_body
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("unknown error");
        return Err(anyhow!("Chat Completions API error (status {status}): {error_msg}"));
    }

    // 提取 token usage
    let usage = response_body
        .get("usage")
        .map(|u| TokenUsage {
            input_tokens: u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
            output_tokens: u.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
            total_tokens: u.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
        });

    // 记录 stats（token 用量）
    if let Some(ref usage) = usage {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let event = ModelCallEvent::new(timestamp)
            .with_agent_id(ctx.agent_id().cloned())
            .with_project_id(ctx.project_id().cloned())
            .with_task_id(ctx.task_id().cloned())
            .with_model_provider_id(ctx.model_provider_id().cloned())
            .with_model_name(ctx.model_name().cloned())
            .with_organization_id(ctx.organization_id().cloned())
            .with_user_id(ctx.user_id().cloned())
            .with_tokens_input(usage.input_tokens)
            .with_tokens_output(usage.output_tokens)
            .with_total_tokens(usage.total_tokens);

        if let Err(e) = ctx.stats().record(ctx.clone(), event).await {
            tracing::warn!(
                log_id = ctx.log_id,
                error = %e,
                "Failed to record stats event for Chat Completions response"
            );
        }
    }

    // 解析 choices[0].message
    let message = response_body
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .ok_or_else(|| anyhow!("Chat Completions response missing choices[0].message"))?;

    // 检查是否有 tool_calls
    let tool_calls = message.get("tool_calls");

    let result = if let Some(tool_calls) = tool_calls {
        // 取第一个 tool_call（单工具调用模式）
        let tool_call = tool_calls
            .get(0)
            .ok_or_else(|| anyhow!("tool_calls array is empty"))?;

        let function = tool_call
            .get("function")
            .ok_or_else(|| anyhow!("tool_call missing function field"))?;

        let tool_name = function
            .get("name")
            .and_then(|n| n.as_str())
            .ok_or_else(|| anyhow!("tool_call function missing name"))?
            .to_string();

        let arguments_str = function
            .get("arguments")
            .and_then(|a| a.as_str())
            .unwrap_or("{}");

        let arguments: Value = serde_json::from_str(arguments_str).unwrap_or(json!({}));

        ThinkResult::ToolCall(ToolCallRequest {
            tool_name,
            arguments,
        })
    } else {
        // 最终回答
        let content = message
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();
        ThinkResult::Final(content)
    };

    Ok(ChatCompletionsResponse { result, usage })
}

/// 调用 Embeddings API
pub async fn call_embeddings(
    http: &Client,
    base_url: &str,
    api_key: &str,
    model: &str,
    texts: &[String],
) -> Result<Vec<Vec<f32>>> {
    let url = format!("{}/embeddings", base_url.trim_end_matches('/'));

    let body = json!({
        "model": model,
        "input": texts,
    });

    let resp = http
        .post(&url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| anyhow!("Embeddings request failed: {e}"))?;

    let status = resp.status();
    let response_body: Value = resp
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse Embeddings response (status {status}): {e}"))?;

    if !status.is_success() {
        let error_msg = response_body
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("unknown error");
        return Err(anyhow!("Embeddings API error (status {status}): {error_msg}"));
    }

    // 提取向量
    let data = response_body
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| anyhow!("Embeddings response missing data array"))?;

    let vectors: Vec<Vec<f32>> = data
        .iter()
        .map(|item| {
            item.get("embedding")
                .and_then(|e| e.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                        .collect()
                })
                .unwrap_or_default()
        })
        .collect();

    Ok(vectors)
}

/// 将 ToolDescriptor 列表转换为 OpenAI tools 格式
pub fn tools_to_openai_format(tools: &[ToolDescriptor]) -> Vec<Value> {
    tools.iter().map(|t| t.to_openai_tool()).collect()
}

/// 调用多模态 Embeddings API（如豆包 Vision 的 /embeddings/multimodal）
///
/// 与标准 /embeddings 的差异：
/// - 请求体：input 是对象数组 [{type:"text", text:"..."}] 而非字符串数组
/// - 响应体：data.embedding（单对象）而非 data[0].embedding（数组元素）
/// - 多文本场景必须逐条请求（multimodal endpoint 会把 input 数组融合成一个 embedding）
pub async fn call_embeddings_multimodal(
    http: &Client,
    base_url: &str,
    api_key: &str,
    model: &str,
    texts: &[String],
) -> Result<Vec<Vec<f32>>> {
    let url = format!("{}/embeddings/multimodal", base_url.trim_end_matches('/'));

    // multimodal endpoint 逐条请求（多文本会融合成一个 embedding）
    let mut results = Vec::with_capacity(texts.len());
    for text in texts {
        let body = json!({
            "model": model,
            "input": [{"type": "text", "text": text}],
        });

        let resp = http
            .post(&url)
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow!("Multimodal embedding request failed: {e}"))?;

        let status = resp.status();
        let response_body: Value = resp
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse multimodal embedding response (status {status}): {e}"))?;

        if !status.is_success() {
            let error_msg = response_body
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error");
            return Err(anyhow!("Multimodal embedding API error (status {status}): {error_msg}"));
        }

        // multimodal 响应格式：data.embedding（单对象，非数组）
        let vector: Vec<f32> = response_body
            .get("data")
            .and_then(|d| d.get("embedding"))
            .and_then(|e| e.as_array())
            .map(|arr| arr.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect())
            .ok_or_else(|| anyhow!("Multimodal embedding response missing data.embedding"))?;

        results.push(vector);
    }

    Ok(results)
}
```

- [ ] **Step 2: Create `src/service/dao/cortex/native/openai.rs` — `OpenAiCompatibleCortexDao`（不再是 Cortex struct）**

```rust
//! Native OpenAI-compatible CortexDao — 直接用 reqwest 调用 Chat Completions API
//!
//! 适用于所有 OpenAI 兼容的 provider（OpenAI/DeepSeek/Qwen/Doubao/DoubaoVision/Ollama/Custom），
//! 通过 `&ModelProviderPo` 在方法内读取 base_url/api_key/model_name（provider 配置优先于默认值）。
//! 该 DAO 为单例，仅持有共享的 `reqwest::Client`（连接池），不持有任何 per-call 状态。

use anyhow::Result;
use async_trait::async_trait;
use common::enums::ProviderType;
use reqwest::Client;

use crate::models::cortex_types::{ThinkResult, ToolDescriptor};
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::request_context::RequestContext;

use super::http::{
    call_chat_completions, call_embeddings, call_embeddings_multimodal, tools_to_openai_format,
    ChatCompletionsRequest,
};

/// OpenAI 兼容 CortexDao（单例，处理所有 OpenAI 兼容 provider）
///
/// 不再持有 api_key/base_url/model_name/ctx/provider_type 字段 —— 这些全部
/// 从每次 `think()` / `embed()` 调用传入的 `&ModelProviderPo` 中读取。
pub struct OpenAiCompatibleCortexDao {
    /// 共享 HTTP client（连接池），所有 provider 共用
    http: Client,
}

impl OpenAiCompatibleCortexDao {
    /// 构造单例（由 `CortexDaoRegistry::init` 调用）
    pub fn new() -> Result<Self> {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build HTTP client: {e}"))?;
        Ok(Self { http })
    }
}

#[async_trait]
impl super::super::CortexDao for OpenAiCompatibleCortexDao {
    async fn think(
        &self,
        ctx: RequestContext,
        provider: &ModelProviderPo,
        prompt: &str,
        tools: &[ToolDescriptor],
    ) -> Result<ThinkResult> {
        // 从 provider 读取配置（base_url 优先用 provider 配置，否则用默认值）
        let base_url = resolve_base_url(provider);
        let api_key = &provider.api_key;
        let model = &provider.model_name;

        // 调用 Chat Completions API
        let request = ChatCompletionsRequest {
            model: model.clone(),
            messages: vec![serde_json::json!({
                "role": "user",
                "content": prompt,
            })],
            tools: tools_to_openai_format(tools),
            temperature: Some(0.7),
        };

        let response = call_chat_completions(&self.http, &base_url, api_key, &request, &ctx).await?;
        Ok(response.result)
    }

    async fn embed(
        &self,
        ctx: RequestContext,
        provider: &ModelProviderPo,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        let base_url = resolve_base_url(provider);
        let api_key = &provider.api_key;
        let model = &provider.model_name;

        // DoubaoVision 走多模态 embedding endpoint
        if provider.provider_type == ProviderType::DoubaoVision {
            return call_embeddings_multimodal(
                &self.http, &base_url, api_key, model, texts,
            )
            .await;
        }

        // 标准 embedding endpoint
        call_embeddings(&self.http, &base_url, api_key, model, texts).await
    }
}

/// 根据 provider_type 解析 base_url（provider 配置优先于默认值）
///
/// 优先使用 `provider.base_url`（如果指定），否则按 `provider_type` 返回默认值。
fn resolve_base_url(provider: &ModelProviderPo) -> String {
    if let Some(ref url) = provider.base_url {
        return url.clone();
    }
    match provider.provider_type {
        ProviderType::OpenAI => "https://api.openai.com/v1".to_string(),
        ProviderType::DeepSeek => "https://api.deepseek.com".to_string(),
        ProviderType::Qwen => "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(),
        ProviderType::Doubao | ProviderType::DoubaoVision => {
            "https://ark.cn-beijing.volces.com/api/v3".to_string()
        }
        ProviderType::Ollama => "http://localhost:11434/v1".to_string(),
        // Custom 必须由 provider 提供base_url，否则返回空（调用方会因 URL 异常失败）
        _ => String::new(),
    }
}
```

> **关键变化（相比旧 `OpenAiCompatibleCortex`）：**
> - struct 改名为 `OpenAiCompatibleCortexDao`，实现 `CortexDao` trait（不再是 `CortexTrait`）
> - struct 只持有 `http: Client`（共享连接池），不再持有 `api_key`/`base_url`/`model_name`/`ctx`/`provider_type` 字段
> - `new()` 不再接收这些参数，只构造 HTTP client
> - `think()` / `embed()` 从 `&ModelProviderPo` 读取所有配置
> - `resolve_base_url(provider)` 函数封装了"provider 配置优先于默认值"的解析逻辑
> - 不再有 `capability()` / `model_provider_id()` / `model_name()` 方法（这些元数据由 trait 默认实现的 `embed_entity` / `embed_text_for_search` 直接从 `provider` 读取）
> - `embed()` 根据 `provider.provider_type == DoubaoVision` 分支走多模态 endpoint

- [ ] **Step 3: Create `src/service/dao/cortex/native/mod.rs` — `CortexDaoRegistry` + 单例访问**

```rust
//! Native CortexDao 注册表 — 按 ProviderType 分发到具体的 CortexDao 实现
//!
//! 不再有 `create_cortex_trait()` 工厂方法，也不再预构造 Cortex 对象。
//! BrainDal 在 `think()` 时从 `brain.model_provider.as_ref()` 读取 provider，
//! 然后调用 `cortex_registry().get(provider.provider_type)` 获取对应的 `Arc<dyn CortexDao>`，
//! 再调用 `dao.think(ctx, provider, prompt, tools)`。

use anyhow::Result;
use common::enums::ProviderType;
use std::sync::{Arc, OnceLock};

pub mod http;
pub mod openai;

// FastEmbedCortexDao / ExternalCortexDao 在 Task 6 中实现并接入 registry
// use crate::service::dao::cortex::fastembed::FastEmbedCortexDao;
// use crate::service::dao::cortex::external::ExternalCortexDao;

use crate::service::dao::cortex::CortexDao;

/// CortexDao 注册表 - 按 ProviderType 分发
///
/// 持有三个单例 DAO 实现，根据 `provider_type` 返回对应的 `Arc<dyn CortexDao>`。
/// BrainDal 在 `think()` 中通过 `cortex_registry().get(provider.provider_type)` 获取。
/// `ExternalCortexDao` 用于 `AgentKind::Cli`/`Remote`，不通过 `get()` 选择
/// （由 BrainDal 在 `brain.kind` 分支中直接持有）。
pub struct CortexDaoRegistry {
    openai_compatible: Arc<openai::OpenAiCompatibleCortexDao>,
    // 以下两项在 Task 6 中接入：
    // fastembed: Arc<crate::service::dao::cortex::fastembed::FastEmbedCortexDao>,
    // external: Arc<crate::service::dao::cortex::external::ExternalCortexDao>,
}

static CORTEX_REGISTRY: OnceLock<Arc<CortexDaoRegistry>> = OnceLock::new();

/// 获取 CortexDaoRegistry 单例
pub fn cortex_registry() -> Arc<CortexDaoRegistry> {
    CORTEX_REGISTRY.get().expect("CortexDaoRegistry not initialized").clone()
}

/// 初始化 CortexDaoRegistry（在 `src/lib.rs` 启动时调用一次）
pub fn init() {
    let openai_compatible = Arc::new(
        openai::OpenAiCompatibleCortexDao::new()
            .expect("Failed to construct OpenAiCompatibleCortexDao"),
    );

    let registry = Arc::new(CortexDaoRegistry {
        openai_compatible,
        // fastembed / external 在 Task 6 接入
    });
    let _ = CORTEX_REGISTRY.set(registry);
}

impl CortexDaoRegistry {
    /// 根据 `provider_type` 获取对应的 CortexDao
    ///
    /// 用于 `AgentKind::Local` 路径：BrainDal 调用此方法获取 DAO，
    /// 再调用 `dao.think(ctx, provider, prompt, tools)`。
    pub fn get(&self, provider_type: ProviderType) -> Arc<dyn CortexDao> {
        match provider_type {
            ProviderType::OpenAI
            | ProviderType::DeepSeek
            | ProviderType::Qwen
            | ProviderType::Doubao
            | ProviderType::DoubaoVision
            | ProviderType::Ollama
            | ProviderType::Custom => self.openai_compatible.clone(),
            ProviderType::FastEmbed => {
                // Task 6 接入：self.fastembed.clone()
                unimplemented!("FastEmbedCortexDao 将在 Task 6 中接入")
            }
        }
    }

    // Task 6 接入：暴露 external DAO 给 BrainDal 的 Cli/Remote 分支使用
    // pub fn external(&self) -> Arc<dyn CortexDao> {
    //     self.external.clone()
    // }
}
```

> **关键变化（相比旧 `NativeCortexDao`）：**
> - 不再有 `create_cortex_trait()` 工厂方法 — DAO 是无状态单例，不预构造 Cortex 对象
> - 不再有按 `ModelCapability` × `ProviderType` 的二维 match 分发（旧代码 80+ 行）
> - 新的 `get(provider_type)` 一维 match 直接返回 `Arc<dyn CortexDao>`
> - `cortex_registry()` 取代旧 `dao()`，返回注册表而非单一 DAO
> - `init()` 构造三个单例 DAO（OpenAI compatible / FastEmbed / External），Task 6 接入后两者

- [ ] **Step 4: Add native module to cortex mod.rs**

In `src/service/dao/cortex/mod.rs`，添加 native 模块声明（保留 `mod rig;` 临时用于 Phase 1，Task 9 删除）：

```rust
pub mod native;
```

`CortexDao` trait 已在 Task 2 中重写。这里只需确保 `mod.rs` 不再 `pub use self::rig::{RigCortexDao, dao, init};`（在 Task 2 Step 2 已删除），改为通过 `native::cortex_registry()` / `native::init()` 访问。

- [ ] **Step 5: Verify native module compiles (rig module will still have errors — that's OK)**

Run: `cargo check --message-format=short 2>&1 | grep "native"`
Expected: no errors in `native/` files (errors in `rig.rs` and downstream are expected).

---

### Task 4: Update BrainDal::think() signature（不接收 provider，从 brain 读取）

**Files:**
- Modify: `src/service/dal/brain.rs:200-333` (and `think_without_memory` if it exists)

- [ ] **Step 1: Update `think()` signature in BrainDal**

In `src/service/dal/brain.rs`, find the `think` method signature (around line 200):

```rust
async fn think(&self, ctx: RequestContext, brain: &Brain, prompt: &str) -> Result<String> {
```

Replace with（新增 `tools` 参数，**不接收 `provider`**（从 `brain.model_provider` 读取），返回类型改为 `ThinkResult`）:

```rust
async fn think(
    &self,
    ctx: RequestContext,
    brain: &Brain,
    prompt: &str,
    tools: &[crate::models::cortex_types::ToolDescriptor],
    // 不接收 provider 参数，从 brain.model_provider 读取
) -> Result<crate::models::cortex_types::ThinkResult> {
```

Then update the body. Find the `Local` branch (around line 220)：

```rust
let result = self
    .cortex_dao
    .prompt(ctx.clone(), cortex.cortex(), prompt)
    .await
    .map_err(|e: anyhow::Error| {
        err!(Internal, "brain think failed: {e}")
            .with_source::<common::error::Error>(e.into())
    });
```

Replace with（按 `brain.kind` 分支：`Local` 从 `brain.model_provider.as_ref()` 读取 provider，走 `CortexDaoRegistry` 分发；`Cli`/`Remote` 走 `ExternalCortexDao` 的 `execute_cli`/`execute_a2a`；不再依赖 `cortex.cortex()`，也不接收 provider 参数）:

```rust
let result = match brain.kind {
    common::enums::AgentKind::Local => {
        // 从 brain 读取 provider（brain 自带 model_provider 字段）
        let provider = brain.model_provider.as_ref()
            .ok_or_else(|| err!(Internal, "Local brain 缺少 model_provider"))?;
        // 根据 provider_type 选择 cortex dao
        let dao = crate::service::dao::cortex::native::cortex_registry()
            .get(provider.provider_type);
        dao.think(ctx.clone(), provider, prompt, tools)
            .await
            .map_err(|e: anyhow::Error| {
                err!(Internal, "brain think failed: {e}")
                    .with_source::<common::error::Error>(e.into())
            })
    }
    common::enums::AgentKind::Cli => {
        // 外部 agent（CLI 子进程），通过 ExternalCortexDao 执行，不需要 provider
        // （brain.model_provider 在外部 agent 路径下为 None，但也不需要读取）
        let external_dao = crate::service::dao::cortex::native::cortex_registry()
            .external();
        external_dao.execute_cli(ctx.clone(), brain, prompt)
            .await
            .map(|text| crate::models::cortex_types::ThinkResult::Final(text))
            .map_err(|e: anyhow::Error| {
                err!(Internal, "brain think (cli) failed: {e}")
                    .with_source::<common::error::Error>(e.into())
            })
    }
    common::enums::AgentKind::Remote => {
        // 外部 agent（A2A 协议），通过 ExternalCortexDao 执行，不需要 provider
        let external_dao = crate::service::dao::cortex::native::cortex_registry()
            .external();
        external_dao.execute_a2a(ctx.clone(), brain, prompt)
            .await
            .map(|text| crate::models::cortex_types::ThinkResult::Final(text))
            .map_err(|e: anyhow::Error| {
                err!(Internal, "brain think (a2a) failed: {e}")
                    .with_source::<common::error::Error>(e.into())
            })
    }
};
```

> **关键变化：**
> - `think()` **不接收 `provider` 参数**，从 `brain.model_provider.as_ref()` 读取（Local 分支）
> - Local 分支：`brain.model_provider.as_ref().ok_or_else(...)?` 解出 provider 后传给 `cortex_registry().get(provider.provider_type).think(ctx, provider, prompt, tools)`
> - Cli/Remote 分支：不读取 `brain.model_provider`（外部 agent 不需要 provider），直接调 `cortex_registry().external().execute_cli(...)` / `execute_a2a(...)`
> - 不再依赖 `cortex.cortex()`（`cortex` 字段已删除）
> - 不再需要 `self.cortex_dao` 字段（如果 BrainDal 仍有该字段，可在本步骤删除）

- [ ] **Step 2: Update `think_without_memory()` if it exists**

Search for `think_without_memory` in the same file. If found, update its signature similarly (add `tools` parameter, **不接收 provider**（从 brain 读取），change return type to `ThinkResult`). If it delegates to `think()`, just pass through the `tools` parameter.

- [ ] **Step 3: 新增 BrainDal embed 入口（`embed_entity` / `embed_text_for_search`）**

在 `src/service/dal/brain.rs` 中，为 `BrainDal` trait 新增两个向量化入口方法。这些方法是 **domain 层入口**：内部查询默认 Embedding Provider，然后调用 cortex 包级函数（Task 2 Step 2b 定义）完成向量化。

**设计动机（用户原话）：**
> 1. brain dal 仍然提供这样的方法入口，不过对于向量化这种底层能力，我们可以在 cortex dao 提供一个包级函数，直接根据传入的 provider 和可向量化实体完成向量化调用。
> 2. brain dal 提供的入口则是多了一部分获取默认向量化 provider 的能力，然后调用这个方法完成向量化。
> 3. 如果向量化发生在 domain 层，就可以直接复用 brain dal 了。

在 `BrainDal` trait 中新增两个方法：

```rust
use crate::models::vector::{VectorIndexParams, Vectorizable};

#[async_trait::async_trait]
pub trait BrainDal: Send + Sync {
    // 现有方法...
    async fn wake_brain(/* ... */) -> Result<crate::models::brain::Brain>;
    async fn think(
        &self,
        ctx: RequestContext,
        brain: &crate::models::brain::Brain,
        prompt: &str,
        tools: &[crate::models::cortex_types::ToolDescriptor],
    ) -> Result<crate::models::cortex_types::ThinkResult>;

    /// 向量化实体（自动获取默认 Embedding Provider）
    ///
    /// Domain 层入口：内部查询默认 Embedding Provider，然后调用
    /// `cortex::embed_entity()` 包级函数完成向量化。
    ///
    /// 返回 `None` 表示无可用 provider，调用方应降级跳过向量化。
    /// 返回 `Some(params)` 表示向量化成功。
    async fn embed_entity(
        &self,
        ctx: RequestContext,
        entity: &dyn Vectorizable,
    ) -> Result<Option<VectorIndexParams>>;

    /// 向量化搜索关键词（自动获取默认 Embedding Provider）
    ///
    /// 与 `embed_entity` 类似，内部查默认 provider 后调用
    /// `cortex::embed_text_for_search()` 包级函数。
    async fn embed_text_for_search(
        &self,
        ctx: RequestContext,
        text: &str,
    ) -> Result<Option<VectorIndexParams>>;
}
```

`BrainDalImpl` 实现内部逻辑（两个方法模式一致）：

```rust
async fn embed_entity(
    &self,
    ctx: RequestContext,
    entity: &dyn Vectorizable,
) -> Result<Option<VectorIndexParams>> {
    // 1. 查询默认 Embedding Provider
    let provider = self
        .model_provider_dao
        .get_default_embedding_provider(ctx.clone())
        .await?;

    // 2. None → 降级跳过（Ok(None)）
    let provider = match provider {
        None => return Ok(None),
        Some(p) => p,
    };

    // 3. 调用 cortex 包级函数完成向量化
    let params = crate::service::dao::cortex::embed_entity(ctx, &provider, entity).await?;
    Ok(Some(params))
}

async fn embed_text_for_search(
    &self,
    ctx: RequestContext,
    text: &str,
) -> Result<Option<VectorIndexParams>> {
    let provider = self
        .model_provider_dao
        .get_default_embedding_provider(ctx.clone())
        .await?;

    let provider = match provider {
        None => return Ok(None),
        Some(p) => p,
    };

    let params = crate::service::dao::cortex::embed_text_for_search(ctx, &provider, text).await?;
    Ok(Some(params))
}
```

> **关键设计：**
> - BrainDal 的 `embed_entity` / `embed_text_for_search` 是 **domain 层入口**，返回 `Option<VectorIndexParams>`（`None` 表示无可用 provider，调用方降级跳过）。
> - 内部先查默认 Embedding Provider（`model_provider_dao.get_default_embedding_provider()`），再调用 cortex 包级函数 `crate::service::dao::cortex::embed_entity()` / `embed_text_for_search()`。
> - **签名与 cortex 包级函数的区别**：BrainDal 入口不接收 `provider` 参数（内部自动查），返回 `Option`（允许降级）；cortex 包级函数接收 `provider`（调用方负责获取），返回 `Result<VectorIndexParams>`（无降级）。
> - **DAL 不能调用 DAL（项目约束）**：因此 DAL 层的向量化调用点不通过 BrainDal 入口，而是直接调用 cortex 包级函数（见 Task 6b）。BrainDal 入口仅供 domain 层使用。
> - Domain 层向量化场景可直接复用 BrainDal 入口，无需自行注入 `model_provider_dao` + `cortex_dao`。

- [ ] **Step 4: Verify compilation errors (expected — awakening.rs not yet updated)**

Run: `cargo check --message-format=short 2>&1`
Expected: errors in `awakening.rs` (calls `think()` with old signature, still references `cortex`). Will be fixed in Task 5.

---

### Task 5: Implement explicit tool-calling loop in awakening.rs

**Files:**
- Modify: `src/service/domain/runtime/awakening.rs`

This is the core architectural change. The `awaken()` method now implements an explicit loop: call `brain.think()` → if `ToolCall`, execute the tool, append result to prompt, loop back.

- [ ] **Step 1: Add a constant for max tool rounds**

Near the top of `awakening.rs` (after imports, before the first function), add:

```rust
/// 单次 awaken 内的最大工具调用轮次（防止无限循环）
const MAX_TOOL_ROUNDS: usize = 10;
```

- [ ] **Step 2: Build the tool descriptor list from agent.tools**

In the `awaken()` method, after the existing prompt building (around line 232 where `builder.tools(&all_tools)` is called), add code to build `ToolDescriptor` list. Find the section after `builder.build()` produces the `prompt` string, and add:

```rust
// 构建工具描述列表（传递给 cortex DAO，用于 LLM function calling）
// 使用 From<&Tool> trait 直接从业务 Tool 派生 ToolDescriptor（Task 1 定义）
let tool_descriptors: Vec<crate::models::cortex_types::ToolDescriptor> = agent
    .tools()
    .iter()
    .filter(|t| t.po.control_mode == common::enums::tool::ControlMode::Auto)
    .map(crate::models::cortex_types::ToolDescriptor::from)  // 使用 From trait 派生
    .collect();
```

> **关键变化：** 工具列表构建从手动逐字段构建 `ToolDescriptor` 简化为 `map(ToolDescriptor::from)`。业务代码层面直接传递我们的 `Tool`，由 `From<&Tool>` 实现（Task 1 定义）负责从 `tool.po`（`ToolPo`）的 `name` / `description` / `parameters_schema` 字段派生 `ToolDescriptor`。cortex dao 层在 `think()` 内部再将 `ToolDescriptor` 转换为下游协议（OpenAI Chat Completions）需要的 tool 信息结构。

Note: Only `Auto` tools are passed as `ToolDescriptor` to the LLM's `tools` field. `Manual` tools are still listed in the prompt text by PromptBuilder (for the agent to request via `request_tool_call` / `send_tool_call_message`), but they don't appear in the LLM's function-calling `tools` array.

- [ ] **Step 3: 不再单独加载 ModelProvider（已合并到 wake_agent_brain）**

> **架构变化：** Brain 现在直接持有 `model_provider`（见 Task 2 + Task 5 Step 6），`awaken()` 不再需要单独加载 `ModelProviderPo` 传给 `think()`。`brain.think()` 不接收 provider 参数，由 BrainDal 从 `brain.model_provider` 读取。
>
> 旧版本的 Step 3（加载 provider）已合并到 Step 6 的 `wake_agent_brain()` 中：在装配 Brain 时加载 ModelProvider 并传给 `Brain::new_local(..., model_provider)`。因此本步骤为空操作，直接跳到 Step 4。

- [ ] **Step 4: Replace the single `brain.think()` call with a loop（不传 provider）**

Find the existing `brain.think()` call in `awaken()` (around line 264):

```rust
let result = self
    .brain_dal()
    .think(ctx.clone(), brain, &prompt)
    .await;
```

Replace it with the explicit tool-calling loop（注意 `think()` 现在不接收 `provider` 参数，brain 自带）:

```rust
// 显式工具调用循环：brain.think() → ToolCall → 执行 → 拼回 prompt → 再次 think
let mut current_prompt = prompt.clone();
let mut current_tools = tool_descriptors.clone();
let mut final_answer = String::new();
let mut tool_round = 0;

loop {
    if tool_round >= MAX_TOOL_ROUNDS {
        final_answer = format!(
            "已达到最大工具调用轮次（{}），当前思考结果：\n{}",
            MAX_TOOL_ROUNDS, current_prompt
        );
        break;
    }

    // brain 自带 model_provider，think() 不需要传 provider
    let think_result = self
        .brain_dal()
        .think(ctx.clone(), brain, &current_prompt, &current_tools)
        .await?;

    match think_result {
        crate::models::cortex_types::ThinkResult::Final(answer) => {
            final_answer = answer;
            break;
        }
        crate::models::cortex_types::ThinkResult::ToolCall(tool_call_request) => {
            // 查找对应的工具
            let tool = agent
                .tools()
                .iter()
                .find(|t| t.po.name == tool_call_request.tool_name);

            let tool_result = match tool {
                Some(tool) => {
                    // 执行工具（通过 ToolCallLoggingDecorator 记录审计）
                    use crate::models::tool::CoreTool;
                    let cloned: Box<dyn CoreTool + Send + Sync> =
                        dyn_clone::clone_box(&*tool.our_tool);
                    let decorated = crate::pkg::tool_tracing::ToolCallLoggingDecorator::new(cloned);
                    match decorated.call(ctx.clone(), tool_call_request.arguments.clone()).await {
                        Ok(result_value) => {
                            serde_json::to_string_pretty(&result_value)
                                .unwrap_or_else(|_| result_value.to_string())
                        }
                        Err(e) => format!("工具执行失败: {e}"),
                    }
                }
                None => {
                    format!("未找到工具: {}", tool_call_request.tool_name)
                }
            };

            // 把工具调用详情拼回 prompt（让模型在下一轮看到结果）
            current_prompt.push_str(&format!(
                "\n\n## 工具调用\n- 工具: {}\n- 参数: {}\n- 结果: {}\n",
                tool_call_request.tool_name,
                serde_json::to_string(&tool_call_request.arguments)
                    .unwrap_or_else(|_| "{}".to_string()),
                tool_result
            ));

            tool_round += 1;
        }
    }
}
```

> **关键变化：** `brain.think()` 调用**不再传 `provider` 参数**（brain 自带 `model_provider` 字段）。provider 在 `wake_agent_brain()` 装配 Brain 时注入一次（见 Step 6），循环内每轮 BrainDal 从 `brain.model_provider` 读取同一 provider（同一 agent 的 provider 不会在循环中变化）。

- [ ] **Step 5: Update the result handling (was using `result` variable, now uses `final_answer`)**

Find the code after the old `brain.think()` call that used `result`. It likely writes to `trace` and returns `AwakeningResult`. Replace references to `result` with `final_answer`. For example:

```rust
// 旧代码可能是：
// let answer = result?;
// trace.output = answer.clone();

// 新代码：
trace.output = final_answer.clone();
```

- [ ] **Step 6: Update `wake_agent_brain()` — 加载 ModelProvider 并注入 Brain**

In `wake_agent_brain()` (around line 130-160)，旧代码会：
1. 分区 Auto/Manual 工具（`rig_tools` 给 rig，`manual` 保留在 `agent.tools`）
2. 调用 `create_cortex_trait(ctx, provider, rig_tools)` 构造 `Box<dyn CortexTrait>`
3. 用 cortex 构造 `Brain { ..., cortex: Some(Cortex::new(...)) }`

新架构下这些都不需要了。`wake_agent_brain()` 只负责：
1. 加载 agent
2. **加载 agent 关联的 `ModelProviderPo`**（通过 agent 的 model provider 配置进入运行时 brain）
3. 构造 `Brain`：
   - **Local agent**: 调用 `Brain::new_local(..., model_provider)` 注入 provider（保存为 `Some`）
   - **Cli/Remote agent**: 调用 `Brain::new_external(kind, ...)` 不传 provider（保存为 `None`）
4. 返回 Brain

找到旧代码：

```rust
let rig_tools: Vec<crate::models::cortex_types::ToolDescriptor> = {
    let auto = agent
        .tools()
        .iter()
        .filter(|t| t.po.control_mode == common::enums::tool::ControlMode::Auto)
        .map(|t| crate::models::cortex_types::ToolDescriptor {
            name: t.po.name.clone(),
            description: t.po.description.clone(),
            parameters: t.po.parameters_schema.clone().unwrap_or_else(|| {
                serde_json::json!({"type": "object", "properties": {}})
            }),
        })
        .collect::<Vec<_>>();
    // ... manual tools assignment
    auto
};

// ... 旧代码：create_cortex_trait(ctx, provider, rig_tools) 构造 cortex
// ... 旧代码：Brain::new_local(..., Some(cortex))
```

替换为（删除 `rig_tools` 分区、删除 `create_cortex_trait` 调用、加载 ModelProvider 后传给 `Brain::new_local`）：

```rust
// wake_agent_brain() 不再分区 Auto/Manual 工具，也不再构造 cortex
// 工具描述符（ToolDescriptor）在 awaken() 中按需构建（Step 2）
// ModelProvider 在本步骤加载并注入 Brain（brain 持有 provider，think() 时不传）

let kind: common::enums::AgentKind = agent.po().kind;

let brain = match kind {
    common::enums::AgentKind::Local => {
        // 加载 agent 关联的 ModelProvider（Local agent 需要）
        // 通过 agent 的 model provider 配置进入运行时 brain
        let model_provider: crate::models::model_provider::ModelProviderPo = {
            // 通过 agent 加载其关联的 model provider
            // 例如：self.model_provider_dal().get(ctx.clone(), agent.po().model_provider_id).await?
            // 或如果 Agent 已持有：agent.model_provider().clone()
            agent.model_provider().clone()  // 假设 agent 已持有 provider；若未持有则从 dal 加载
        };

        crate::models::brain::Brain::new_local(
            agent.po().id.clone(),
            agent.po().name.clone(),
            agent.runtime_config().clone(),
            memories,  // 已加载的 memories
            model_provider,  // 注入 brain，brain 自带 provider
        )
    }
    common::enums::AgentKind::Cli | common::enums::AgentKind::Remote => {
        // 外部 agent 不需要 model_provider（自行管理 LLM 调用）
        crate::models::brain::Brain::new_external(
            kind,
            agent.po().id.clone(),
            agent.po().name.clone(),
            agent.runtime_config().clone(),
            memories,
        )
    }
};
```

> **关键变化：**
> - 删除 `rig_tools` 变量（不再需要，工具描述符在 `awaken()` 中构建）
> - 删除 `create_cortex_trait()` 调用（方法已删除）
> - 删除 `Cortex::new(...)` 构造（struct 已删除）
> - **`wake_agent_brain()` 加载 ModelProvider 并传给 `Brain::new_local(..., model_provider)`**（brain 持有 provider）
> - **外部 agent 走 `Brain::new_external(kind, ...)`，不传 provider**（`model_provider: None`）
> - `awaken()` 不再单独加载 provider（已合并到本步骤）
> - 不再修改 `agent.tools`（旧代码会把 manual 工具从 agent.tools 中移除，新架构保留所有工具）

- [ ] **Step 7: Verify compilation (downstream errors expected but awakening.rs should compile)**

Run: `cargo check --message-format=short 2>&1 | grep "awakening"`
Expected: no errors in awakening.rs (other files may still have errors).

---

### Task 6: 实现 ExternalCortexDao + FastEmbedCortexDao，接入 CortexDaoRegistry

**Files:**
- Modify: `src/service/dao/cortex/external.rs` — `ExternalCortex` 改名为 `ExternalCortexDao`，实现新 `CortexDao` trait
- Modify: `src/service/dao/cortex/fastembed.rs` — `FastEmbedCortex` 改名为 `FastEmbedCortexDao`，实现新 `CortexDao` trait（从 `rig/fastembed.rs` 迁移出来，成为独立实现）
- Modify: `src/service/dao/cortex/native/mod.rs` — 在 `CortexDaoRegistry` 中接入 `fastembed` + `external` 字段
- Modify: `src/service/dao/cortex/rig.rs` (temporary — make it compile with stubbed `CortexDao` impl, will be deleted in Phase 2)

> **Note:** `doubao_vision.rs` 的多模态 embedding 功能已集成到 native cortex（http.rs 的 `call_embeddings_multimodal` + `OpenAiCompatibleCortexDao` 的 `provider_type` 分支），不再需要独立修改 `DoubaoVisionCortex`。该文件在 Task 9 中删除。

- [ ] **Step 1: 重写 `ExternalCortexDao` in `external.rs`**

`ExternalCortex` 改名为 `ExternalCortexDao`，实现新 `CortexDao` trait（不再是 `CortexTrait`）。该 DAO 处理 `AgentKind::Cli`/`Remote`，不依赖 `provider_type` —— BrainDal 在 `brain.kind` 为 `Cli`/`Remote` 时直接调用 `ExternalCortexDao.execute_cli()` / `execute_a2a()`（不经过 `CortexDaoRegistry::get()`，也不调用 `think()`）。

```rust
//! ExternalCortexDao — 处理外部 agent（CLI 子进程 / A2A 协议）
//!
//! 不依赖 provider_type，由 BrainDal 在 `brain.kind == Cli | Remote` 时直接调用。
//! 不支持 function calling（外部 agent 自行管理工具），`think()` 永远返回 `ThinkResult::Final`。
//! 不支持 embedding（外部 agent 不提供向量化能力），`embed()` 返回 Err。

use anyhow::Result;
use async_trait::async_trait;

use crate::models::cortex_types::{ThinkResult, ToolDescriptor};
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::request_context::RequestContext;

/// External CortexDao（单例，处理 CLI / A2A 外部 agent）
pub struct ExternalCortexDao {
    // 持有 CLI / A2A 执行所需的状态（如进程池、HTTP client 等）
    // 具体字段取决于现有 ExternalCortex 的实现
}

impl ExternalCortexDao {
    pub fn new() -> Result<Self> {
        Ok(Self { /* ... */ })
    }

    /// 执行 CLI 子进程（原有 execute_prompt 逻辑）
    async fn execute_cli(
        &self,
        ctx: RequestContext,
        brain: &crate::models::brain::Brain,
        prompt: &str,
    ) -> Result<String> {
        // 沿用原 ExternalCortex::execute_prompt 的实现
        todo!("迁移原 ExternalCortex::execute_prompt 逻辑")
    }

    /// 执行 A2A 协议调用
    async fn execute_a2a(
        &self,
        ctx: RequestContext,
        brain: &crate::models::brain::Brain,
        prompt: &str,
    ) -> Result<String> {
        // 沿用原 ExternalCortex 的 A2A 实现
        todo!("迁移原 ExternalCortex A2A 逻辑")
    }
}

#[async_trait]
impl super::CortexDao for ExternalCortexDao {
    async fn think(
        &self,
        ctx: RequestContext,
        _provider: &ModelProviderPo,  // 外部 agent 不使用 provider，忽略
        prompt: &str,
        _tools: &[ToolDescriptor],  // 外部 agent 不支持 function calling，忽略
    ) -> Result<ThinkResult> {
        // 外部 agent 不支持工具调用，直接执行并返回 Final
        // 注意：这里需要 brain 来判断是 CLI 还是 A2A；如果 BrainDal 已经在 think() 中
        // 按 brain.kind 分支调用 execute_cli/execute_a2a，那么 ExternalCortexDao.think()
        // 可能不会被调用（BrainDal 直接调 execute_cli/execute_a2a）。
        // 两种实现方式择一：
        //   A) BrainDal 在 Cli/Remote 分支直接调 cortex_registry().external().execute_*(...)，
        //      不经过 ExternalCortexDao.think()
        //   B) BrainDal 在 Cli/Remote 分支调 cortex_registry().external().think(...)，
        //      由 ExternalCortexDao.think() 内部判断 kind 并分发
        // 本计划采用方式 A（BrainDal 直接调 execute_cli/execute_a2a），因此
        // ExternalCortexDao.think() 仅为满足 trait 约束，返回未实现错误：
        Err(anyhow::anyhow!(
            "ExternalCortexDao.think() 不应被直接调用；BrainDal 应调用 execute_cli/execute_a2a"
        ))
    }

    async fn embed(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
        _texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        // 外部 agent 不提供 embedding 能力
        Err(anyhow::anyhow!("ExternalCortexDao 不支持 embedding 能力"))
    }
}
```

> **关键变化：**
> - `ExternalCortex` 改名为 `ExternalCortexDao`，实现 `CortexDao` trait（不再是 `CortexTrait`）
> - `think()` 接收 `&ModelProviderPo`（忽略，外部 agent 不用）+ `&[ToolDescriptor]`（忽略，不支持 function calling）
> - BrainDal 在 `brain.kind == Cli | Remote` 分支直接调 `cortex_registry().external().execute_cli(...)` / `execute_a2a(...)`，不经过 `think()`
> - `embed()` 返回 `Err`（外部 agent 不提供向量化）
> - 删除 `support_tools()` 方法（trait 已删除该方法）

- [ ] **Step 2: 删除独立的 DoubaoVisionCortex**

`doubao_vision.rs` 已被集成到 native cortex（http.rs 的 `call_embeddings_multimodal` + `OpenAiCompatibleCortexDao` 的 `provider_type` 分支），不再需要独立文件。在 Task 9 中删除。

无需修改 `DoubaoVisionCortex` 的签名 — 该类型在 Phase 1 期间不会被 native 路径调用（`CortexDaoRegistry::get()` 对 `DoubaoVision` 返回 `OpenAiCompatibleCortexDao`）。如果 Phase 1 期间 `rig/doubao_vision.rs` 仍被旧 `RigCortexDao` 引用，保持其原签名即可（rig 路径在 Task 9 整体删除）。

- [ ] **Step 3: 重写 `FastEmbedCortexDao` in `fastembed.rs`（从 `rig/fastembed.rs` 迁移）**

`FastEmbedCortex` 改名为 `FastEmbedCortexDao`，从 `rig/fastembed.rs` 迁移到 `src/service/dao/cortex/fastembed.rs`，实现新 `CortexDao` trait（不再是 `CortexTrait`）。该 DAO 处理 `ProviderType::FastEmbed`，走本地 fastembed crate。

```rust
//! FastEmbedCortexDao — 本地 fastembed crate 实现的 embedding DAO
//!
//! 处理 ProviderType::FastEmbed，仅支持 embedding（不支持 think）。
//! 由 CortexDaoRegistry::get(ProviderType::FastEmbed) 返回。

use anyhow::Result;
use async_trait::async_trait;

use crate::models::cortex_types::{ThinkResult, ToolDescriptor};
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::request_context::RequestContext;

/// FastEmbed CortexDao（单例，本地 fastembed crate）
pub struct FastEmbedCortexDao {
    // 持有 fastembed model loader / cache
    // 具体字段沿用原 FastEmbedCortex 的实现
}

impl FastEmbedCortexDao {
    pub fn new() -> Result<Self> {
        Ok(Self { /* ... */ })
    }

    /// 调用本地 fastembed crate 生成向量
    async fn embed_via_fastembed(
        &self,
        model_name: &str,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        // 沿用原 FastEmbedCortex::embeddings 的实现
        todo!("迁移原 FastEmbedCortex::embeddings 逻辑")
    }
}

#[async_trait]
impl super::CortexDao for FastEmbedCortexDao {
    async fn think(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
        _prompt: &str,
        _tools: &[ToolDescriptor],
    ) -> Result<ThinkResult> {
        // FastEmbed 仅支持 embedding，不支持 think
        Err(anyhow::anyhow!("FastEmbedCortexDao 不支持 think 能力"))
    }

    async fn embed(
        &self,
        _ctx: RequestContext,
        provider: &ModelProviderPo,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        // 从 provider 读取 model_name，调用本地 fastembed
        let model = &provider.model_name;
        self.embed_via_fastembed(model, texts).await
    }
}
```

> **关键变化：**
> - `FastEmbedCortex` 改名为 `FastEmbedCortexDao`，从 `rig/fastembed.rs` 迁移到 `src/service/dao/cortex/fastembed.rs`
> - 实现 `CortexDao` trait（不再是 `CortexTrait`）
> - `think()` 返回 `Err`（FastEmbed 仅支持 embedding）
> - `embed()` 从 `provider.model_name` 读取模型名，调用本地 fastembed crate
> - 删除 `support_tools()` 方法

- [ ] **Step 4: 在 `CortexDaoRegistry` 中接入 fastembed + external**

Update `src/service/dao/cortex/native/mod.rs`（Task 3 Step 3 中的占位代码），取消注释并接入 `FastEmbedCortexDao` + `ExternalCortexDao`：

```rust
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::cortex::external::ExternalCortexDao;
use crate::service::dao::cortex::fastembed::FastEmbedCortexDao;

pub struct CortexDaoRegistry {
    openai_compatible: Arc<openai::OpenAiCompatibleCortexDao>,
    fastembed: Arc<FastEmbedCortexDao>,
    external: Arc<ExternalCortexDao>,
}

/// 初始化 CortexDaoRegistry（在 `src/lib.rs` 启动时调用一次）
pub fn init() {
    let openai_compatible = Arc::new(
        openai::OpenAiCompatibleCortexDao::new()
            .expect("Failed to construct OpenAiCompatibleCortexDao"),
    );
    let fastembed = Arc::new(
        FastEmbedCortexDao::new()
            .expect("Failed to construct FastEmbedCortexDao"),
    );
    let external = Arc::new(
        ExternalCortexDao::new()
            .expect("Failed to construct ExternalCortexDao"),
    );

    let registry = Arc::new(CortexDaoRegistry {
        openai_compatible,
        fastembed,
        external,
    });
    let _ = CORTEX_REGISTRY.set(registry);
}

impl CortexDaoRegistry {
    pub fn get(&self, provider_type: ProviderType) -> Arc<dyn CortexDao> {
        match provider_type {
            ProviderType::OpenAI
            | ProviderType::DeepSeek
            | ProviderType::Qwen
            | ProviderType::Doubao
            | ProviderType::DoubaoVision
            | ProviderType::Ollama
            | ProviderType::Custom => self.openai_compatible.clone(),
            ProviderType::FastEmbed => self.fastembed.clone(),
        }
    }

    /// 暴露 external DAO 给 BrainDal 的 Cli/Remote 分支使用
    pub fn external(&self) -> Arc<ExternalCortexDao> {
        self.external.clone()
    }
}
```

> **关键变化：**
> - `CortexDaoRegistry` 新增 `fastembed` + `external` 字段
> - `get(ProviderType::FastEmbed)` 返回 `FastEmbedCortexDao`（不再 `unimplemented!`）
> - 新增 `external()` 方法，BrainDal 在 `brain.kind == Cli | Remote` 时调用 `cortex_registry().external()` 获取 `ExternalCortexDao`
> - BrainDal 在 Cli/Remote 分支调用 `cortex_registry().external().execute_cli(...)` / `execute_a2a(...)`

- [ ] **Step 5: Temporarily update RigCortexDao in `rig.rs` to compile (will be deleted in Phase 2)**

由于 `CortexDao` trait 已重写（删除 `create_cortex_trait` / `prompt` / `embed_text_raw` / `embed_entity` / `embed_text_for_search`），旧 `RigCortexDao` 无法再实现该 trait。Phase 1 期间让 `rig.rs` 临时编译通过的方案：

1. 删除 `impl CortexDao for RigCortexDao` 块（不再可实现，trait 签名完全变了）
2. 删除 `RigCortexDao` struct（或保留为空 struct，仅用于让 `mod rig;` 编译）
3. 删除 `pub use self::rig::{RigCortexDao, dao, init};`（Task 2 Step 2 已完成）
4. `rig/` 目录下的 `CortexTrait` 实现们（`openai.rs` / `openai_compatible.rs` / `ollama.rs` / `doubao_vision.rs` / `fastembed.rs`）保留原样（它们引用已删除的 `CortexTrait`，会有编译错误，但 `mod rig;` 可以临时用 `#[cfg(disabled)]` 跳过）

最简方案：在 `src/service/dao/cortex/mod.rs` 中将 `mod rig;` 临时改为：

```rust
#[cfg(disabled)]
mod rig;
```

这样 rig 模块完全不参与编译，Phase 2 Task 9 删除时再清理。`native` 模块成为唯一的 CortexDao 实现来源。

> **Note for `doubao_vision.rs` / `fastembed.rs`（rig 目录下）：** 这些文件在 Phase 1 期间被 `#[cfg(disabled)]` 跳过，不参与编译。其功能已分别迁移到 `OpenAiCompatibleCortexDao`（多模态 embedding）和 `FastEmbedCortexDao`（独立文件）。Phase 2 Task 9 整体删除 `rig/` 目录。

- [ ] **Step 6: Verify full compilation**

Run: `cargo check --message-format=short 2>&1`
Expected: zero errors. All CortexDao implementations (`OpenAiCompatibleCortexDao` / `FastEmbedCortexDao` / `ExternalCortexDao`) satisfy the new `CortexDao` trait. `rig/` 目录被 `#[cfg(disabled)]` 跳过，不参与编译。

---

### Task 6b: DAL 层向量化统一改造（删除 helper，DAL 方法直接调 cortex 包级函数）

**Files:**
- Modify: `src/service/dal/agent.rs` — 删除 `try_build_vector_params_for_search` 方法；改造 `upsert_vector_index`（原本就内联逻辑）+ `search` 方法
- Modify: `src/service/dal/tool.rs` — 删除自由函数 `try_build_vector_params_for_entity`；改造 `upsert_vector_index` / `rebuild_vectors` / `search` 等调用点
- Modify: `src/service/dal/memory.rs` — 删除自由函数 `try_build_vector_params_for_entity`；改造调用点
- Modify: `src/service/dal/message.rs` — 删除自由函数 `try_build_vector_params_for_entity` / `try_build_vector_params_for_search`；改造调用点
- Modify: `src/service/dal/task.rs` — 删除自由函数 `try_build_vector_params_for_entity` / `try_build_vector_params_for_search`；改造调用点
- Modify: `src/service/dal/project.rs` — 删除自由函数 `try_build_vector_params_for_entity` / `try_build_vector_params_for_search`；改造调用点

**背景：** 当前 DAL 层向量化调用存在三种模式（详见 Step 1 对照）：(1) 5 个文件（tool/memory/message/task/project）以**自由函数**形式重复定义 `try_build_vector_params_for_entity` / `try_build_vector_params_for_search`，签名形如 `async fn try_build_vector_params_for_entity(ctx, cortex_dao: &Arc<dyn CortexDao>, model_provider_dao: &Arc<dyn ModelProviderDao>, entity)`，内部逻辑完全相同（查 provider → None 降级 → Some 调 `cortex_dao.create_cortex_trait + embed_entity`）。(2) `agent.rs` 以**方法**形式定义 `try_build_vector_params_for_search(&self, ctx, text)`。(3) `agent.rs` 的 `upsert_vector_index` 方法直接**内联**了"查 provider + create_cortex_trait + embed_entity"逻辑（未走 helper）。共 17 处向量化调用点散落在各 DAL 方法中，均依赖 `self.cortex_dao.create_cortex_trait` 这一步。

**用户反馈核心诉求：**
> 1. cortex dao 提供的本质上是一个路由函数，可以直接走它来提供通过 provider 路由一个可用的 cortex dao 具体实现。
> 2. 目前所有的 dal 应该调用 cortex dao 提供的新的包级路由函数，不然每一处都需要重复同样的路由逻辑。

**改造目标：** 删除所有 `try_build_vector_params_*` helper（自由函数 + 方法），由各 DAL 方法内部直接调 cortex 包级路由函数（Task 2 Step 2b 定义）。DAL 方法内部仍需自查 provider（`model_provider_dao.get_default_embedding_provider()`），然后通过包级函数完成向量化——包级函数负责按 provider 路由到具体 cortex dao 实现。这样消除 6 个文件中重复的 helper 定义，也消除 17 处调用点对 helper 的间接依赖。

> **约束：** DAL 不能调用 DAL（项目约束），因此 DAL 层不通过 BrainDal 的 `embed_entity` 入口（Task 4 Step 3 定义），而是直接调用 cortex 包级函数（cortex 是 DAO 层，不是 DAL 层，不违反约束）。cortex 包级函数只路由+调用，不查 DB；provider 查询逻辑保留在 DAL 方法内部。

- [ ] **Step 1: 定位所有需要改造的 helper 与调用点**

Run: `grep -rn "fn try_build_vector_params_for_entity\|fn try_build_vector_params_for_search" src/service/dal/`

Expected: 6 个文件中各有一份定义（部分文件只定义 `_for_entity`，部分两个都定义）。同时定位所有调用点：

Run: `grep -rn "try_build_vector_params_for_entity\|try_build_vector_params_for_search" src/service/dal/`

Expected: 17 处调用点分布在各 DAL 方法中（如 `create` / `update` / `upsert_vector_index` / `search` / `rebuild_vectors`）。这些调用点需要改为"DAL 方法内部直接调 cortex 包级函数"。

> **三种旧模式对照（实际代码）：**
>
> 模式 A — **自由函数**（tool/memory/message/task/project）：
> ```rust
> // 定义（文件末尾）：
> async fn try_build_vector_params_for_entity(
>     ctx: RequestContext,
>     cortex_dao: &Arc<dyn CortexDao>,
>     model_provider_dao: &Arc<dyn ModelProviderDao>,
>     entity: &dyn Vectorizable,
> ) -> Result<Option<VectorIndexParams>> {
>     let Some(provider) = model_provider_dao.get_default_embedding_provider(ctx.clone()).await? else { return Ok(None); };
>     let cortex = cortex_dao.create_cortex_trait(ctx.clone(), &provider, vec![])?;
>     let params = cortex_dao.embed_entity(ctx, cortex.as_ref(), entity).await?;
>     Ok(Some(params))
> }
>
> // 调用点（DAL 方法内部）：
> match try_build_vector_params_for_entity(
>     ctx.clone(), &self.cortex_dao, &self.model_provider_dao, &task.po,
> ).await {
>     Ok(Some(vec_params)) => { /* upsert_vector */ }
>     Ok(None) => { /* 跳过 */ }
>     Err(e) => { /* warn 降级 */ }
> }
> ```
>
> 模式 B — **方法**（agent.rs）：
> ```rust
> // 定义（impl 块内）：
> async fn try_build_vector_params_for_search(
>     &self, ctx: RequestContext, text: &str,
> ) -> Result<Option<VectorIndexParams>> {
>     let Some(provider) = self.model_provider_dao.get_default_embedding_provider(ctx.clone()).await? else { return Ok(None); };
>     let cortex = self.cortex_dao.create_cortex_trait(ctx.clone(), &provider, vec![])?;
>     let params = self.cortex_dao.embed_text_for_search(ctx, cortex.as_ref(), text).await?;
>     Ok(Some(params))
> }
>
> // 调用点（search 方法内部）：
> match self.try_build_vector_params_for_search(ctx.clone(), keyword).await { ... }
> ```
>
> 模式 C — **内联逻辑**（agent.rs 的 `upsert_vector_index`，未走 helper）：
> ```rust
> async fn upsert_vector_index(&self, ctx: RequestContext, po: &AgentPo) {
>     let provider = match self.model_provider_dao.get_default_embedding_provider(ctx.clone()).await {
>         Ok(Some(p)) => p,
>         Ok(None) => { log_debug!(...); return; }
>         Err(e) => { log_warn!(...); return; }
>     };
>     let cortex = match self.cortex_dao.create_cortex_trait(ctx.clone(), &provider, vec![]) { ... };
>     match self.cortex_dao.embed_entity(ctx.clone(), cortex.as_ref(), po).await { ... }
> }
> ```

- [ ] **Step 2: 删除所有 `try_build_vector_params_*` helper 定义**

在 6 个文件中删除 `try_build_vector_params_for_entity` / `try_build_vector_params_for_search` 的定义（自由函数和方法形式都删）。删除后这些文件会因调用点仍引用 helper 而产生编译错误，下一步统一改造调用点。

- [ ] **Step 3: 改造 DAL 方法内部调用点 — 直接调 cortex 包级函数**

对每个原调用点（17 处），将"调 helper"改为"DAL 方法内部自查 provider + 调 cortex 包级函数"。

**模式 A 改造（自由函数调用点 → 内联自查 provider + 调包级函数）：**

旧代码（tool/task/project/memory/message 等）：
```rust
// 2. 向量索引自动维护（失败仅 warn 降级，不影响主流程）
match try_build_vector_params_for_entity(
    ctx.clone(),
    &self.cortex_dao,
    &self.model_provider_dao,
    &task.po,
)
.await
{
    Ok(Some(vec_params)) => {
        if let Err(e) = self.task_vector_dao.upsert_vector(ctx.clone(), &task.po.id, &vec_params).await {
            log_warn!(ctx, "vector_index", task_id = %task.po.id, error = ?e, "任务向量索引写入失败，已降级");
        }
    }
    Ok(None) => {
        log_debug!(ctx, "vector_index", task_id = %task.po.id, "无可用 Embedding Provider，跳过向量索引");
    }
    Err(e) => {
        log_warn!(ctx, "vector_index", task_id = %task.po.id, error = ?e, "任务向量化失败，已降级");
    }
}
```

新代码（删除 helper 调用，DAL 方法内部自查 provider + 调 cortex 包级函数）：
```rust
// 2. 向量索引自动维护（失败仅 warn 降级，不影响主流程）
// DAL 方法内部自查 provider，再调 cortex 包级路由函数完成向量化
match self.model_provider_dao.get_default_embedding_provider(ctx.clone()).await {
    Ok(Some(provider)) => {
        match crate::service::dao::cortex::embed_entity(ctx.clone(), &provider, &task.po).await {
            Ok(vec_params) => {
                if let Err(e) = self.task_vector_dao.upsert_vector(ctx.clone(), &task.po.id, &vec_params).await {
                    log_warn!(ctx, "vector_index", task_id = %task.po.id, error = ?e, "任务向量索引写入失败，已降级");
                }
            }
            Err(e) => {
                log_warn!(ctx, "vector_index", task_id = %task.po.id, error = ?e, "任务向量化失败，已降级");
            }
        }
    }
    Ok(None) => {
        log_debug!(ctx, "vector_index", task_id = %task.po.id, "无可用 Embedding Provider，跳过向量索引");
    }
    Err(e) => {
        log_warn!(ctx, "vector_index", task_id = %task.po.id, error = ?e, "查询 Embedding Provider 失败，跳过向量化");
    }
}
```

搜索场景同理（`try_build_vector_params_for_search` 调用点改为自查 provider + 调 `crate::service::dao::cortex::embed_text_for_search`）。

**模式 B 改造（方法形式调用点 → 内联自查 provider + 调包级函数）：**

旧代码（agent.rs `search` 方法）：
```rust
match self.try_build_vector_params_for_search(ctx.clone(), keyword).await {
    Ok(Some(vec_params)) => { /* 向量搜索 */ }
    Ok(None) => { log_debug!(&ctx, "vector_search", "无可用 Embedding Provider，跳过向量搜索"); }
    Err(e) => { log_warn!(&ctx, "vector_search", "Agent 向量搜索参数构建失败: {}", e); }
}
```

新代码：
```rust
match self.model_provider_dao.get_default_embedding_provider(ctx.clone()).await {
    Ok(Some(provider)) => {
        match crate::service::dao::cortex::embed_text_for_search(ctx.clone(), &provider, keyword).await {
            Ok(vec_params) => { /* 向量搜索（原 Ok(Some(vec_params)) 分支逻辑） */ }
            Err(e) => { log_warn!(&ctx, "vector_search", "Agent 向量搜索参数构建失败: {}", e); }
        }
    }
    Ok(None) => { log_debug!(&ctx, "vector_search", "无可用 Embedding Provider，跳过向量搜索"); }
    Err(e) => { log_warn!(&ctx, "vector_search", "Agent 查询 Embedding Provider 失败: {}", e); }
}
```

**模式 C 改造（内联逻辑的 `upsert_vector_index` → 改调包级函数）：**

旧代码（agent.rs `upsert_vector_index`）：
```rust
async fn upsert_vector_index(&self, ctx: RequestContext, po: &AgentPo) {
    // 1. 取默认 Embedding ModelProvider；无则跳过
    let provider = match self.model_provider_dao.get_default_embedding_provider(ctx.clone()).await {
        Ok(Some(p)) => p,
        Ok(None) => { log_debug!(...); return; }
        Err(e) => { log_warn!(...); return; }
    };
    // 2. 创建 Cortex（trait 对象）  ← 这一步在新架构中由 cortex 包级函数内部路由完成
    let cortex = match self.cortex_dao.create_cortex_trait(ctx.clone(), &provider, vec![]) { ... };
    // 3. 调 embed_entity 生成 VectorIndexParams
    match self.cortex_dao.embed_entity(ctx.clone(), cortex.as_ref(), po).await { ... }
}
```

新代码（删除"创建 Cortex"步骤，直接调 cortex 包级路由函数）：
```rust
async fn upsert_vector_index(&self, ctx: RequestContext, po: &AgentPo) {
    // 1. 取默认 Embedding ModelProvider；无则跳过（合法场景）
    let provider = match self.model_provider_dao.get_default_embedding_provider(ctx.clone()).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            log_debug!(&ctx, "vector_index", agent_id = %po.id, "无可用 Embedding Provider，跳过向量索引");
            return;
        }
        Err(e) => {
            log_warn!(&ctx, "vector_index", agent_id = %po.id, error = ?e, "Agent 查询 Embedding Provider 失败，跳过向量化");
            return;
        }
    };

    // 2. 调 cortex 包级路由函数完成向量化（内部按 provider_type 路由到具体 cortex dao 实现）
    //    不再需要 create_cortex_trait + cortex_dao.embed_entity 两步
    match crate::service::dao::cortex::embed_entity(ctx.clone(), &provider, po).await {
        Ok(vec_params) => {
            if let Err(e) = self.agent_vector_dao.upsert_vector(ctx.clone(), &po.id, &vec_params).await {
                log_warn!(&ctx, "vector_index", agent_id = %po.id, error = ?e, "Agent 向量索引写入失败，已降级（可能 vss0 扩展未安装）");
            }
        }
        Err(e) => {
            log_warn!(&ctx, "vector_index", agent_id = %po.id, error = ?e, "Agent 向量化失败，已降级");
        }
    }
}
```

> **关键变化：**
> - **删除所有 `try_build_vector_params_*` helper**（6 个文件，自由函数 + 方法形式全部删除）
> - 各 DAL 方法（`upsert_vector_index` / `search` / `create` / `update` / `rebuild_vectors` 等）内部直接调 cortex 包级路由函数 `crate::service::dao::cortex::embed_entity()` / `embed_text_for_search()`
> - DAL 方法内部仍自查 provider（`model_provider_dao.get_default_embedding_provider()`），cortex 包级函数不查 DB（只路由+调用）
> - 不再调用 `self.cortex_dao.create_cortex_trait()`（方法已删除）和 `self.cortex_dao.embed_entity()`（trait 默认实现已改为接收 `&ModelProviderPo`）
> - cortex 包级函数内部按 `provider.provider_type` 路由到具体 cortex dao 实现（OpenAiCompatible / FastEmbed / External），DAL 无需感知路由逻辑
> - "查 provider → None 降级 → Some 调包级函数"的三分支匹配模式在各 DAL 方法中重复出现（这是合理的，因为降级日志需要携带各自的实体 ID 上下文，无法再抽 helper）

- [ ] **Step 4: 移除不再需要的 `cortex_dao` 字段**

改造完成后，6 个 DAL 文件中的 `cortex_dao` 字段不再被任何方法使用（所有向量化调用都改走 cortex 包级函数，不再经 `self.cortex_dao`）。移除这些文件中 `cortex_dao` 字段及其构造注入。

检查是否有遗漏用途：

Run: `grep -rn "self.cortex_dao" src/service/dal/agent.rs src/service/dal/tool.rs src/service/dal/memory.rs src/service/dal/message.rs src/service/dal/task.rs src/service/dal/project.rs`

Expected: zero matches（所有引用已在 Step 3 改造中消除）。

> **Note:** `brain.rs` 的 `think()` 已在 Task 4 中改为通过 `cortex_registry().get(provider.provider_type)` 获取 DAO，不再使用 `self.cortex_dao`。因此 `brain.rs` 的 `cortex_dao` 字段也可移除（如果存在）。Task 4 Step 1 已处理。

> **Note:** 各 DAL 文件的 `model_provider_dao` 字段保留（DAL 方法内部仍需自查 provider）。仅 `cortex_dao` 字段被移除。

- [ ] **Step 5: Verify compilation**

Run: `cargo check --message-format=short 2>&1`
Expected: zero errors. 所有 `try_build_vector_params_*` helper 已删除，DAL 方法内部直接调 cortex 包级函数，`create_cortex_trait` / `self.cortex_dao` 引用已全部消除。

---

### Task 7: 更新 cortex 测试（disable rig_test，新增 native 测试）

**Files:**
- Modify: `src/service/dao/cortex/rig_test.rs` — `#[cfg(disabled)]` 跳过（rig 模块已被 disable）
- Create: `src/service/dao/cortex/native_test.rs` — 新增 CortexDao 实现 tests

- [ ] **Step 1: Disable `rig_test.rs`**

由于 `rig/` 模块在 Task 6 Step 5 中被 `#[cfg(disabled)]` 跳过，`rig_test.rs`（依赖 `DynamicTool` / `RigCortexDao`）也无法编译。在 `src/service/dao/cortex/mod.rs` 中将 `rig_test` 模块声明同样 disable：

```rust
#[cfg(disabled)]
#[cfg(test)]
mod rig_test;
```

> **Note:** `rig_test.rs` 在 Phase 2 Task 9 中整体删除。Phase 1 期间只需让它不参与编译。

- [ ] **Step 2: Create `src/service/dao/cortex/native_test.rs` — 测试新的 CortexDao 实现**

新增测试文件，验证 `CortexDaoRegistry` 分发逻辑 + `OpenAiCompatibleCortexDao` 的 `resolve_base_url` 逻辑（不依赖真实 API 调用）：

```rust
#[cfg(test)]
mod tests {
    use crate::models::model_provider::ModelProviderPo;
    use crate::service::dao::cortex::native::openai::resolve_base_url;  // 注意：resolve_base_url 需为 pub(crate) 或 pub
    use common::enums::ProviderType;

    /// 测试 resolve_base_url：provider.base_url 优先于默认值
    #[test]
    fn test_resolve_base_url_provider_overrides_default() {
        let provider = ModelProviderPo {
            provider_type: ProviderType::OpenAI,
            base_url: Some("https://custom.openai.proxy.com/v1".to_string()),
            // ... 其他字段填充测试值
            ..Default::default()
        };
        assert_eq!(resolve_base_url(&provider), "https://custom.openai.proxy.com/v1");
    }

    /// 测试 resolve_base_url：provider 未指定 base_url 时用默认值
    #[test]
    fn test_resolve_base_url_defaults() {
        let openai = ModelProviderPo {
            provider_type: ProviderType::OpenAI,
            base_url: None,
            ..Default::default()
        };
        assert_eq!(resolve_base_url(&openai), "https://api.openai.com/v1");

        let deepseek = ModelProviderPo {
            provider_type: ProviderType::DeepSeek,
            base_url: None,
            ..Default::default()
        };
        assert_eq!(resolve_base_url(&deepseek), "https://api.deepseek.com");

        let qwen = ModelProviderPo {
            provider_type: ProviderType::Qwen,
            base_url: None,
            ..Default::default()
        };
        assert_eq!(resolve_base_url(&qwen), "https://dashscope.aliyuncs.com/compatible-mode/v1");

        let doubao = ModelProviderPo {
            provider_type: ProviderType::Doubao,
            base_url: None,
            ..Default::default()
        };
        assert_eq!(resolve_base_url(&doubao), "https://ark.cn-beijing.volces.com/api/v3");

        let doubao_vision = ModelProviderPo {
            provider_type: ProviderType::DoubaoVision,
            base_url: None,
            ..Default::default()
        };
        assert_eq!(resolve_base_url(&doubao_vision), "https://ark.cn-beijing.volces.com/api/v3");

        let ollama = ModelProviderPo {
            provider_type: ProviderType::Ollama,
            base_url: None,
            ..Default::default()
        };
        assert_eq!(resolve_base_url(&ollama), "http://localhost:11434/v1");
    }

    /// 测试 CortexDaoRegistry::get() 按 provider_type 分发
    #[test]
    fn test_cortex_registry_dispatch() {
        // 注意：需要先调用 init() 构造单例
        crate::service::dao::cortex::native::init();

        let registry = crate::service::dao::cortex::native::cortex_registry();

        // OpenAI 兼容 providers 都返回 OpenAiCompatibleCortexDao
        let openai_dao = registry.get(ProviderType::OpenAI);
        let deepseek_dao = registry.get(ProviderType::DeepSeek);
        let doubao_vision_dao = registry.get(ProviderType::DoubaoVision);
        let ollama_dao = registry.get(ProviderType::Ollama);

        // FastEmbed 返回 FastEmbedCortexDao
        let fastembed_dao = registry.get(ProviderType::FastEmbed);

        // 验证 external() 方法可访问
        let _external_dao = registry.external();

        // 由于返回的是 Arc<dyn CortexDao>，无法直接比较类型，但可以验证调用不 panic
        // 真正的类型验证通过 think()/embed() 的行为测试完成（需要 mock 或真实 API）
    }
}
```

> **关键变化：**
> - `rig_test.rs` 被 `#[cfg(disabled)]` 跳过（不再修改 `DynamicTool` → `ToolDescriptor`，因为整个 rig 模块不参与编译）
> - 新增 `native_test.rs` 测试 `resolve_base_url` + `CortexDaoRegistry::get()` 分发逻辑
> - 测试不依赖真实 API 调用（`resolve_base_url` 是纯函数，`get()` 只验证分发不 panic）

> **Note on `resolve_base_url` visibility:** 为便于测试，`resolve_base_url` 函数需要从 `private` 改为 `pub(crate)` 或 `pub`。在 `openai.rs` 中将 `fn resolve_base_url(...)` 改为 `pub(crate) fn resolve_base_url(...)`。

- [ ] **Step 3: Verify tests compile**

Run: `cargo test -p ai_orz --lib service::dao::cortex::native_test --no-run 2>&1`
Expected: compilation succeeds.

- [ ] **Step 4: Run cortex tests**

Run: `cargo test -p ai_orz --lib service::dao::cortex::native_test 2>&1`
Expected: all tests pass (resolve_base_url + registry dispatch tests).

---

### Task 8: 更新初始化点（CortexDaoRegistry）+ full verification

**Files:**
- Modify: `src/lib.rs` or `src/main.rs` (wherever `cortex::rig::init()` is called)
- Verify: full build + clippy + fmt

- [ ] **Step 1: Change cortex DAO initialization**

Search for `cortex::rig::init` in the codebase. Replace with:

```rust
crate::service::dao::cortex::native::init();
```

Also update any `cortex::rig::dao()` calls to `crate::service::dao::cortex::native::cortex_registry()`.

> **关键变化：** 旧 `dao()` 返回 `Arc<NativeCortexDao>`，新 `cortex_registry()` 返回 `Arc<CortexDaoRegistry>`。调用方需要改为 `cortex_registry().get(provider.provider_type)` 获取具体 DAO，或 `cortex_registry().external()` 获取 ExternalCortexDao。

- [ ] **Step 2: Search for remaining old API references**

Run: `grep -rn "ToolDyn\|DynamicTool\|rig::tool\|rig::agent\|rig::completion\|rig::embeddings\|rig::providers\|rig::prelude\|create_cortex_trait\|CortexTrait\|cortex_dao\.prompt\|cortex\.cortex()" src/`
Expected: only in `src/service/dao/cortex/rig/`（被 `#[cfg(disabled)]` 跳过）和 `src/models/tool.rs`（RigToolAdapter）— these will be cleaned in Phase 2.

- [ ] **Step 3: Full compilation check**

Run: `cargo check --all-targets --message-format=short 2>&1`
Expected: zero errors.

- [ ] **Step 4: Run clippy**

Run: `cargo clippy --all-targets -- -D warnings 2>&1`
Expected: zero warnings.

- [ ] **Step 5: Run format check**

Run: `cargo fmt --all -- --check 2>&1`
Expected: no formatting issues. If issues, run `cargo fmt --all`.

- [ ] **Step 6: Run cortex tests**

Run: `cargo test -p ai_orz --lib service::dao::cortex 2>&1`
Expected: all native_test tests pass.

- [ ] **Step 7: Commit Phase 1**

```bash
git add -A
git commit -m "$(cat <<'EOF'
feat(cortex): 扁平化架构 - BrainDal 直接调用 CortexDao，删除 CortexTrait 抽象

Phase 1 of removing rig dependency:
- Add ThinkResult/ToolDescriptor/ToolCallRequest types
  + impl From<&Tool> for ToolDescriptor（从业务 Tool 直接派生）
- 删除 CortexTrait trait + Cortex 实体，Brain 不再持有 cortex 字段
  Brain 直接持有 model_provider: Option<ModelProviderPo>（Local 为 Some，外部 agent 为 None）
- 重写 CortexDao trait：think(ctx, provider, prompt, tools) + embed(ctx, provider, texts)
  （+ trait 默认实现的 embed_text / embed_entity / embed_text_for_search）
- 新增 cortex 包级函数 embed_entity / embed_text_for_search（不依赖任何 DAL，纯协议层）
- 实现 OpenAiCompatibleCortexDao（单例，从 &ModelProviderPo 读取配置）
- 实现 FastEmbedCortexDao + ExternalCortexDao，接入 CortexDaoRegistry
- CortexDaoRegistry 按 provider_type 分发
- BrainDal.think() 不接收 provider 参数，从 brain.model_provider.as_ref() 读取
- BrainDal.think() 按 brain.kind 分支：Local 走 registry.get()，Cli/Remote 走 external()
- BrainDal 新增 embed_entity / embed_text_for_search 入口（domain 层，内部查默认 provider + 调 cortex 包级函数）
- wake_agent_brain() 加载 ModelProvider 并注入 Brain（Brain::new_local(..., model_provider)）
- awaken() 实现显式工具调用循环，brain.think() 不传 provider（brain 自带）
- awaken() 工具列表构建改用 map(ToolDescriptor::from)（从业务 Tool 直接派生）
- DAL 层删除 try_build_vector_params_* helper，各 DAL 方法内部自查 provider + 直接调 cortex 包级路由函数（消除 6 个文件重复定义）
- All providers now use /chat/completions (fixes Responses API compatibility)
- Token usage extracted from HTTP response body (no rig hook needed)
- rig-backed cortex 模块被 #[cfg(disabled)] 跳过（Phase 2 删除）
EOF
)"
```

---

## Phase 2: Remove rig dependency (cleanup)

### Task 9: Remove rig-backed cortex + RigToolAdapter + RuntimeMonitoringHook

**Files:**
- Delete: `src/service/dao/cortex/rig/` (entire directory, including `doubao_vision.rs` — 其多模态 embedding 功能已集成到 `native/http.rs` 的 `call_embeddings_multimodal`)
- Delete: `src/service/dao/cortex/rig.rs`
- Delete: `src/service/dao/cortex/rig_test.rs`
- Delete: `src/pkg/monitoring/rig_hook.rs`
- Modify: `src/models/tool.rs` (remove RigToolAdapter)
- Modify: `src/service/dao/cortex/mod.rs` (remove `#[cfg(disabled)] mod rig;` 和 `#[cfg(disabled)] mod rig_test;`)

- [ ] **Step 1: Delete rig-backed cortex files**

```bash
rm -rf src/service/dao/cortex/rig/
rm src/service/dao/cortex/rig.rs
rm src/service/dao/cortex/rig_test.rs
rm src/pkg/monitoring/rig_hook.rs
```

> **Note:** `src/service/dao/cortex/rig/doubao_vision.rs` 和 `src/service/dao/cortex/rig/fastembed.rs` 都包含在 `rm -rf src/service/dao/cortex/rig/` 中。前者多模态 embedding 功能已在 Task 3 集成到 `native/http.rs` 的 `call_embeddings_multimodal` + `OpenAiCompatibleCortexDao` 的 `provider_type` 分支；后者已由独立的 `src/service/dao/cortex/fastembed.rs`（`FastEmbedCortexDao`）取代。

- [ ] **Step 2: Remove RigToolAdapter from `src/models/tool.rs`**

Remove the entire `RigToolAdapter` struct and its `impl` block (the `into_dynamic_tool()` method). Also remove the rig imports:

```rust
// Remove these imports:
use rig::tool::{DynamicTool, ToolContext, ToolErrorKind, ToolExecutionError, ToolOutput};
```

- [ ] **Step 3: Update `src/service/dao/cortex/mod.rs`**

Remove the `#[cfg(disabled)]` 跳过的模块声明（Task 6 Step 5 / Task 7 Step 1 中添加的临时 disable）:

```rust
// Remove these (已 disabled 的临时声明):
#[cfg(disabled)]
mod rig;
#[cfg(disabled)]
#[cfg(test)]
mod rig_test;
```

Ensure the final `mod.rs` 只导出 native + fastembed + external:

```rust
pub mod native;
pub mod fastembed;
pub mod external;

// CortexDaoRegistry 单例访问通过 native 模块：
// crate::service::dao::cortex::native::cortex_registry()
// crate::service::dao::cortex::native::init()
// 不需要 pub use 重导出（调用方直接用完整路径）
```

> **关键变化（相比旧计划）：** 旧计划写的是 `pub use native::{NativeCortexDao, dao, init};`，但 `NativeCortexDao` 已不存在（被 `CortexDaoRegistry` 取代），`dao()` 也被 `cortex_registry()` 取代。因此本步骤不再 `pub use` 任何符号，调用方直接用 `crate::service::dao::cortex::native::cortex_registry()` / `init()` 完整路径。

> **Note:** `pub mod doubao_vision;` 不再需要 — `DoubaoVisionCortex` 已删除，DoubaoVision 的 embedding 能力由 `native/openai.rs` 的 `OpenAiCompatibleCortexDao` 处理（通过 `provider_type` 标记走 `/embeddings/multimodal`）。

- [ ] **Step 4: Remove `mod rig;` from `src/pkg/monitoring/mod.rs`**

Find and remove the `rig_hook` module declaration (if exists).

- [ ] **Step 5: Verify compilation (expected errors in callers of removed code)**

Run: `cargo check --message-format=short 2>&1`
Expected: errors in files that referenced `RigToolAdapter`, `wrap_for_rig`, `DynamicTool`, `ToolErrorKind`, `ToolExecutionError`. Fix these in subsequent steps.

---

### Task 10: Remove wrap_for_rig from ToolCallDao

**Files:**
- Modify: `src/service/dao/tool_call/mod.rs`
- Modify: `src/service/dao/tool_call/impl.rs`
- Modify: `src/service/dao/tool_call/mcp.rs`
- Modify: `src/service/dal/tool.rs`
- Modify: `src/service/dal/tool_test.rs`

- [ ] **Step 1: Remove `wrap_for_rig` from ToolCallDao trait**

In `src/service/dao/tool_call/mod.rs`, remove the `wrap_for_rig` method from the `ToolCallDao` trait:

```rust
// Remove this method from the trait:
fn wrap_for_rig(&self, tools: &[Tool], ctx: RequestContext) -> Vec<rig::tool::DynamicTool>;
```

- [ ] **Step 2: Remove `wrap_for_rig` implementation from ToolCallDaoImpl**

In `src/service/dao/tool_call/impl.rs`, remove the `wrap_for_rig` method implementation and the `use rig::tool::DynamicTool;` import.

- [ ] **Step 3: Remove `wrap_for_rig` from McpToolCallDaoImpl**

In `src/service/dao/tool_call/mcp.rs`, remove the `wrap_for_rig` method and the `use rig::tool::DynamicTool;` import.

- [ ] **Step 4: Remove `wrap_for_rig` proxy from ToolDal**

In `src/service/dal/tool.rs`, remove the `wrap_for_rig` method from the `ToolDal` trait and its implementation.

- [ ] **Step 5: Update tool_test.rs**

In `src/service/dal/tool_test.rs`, remove any tests that call `wrap_for_rig`.

- [ ] **Step 6: Verify compilation**

Run: `cargo check --message-format=short 2>&1`
Expected: errors only in files that used `ToolErrorKind`/`ToolExecutionError` (fixed in Task 11).

---

### Task 11: Replace ToolErrorKind/ToolExecutionError with common::error::Error

**Files:**
- Modify: `common/src/error/types.rs`
- Modify: `src/pkg/tool_registry/handler_adapter/mod.rs`
- Modify: `src/pkg/tool_registry/mcp.rs`
- Modify: `src/pkg/tool_tracing/tests.rs`
- Modify: `src/service/domain/runtime/tool_execution_test.rs`
- Modify: `src/service/domain/message/delivery_test.rs`

- [ ] **Step 1: Remove `From<rig::tool::ToolExecutionError>` from common error types**

In `common/src/error/types.rs`, find and remove:

```rust
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

- [ ] **Step 2: Update `handler_adapter/mod.rs`**

In `src/pkg/tool_registry/handler_adapter/mod.rs`:

Remove import:
```rust
use rig::tool::{ToolErrorKind, ToolExecutionError};
```

In the `call` method, replace error construction:

```rust
// Old:
return Err(ToolExecutionError::new(ToolErrorKind::InvalidArgs, e.to_string()).into());
// New:
return Err(common::error::Error::tool_execution_failed(e.to_string()));
```

```rust
// Old:
Err(app_error) => Err(ToolExecutionError::new(ToolErrorKind::Other, app_error.to_string()).into()),
// New:
Err(app_error) => Err(app_error),
```

Remove the `app_error_to_tool_error` helper function (no longer needed).

- [ ] **Step 3: Update `mcp.rs`**

In `src/pkg/tool_registry/mcp.rs`:

Remove import:
```rust
use rig::tool::{ToolErrorKind, ToolExecutionError};
```

Replace `ToolExecutionError::new(ToolErrorKind::Other, ...)` with `common::error::Error::tool_execution_failed(...)` or `common::error::err!(ToolExecutionFailed, ...)`.

- [ ] **Step 4: Update test files**

In `src/pkg/tool_tracing/tests.rs`, `src/service/domain/runtime/tool_execution_test.rs`, `src/service/domain/message/delivery_test.rs`:

Remove imports of `ToolErrorKind`, `ToolExecutionError`. Replace error construction with `common::error::Error::tool_execution_failed(...)` or `anyhow::anyhow!(...)`.

- [ ] **Step 5: Add `tool_execution_failed` helper to common error (if not exists)**

Check if `common::error::Error` has a `tool_execution_failed` constructor. If not, add it to `common/src/error/types.rs`:

```rust
impl Error {
    pub fn tool_execution_failed(msg: impl Into<String>) -> Self {
        Error::new(crate::error::ErrorCode::ToolExecutionFailed, msg.into())
    }
}
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check --all-targets --message-format=short 2>&1`
Expected: zero errors.

---

### Task 12: Remove rig dependency from Cargo.toml + cleanup

**Files:**
- Modify: `Cargo.toml`
- Modify: `common/Cargo.toml`
- Delete: `patches/rig-fastembed/` (entire directory)

- [ ] **Step 1: Remove rig from workspace Cargo.toml**

In `Cargo.toml`, remove line 29:
```toml
rig = { version = "0.41", default-features = false, features = ["agent", "derive", "rustls", "reqwest"] }
```

Remove the entire `[patch.crates-io]` section:
```toml
[patch.crates-io]
rig-fastembed = { path = "patches/rig-fastembed" }
```

- [ ] **Step 2: Remove rig from common/Cargo.toml**

In `common/Cargo.toml`, remove line 15:
```toml
rig = { version = "0.41", optional = true, default-features = false }
```

Remove the `rig-integration` feature (line 29):
```toml
rig-integration = ["dep:rig"]
```

Also remove `rig-integration` from the `common` dependency features in workspace `Cargo.toml` line 43:
```toml
# Change:
common = { path = "./common", features = ["sqlx", "axum-integration", "bincode-integration", "rig-integration", "toml-integration", "reqwest-integration", "tokio-integration"] }
# To:
common = { path = "./common", features = ["sqlx", "axum-integration", "bincode-integration", "toml-integration", "reqwest-integration", "tokio-integration"] }
```

- [ ] **Step 3: Delete patches directory**

```bash
rm -rf patches/
```

- [ ] **Step 4: Update Cargo.lock**

Run: `cargo update 2>&1`
Expected: rig and related crates removed from Cargo.lock.

- [ ] **Step 5: Verify no rig references remain**

Run: `grep -rn "rig" Cargo.toml common/Cargo.toml Cargo.lock | grep -v "rigid\|origin\|trigger\|navigate"`
Expected: no matches (or only false positives).

- [ ] **Step 6: Full build + clippy + fmt + test**

Run:
```bash
cargo check --all-targets --message-format=short 2>&1
cargo clippy --all-targets -- -D warnings 2>&1
cargo fmt --all -- --check 2>&1
cargo test -p ai_orz --lib service::dao::cortex 2>&1
```
Expected: all pass with zero errors/warnings.

- [ ] **Step 7: Commit Phase 2**

```bash
git add -A
git commit -m "$(cat <<'EOF'
refactor: remove rig dependency entirely

Phase 2 of removing rig dependency:
- Delete rig-backed cortex implementations (rig/, rig.rs, rig_test.rs)
- Delete RuntimeMonitoringHook (token usage now extracted from HTTP response)
- Delete RigToolAdapter (cortex DAO no longer needs DynamicTool adaptation)
- Remove wrap_for_rig from ToolCallDao (tools passed per think() call)
- Replace ToolErrorKind/ToolExecutionError with common::error::Error
- Remove rig dependency from Cargo.toml and common/Cargo.toml
- Delete patches/rig-fastembed stub crate
- All providers now use self-built CortexDao implementations
  (OpenAiCompatibleCortexDao / FastEmbedCortexDao / ExternalCortexDao)
  via CortexDaoRegistry + reqwest + Chat Completions API
- 架构扁平化：BrainDal 直接调用 CortexDao，无 CortexTrait/Cortex 抽象层
EOF
)"
```

---

## Self-Review Notes

**Spec coverage:**
- ✅ CortexDao 层自建（扁平化，无 CortexTrait/Cortex 实体）: Tasks 1-3 (types + OpenAiCompatibleCortexDao + HTTP helpers + CortexDaoRegistry)
- ✅ 删除 CortexTrait + Cortex 实体，Brain 不再持有 cortex: Task 2
- ✅ Brain 直接持有 `model_provider: Option<ModelProviderPo>`（Local 为 Some，外部 agent 为 None）: Task 2
- ✅ CortexDao trait 重写（think + embed，接收 &ModelProviderPo）: Task 2
- ✅ BrainDal.think() **不接收 provider 参数**，从 `brain.model_provider.as_ref()` 读取后按 provider_type 分发: Task 4
- ✅ Explicit tool-calling loop at awakening layer: Task 5
- ✅ Dynamic tool list (per think() call): Task 3 (ToolDescriptor in think signature)
- ✅ Token usage extraction without rig hook: Task 3 (http.rs extracts usage from response body)
- ✅ All providers use /chat/completions: Task 3 (unified OpenAiCompatibleCortexDao)
- ✅ DoubaoVision multimodal embedding 集成: Task 3 (http.rs `call_embeddings_multimodal` + `OpenAiCompatibleCortexDao::embed()` 的 `provider_type` 分支，替代独立 `DoubaoVisionCortex`)
- ✅ FastEmbedCortexDao 独立实现（从 rig/fastembed.rs 迁移）: Task 6
- ✅ ExternalCortexDao 处理 Cli/Remote: Task 6
- ✅ CortexDaoRegistry 按 provider_type 分发: Task 3 + Task 6
- ✅ brain.think() 按 brain.kind 分支（Local→registry.get, Cli/Remote→external）: Task 4
- ✅ wake_agent_brain() 加载 ModelProvider 并注入 Brain: Task 5 Step 6
- ✅ Phase 1 keeps rig as disabled (#[cfg(disabled)]): Tasks 1-8
- ✅ Phase 2 removes rig entirely: Tasks 9-12
- ✅ **ToolDescriptor 从 Tool 直接派生**（`From<&Tool>` 实现）: Task 1（业务层传递 Tool，cortex dao 层通过 From trait 派生）
- ✅ **工具列表构建简化**（`map(ToolDescriptor::from)`）: Task 5 Step 2
- ✅ **cortex 包级函数 `embed_entity` / `embed_text_for_search`**（不依赖任何 DAL，纯协议层）: Task 2 Step 2b
- ✅ **BrainDal embed 入口**（`embed_entity` / `embed_text_for_search`，内部查默认 provider + 调 cortex 包级函数）: Task 4 Step 3
- ✅ **DAL 层向量化统一改造**（删除 6 个文件重复的 `try_build_vector_params_*` helper，各 DAL 方法内部自查 provider + 直接调 cortex 包级路由函数；移除 `cortex_dao` 字段）: Task 6b

**Placeholder scan:** Task 6 中 `ExternalCortexDao::execute_cli` / `execute_a2a` 和 `FastEmbedCortexDao::embed_via_fastembed` 使用 `todo!("迁移原 ... 逻辑")` 占位 — 这些是迁移现有逻辑的占位符，具体实现取决于现有 `ExternalCortex` / `FastEmbedCortex` 的代码结构。其余代码块包含完整、具体的代码。

**Type consistency:**
- `ThinkResult` used consistently across all tasks (defined Task 1, used Tasks 2-6)
- `ToolDescriptor` used consistently (defined Task 1, used Tasks 2-5)
- `ToolCallRequest` used consistently (defined Task 1, used Tasks 2, 5)
- `CortexDao` trait used consistently (defined Task 2, implemented Tasks 3+6, called Task 4)
- `OpenAiCompatibleCortexDao` used consistently (defined Task 3 Step 2 with `http: Client` only，referenced in Task 3 Step 3 CortexDaoRegistry；`new()` 无参数，`think()`/`embed()` 从 `&ModelProviderPo` 读取配置)
- `CortexDaoRegistry` used consistently (defined Task 3 Step 3，extended Task 6 Step 4 with fastembed+external fields；`get(provider_type)` + `external()` 方法)
- `resolve_base_url(provider)` defined in Task 3 Step 2 openai.rs, called from `OpenAiCompatibleCortexDao::think()` + `embed()`；tested in Task 7
- `call_embeddings_multimodal` defined in Task 3 http.rs, called from `OpenAiCompatibleCortexDao::embed()` when `provider.provider_type == DoubaoVision`
- **`Brain.model_provider: Option<ModelProviderPo>` 字段一致**（Task 2 定义；Task 4 `BrainDal::think()` 通过 `brain.model_provider.as_ref().ok_or_else(...)?` 读取；Task 5 Step 6 `wake_agent_brain()` 通过 `Brain::new_local(..., model_provider)` 注入，`Brain::new_external(...)` 保存为 `None`）
- **`Brain::new_local(...)` 接收 `model_provider: ModelProviderPo` 参数**（Task 2 定义，Task 5 Step 6 wake_agent_brain 调用）
- **`Brain::new_external(kind, ...)` 不传 provider**（Task 2 定义，Task 5 Step 6 wake_agent_brain 对 Cli/Remote agent 调用）
- **`BrainDal::think(ctx, brain, prompt, tools)` 签名一致**（Task 4 定义，**不接收 provider**，Task 5 Step 4 调用同样不传 provider）
- Brain.kind == Local 时，brain.model_provider 必为 Some（由 `Brain::new_local` 装配时保证）；Brain.kind == Cli/Remote 时，brain.model_provider 必为 None（由 `Brain::new_external` 装配时保证）
- **`From<&Tool> for ToolDescriptor` 一致**（Task 1 定义；Task 5 Step 2 `awaken()` 通过 `map(ToolDescriptor::from)` 使用；从 `tool.po.name` / `tool.po.description` / `tool.po.parameters_schema` 派生，`parameters_schema` 为 None 时用默认空 JSON Schema）
- **cortex 包级函数 `embed_entity(ctx, provider, entity) -> Result<VectorIndexParams>` 签名一致**（Task 2 Step 2b 定义；Task 4 Step 3 BrainDal 入口调用；Task 6b DAL 方法内部直接调用）。与 `CortexDao::embed_entity` trait 默认实现签名一致（`ctx, provider, entity`），区别是包级函数内部自动从 registry 获取 DAO（路由函数语义）。
- **cortex 包级函数 `embed_text_for_search(ctx, provider, text) -> Result<VectorIndexParams>` 签名一致**（Task 2 Step 2b 定义；Task 4 Step 3 BrainDal 入口调用；Task 6b DAL 方法内部直接调用）
- **BrainDal `embed_entity(ctx, entity) -> Result<Option<VectorIndexParams>>` 签名一致**（Task 4 Step 3 定义）。与 cortex 包级函数的区别：不接收 `provider`（内部自动查默认 provider），返回 `Option`（`None` 表示降级跳过）。
- **BrainDal `embed_text_for_search(ctx, text) -> Result<Option<VectorIndexParams>>` 签名一致**（Task 4 Step 3 定义）
- **DAL 层删除 `try_build_vector_params_*` helper**（Task 6b：自由函数 + 方法形式全部删除），各 DAL 方法（`upsert_vector_index` / `search` / `create` / `update` / `rebuild_vectors` 等）内部自查 provider（`model_provider_dao.get_default_embedding_provider()`）后直接调 `cortex::embed_entity` / `cortex::embed_text_for_search` 包级路由函数；`cortex_dao` 字段从 6 个 DAL 文件中移除（不再调 `create_cortex_trait`）

**Risk areas:**
1. **ToolCallLoggingDecorator visibility**: Task 5 Step 4 references `crate::pkg::tool_tracing::ToolCallLoggingDecorator`. Verify this is public (or use `pub(crate)`). Check `src/pkg/tool_tracing/mod.rs` for the re-export.
2. **ModelCallEvent builder methods**: Task 3 uses `.with_agent_id()`, `.with_tokens_input()` etc. Verify these exist in the current `ModelCallEvent` API (they're used in the current `rig_hook.rs`, so they should exist).
3. **Ollama API key**: Ollama doesn't require an API key, but `bearer_auth("")` is harmless. If Ollama rejects the header, add conditional logic to skip the auth header for Ollama.
4. **ModelProviderPo loading in wake_agent_brain()**: Task 5 Step 6 假设 `agent.model_provider()` 可获取 provider；若 Agent 不持有 provider，需通过 `ModelProviderDal` 从数据库加载（如 `self.model_provider_dal().get(ctx.clone(), agent.po().model_provider_id).await?`）。具体路径取决于现有代码结构。本步骤是 Brain 持有 provider 的关键装配点。
5. **ExternalCortexDao 设计选择**: Task 6 Step 1 采用方式 A（BrainDal 直接调 `execute_cli`/`execute_a2a`，不经过 `think()`）。`ExternalCortexDao.think()` 仅为满足 trait 约束返回 Err。如果未来希望统一入口，可改为方式 B（`think()` 内部按 kind 分发）。
6. **resolve_base_url visibility**: Task 7 测试需要访问 `resolve_base_url`，需将其从 `fn` 改为 `pub(crate) fn`。
7. **#[cfg(disabled)] for rig module**: Task 6 Step 5 使用 `#[cfg(disabled)]` 跳过 rig 模块。这是非标准用法（`disabled` 不是内置 cfg），但 Rust 允许任意 cfg flag，`#[cfg(disabled)]` 永远为 false，等效于注释掉模块。Phase 2 Task 9 删除该声明。
8. **Cargo.lock**: After removing rig, `cargo update` may pull in different versions of shared deps (reqwest, tokio, etc.). Run `cargo update` and verify no breaking changes.
9. **Local brain 缺少 model_provider 的兜底**: Task 4 BrainDal `Local` 分支用 `brain.model_provider.as_ref().ok_or_else(|| err!(Internal, "Local brain 缺少 model_provider"))?` 兜底。正常路径下不会触发（`Brain::new_local` 装配时保证 Some），仅用于防御性编程。
10. **cortex 包级函数依赖 registry 初始化**: Task 2 Step 2b 的 `embed_entity` / `embed_text_for_search` 包级函数内部调用 `cortex_registry()`，必须在 `cortex::native::init()` 之后才能使用（Task 8 修改初始化点）。Phase 1 期间 DAL 层调用点暂保持现状（Task 6b 统一改造），确保 init() 在 DAL 层调用前执行。
11. **`From<&Tool>` 依赖 Tool struct 的 `po` 字段**: Task 1 的 `impl From<&Tool> for ToolDescriptor` 访问 `tool.po.name` / `tool.po.description` / `tool.po.parameters_schema`。需验证 `Tool` struct 确实持有 `pub po: ToolPo` 字段，且 `ToolPo` 包含这三个字段（`parameters_schema` 为 `Option<serde_json::Value>`）。
12. **DAL 层 `cortex_dao` 字段移除影响**: Task 6b Step 4 移除 6 个 DAL 文件中的 `cortex_dao` 字段。改造前需逐文件 grep 确认 `self.cortex_dao` 在 Step 3 改造后已无残留引用（特别是 agent.rs 的 `upsert_vector_index` 原本内联了 `cortex_dao.create_cortex_trait` 调用，必须确保已改为调包级函数）。`brain.rs` 的 `cortex_dao` 字段由 Task 4 Step 1 处理（`think()` 改走 `cortex_registry().get()`）。
13. **BrainDal embed 入口的 `model_provider_dao` 依赖**: Task 4 Step 3 的 `BrainDalImpl::embed_entity` / `embed_text_for_search` 内部调用 `self.model_provider_dao.get_default_embedding_provider()`。需验证 `BrainDalImpl` 已注入 `model_provider_dao` 字段（或可通过其他 DAL 获取）。

**Parallelization:** Tasks 1-8 + Task 6b (Phase 1) are sequential due to type dependencies. Task 6b 依赖 Task 6（所有 CortexDao 实现就绪）和 Task 2 Step 2b（cortex 包级函数定义）。Tasks 9-12 (Phase 2) are also sequential (deletion order matters). No parallel execution within a phase.
