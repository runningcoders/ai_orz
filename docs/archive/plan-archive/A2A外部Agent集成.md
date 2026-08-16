# A2A 外部 Agent 集成（CLI 子进程 + 通用远程）

> 📦 归档标记（2026-08-16）：归档冻结。保留原因：A2A外部Agent集成 功能已完成并通过验收，文档转为历史快照。生效方案：见源码和 wiki 长文。

> 状态：完成（2026-07-18）
> 查阅场景：
> - 注册新的外部 Agent 类型（CLI/Remote）时，回看「分发速查表 + 扩展路径」
> - 排查外部 Agent 调用链路/装配问题时，跳 §涉及文件定位
> - 理解 Local/Cli/Remote 三类 Agent 的 Brain 装配差异与统一入口
> 关联文档：
> - [external_agent_design.md](../archive/design-archive/external_agent_design.md) — 外部 Agent 设计决策（架构决策+关键约束）
> - [AGENTS.md](../../AGENTS.md) — 分层架构规范 §3 Domain/DAL/DAO 职责边界
> - [身份凭证Domain统一CRUD重构.md](./身份凭证Domain统一CRUD重构.md) — 同类派生 Dal 模式参考（Lark/Github 双分支）

---

## 一、重构目标（为什么做）

让 ai_orz 组织能够注册并调用**外部 Agent**（非本仓库的执行后端），同时**补全本地 Agent 的 brain 装配链路**（原链路缺失导致"Agent 大脑未唤醒"生产故障）：

| 问题维度 | 解决方式 |
|---------|---------|
| (a) 单一 Local 执行后端：无法复用 Codex / Claude Code / Aider 等成熟 CLI 工程化 Agent | 新增 **AgentKind::Cli**（子进程执行后端）；独立 DAO 层 `CodexRuntimeDao` 用 tokio::process 包装 stdin/stdout 协议 |
| (b) 无法跨组织调用远程 Agent：其他 ai_orz 实例/兼容 A2A 的 Agent 无法接入 | 新增 **AgentKind::Remote**（通用 HTTP 执行后端）；`A2aRuntimeDao` 用 reqwest 调用 A2A Server endpoint |
| (c) 外部 Agent 与本地 Agent 执行链路分叉：需要独立 awaken 逻辑，重复记忆/技能/Trace/统计注入 | **Brain 结构升级**：持有 `kind` + `runtime_config`；`BrainDal.think()` 内部分发（Local→cortex, Cli→execute_cli, Remote→execute_a2a）；上层 `RuntimeDomain.awaken()` 零改动 |
| (d) 不同 Agent 的 prompt 组装方式不同，硬编码在 awakening 中 | 新增 **PromptBuilder trait 抽象**（定义在 models 层）；各 Agent dal 提供专属 builder（Default/Cli/Remote）；RuntimeDomain 按 agent.kind 路由获取 builder |
| (e) 历史遗留装配链路缺失：AgentDal::wake_brain 与 BrainDal::wake_brain 无生产调用者，awakening 依赖 agent.brain 必须 Some | `HrDomain.get_agent()` 返回 agent 时**按 kind 路由装配**：Local 走 `AgentDal.wake_agent_brain()`（构造真实 Brain）；外部 agent 跳过（v5 决策，外部 agent 不装配 cortex Brain） |

**收敛后效果**：三类 Agent（Local/Cli/Remote）走**同一条 awaken 统一链路**；新增外部 Agent 类型仅需「1 个 AgentKind 变体 + 1 个 RuntimeDao 实现 + 1 个派生 Dal + 1 个 PromptBuilder」，上层 Domain/Handler 零改动。

---

## 二、架构思路（怎么做的）

v5 架构：Brain 内部分发 + PromptBuilder 工厂 + 派生 Dal 三足鼎立

