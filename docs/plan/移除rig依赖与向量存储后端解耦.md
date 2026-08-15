# 移除 rig 依赖与 Cortex 层扁平化重构

> 🎯 **本文档定位**：规划与落地结果快照（概览级，不包含代码细节；具体实现以代码路径为准）
>
> 状态：完成（2026-08-04）
>
> 查阅场景：
> - 新增 Provider 类型时，回看「分发速查表 + 扩展路径」两处即可
> - 排查向量化/DAL 调用链问题时，直接跳 §涉及文件对路径定位
> - 理解为什么删除 rig、工具调用循环为何上移到 awakening 层
>
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 分层架构规范 §3 Trait 位置约定
> - [external_agent_design.md](../design/external_agent_design.md) — 外部 Agent（Cli/Remote）协议设计
> - [vector_search_architecture.md](../design/vector_search_architecture.md) — 向量索引架构与 DAL 向量化约束

---

## 一、重构目标（为什么做）

移除 `rig` crate 黑箱依赖，将 4 层调用链（Brain → Cortex 实体 → CortexTrait → rig 内部 loop）扁平化为 2 层（BrainDal → CortexDao），同时解耦 DAL 向量化重复逻辑：

| 问题维度 | 解决方式 |
|---------|---------|
| (a) rig 黑箱 tool-calling loop：工具列表在 Agent 构造时固定，无法动态发现/追加 | 工具循环**上移到 awakening 显式实现**，CortexDao 仅表达 ThinkResult 意图（绝不执行工具）；ToolDescriptor 按 think() 调用**动态传入** |
| (b) CortexTrait/Cortex 实体多层抽象冗余；provider 参数在 think() 签名里反复传递 | 删除 CortexTrait + Cortex 实体；**Brain 直接持有 `model_provider: Option<ModelProviderPo>`**；think() 不再接收 provider 参数，从 brain 读取后分发 |
| (c) base_url / api_key / model_name 解析散落在 8+ 处，优先级（provider 覆盖 vs 默认）不统一 | 解析职责**收敛到各 CortexDao 实现内部**（如 `resolve_base_url(provider)`）；统一规则：`ModelProviderPo` 携带的字段优先于硬编码默认值 |
| (d) 向量化 helper `try_build_vector_params_*` 在 6 个 DAL 文件重复定义（17 处调用点），DRY 违反 + 升级成本高 | 新增**cortex 包级路由函数** `embed_entity(ctx, provider, entity)` / `embed_text_for_search(ctx, provider, text)`（纯协议层，不查 DB）；DAL 层自查 provider 后**直接调包级函数**，删除所有 helper |
| (e) 部分 Provider 走 `/responses` 非标准路径，存在兼容性隐患；DoubaoVision 多模态 embedding 走独立分支 | **所有 Provider 统一走 `/chat/completions`**（OpenAI/DeepSeek/Qwen/Doubao/DoubaoVision/Ollama/Custom）；多模态 embedding 集成到 `OpenAiCompatibleCortexDao::embed()` 内部按 `provider_type` 分支 |

**收敛后效果**：删除整个 rig crate 依赖；新增 Provider 类型仅需改「1 处 enum 变体 + 1 处 match 分支 + 1 行默认 base_url」；DAL 向量化 helper 全部归零。

---

## 二、架构思路（怎么做的）

三层扁平化架构（信息逐层下沉 + 职责单一）：

```
┌──────────────────────────────────────────────────────────┐
│ Execution layer（awakening.rs）                           │
│  ├─ Owns 显式工具调用循环，max_tool_rounds 默认 10         │
│  ├─ 生成 tool_call_id（uuid v7）                           │
│  ├─ 执行 CoreTool（经 ToolCallLoggingDecorator）           │
│  ├─ 将 "{tool_name, args, result}" 追加 prompt             │
│  └─ ToolDescriptor 动态派生：tools.map(ToolDescriptor::from)│
└────────────┬─────────────────────────────────────────────┘
             │ brain.think(ctx, brain, prompt, &tools)
             │   → 从 brain.model_provider.as_ref() 读取
             ▼
┌──────────────────────────────────────────────────────────┐
│ Brain（持有 Provider，按 kind 分发）                       │
│  ├─ model_provider: Option<ModelProviderPo>               │
│  │   Local agent → Some（wake_agent_brain 装配时注入）     │
│  │   Cli / Remote → None（外部 agent 自管）               │
│  ├─ Local 分支 → cortex_registry().get(provider_type)     │
│  └─ Cli/Remote 分支 → registry.external() 直接执行        │
└────────────┬─────────────────────────────────────────────┘
             │ dao.think(ctx, provider, prompt, &tools)
             ▼
┌──────────────────────────────────────────────────────────┐
│ Thinking layer（CortexDao 扁平化 DAO，无抽象层）           │
│  ├─ OpenAiCompatibleCortexDao — reqwest /chat/completions │
│  │   （7 种 ProviderType 共享单例 + HTTP 连接池）          │
│  │   embed() 内 DoubaoVision 分支走 /embeddings/multimodal│
│  ├─ FastEmbedCortexDao — 本地 fastembed crate（仅向量化） │
│  └─ ExternalCortexDao — CLI 子进程 / A2A 协议             │
└──────────────────────────────────────────────────────────┘
```

