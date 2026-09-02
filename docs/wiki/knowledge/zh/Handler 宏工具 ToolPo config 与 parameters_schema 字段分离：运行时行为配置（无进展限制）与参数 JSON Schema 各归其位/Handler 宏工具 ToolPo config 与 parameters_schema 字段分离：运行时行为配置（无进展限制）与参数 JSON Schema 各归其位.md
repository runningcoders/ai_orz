---
kind: rag_card
name: Handler 宏元数据契约 + ToolPo config/parameters_schema 字段分离
category: pkg层基础设施
scope:
- ai-orz-macros/src/lib.rs
- src/models/tool.rs
- src/pkg/tool_registry/**
- src/handlers/**
- src/service/dal/tool.rs
- src/service/dao/tool/sqlite.rs
- src/service/domain/runtime/think_loop.rs
source_files:
- ai-orz-macros/src/lib.rs#L187-L280
- src/models/tool.rs#L98-L126
- src/models/tool.rs#L245-L280
- src/models/tool.rs#L333-L360
- src/service/dao/tool/sqlite.rs#L392-L422
- src/service/domain/runtime/think_loop.rs#L93-L113
- src/handlers/hr/agent/update_agent.rs
- src/handlers/user/profile/update_current_user.rs
- src/pkg/tool_registry/doubao_search.rs
- src/pkg/tool_registry/shell_exec.rs
- docs/wiki/zh/content/架构设计/分层架构设计/DAL 层组合.md
- docs/wiki/knowledge/zh/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法.md
- docs/wiki/knowledge/zh/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验.md

---

# Handler 宏元数据契约 + ToolPo 字段分离

本卡一次性覆盖 `#[register_handler_tool]` 宏和 `BuiltinToolFactory::create_po()` **两条注册路径**上的 ToolPo 元数据完整契约。字段分两组：**运行时行为组**（config + parameters_schema）和 **调用者可见组**（name / description / tags / neural / params）。

---

## §1 字段分离方案（历史 bug 背景 + 现行约定）

`#[register_handler_tool]` 宏展开的 `create_po()` 必须把参数 JSON Schema 写入 `ToolPo.parameters_schema`（`Option<Value>`），`ToolPo.config`（`Value`）置 `Null`——config 留给运维写入运行时行为配置，parameters_schema 专门存参数 schema（供模型 function calling 使用）。

**历史 bug（已修复 a05ce051）**：旧版宏把 schema 误填到 config 位置、parameters_schema 留空。后果：`ToolDescriptor::from` 读 `po.parameters_schema` → None → 所有宏注册的 handler 工具发给模型的参数定义都是空 object `{"type":"object","properties":{}}`，模型只能盲猜参数名。执行链路虽然没坏（`HandlerToolAdapter` 按 params 类型直接反序列化，不经过 ToolPo schema），但模型-工具交互层被破坏。

**与 `HandlerToolBuilder::build` 对比**：手写注册的 `HandlerToolBuilder` 一直是正确写法（config = Null, parameters_schema = Some(schema)），宏只是 bug。

### 运行时行为组

| 字段 | 类型 | 所有权 | 写入时机 |
|------|------|--------|---------|
| `ToolPo.config` | `serde_json::Value` | **运维现场** | 初始为 `Null`，允许运维写入 `{"timeout_ms": ..., "max_output_bytes": ..., "no_progress_max_calls": ...}`；`sync_builtin_tools_to_db` 同步时**绝不覆盖** |
| `ToolPo.parameters_schema` | `Option<Value>` | **代码** | 宏展开时从 params DTO 反射生成 JSON Schema；sync 时以代码为准刷新 |

---

## §2 调用者可见元数据契约（2026-09 全量确立）

`name / description / tags / neural / params` 五字段直接决定**模型如何选择工具**和**管理后台 UI 如何展示**。2026-09 对 144 个文件（134 handler + 10 BuiltinTool）做了统一重写，确立以下契约：

### 2.1 name：human-readable 短语，不是 snake_case 标识符

| ❌ 旧写法 | ✅ 新写法 | 理由 |
|----------|----------|------|
| `"list_tools"` | `"List All Tools"` | 管理后台 UI 直接展示，也帮助模型快速理解这是啥 |
| `"doubao_search"` | `"Search Chinese Web (Doubao)"` | 区分于英文搜索 tavily_search |
| `"fetch_url"` | `"Fetch Web Page"` | 更通用、语义清晰 |
| `"fs_read"` | `"Read File"` | 简洁动词短语 |

**约束**：**handler 路径**（`#[register_handler_tool]`）和 **BuiltinToolFactory 路径**（`BuiltinToolFactory::create_po()`）都遵循同一约定。`id`（handler 宏的 `id = "update_agent"` 或 Builtin 的结构体名）保持 snake_case 不变——id 是内部标识符、name 是面向人和 LLM 的展示名。

### 2.2 description：Agent 视角，回答三个问题

**写 description 时把自己当成「正在规划下一步工具调用的 LLM Agent」**，按以下三段结构组织：

```
[做什么 + 返回什么] · [何时用/分工] · [可预判失败]
```

**5 条硬规则**（从 2026-09 重写中总结）：

| # | 规则 | 示例 |
|---|------|------|
| 1 | **删除内部实现细节**（DAO 名、方法签名、SQL 片段） | ❌ `"调用 ToolDao.query_tools 按 tags_contains 过滤"` → ✅ `"按标签/状态/ID 查询工具列表，支持 FTS5+向量混合搜索"` |
| 2 | **删除 ACL 枚举**（`require Admin role / SuperAdmin / HR manager`） | ❌ `"需要 Admin 角色"` → 删去——Agent 不需要知道权限门槛；知道自己能不能调就行（neural tag + scene 过滤已经替它决定） |
| 3 | **保留可预判的失败场景** | ✅ `"NotFound if the agent does not exist"` / `"crosses user boundary returns require_confirmation, stop and ask user"` |
| 4 | **兄弟工具建立互斥引用**（避免模型同时调冲突工具） | ✅ doubao_search description 里提 `"For English queries prefer tavily_search"`；反之 tavily_search 提 `"For Chinese queries prefer doubao_search"` |
| 5 | **必须描述返回值** | ❌ `"更新工具"` → ✅ `"Returns the updated agent"` / `"Returns the matched search results as array"` |

**require_confirmation vs blocked 语义**：工具因安全边界返回 `{success: false, require_confirmation: true}` 时，description 必须写 `"returns a require_confirmation result instead of content — stop and ask the user for explicit confirmation"`，**绝对不能写 "blocked" / "denied"**（实际行为是请用户确认，不是硬拒绝）。

### 2.3 neural：显式标注是否暴露给 Agent 自动调用

- **不加 neural**（默认）：只在"已绑定到 Agent"或"已装包"场景加载，不在 Agent 工具清单自动出现
- **加 `neural`**：Agent 唤醒加载时 SQL 层 tag_filter 自动追加（免绑定），但**调用时 handler 仍应做业务边界守卫**（见 `工具系统三层调用架构` 卡 §3）

**2026-09 首例 neural + 边界守卫模式**：`update_agent` 加 `neural`，handler 层检测 `ctx.agent_id().is_some()`：
- Agent 上下文：只允许改自己（`params.id == ctx.agent_id`），且**身份路由字段**（name / roles / model_provider_id / runtime_config）**静默忽略**，仅 description / capabilities / soul 生效
- 人类用户：无 agent_id，全部字段可改

### 2.4 tags：决定场景过滤与加载分组

- `"neural"` → 免绑定自动加载（核心思考工具）
- `"internal"` → 加载时从 Agent 列表剔除 + 绑定拒绝（人工运维工具）
- `"neural"` + `"memory"` / `"query"` / `"search"` 等场景标签 → ThinkingScene 过滤时决定哪些场景下允许调用（settle/compact 只放 neural+memory 的工具）

### 2.5 params：必须与 fn 参数 1:1

`params = "common::api::UpdateAgentRequest"` 指定的 DTO 结构体字段必须与 Handler fn 的 `params` 参数类型完全一致。字段名错会导致 Agent 传的 args JSON 无法被 Handler 接收（400）。

---

## §3 关键文件路径表格（读代码直接跳）

| 文件锚点 | 角色 | 核心契约 |
|---------|------|---------|
| [ai-orz-macros/src/lib.rs](ai-orz-macros/src/lib.rs#L187-L280) | 宏展开 ToolPo 构造 | name/description/tags/neural 元数据 + schema_json → parameters_schema（Some）；config = Value::Null |
| [src/models/tool.rs](src/models/tool.rs#L98-L126) | ToolPo struct 定义 | 全部元数据字段的存储载体 |
| [src/models/tool.rs](src/models/tool.rs#L245-L280) | ToolPo::new 签名 | 完整构造函数，参数字序不能错 |
| [src/models/tool.rs](src/models/tool.rs#L333-L360) | 便捷读取方法 | `config_timeout_ms()` / `config_no_progress_max_calls()` — 统一 `.get("snake_case_key")` 风格 |
| [src/service/dao/tool/sqlite.rs](src/service/dao/tool/sqlite.rs#L392-L422) | sync 不变量 | sync_builtin_tools_to_db：代码所有权字段（name/description/control_mode/parameters_schema/tags）→ 以代码为准刷新；运维所有权字段（config/status）→ 绝不写入 |
| [src/service/domain/runtime/think_loop.rs](src/service/domain/runtime/think_loop.rs#L93-L113) | NoProgressPolicy 数据源 | 遍历 `agent.tools()` 读 `t.po.config_no_progress_max_calls()` 收集到 policy_set! |
| [src/handlers/hr/agent/update_agent.rs](src/handlers/hr/agent/update_agent.rs) | neural + 自改守卫模式 | Agent 上下文只改自己 + 静默忽略身份路由字段 |
| [src/pkg/tool_registry/doubao_search.rs](src/pkg/tool_registry/doubao_search.rs) | Builtin 工具注册 | BuiltinToolFactory::create_po() 手动构造 ToolPo，遵循同一 name/description 契约 |
| 【平行卡】策略引擎卡 | NoProgressPolicy 实现 | [策略引擎](docs/wiki/knowledge/zh/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法.md) |
| 【平行卡】工具系统架构卡 | neural tag 加载时机 + 调用链 | [工具系统三层调用架构](docs/wiki/knowledge/zh/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验.md) |

---

## §4 架构约定

1. **config 与 parameters_schema 语义严格分离**：config = 运行时行为配置（timeout_ms / max_output_bytes / no_progress_max_calls 等运维写入的键）；parameters_schema = 参数 JSON Schema（模型 function calling 消费，代码所有权）。
2. **sync_builtin_tools_to_db 的写入边界**：代码所有权字段（name/description/control_mode/parameters_schema/tags）→ 以代码为准刷新写入；运维所有权字段（config/status）→ 绝不写入，保留数据库现场。
3. **handler 工具 config 初始值必须为 Null**：宏展开时显式写 `serde_json::Value::Null`，禁止把 schema 或其他 JSON 填进 config；`fill_defaults_for_builtin()` 不覆盖已有的 config（已存在则保留）。
4. **NoProgressPolicy 数据源走 config_no_progress_max_calls() 便捷方法**：值为 0 或负数视为未配置（不限制），未配置的工具完全不受限，代码执行类高频工具天然免配。
5. **name / description 规范适用于两条注册路径**：handler 宏路径 + BuiltinToolFactory 路径都遵循 §2.1-2.2 契约，避免出现一部分工具是 human-readable、一部分还是 snake_case 的混合态。
6. **neural 工具的调用时身份边界守卫**：任何暴露给 Agent 自调用的管理类工具（如 update_agent / update_memory），handler 层必须检测 `ctx.agent_id()`：Agent 上下文只允许改自己、身份路由字段静默忽略、跨身份报错；人类用户上下文无 agent_id 时全部字段可改。

---

## §5 约束清单

1. ❌ **禁止把参数 schema 写入 config**：这是历史 bug，任何 `create_po()` / `HandlerToolBuilder::build()` 实现如果 `config = schema_json` 一律视为错误；单测 `test_handler_macro_po_fields_convention` 强制约束。
2. ✅ **sync_builtin_tools_to_db 永不覆盖 config**：UPDATE 语句里不能出现 `config = ?` 这种赋值（除非是未来显式设计的"清 config"操作，且有充分理由）。
3. ✅ **新增 ToolPo 便捷方法时统一 `.get("snake_case_key")` 风格**：与 `config_timeout_ms()` / `config_max_output_bytes()` / `cli_command()` 保持一致，不要强类型 struct 反序列化（config 是开放 JSON，未来可能不断加新键）。
4. ✅ **NoProgressPolicy 配置示例**：运维给 search_memory 写运行时限制时，config 应形如 `{"no_progress_max_calls": 15}`，schema 通过 sync 自动刷新到 parameters_schema。
5. ❌ **禁止在 description 里写内部实现细节或 ACL 枚举**：违反 §2.2 规则 1 和规则 2。description 读者是 LLM Agent，它不关心 DAO 层方法名或需要什么数据库角色。
6. ❌ **require_confirmation 场景禁止写 "blocked" / "denied"**：违反 §2.2 规则（require_confirmation 语义是请用户确认，不是硬拒绝）。已在 2026-09 重写中修正 fs_read / fs_write / shell_exec 的 description。
7. ❌ **neural 工具禁止省略调用时身份边界守卫**：违反 §4 约定 6。只靠 neural tag 自动加载不够——handler 必须自己决定 Agent 上下文能改啥、不能改啥。