```
┌──────────────────────────────────────────────────────────┐
│ Consumer 收到 TaskAssignment → awaken 统一入口             │
│ RuntimeDomain.awaken(ctx, agent, message)                │
│  │                                                        │
│  ├─ Step 1-3: 加载记忆 / 技能 / 工具（三类 Agent 相同）   │
│  │                                                        │
│  ├─ Step 4: PromptBuilder 工厂（按 agent.kind 路由）      │
│  │    Local  → DefaultPromptBuilder                       │
│  │    Cli    → CliPromptBuilder （CodexAgentDal 提供）    │
│  │    Remote → RemotePromptBuilder（A2aAgentDal 提供）    │
│  │    → 用 builder 组装完整 prompt                        │
│  │                                                        │
│  └─ Step 5: brain_dal.think(ctx, brain, prompt)          │
│       统一入口，Brain 按 kind 内部分发 ↓↓↓                 │
└──────────────┬───────────────────────────────────────────┘
               │
               ▼
┌──────────────────────────────────────────────────────────┐
│ Brain（思考执行环境统一抽象，不管是否真有 LLM）            │
│  ├─ kind: AgentKind (Local/Cli/Remote)  ← 分发唯一依据    │
│  ├─ agent_id / agent_name / runtime_config               │
│  ├─ memories: Vec<Memory>（三类 Agent 都带）              │
│  ├─ model_provider: Option<ModelProviderPo>              │
│  │   Local   → Some（wake_agent_brain 装配时注入）        │
│  │   Cli / Remote → None（外部 agent 自管）               │
│  └─ runtime_config.external_config → Cli/Remote 配置体    │
└──────────────┬───────────────────────────────────────────┘
               │ BrainDal.think() 内部 match brain.kind
               ▼
┌──────────────────────────────────────────────────────────┐
│ Three execution backends（BrainDal 内部三分）             │
│  ├─ Local   → cortex_registry().get(ptype).think(...)    │
│  │           （ai_orz 内部 LLM + 工具循环）               │
│  ├─ Cli     → CodexRuntimeDao.invoke(ctx, agent, prompt) │
│  │           （tokio::process stdin 写 / stdout 读）      │
│  └─ Remote  → A2aRuntimeDao.invoke(ctx, agent, prompt)   │
│              （reqwest POST A2A endpoint / JSON-RPC）    │
└──────────────────────────────────────────────────────────┘
```

**AgentKind 语义（全库唯一分类标准）**：
```
AgentKind::Local = 0  → ai_orz 内部 Brain + Tools + CortexDao 执行
AgentKind::Cli   = 1  → 子进程包装（Codex / Claude Code / Aider / 任意 stdin/stdout CLI）
AgentKind::Remote = 2 → HTTP 远程（A2A 协议 / 兼容 REST Agent Service）
→ AgentKind::is_external() = Cli | Remote
```

**关键边界（行为红线，回归必保）**：
1. `BrainDal.think()` 是**唯一合法调用入口**；上层 awakening 绝不直接调 Codex/A2a RuntimeDao（保持三分发不可绕过）
2. 外部 Agent（Cli/Remote）的 Brain **永远不装配 cortex/model_provider**（`model_provider: None`，由 `Brain::new_external` 构造保证）；禁止 Local 分支使用 `new_external`
3. `PromptBuilder trait` 必须定义在 **models 层（纯抽象）**，具体实现放在 DAL 层（禁止反向依赖）；派生 Dal 通过 `prompt_builder()` 工厂方法暴露专属实现
4. `create_agent` Domain 方法**只有一个**（通用抽象）；用户行为差异（创建外部 vs 本地）由不同 Handler 处理（Handler 构造好 Agent 后直接调通用 create_agent）
5. 神经工具策略 **L1 边界**：仅通过 PromptBuilder 将工具描述/技能/短期记忆**注入 prompt**（外部 agent 自然语言"看到"能力）；禁止 L2 代理解析外部 agent 输出工具调用（后续独立计划处理）

---

## 三、涉及文件（改动清单 → 查代码直接跳）