**CortexDaoRegistry 分发规则**：
```
ProviderType::OpenAI|DeepSeek|Qwen|Doubao|DoubaoVision|Ollama|Custom
    → OpenAiCompatibleCortexDao
ProviderType::FastEmbed
    → FastEmbedCortexDao
AgentKind::Cli | Remote
    → ExternalCortexDao（.external() 方法，不依赖 provider_type）
```

**关键边界（行为红线，回归必保）**：
1. CortexDao 的 `think()` **绝不执行工具**，只返回 `ThinkResult::Final(text)` 或 `ThinkResult::ToolCall(request)`；`tool_call_id` 由上层 awakening 生成并传入工具执行器
2. 所有 Provider（OpenAI 兼容 7 种）**强制统一走 `POST {base_url}/chat/completions`**，禁止回退到 `/responses` 或其他非标准端点
3. Provider 配置优先级：`ModelProviderPo.base_url/api_key/model_name` **优先于** `resolve_base_url()` 的硬编码默认值；Custom 类型**仅使用** provider 提供的 base_url（无默认）
4. cortex 包级函数 `embed_entity` / `embed_text_for_search` 是**纯路由层**，绝不查 DB / 绝不注入任何 DAL；调用方（DAL 或 BrainDal）**必须自行查 provider** 后再调用
5. Brain 的 `model_provider` 字段语义不可破：`Brain::new_local()` → 必为 `Some`；`Brain::new_external(Cli|Remote)` → 必为 `None`；两处构造方法是**唯一合法装配入口**

---

## 三、涉及文件（改动清单 → 查代码直接跳）

按 AGENTS.md §3.2 目录结构索引：

