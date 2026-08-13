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
    - common/src/config.rs
    - src/handlers/system/logs/query_logs.rs
    - src/service/dal/log_query.rs
    - src/pkg/logging_test.rs
    - docs/design/logging_design.md
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
| `src/pkg/logging_test.rs` | 日志模块单元测试（中文消息、特殊字符、结构化字段等） |

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

### 3.5 日志查询 API
`GET /api/v1/system/logs` 接收 `LogQueryRequest`（keyword、log_id、level、start_time、end_time、page、page_size），DAL 层逐行解析 JSONL 并按条件过滤，返回 `QueryLogsResponse { total, entries, page, page_size }`。该接口受 `require_role_middleware(UserRole::Admin)` 保护。

## 4. 约定与约束

- **必须使用 `log_*!` 宏**：设计文档明确要求业务代码统一使用 `log_*!`，禁止直接调用 `tracing::*!`，以保证上下文字段自动注入和统一格式。
- **带上下文调用必须传 `&ctx` 引用**：宏通过方法调用语法 `($ctx).create_log_span(...)` 接收 `&self`，传值会导致所有权转移；推荐写法是 `&ctx`，`ctx.clone()` 虽不会报错但多余。
- **operation 必须是字符串字面量**：宏用 `$op:literal` 匹配器区分模式，不能传入变量。
- **日志字段单一数据源**：新增上下文字段只需在 struct 定义处加 `#[log_field]` 注解，无需修改日志调用点。
- **bin target 不得重新声明 mod**：设计文档要求 main.rs 通过 `ai_orz::run()` 调用 lib 入口，避免 bin/lib 两个 target 重复编译导致 trait 版本不一致。
- **日志级别控制**：默认 `info`，可通过环境变量覆盖；生产默认 JSON 格式便于分析。
- **文件日志默认开启**：`enable_file_log` 默认为 true，日志目录默认 `.ai_orz/logs`，保留 30 天。
- **前端无日志**：Dioxus WASM 前端未集成 tracing，日志体系仅存在于 Rust 后端。