| 文件 | 角色 | 变更内容 |
|------|------|---------|
| **枚举 + 数据层** | | |
| [common/src/enums/agent_kind.rs](../../common/src/enums/agent_kind.rs) | 分类枚举 | AgentKind 三变体（Local=0/Cli=1/Remote=2）；`From<i32>`/`to_i32()`/`is_external()`/Display；4 个单元测试 |
| [common/src/enums/mod.rs](../../common/src/enums/mod.rs) | 枚举入口 | 注册 agent_kind 模块 + re-export AgentKind |
| [migrations/20260719000000_add_kind_to_agents.sql](../../migrations/20260719000000_add_kind_to_agents.sql) | 数据库迁移 | agents 表 ALTER 新增 `kind INTEGER NOT NULL DEFAULT 0`（向后兼容，现有行全 Local） |
| [src/models/agent.rs](../../src/models/agent.rs) | Agent 模型 | `AgentRuntimeConfig` 新增 `external_config: Option<ExternalAgentConfig>`（serde tag="executor"，Cli/Remote 两变体）；`AgentPo` 新增 `kind: AgentKind` 字段 + `po.get_runtime_config()`/`set_runtime_config()` 便捷方法；8 个序列化/向后兼容测试 |
| **DAO 层（执行后端）** | | |
| [src/service/dao/agent_runtime/mod.rs](../../src/service/dao/agent_runtime/mod.rs) | AgentRuntimeDao trait | 统一 trait：`invoke(&self, ctx, agent: &AgentPo, prompt) -> Result<String>`；子模块声明 codex/a2a |
| [src/service/dao/agent_runtime/codex.rs](../../src/service/dao/agent_runtime/codex.rs) | Codex CLI DAO | tokio::process 启动子进程；stdin 写 prompt；stdout 读响应；超时（timeout_secs）kill；环境变量注入；work_dir 设置；3 个单元测试 |
| [src/service/dao/agent_runtime/a2a.rs](../../src/service/dao/agent_runtime/a2a.rs) | A2A HTTP DAO | reqwest POST `{endpoint}/a2a/invoke`；Bearer auth_token 可选；超时；JSON-RPC request/response；2 个单元测试 |
| [src/service/dao/cortex/external.rs](../../src/service/dao/cortex/external.rs) | ExternalCortex（备用） | 实现原 CortexTrait 的虚拟实现（备用方案，v5 实际未用于装配链路）；保留作兜底兜底；7 个测试 |
| [src/service/dao/mod.rs](../../src/service/dao/mod.rs) | DAO 入口 | 注册 agent_runtime 模块 |
| **DAL 层（派生 Dal + PromptBuilder 抽象）** | | |
| [src/models/prompt_builder.rs](../../src/models/prompt_builder.rs) | PromptBuilder trait | 纯抽象：`system() / user() / memory() / skills() / builtin_tools() / bound_tools() / build()`；定义在 models 层禁止反向依赖 |
| [src/models/mod.rs](../../src/models/mod.rs) | 模型入口 | 注册 prompt_builder 模块 |
| [src/service/dal/prompt_builder_default.rs](../../src/service/dal/prompt_builder_default.rs) | DefaultPromptBuilder | Local agent 用；原 awakening PromptBuilder 链式 API 迁移；实现 PromptBuilder trait；行为测试 3 个 |
| [src/service/dal/agent_codex.rs](../../src/service/dal/agent_codex.rs) | CodexAgentDal | 派生 Dal：管理操作委托通用 AgentDal；`prompt_builder()` 返回 CliPromptBuilder；单元测试 5 个 |
| [src/service/dal/agent_a2a.rs](../../src/service/dal/agent_a2a.rs) | A2aAgentDal | 派生 Dal：管理操作委托通用 AgentDal；`prompt_builder()` 返回 RemotePromptBuilder；单元测试 3 个 |
| [src/service/dal/agent.rs](../../src/service/dal/agent.rs) | 通用 AgentDal | 新增便捷方法 `wake_agent_brain(ctx, agent) -> Result<()>`（Local agent 入口：查 provider + 查 memories/tools → BrainDal.wake_brain → 赋值给 agent） |
| [src/service/dal/brain.rs](../../src/service/dal/brain.rs) | BrainDal | **新增 `invoke_external()` 方法**（持有 AgentRuntimeDao 引用）；`think()` 内部分发：Local→cortex think，Cli/Remote→invoke_external；新增 Brain 构造便捷方法 `new_local` / `new_external(kind, ...)` |
| [src/service/dal/mod.rs](../../src/service/dal/mod.rs) | DAL 入口 | 注册 agent_codex + agent_a2a + prompt_builder_default 模块 |
| **Domain 层（装配 + 路由）** | | |
| [src/service/domain/runtime/awakening.rs](../../src/service/domain/runtime/awakening.rs) | 唤醒链路 | awaken 内部按 `agent.po.kind` 路由：Local 走 think，外部走 invoke_external；**PromptBuilder 获取**：按 kind 路由到对应派生 Dal；短期记忆/技能/Trace/统计注入零改动 |
| [src/service/domain/runtime/context_assembly.rs](../../src/service/domain/runtime/context_assembly.rs) | Prompt 组装 | 原 PromptBuilder 改为实现 trait；保留链式 API 作为 DefaultPromptBuilder 专属方法 |
| [src/service/domain/hr/agent.rs](../../src/service/domain/hr/agent.rs) | HrDomain Agent | `get_agent` 按 kind 路由装配：Local → `agent_dal.wake_agent_brain(agent)`，Cli/Remote 不装配；`create_agent` 外部 agent 跳过 model_provider 校验 |
| [src/service/domain/hr/mod.rs](../../src/service/domain/hr/mod.rs) | HrDomainImpl | 新增字段 `codex_agent_dal: Arc<dyn CodexAgentDal>` / `a2a_agent_dal: Arc<dyn A2aAgentDal>`（main 注入） |
| **API + Handler** | | |
| [common/src/api/external_agent.rs](../../common/src/api/external_agent.rs) | 外部 Agent API DTO | `CreateExternalAgentParams`（kind + CLI 配置 / Remote 配置分支字段）；`CreateExternalAgentResponse { agent_id }` |
| [common/src/api/mod.rs](../../common/src/api/mod.rs) | API 入口 | 注册 external_agent 模块 |
| [src/handlers/hr/agent/create_external_agent.rs](../../src/handlers/hr/agent/create_external_agent.rs) | 创建外部 Agent Handler | POST /api/v1/hr/agents/external；按 kind 校验字段必填 → 构造 ExternalAgentConfig → 设置 po.kind/po.runtime_config.external_config → **直接调用通用 create_agent** |
| [src/handlers/hr/agent/mod.rs](../../src/handlers/hr/agent/mod.rs) + [handlers/mod.rs](../../src/handlers/mod.rs) | Handler 入口 + 路由 | 注册 create_external_agent handler + 路由 |
| [src/main.rs](../../src/main.rs) | 启动注入 | 初始化 CodexAgentDal + A2aAgentDal → 注入 HrDomainImpl |
| **零改动面（验证架构稳定性）** | | |
| 前端页面（本 plan v5 范围不含前端；前端页面由后续独立 Task 处理）/ 通用 Agent CRUD Handler / awaken 链路短期记忆+技能+Trace+统计注入机制 / CoreTool trait | 对外契约不变 | 无修改；Local agent 行为与之前完全等价 |