| 文件 | 角色 | 变更内容 |
|------|------|---------|
| **新增（类型 + 自建 Cortex 实现）** | | |
| [models/cortex_types.rs](../../src/models/cortex_types.rs) | Cortex 契约类型 | 定义 `ThinkResult`（Final/ToolCall）、`ToolCallRequest`、`ToolDescriptor` + `impl From<&Tool> for ToolDescriptor` |
| [service/dao/cortex/native/mod.rs](../../src/service/dao/cortex/native/mod.rs) | CortexDaoRegistry | 按 `provider_type` 分发的注册表单例；暴露 `cortex_registry()` + `init()`；`get(provider_type)` + `external()` 两入口 |
| [service/dao/cortex/native/http.rs](../../src/service/dao/cortex/native/http.rs) | HTTP 协议帮助 | `call_chat_completions` + 解析 usage 并记录 stats；`call_embeddings`；`call_embeddings_multimodal`（DoubaoVision 多模态）；`tools_to_openai_format` |
| [service/dao/cortex/native/openai.rs](../../src/service/dao/cortex/native/openai.rs) | OpenAI 兼容 DAO | `OpenAiCompatibleCortexDao` 单例（仅持 `reqwest::Client`）；`resolve_base_url(provider)` 统一默认值解析；embed() 内 DoubaoVision 分支 |
| [service/dao/cortex/native_test.rs](../../src/service/dao/cortex/native_test.rs) | 单元测试 | `resolve_base_url` 优先级（provider 覆盖默认值）+ Registry 分发逻辑；替代原 rig_test |
| **核心迁移（Brain/Provider 装配）** | | |
| [models/brain.rs](../../src/models/brain.rs) | Brain 模型 | 删除 `CortexTrait` trait + `Cortex` 实体；`Brain` 去掉 `cortex` 字段，新增 `model_provider: Option<ModelProviderPo>`；`new_local(..., model_provider)`；`new_external(kind, ...)`（不传 provider） |
| [models/mod.rs](../../src/models/mod.rs) | 模型模块入口 | 声明 `pub mod cortex_types;` + re-export `ThinkResult`/`ToolCallRequest`/`ToolDescriptor` |
| [service/dao/cortex/mod.rs](../../src/service/dao/cortex/mod.rs) | CortexDao trait + 包级路由 | 重写 trait：`think(ctx, provider, prompt, tools)` + `embed(ctx, provider, texts)`（默认实现 `embed_text/embed_entity/embed_text_for_search`）；**新增包级函数** `embed_entity(ctx, provider, entity)` / `embed_text_for_search(ctx, provider, text)`；删除 `mod rig` 的 pub use |
| [service/dao/cortex/external.rs](../../src/service/dao/cortex/external.rs) | 外部 Agent DAO | 重命名 `ExternalCortex` → `ExternalCortexDao`；实现新 trait（think 返回 Final，embed 返回 Err）；暴露 `execute_cli` / `execute_a2a` 给 BrainDal |
| [service/dao/cortex/fastembed.rs](../../src/service/dao/cortex/fastembed.rs) | 本地向量化 DAO | 重命名 `FastEmbedCortex` → `FastEmbedCortexDao`；实现新 trait（think 返回 Err，embed 走本地 fastembed） |
| [service/dal/brain.rs](../../src/service/dal/brain.rs) | BrainDal | `think()` **去掉 provider 参数**，从 `brain.model_provider.as_ref()` 读取后按 `brain.kind` 分支；新增 BrainDal 入口 `embed_entity(ctx, entity) -> Option<VectorIndexParams>` / `embed_text_for_search(ctx, text)`（内部查默认 provider + 调 cortex 包级函数） |
| [service/domain/runtime/awakening.rs](../../src/service/domain/runtime/awakening.rs) | 唤醒 + 工具循环 | 实现显式工具调用循环（think → ToolCall → 执行 → 追加 → loop）；`wake_agent_brain()` 加载 ModelProvider 并注入 `Brain::new_local`；Cli/Remote 走 `Brain::new_external` |
| **DAL 层向量化统一改造（删 helper → 直接调包级函数）** | | |
| [service/dal/agent.rs](../../src/service/dal/agent.rs) | Agent DAL | 删除方法 `try_build_vector_params_for_search`；改造 `upsert_vector_index`（内联逻辑改调包级函数）+ `search`；移除 `cortex_dao` 字段 |
| [service/dal/tool.rs](../../src/service/dal/tool.rs) | Tool DAL | 删除自由函数 `try_build_vector_params_for_entity`；改造 `upsert_vector_index` / `rebuild_vectors` / `search`；移除 `cortex_dao` 字段 |
| [service/dal/memory.rs](../../src/service/dal/memory.rs) | Memory DAL | 删除自由函数 `try_build_vector_params_for_entity`；改造 create/update/upsert_vector_index/search；移除 `cortex_dao` 字段 |
| [service/dal/message.rs](../../src/service/dal/message.rs) | Message DAL | 删除自由函数 `try_build_vector_params_for_entity` + `_for_search`；改造调用点；移除 `cortex_dao` 字段 |
| [service/dal/task.rs](../../src/service/dal/task.rs) | Task DAL | 删除自由函数 `try_build_vector_params_for_entity` + `_for_search`；改造 create/update/upsert_vector_index/search；移除 `cortex_dao` 字段 |
| [service/dal/project.rs](../../src/service/dal/project.rs) | Project DAL | 删除自由函数 `try_build_vector_params_for_entity` + `_for_search`；改造调用点；移除 `cortex_dao` 字段 |
| **Phase 2 清理（整体删除）** | | |
| `src/service/dao/cortex/rig/`（目录） | rig 旧实现 | 全删（openai/openai_compatible/ollama/doubao_vision/fastembed）；多模态已迁 native/http.rs，fastembed 已迁独立文件 |
| `src/service/dao/cortex/rig.rs` | RigCortexDao | 全删 |
| `src/service/dao/cortex/rig_test.rs` | rig 测试 | 全删（已由 native_test 替代） |
| `src/pkg/monitoring/rig_hook.rs` | token 采集 hook | 全删（改为 cortex/native/http.rs 直接从 HTTP body 提取 usage） |
| `patches/rig-fastembed/`（目录） | rig patch stub | 全删 |
| **Phase 2 清理（修改去 rig）** | | |
| [Cargo.toml](../../Cargo.toml) | 工作空间根 | 移除 `rig` 依赖；删除整个 `[patch.crates-io]` 节；common 特征去掉 `rig-integration` |
| [common/Cargo.toml](../../common/Cargo.toml) | common 配置 | 移除 `rig` 依赖 + `rig-integration` feature |
| [common/src/error/types.rs](../../common/src/error/types.rs) | 公共错误 | 删除 `From<rig::tool::ToolExecutionError>` impl；新增 `Error::tool_execution_failed(...)` 构造器（如不存在） |
| [pkg/tool_registry/handler_adapter/mod.rs](../../src/pkg/tool_registry/handler_adapter/mod.rs) | 工具适配 | `ToolErrorKind`/`ToolExecutionError` → `common::error::Error::tool_execution_failed`；删 helper `app_error_to_tool_error` |
| [pkg/tool_registry/mcp.rs](../../src/pkg/tool_registry/mcp.rs) | MCP 适配 | 同上，错误类型统一替换 |
| [service/dao/tool_call/mod.rs](../../src/service/dao/tool_call/mod.rs) | ToolCallDao trait | 删除 `wrap_for_rig` 方法签名 |
| [service/dao/tool_call/impl.rs](../../src/service/dao/tool_call/impl.rs) | ToolCallDao 实现 | 删除 `wrap_for_rig` 实现 + rig 导入 |
| [service/dao/tool_call/mcp.rs](../../src/service/dao/tool_call/mcp.rs) | MCP ToolCall | 同上 |
| [service/dal/tool.rs](../../src/service/dal/tool.rs) | ToolDal | 删除 `wrap_for_rig` 代理 |
| **零改动面（验证架构稳定性）** | | |
| 前端 / API DTO / 路由 / 集成测试 / `CoreTool` trait / `ToolCallLoggingDecorator` / StatsCollector 接口 | 对外契约不变 | 无修改；唤醒/工具执行对外行为等价 |

