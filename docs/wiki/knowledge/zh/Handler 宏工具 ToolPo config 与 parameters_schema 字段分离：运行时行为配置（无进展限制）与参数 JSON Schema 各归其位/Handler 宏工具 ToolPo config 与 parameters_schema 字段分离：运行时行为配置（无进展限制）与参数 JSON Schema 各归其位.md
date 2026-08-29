---
kind: rag_card
name: Handler 宏工具 ToolPo config 与 parameters_schema 字段分离：运行时行为配置（无进展限制）与参数 JSON Schema 各归其位
category: pkg层基础设施
scope:
- ai-orz-macros/src/lib.rs
- src/models/tool.rs
- src/pkg/tool_registry/**
- src/service/dal/tool.rs
- src/service/dao/tool/sqlite.rs
- src/service/domain/runtime/think_loop.rs
source_files:
- ai-orz-macros/src/lib.rs#L187-L210
- src/models/tool.rs#L98-L126
- src/models/tool.rs#L245-L280
- src/models/tool.rs#L333-L360
- src/service/dao/tool/sqlite.rs#L392-L422
- src/service/domain/runtime/think_loop.rs#L93-L113
- docs/wiki/zh/content/架构设计/分层架构设计/DAL 层组合.md
- docs/wiki/knowledge/zh/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法.md

---

# Handler 宏 ToolPo 字段分离

## §1 整体方案

`#[register_handler_tool]` 宏展开的 `create_po()` 必须把参数 JSON Schema 写入 `ToolPo.parameters_schema`（`Option<Value>`），`ToolPo.config`（`Value`）置 `Null`——config 留给运维写入运行时行为配置，parameters_schema 专门存参数 schema（供模型 function calling 使用）。

**历史 bug（已修复 a05ce051）**：旧版宏把 schema 误填到 config 位置、parameters_schema 留空。后果：`ToolDescriptor::from` 读 `po.parameters_schema` → None → 所有宏注册的 handler 工具发给模型的参数定义都是空 object `{"type":"object","properties":{}}`，模型只能盲猜参数名。执行链路虽然没坏（`HandlerToolAdapter` 按 params 类型直接反序列化，不经过 ToolPo schema），但模型-工具交互层被破坏。

**与 `HandlerToolBuilder::build` 对比**：手写注册的 `HandlerToolBuilder` 一直是正确写法（config = Null, parameters_schema = Some(schema)），宏只是 bug。

## §2 关键文件路径表格（读代码直接跳）

| 文件锚点 | 角色 | 核心契约 |
|---------|------|---------|
| [ai-orz-macros/src/lib.rs](ai-orz-macros/src/lib.rs#L187-L210) | 宏展开 ToolPo 构造 | schema_json → 第 6 参数位 parameters_schema（Some(schema_json)）；第 5 参数位 config = serde_json::Value::Null |
| [src/models/tool.rs](src/models/tool.rs#L98-L126) | ToolPo struct 定义 | `pub config: serde_json::Value`（运行时行为配置）+ `pub parameters_schema: Option<serde_json::Value>`（参数 JSON Schema） |
| [src/models/tool.rs](src/models/tool.rs#L245-L280) | ToolPo::new 签名 | 第 5 参数 config，第 6 参数 parameters_schema，顺序不能错 |
| [src/models/tool.rs](src/models/tool.rs#L333-L360) | 便捷读取方法 | `config_timeout_ms()` / `config_max_output_bytes()` / `config_no_progress_max_calls()` — 统一 `.get("snake_case_key")` 风格 |
| [src/service/dao/tool/sqlite.rs](src/service/dao/tool/sqlite.rs#L392-L422) | sync 不变量 | 每次启动 `sync_builtin_tools_to_db`：config 绝不写入（保留运维现场），parameters_schema 以代码为准刷新 |
| [src/service/domain/runtime/think_loop.rs](src/service/domain/runtime/think_loop.rs#L93-L113) | NoProgressPolicy 数据源 | 遍历 `agent.tools()` 读 `t.po.config_no_progress_max_calls()` 收集到 policy_set!，未配置的工具天然不限制 |
| 【平行卡】策略引擎卡 | NoProgressPolicy 实现 | [策略引擎](docs/wiki/knowledge/zh/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法.md) |

## §3 架构约定

1. **config 与 parameters_schema 语义严格分离**：config = 运行时行为配置（timeout_ms / max_output_bytes / no_progress_max_calls 等运维写入的键）；parameters_schema = 参数 JSON Schema（模型 function calling 消费，代码所有权）。
2. **sync_builtin_tools_to_db 的写入边界**：代码所有权字段（name/description/control_mode/parameters_schema/tags）→ 以代码为准刷新写入；运维所有权字段（config/status）→ 绝不写入，保留数据库现场。
3. **handler 工具 config 初始值必须为 Null**：宏展开时显式写 `serde_json::Value::Null`，禁止把 schema 或其他 JSON 填进 config；`fill_defaults_for_builtin()` 不覆盖已有的 config（已存在则保留）。
4. **NoProgressPolicy 数据源走 config_no_progress_max_calls() 便捷方法**：值为 0 或负数视为未配置（不限制），未配置的工具完全不受限，代码执行类高频工具天然免配。

## §4 约束清单

1. ❌ **禁止把参数 schema 写入 config**：这是历史 bug，任何 `create_po()` / `HandlerToolBuilder::build()` 实现如果 `config = schema_json` 一律视为错误；单测 `test_handler_macro_po_fields_convention` 强制约束。
2. ✅ **sync_builtin_tools_to_db 永不覆盖 config**：UPDATE 语句里不能出现 `config = ?` 这种赋值（除非是未来显式设计的"清 config"操作，且有充分理由）。
3. ✅ **新增 ToolPo 便捷方法时统一 `.get("snake_case_key")` 风格**：与 `config_timeout_ms()` / `config_max_output_bytes()` / `cli_command()` 保持一致，不要强类型 struct 反序列化（config 是开放 JSON，未来可能不断加新键）。
4. ✅ **NoProgressPolicy 配置示例**：运维给 search_memory 写运行时限制时，config 应形如 `{"no_progress_max_calls": 15}`，schema 通过 sync 自动刷新到 parameters_schema。
