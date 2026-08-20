---
kind: logging_system
name: 基于 tracing 的结构化日志系统（宏 + 文件滚动 + JSONL 查询）
category: logging_system
scope:
    - '**'
source_files:
    - src/pkg/logging.rs
    - src/lib.rs
    - ai-orz-macros/src/log_fields.rs
    - ai-orz-macros/src/lib.rs
    - common/src/config.rs
    - src/handlers/system/logs/query_logs.rs
    - src/service/dal/log_query.rs
    - src/pkg/logging_test.rs
    - src/pkg/tool_tracing/mod.rs
    - src/pkg/tool_tracing/entry.rs
    - src/pkg/tool_tracing/logger.rs
    - src/pkg/daily_jsonl.rs
    - docs/design/logging_design.md
    - docs/wiki/zh/content/基础设施/日志系统.md
---

## 1. 使用的框架与工具

- **核心框架**：`tracing` + `tracing-subscriber` + `tracing-appender`。应用通过自定义宏统一暴露日志 API，底层全部转发到 tracing。
- **输出目标**：
  - 控制台层：`tracing_subscriber::fmt::layer()`，支持 text/json 两种格式。
  - 文件层：`tracing_appender::rolling::daily` 按日滚动生成 `ai_orz.log.YYYY-MM-DD` 文件，使用 `non_blocking` worker 异步写入并通过全局 `WorkerGuard` 保证进程退出前 flush。
- **过滤机制**：`EnvFilter`，默认级别 `info`，可通过环境变量覆盖（如 `RUST_LOG=...`）。
- **配置来源**：`common::config::LoggingConfig`（`enable_file_log`、`format`、`log_subdir`、`retention_days`），路径由 `AppConfig::log_dir()` 解析到 `{base_data_path}/logs`。
- **日志查询**：`src/service/dal/log_query.rs` 直接扫描 JSONL 日志文件，提供 `GET /api/v1/system/logs` 管理端点（需 Admin/SuperAdmin 角色）。

## 2. 关键文件与包

| 文件 | 职责 |
|---|---|
| `src/pkg/logging.rs` | `init(config)` 初始化 tracing registry；定义 `LogFields` trait；实现旧日志清理 |
| `src/lib.rs` | 导出 `log_info!`/`log_warn!`/`log_error!`/`log_debug!` 四个宏及 `sys_*` 兼容别名 |
| `ai-orz-macros/src/log_fields.rs` | `#[derive(LogFields)]` 过程宏，扫描 `#[log_field]` 字段并生成带 span 字段的 `create_log_span` |
| `common/src/config.rs` | `LoggingConfig` 结构体及默认值（默认 JSON 格式、保留 30 天、启用文件日志） |
| `src/handlers/system/logs/query_logs.rs` | HTTP handler，调用 DAL 查询日志 |
| `src/service/dal/log_query.rs` | 读取 JSONL 日志文件，支持 keyword/log_id/level/时间范围分页过滤 |
| `src/pkg/logging_test.rs` | 日志模块单元测试（中文消息、特殊字符、结构化字段、多日志同上下文等场景） |
| `ai-orz-macros/src/lib.rs` | 导出 `LogFields` derive 宏 |
| `src/pkg/tool_tracing/mod.rs` | 工具调用追踪模块入口 |
| `src/pkg/tool_tracing/logger.rs` | 基于 `DailyJsonlWriter` 的每日 JSONL 文件持久化工具调用记录，提供查询 API |
| `src/pkg/tool_tracing/entry.rs` | `ToolCallEntry` 结构体与外部协议工具的 trace 脱敏函数 `redact_trace_values_for_tool` |
| `src/pkg/daily_jsonl.rs` | 通用每日 JSONL 写入器（被工具追踪复用） |

## 3. 架构与设计约定

### 3.1 统一宏入口，零成本抽象
所有业务代码只通过 `log_info!`/`log_warn!`/`log_error!`/`log_debug!` 写日志，不直接调用 `tracing::*!`。宏通过 `macro_rules!` 模式匹配自动区分两种调用形式：
- 无上下文：`log_info!("message", fields...)` → 直接 `tracing::info!`
- 带上下文：`log_info!(&ctx, "operation", fields...)` → 先构造 span，再进入后写日志

匹配顺序严格优先字符串字面量（避免 `log_info!("op", "msg")` 被误判为带上下文模式）。

### 3.2 上下文字段自动注入
通过 `#[derive(LogFields)]` 在 `RequestContext` 上标注 `#[log_field]` 的字段，自动生成 `impl LogFields`，在创建 span 时把 `log_id`、`user_id`、`username`、`organization_id`、`agent_id`、`task_id`、`project_id`、`model_provider_id`、`model_name`、`operation` 等字段注入到每个请求级 span。类型处理规则：
- `String` → `%self.field.as_str()`
- `Option<String>` → `%self.field.as_deref().unwrap_or("")`
- `Option<T>`（非 String）→ `?self.field`（Debug 格式）
- 其他 → `%self.field`（Display 格式）

