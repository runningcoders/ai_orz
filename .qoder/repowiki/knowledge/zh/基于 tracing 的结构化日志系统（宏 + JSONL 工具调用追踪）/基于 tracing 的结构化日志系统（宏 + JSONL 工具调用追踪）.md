---
kind: logging_system
name: 基于 tracing 的结构化日志系统（宏 + JSONL 工具调用追踪）
category: logging_system
scope:
    - '**'
source_files:
    - docs/design/logging_design.md
    - src/pkg/logging.rs
    - src/lib.rs
    - ai-orz-macros/src/log_fields.rs
    - src/pkg/tool_tracing/mod.rs
    - src/pkg/tool_tracing/entry.rs
    - src/pkg/tool_tracing/logger.rs
    - common/src/config.rs
---

## 1. 整体方案

项目采用 `tracing` 作为底层日志框架，通过自定义宏与 derive 过程宏封装成统一的 `log_*!` 调用面；同时为工具调用（Tool Call）提供独立的 JSONL 持久化追踪子系统。日志输出同时支持控制台和按日期滚动的文件，默认 JSON 格式，支持保留天数清理。

## 2. 核心组件与文件

- **日志初始化与 sink**：`src/pkg/logging.rs` — 通过 `tracing_subscriber::registry()` 组合 `EnvFilter`、`fmt::layer`（控制台/文件）、`rolling::daily` 按日滚动写入 `{base_data_path}/logs/ai_orz.log.*`，使用 `tracing_appender::non_blocking` 异步写盘并全局持有 `WorkerGuard` 保证退出 flush。
- **统一日志宏**：`src/lib.rs` 中定义 `log_info! / log_warn! / log_error! / log_debug!` 四个宏，以及 `sys_*` 别名；两个分支匹配：
  - 无上下文模式：第一个参数是字符串字面量 → 直接转发到 `tracing::info!` 等。
  - 带上下文模式：`(ctx, "operation", fields...)` → 调用 `LogFields::create_log_span` 创建 span 再输出。
- **结构化字段注入**：`ai-orz-macros/src/log_fields.rs` 实现 `#[derive(LogFields)]`，扫描标注 `#[log_field]` 的字段，生成对应 level 的 `tracing::{error,warn,info,debug,trace}_span!("request", ...)`，自动注入 `operation = %operation`。
- **配置**：`common/src/config.rs` 中的 `LoggingConfig` 提供 `enable_file_log`、`format`（text/json）、`retention_days`、`log_subdir`，默认启用文件日志且格式为 json，保留 30 天。
- **工具调用追踪**：`src/pkg/tool_tracing/` 独立于通用日志，使用 `DailyJsonlWriter` 将 `ToolCallEntry` 写入 `{base_data_path}/tools/{tool_id}/call_trace/YYYYMMDD.jsonl`，提供查询 API（`query_calls`、`read_call_by_id`），并对 HTTP/MCP 外部工具的 input/output/error 做 `[REDACTED]` 脱敏。

## 3. 架构与约定

- **零成本抽象**：设计文档明确所有日志调用必须走宏，不直接调用 `tracing::*!`；宏在编译期展开，运行时等价于原生 tracing 调用。
- **自动上下文检测**：宏通过 `$msg:literal` vs `$ctx:expr` 语法匹配区分两种调用形式，避免手动选择不同宏。
- **RequestContext 字段自动注入**：通过 `#[derive(LogFields)]` + `#[log_field]` 声明式维护需要出现在 span 中的字段列表（如 `log_id`、`user_id`、`organization_id`、`agent_id`、`task_id`、`project_id`、`model_provider_id`、`model_name`），新增字段只需加注解，无需修改日志代码。
- **Span 命名**：所有带上下文日志创建的 span 统一命名为 `"request"`，并通过 `operation` 字段标识具体操作名（必须是字符串字面量）。
- **过滤层**：通过 `EnvFilter::try_from_default_env()` 读取环境变量控制级别，未设置时回退到 `info`。
- **bin target 约束**：要求只有一份编译产物（`main.rs` 调用 `ai_orz::run()` 而非重新声明 mod），否则 lib/bin 下 trait 实例不匹配导致 `LogFields` 无法解析。
- **工具调用追踪隔离**：Tool call 日志不走 tracing，而是独立 JSONL 文件，按 tool_id 分目录、按天分文件，便于单独分析工具性能与输入输出。

## 4. 约定与约束

- **调用规范**（来自 `docs/design/logging_design.md`）：
  - 业务代码统一使用 `log_*!` 宏，禁止直接调用 `tracing::*!`。
  - 带上下文场景必须传 `&ctx` 引用（或 `ctx.clone()`），第二个参数必须是字符串字面量的 operation 名。
  - operation 命名采用动词+名词（如 `create_memory`、`update_project`）。
  - 无上下文场景使用 `log_info!` 等；`sys_*` 系列仅用于系统级日志（向后兼容别名）。
- **字段类型处理规则**（derive 宏内实现）：`String` → `%self.field.as_str()`；`Option<String>` → `as_deref().unwrap_or("")`；`Option<T>`（非 String）→ `?self.field`（Debug）；其他 → `%self.field`（Display）。
- **日志清理**：启动时若 `retention_days > 0`，会删除 `ai_orz.log.*` 中修改时间超过保留期的文件。
- **工具调用脱敏**：HTTP/MCP 协议的 tool 的 input/output/error 一律替换为 `[REDACTED]`，Builtin 工具保留原值；路径校验拒绝包含 `/`、`\`、`.`、`..` 的 tool_id。
- **查询限制**：工具调用查询默认 limit 1，最大不超过 `MAX_TOOL_CALL_QUERY_LIMIT`（100）。
- **环境变量覆盖**：日志级别可通过标准 `RUST_LOG` 等 tracing 环境变量控制（由 `EnvFilter::try_from_default_env` 支持）。