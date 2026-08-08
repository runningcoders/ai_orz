---
kind: logging_system
name: 基于 tracing 的结构化日志系统（宏 + 文件滚动 + JSONL 工具追踪）
category: logging_system
scope:
    - '**'
source_files:
    - src/pkg/logging.rs
    - common/src/config.rs
    - docs/logging_design.md
    - ai-orz-macros/src/log_fields.rs
    - src/pkg/tool_tracing/logger.rs
    - src/pkg/daily_jsonl.rs
---

## 1. 使用的框架与整体方案

- **核心库**：`tracing` + `tracing-subscriber` + `tracing-appender`。所有业务日志统一通过项目自研的 `log_*` / `sys_*` 宏发出，底层直接调用 `tracing`。
- **输出目标**：
  - 控制台层：`tracing_subscriber::fmt::layer()`，默认开启 target、file、line_number。
  - 文件层：`tracing_appender::rolling::daily` 按天滚动写入 `{base_data_path}/logs/ai_orz.log.*`，并通过 `non_blocking` 异步写入，由全局 `OnceCell<WorkerGuard>` 持有直到进程退出。
- **格式**：支持 `text` 与 `json` 两种格式，由配置项 `logging.format` 决定；默认值为 `json`。
- **过滤级别**：通过 `EnvFilter` 从环境变量读取（如 `RUST_LOG`），未设置时回退到 `info`。
- **日志清理**：启动时根据 `logging.retention_days` 扫描日志目录，删除修改时间超过保留期的 `ai_orz.log.*` 文件。
- **工具调用追踪**：独立子系统 `src/pkg/tool_tracing/logger.rs`，使用 `DailyJsonlWriter` 将每次工具调用以 JSONL 行写入 `{base_data_path}/tools/{tool_id}/call_trace/{YYYYMMDD}.jsonl`，并提供按 call_id / agent_id / project_id / task_id / tool_id / status / 时间范围查询的能力。

## 2. 关键文件与位置

| 文件 | 职责 |
|---|---|
| `src/pkg/logging.rs` | 日志子系统初始化（`init`）、文件滚动、JSON/text 双格式、日志清理、`LogFields` trait 定义 |
| `common/src/config.rs` | `LoggingConfig`（`enable_file_log`、`log_subdir`、`format`、`retention_days`）、`AppConfig::log_dir()` |
| `docs/logging_design.md` | 日志系统设计文档，规定宏 API、匹配顺序、上下文字段、迁移指南与最佳实践 |
| `ai-orz-macros/src/log_fields.rs` | `#[derive(LogFields)]` 过程宏，为标注 `#[log_field]` 的字段生成 `create_log_span` 实现 |
| `src/pkg/tool_tracing/logger.rs` | 工具调用追踪单例 `ToolCallLogger`，JSONL 写入与查询 |
| `src/pkg/daily_jsonl.rs` | 通用每日 JSONL 写入器（被工具追踪复用） |

## 3. 架构与约定

### 3.1 统一日志宏（零成本抽象）

- 暴露 `log_info!` / `log_warn!` / `log_error!` / `log_debug!` 四个主宏，以及 `sys_info!` / `sys_warn!` / `sys_error!` / `sys_debug!` 别名。
- **自动上下文检测**：宏通过语法模式匹配区分两种调用形式：
  - 第一个参数是字符串字面量 → 无上下文模式，直接透传至 `tracing::xxx!`。
  - 第一个参数是表达式、第二个是字符串字面量 → 带上下文模式，先构造 span 再记录。
- 匹配顺序严格：**优先匹配无上下文模式**，避免 `log_info!("op", "msg")` 被误判为带上下文。
- 带上下文模式下，通过 `LogFields` trait（由 `#[derive(LogFields)]` 生成）把 `RequestContext` 中标注 `#[log_field]` 的业务字段注入 span，包括 `log_id`、`user_id`、`username`、`organization_id`、`agent_id`、`task_id`、`project_id`、`model_provider_id`、`model_name` 等。
- operation 必须是字符串字面量（宏用 `$op:literal` 匹配），用于标识当前操作名。

### 3.2 运行时初始化流程

应用启动时调用 `pkg::logging::init(&config)`：
1. 解析 `config.logging.format` 决定 JSON 或文本格式。
2. 构建 `EnvFilter` 作为第一层过滤。
3. 若 `enable_file_log == true`：创建 `rolling::daily` 按日滚动 appender，包装为 `non_blocking` writer，同时注册 console layer 与 file layer，并将 `WorkerGuard` 存入全局 `OnceCell`。
4. 否则仅注册 console layer。
5. 若 `retention_days > 0`，启动时清理过期日志文件。

### 3.3 工具调用追踪子系统

- 独立的 `ToolCallLogger` 单例，通过 `init(base_data_path)` 初始化后全局可用。
- 每个工具一个子目录：`{base_data_path}/tools/{tool_id}/call_trace/{YYYYMMDD}.jsonl`。
- 每条记录为 `ToolCallEntry`（含 call_id、agent_id、project_id、task_id、tool_id、status、started_at、finished_at 等），序列化为一行 JSON。
- 提供 `query_calls(ToolCallQuery)` 支持多条件过滤，并限制最大返回条数 `MAX_TOOL_CALL_QUERY_LIMIT = 100`。

### 3.4 配置来源

日志行为完全由 `AppConfig.logging` 控制：
- `enable_file_log`：是否启用文件日志（默认 `true`）。
- `log_subdir`：日志子目录（默认 `logs`）。
- `format`：`text` 或 `json`（默认 `json`）。
- `retention_days`：保留天数（默认 `30`，`0` 表示不清理）。
- 日志根路径由 `AppConfig::base_data_path()` 决定，可通过环境变量 `AI_ORZ_BASE_PATH` 覆盖。

## 4. 约定与约束

- **业务代码必须使用 `log_*` 宏**，不直接调用 `tracing::*!`（设计文档明确推荐）。
- **带上下文场景必须传入 `&ctx` 引用**（而非值），因为宏通过方法调用 `($ctx).create_log_span(...)` 接收 `&self`，避免所有权转移。
- **operation 必须是字符串字面量**，这是宏区分上下文模式的语法级约束（`$op:literal`）。
- **禁止混用 `sys_*` 与 `log_*`**（除非确实是系统级无上下文日志）。
- **新增 RequestContext 字段需加 `#[log_field]` 注解**，才能自动注入到 span；字段列表是唯一数据源。
- **bin target 不能重新声明 mod**，否则 `crate::` 路径在 lib/bin 两个 target 下解析到不同实例导致 trait 不匹配（设计文档中的前置依赖约束）。
- 工具调用追踪的 `tool_id` 不允许包含 `/`、`\`、`.`、`..`，防止路径穿越。
- 日志文件命名固定为 `ai_orz.log.*`，清理逻辑只处理该前缀的文件。
- 工具调用查询结果按 `started_at` 降序排序，且最多返回 `limit` 条（默认 1，上限 100）。