### 3.3 启动流程中的初始化顺序
`src/lib.rs::run()` 中，`pkg::init_all(&config)` 会调用 `logging::init(&config)`，在 service/producer/consumer 注册之前完成，确保后续所有组件都能使用统一的 tracing 子系统。

### 3.4 日志文件格式与持久化
- 文件命名：`ai_orz.log.YYYY-MM-DD`（tracing-appender daily rolling）
- 每行一个 JSON 对象（tracing-subscriber JSON 格式），包含 `timestamp`、`level`、`fields`（含 message/log_id/user_id/operation）、`target`、`filename`、`line_number`
- 启动时根据 `retention_days > 0` 清理超过保留期的旧日志文件
- 查询时最多扫描最近 30 天的文件（`MAX_SCAN_DAYS`），单次查询最多收集 10000 条记录（`MAX_SCAN_ENTRIES`）防止内存溢出

### 3.5 工具调用追踪子系统（独立于应用日志）

`tool_tracing` 模块是独立于 `tracing` 应用日志的专用追踪子系统，用于持久化工具调用的输入/输出/错误：
- 存储路径：`{base_data_path}/tools/{tool_id}/call_trace/{YYYYMMDD}.jsonl`
- 每条记录是一个 `ToolCallEntry`，包含 `call_id`、`tool_id`、`started_at`、`finished_at`、`duration_ms`、`input`、`output`、`error`、`status`、`metadata`
- 通过 `DailyJsonlWriter` 按日追加写入
- 对外部协议工具（HTTP/MCP）的 input/output/error 默认脱敏为 `[REDACTED]`，内置工具保留原值
- 提供 `query_calls(ToolCallQuery)` 支持按 call_id、agent_id、project_id、task_id、tool_id、status、时间范围查询，限制最大返回 100 条

### 3.6 日志查询 API
`GET /api/v1/system/logs` 接收 `LogQueryRequest`（keyword、log_id、level、start_time、end_time、page、page_size），DAL 层逐行解析 JSONL 并按条件过滤，返回 `QueryLogsResponse { total, entries, page, page_size }`。该接口受 `require_role_middleware(UserRole::Admin)` 保护。

## 4. 约定与约束

1. **必须使用 `log_*!` 宏**：设计文档明确要求业务代码统一使用 `log_*!`，禁止直接调用 `tracing::*!`，以保证上下文字段自动注入和统一格式。
2. **带上下文调用必须传 `&ctx` 引用**：宏通过方法调用语法 `($ctx).create_log_span(...)` 接收 `&self`，传值会导致所有权转移；推荐写法是 `&ctx`，`ctx.clone()` 虽不会报错但多余。
3. **operation 必须是字符串字面量**：宏用 `$op:literal` 匹配器区分模式，不能传入变量。
4. **operation 命名采用动词+名词**（如 `create_memory`、`update_project`）。
5. **日志字段单一数据源**：新增上下文字段只需在 struct 定义处加 `#[log_field]` 注解，无需修改日志调用点。
6. **bin target 不得重新声明 mod**：设计文档要求 main.rs 通过 `ai_orz::run()` 调用 lib 入口，避免 bin/lib 两个 target 重复编译导致 trait 版本不一致。
7. **日志级别控制**：默认 `info`，可通过环境变量覆盖；生产默认 JSON 格式便于分析。
8. **文件日志默认开启**：`enable_file_log` 默认为 true，日志目录默认 `.ai_orz/logs`，保留 30 天。
9. **前端无日志**：Dioxus WASM 前端未集成 tracing，日志体系仅存在于 Rust 后端。
10. **向后兼容**：`sys_info!` / `sys_warn!` / `sys_error!` / `sys_debug!` 仍可用，等价于无上下文模式的 `log_*`。
11. **字符转义**：在 tracing 宏字符串中 `{`、`}` 需写成 `{{`、`}}` 以输出字面花括号。
12. **工具调用追踪的 `tool_id` 不允许包含 `/`、`\`、`.`、`..`**，防止路径穿越。
13. **工具调用脱敏**：HTTP/MCP 协议的 tool call trace 默认对 input/output/error 脱敏为 `[REDACTED]`，防止敏感数据泄露。
14. **工具调用查询限制**：默认 limit 1，最大不超过 `MAX_TOOL_CALL_QUERY_LIMIT`（100），查询结果按 `started_at` 降序排序。
15. **禁止混用 `sys_*` 与 `log_*`**（除非确实是系统级无上下文日志）。
16. **日志文件命名固定为 `ai_orz.log.*`**，清理逻辑只处理该前缀的文件。
17. **环境变量覆盖**：日志级别可通过标准 `RUST_LOG` 等 tracing 环境变量控制（由 `EnvFilter::try_from_default_env` 支持）。
18. **新增 RequestContext 字段需加 `#[log_field]` 注解**，才能自动注入到 span；字段列表是唯一数据源。