---

## 四、分发速查表（新增同类功能时改 N 处）

### 4.1 新增 CLI Agent 变体（新的外部命令如 `Claude Code` / `Aider` / 自定义脚本）

大多数 CLI 变体**不需要改代码**，直接在前端页面或通过 API 创建时配置 `command/args/work_dir/env` 即可复用 Codex 管道。

若需要**特化 prompt 组装**（例如 Aider 需要特殊的 prompt 前缀）：

| 改动位置 | 操作 | 参考现有样板 |
|---------|------|------------|
| 无需新增 AgentKind（仍走 Cli） | —— | 利用现有 `AgentKind::Cli` 语义 |
| 新建派生 Dal（可选）：`src/service/dal/agent_aider.rs` | 管理操作委托通用 AgentDal；`prompt_builder()` 返回 AiderPromptBuilder（继承 Default，override 前缀） | [agent_codex.rs](../../src/service/dal/agent_codex.rs) |
| main.rs 注入新增派生 Dal + HrDomain 扩展字段 | new 时传 Arc；注册到 DAL module | [main.rs](../../src/main.rs) 现有 codex/a2a 注入段 |

> 代码入口：[CodexAgentDal 派生模式](../../src/service/dal/agent_codex.rs)（委托 + prompt_builder 工厂）