---

## 四、分发速查表（新增同类功能时改 N 处）

### 4.1 新增 OpenAI 兼容 Provider（如 Anthropic / xAI）

| 改动位置 | 操作 | 参考现有分支 |
|---------|------|------------|
| `common/src/enums.rs` 的 `ProviderType` 枚举 | 加变体（如 `Anthropic`） | `ProviderType::DeepSeek` |
| `cortex/native/mod.rs` 的 `CortexDaoRegistry::get()` | match 加 arm → `self.openai_compatible.clone()` | `ProviderType::Ollama => self.openai_compatible.clone()` |
| `cortex/native/openai.rs` 的 `resolve_base_url(provider)` | 加 match arm 返回默认 base_url | `ProviderType::DeepSeek => "https://api.deepseek.com"` |

> 代码入口：[openai.rs :: resolve_base_url](../../src/service/dao/cortex/native/openai.rs)

### 4.2 DAL 层新增向量化调用点（新增实体的 upsert_vector_index / search 等）

**固定 3 分支模式（不得重新抽 helper）**：

| 步骤 | 代码形态 | 备注 |
|------|---------|------|
| 1. 查默认 Provider | `self.model_provider_dao.get_default_embedding_provider(ctx.clone()).await` | 保留 `Ok(None)` 降级日志，携带实体 ID |
| 2. 调 cortex 包级函数（实体场景） | `crate::service::dao::cortex::embed_entity(ctx, &provider, &po).await` | 替代旧 `create_cortex_trait + embed_entity` 两步 |
| 2'. 调包级函数（搜索场景） | `crate::service::dao::cortex::embed_text_for_search(ctx, &provider, keyword).await` | search 类方法走这个 |
| 3. 写向量索引或搜索 | 原逻辑不变，接 `vec_params` | 失败仅 warn 降级，不阻断主流程 |

> 代码入口：[dal/agent.rs :: upsert_vector_index](../../src/service/dal/agent.rs)（最完整的参考样板）

---

## 五、验收清单（2026-08-04 全部达成 ✅）

- [x] 工作空间 `Cargo.toml` / `common/Cargo.toml` 无 `rig` 字样；`Cargo.lock` 无 rig 相关条目
- [x] `docs/` `src/` 中 grep `use rig::` / `rig::` / `DynamicTool` / `CortexTrait` / `wrap_for_rig` 零结果
- [x] `Brain` struct 无 `cortex` 字段；仅持有 `model_provider: Option<ModelProviderPo>`
- [x] `BrainDal::think()` 签名无 `provider` 参数；从 `brain.model_provider.as_ref()` 读取并分支
- [x] awakening 层拥有显式 tool-calling loop；ToolCall 时由 awakening 生成 `tool_call_id`
- [x] 工具列表按 think() 调用动态传入；ToolDescriptor 通过 `From<&Tool>` 派生
- [x] Token 用量从 HTTP response body 的 `usage` 提取并走 `ctx.stats().record()`，无 rig hook
- [x] 所有 OpenAI 兼容 Provider 统一走 `/chat/completions`；DoubaoVision embedding 走 `call_embeddings_multimodal`
- [x] 6 个 DAL 文件无 `try_build_vector_params_*` 定义与调用；全部改调 cortex 包级函数
- [x] 6 个 DAL 文件无 `cortex_dao` 字段注入；`self.cortex_dao` 全库 grep 零结果
- [x] Provider 配置覆盖默认值：自定义 `ModelProviderPo.base_url` 时 `resolve_base_url` 返回自定义值
- [x] `cargo check --all-targets` + `cargo clippy --all-targets -- -D warnings` + `cargo fmt --all -- --check` 全通过；`cargo test -p ai_orz --lib service::dao::cortex` 全通过