### 4.2 新增非 A2A 标准的 Remote 协议（如自定义 REST / WebSocket Agent）

| 改动位置 | 操作 | 参考现有样板 |
|---------|------|------------|
| 新增 AgentKind 变体（如 `WebSocketRemote`）→ 可选；亦可复用 Remote 语义通过 endpoint scheme 区分 | [agent_kind.rs](../../common/src/enums/agent_kind.rs) 加变体 + From<i32>/to_i32/is_external | AgentKind::Remote 定义 |
| 新增独立 RuntimeDao：`dao/agent_runtime/custom_rest.rs` | 实现 `AgentRuntimeDao::invoke()`，协议自行适配（reqwest WebSocket/tokio-tungstenite） | [a2a.rs](../../src/service/dao/agent_runtime/a2a.rs) |
| agent_runtime/mod.rs 注册新模块；CortexDaoRegistry/BrainDal 外部分支扩展 | mod.rs 声明子模块；BrainDal.invoke_external 按 kind 多分支 | [awakening.rs](../../src/service/domain/runtime/awakening.rs) 路由段 |
| 新增派生 Dal（可选） + PromptBuilder | 参考上条 | [agent_a2a.rs](../../src/service/dal/agent_a2a.rs) |

> 代码入口：[A2aRuntimeDao 实现模式](../../src/service/dao/agent_runtime/a2a.rs)（reqwest client + invoke 签名）

---

## 五、验收清单（2026-07-18 全部达成 ✅）

**未完成项说明**：前端 Agent 列表页/详情页的外部 Agent 创建入口与类型徽章（本计划 v5 范围不含，已拆分独立前端 Task 处理，后端 API 已就绪可直接对接）。

---

见 Plan 文档对应 Git 提交记录 / 对应执行任务。

## 六、执行结果摘要（2026-07-18，14 个 Task 分阶段落地）

| 模块 | 验证结果 |
|------|---------|
| AgentKind 枚举（common） | 4/4 tests PASS |
| ExternalAgentConfig 序列化（AgentPo） | 8/8 tests PASS（向后兼容零破坏） |
| CodexRuntimeDao（子进程） | 3/3 tests PASS（含超时 kill） |
| A2aRuntimeDao（HTTP） | 2/2 tests PASS |
| ExternalCortex 备用实现 | 7/7 tests PASS |
| PromptBuilder trait + DefaultPromptBuilder | 3/3 tests PASS |
| CodexAgentDal 派生 Dal | 5/5 tests PASS |
| A2aAgentDal 派生 Dal | 3/3 tests PASS |
| RuntimeDomain awaken 路由（按 kind 分发） | 2/2 tests PASS |
| HrDomain get_agent 装配链路（Local vs 外部） | 1/1 test PASS |
| 后端 lib 全量 后端全量测试（回归 120） | 120/120 tests PASS |
| 工程编译 workspace） | Zero errors |
| 手动冒烟（cat CLI Agent 创建） | kind=Cli + external_config 写入 DB 正确 |
| 文档 docs/external_agent_design.md | 已创建，涵盖设计决策 + API 用法 |