---

## 六、执行结果摘要（2026-08-04，Phase 1 + Phase 2 两阶段落地）

| 模块 | 验证结果 |
|------|---------|
| cortex/native 单元测试（resolve_base_url + Registry 分发） | All passed |
| service::dao::cortex 库级测试（原 rig_test 替代） | All passed |
| DAL 层 6 文件向量化改造回归（search / upsert_vector_index 各场景） | All passed（warn 降级路径覆盖） |
| 后端 lib 全量 cargo test | All passed（无 rig 相关测试崩溃） |
| cargo check --all-targets | Zero errors |
| cargo clippy --all-targets -- -D warnings | Zero errors / warnings |
| cargo fmt --all -- --check | Pass（格式一致） |
| 集成测试（awakening 工具循环 + 外部 Agent Cli/Remote 路径） | All PASS（行为安全网） |

### 与计划的 3 处微小偏离（均为实现细节精度，架构零影响）
1. `#[cfg(disabled)]` 用于临时跳过 rig 模块：原计划 Phase 1 用标准 cfg flag，最终采用"任意 cfg flag 恒为 false"模式，语义等效、操作更简单
2. `resolve_base_url` 初始为私有 `fn`，测试需要可见性 → 改为 `pub(crate) fn`（crate 内可见，未对外暴露 API）
3. `ExternalCortexDao.think()` 原计划返回 `Err(...)` 以满足 trait；实际实现中 Cli/Remote 分支改由 BrainDal 直接调 `execute_cli/execute_a2a`（不走统一 think 入口），think() 仍返回 Err 作为防御性兜底，未实际触发

---

## 七、后续扩展路径（新增能力 4 步模板）

> **核心不变量**：cortex 包级路由 / Brain.model_provider 持有语义 / awakening 工具循环职责 三项不动。

1. **新增非标准协议 Provider（非 OpenAI 兼容，如 Google Gemini REST、AWS Bedrock）**：
   - 新建独立 DAO 文件：[service/dao/cortex/native/gemini.rs](../../src/service/dao/cortex/native/)（单例，持 HTTP client）
   - 实现 `CortexDao` trait（think/embed，协议自行适配）
   - `ProviderType` 加变体 → [common/src/enums.rs](../../common/src/enums.rs)
   - `CortexDaoRegistry` 新增字段 + `get()` match 加 arm → [native/mod.rs](../../src/service/dao/cortex/native/mod.rs)

2. **新增向量化实体（DAL 层新增实体的向量索引维护）**：
   - 参考 [dal/task.rs](../../src/service/dal/task.rs) 的 3 分支模式：查默认 provider → `cortex::embed_entity(ctx, &provider, &po)` → upsert_vector
   - 搜索场景参考 [dal/agent.rs :: search](../../src/service/dal/agent.rs)，调 `cortex::embed_text_for_search`
   - **严禁**重新定义 `try_build_vector_params_*` helper
   - DAL struct 移除 `cortex_dao` 字段（保留 `model_provider_dao`）

3. **在 awakening 工具循环中增加 think 后处理 Hook（如 PII 脱敏、响应审计）**：
   - 插入位置：awakening 循环中 `ThinkResult::Final(content)` 返回之前
   - 代码入口：[awakening.rs](../../src/service/domain/runtime/awakening.rs) 的 awaken() 循环体
   - 通过 `ctx.hooks()` 或 `ctx.pipeline()` 机制扩展，不得在 cortex DAO 层加入后处理（破坏单一职责）

4. **动态工具发现（工具调用结果包含工具定义 → 追加到下轮 tools）**：
   - 觉醒层：awakening 循环中，ToolCall 执行后解析结果；若结果含"工具描述"类字段，追加到 `tools: Vec<ToolDescriptor>` 副本
   - CortexDao 侧：零改动（已支持动态工具列表，think() 每调用重新传入）
   - 参考样板：awakening 循环中每次 `think()` 传入的 `&tools` 可变 → [awakening.rs](../../src/service/domain/runtime/awakening.rs) 工具循环段