### 与计划的 2 处偏离（均为 v5 架构决策演进，非 bug）
1. **v3/v4 方案用 ExternalCortex 虚拟装配，v5 改为 BrainDal 三分发**：原方案外部 Agent 也走 CortexTrait（v3/v4），v5 演变为 `BrainDal.think()` 内部 `match brain.kind` 直分三路；带来的收益是去掉了 ExternalCortex 作为"中间垫片"的一层抽象，调用链更直接。ExternalCortex 文件保留作备用兜底（不参与主路径）
2. **v4 外部 Agent 也装配 Brain（model_provider dummy），v5 改为不装配**：原决策外部 agent 也走 `wake_brain` 构造，v5 最终精简为 `HrDomain.get_agent` 对 Cli/Remote**跳过 brain 装配**（runtime_config 已存在 AgentPo，执行时直接读取）；减少了无意义的 dummy provider 构造，代码更清晰

---

## 七、后续扩展路径（新增能力 4 步模板）

> **核心不变量**：AgentKind 分类语义 / BrainDal.think() 三分发入口 / PromptBuilder models 层定义 三项不动。

1. **A2A Server（让本 ai_orz 组织本身暴露 A2A endpoint，供外部调用）**：
   - 新增 handlers：`src/handlers/a2a_server/invoke_agent.rs`（`POST /a2a/v1/invoke` 接收 JSON-RPC 请求）
   - 鉴权：通过 auth_token → 反查 Organization + 目标 Agent Card；代码入口参考现有鉴权模式：[handlers 目录](../../src/handlers/)
   - 业务逻辑：直接复用 RuntimeDomain awaken（收到 prompt → awaken → 返回 answer）；**零拷贝直接走 awaken 链路**，不写新执行逻辑
   - 对应设计文档位置：[external_agent_design.md](../archive/design-archive/external_agent_design.md) 的"未来扩展 A2A Server"章

2. **跨组织路由（OrganizationScope::Remote 实际实现）**：
   - enums 扩展：`OrganizationScope` 枚举加 `Remote { endpoint, auth_token }` 变体 → [common/src/enums.rs](../../common/src/enums/)
   - HrDomain 获取 Project/Agent 时：如果 OrganizationScope 是 Remote，走内部 A2aRuntimeDao **跨组织调用**对端的 GET/PUT 接口；本地不存数据镜像
   - 路由入口：[HrDomain get_project/get_agent](../../src/service/domain/hr/agent.rs) 扩展 match scope 分支

3. **神经工具 L2（解析外部 Agent 输出中的工具调用意图 → 代理执行 CoreTool）**：
   - 在 BrainDal 的 Cli/Remote 分支**读取响应后**新增 parse 阶段；代码入口：[brain.rs :: invoke_external](../../src/service/dal/brain.rs)
   - 派生 Dal 增加 `parse_response(response) -> Vec<ToolCallRequest>` 方法；默认实现返回空（L1 兼容）；Codex/A2a 派生 Dal 可 override 为 regex/JSON 解析
   - 工具执行复用 awakening 现有 `ToolCallLoggingDecorator`；结果追加 prompt → 再次 invoke_external → loop（最多 N 轮）

4. **通用派生 Dal 模板（新增第 N 个外部 Agent 类型的派生 Dal）**：
   - 文件：`src/service/dal/agent_xxx.rs`（复制 CodexAgentDal 模板）
   - 模式：管理操作（create/update/delete/list/get）**全部委托** `Arc<dyn AgentDal>`，仅 `prompt_builder()` 工厂方法返回自己的 builder
   - PromptBuilder 实现：继承 DefaultPromptBuilder（Deref），仅 override `build()` 头尾拼接自己的前缀/后缀
   - 代码入口样板：[agent_codex.rs](../../src/service/dal/agent_codex.rs)（委托 9 方法 + prompt_builder 工厂最简模式）