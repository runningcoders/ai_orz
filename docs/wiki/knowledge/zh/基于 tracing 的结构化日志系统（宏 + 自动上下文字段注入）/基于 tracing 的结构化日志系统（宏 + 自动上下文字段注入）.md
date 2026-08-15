---
kind: logging_system
name: 基于 tracing 的结构化日志系统（宏 + 自动上下文字段注入）
category: logging_system
scope:
    - '**'
source_files:
    - src/lib.rs
    - src/pkg/logging.rs
    - ai-orz-macros/src/log_fields.rs
    - common/src/config.rs
    - src/pkg/logging_test.rs
    - docs/design/logging_design.md
---

## 1. 使用的框架与工具

- **底层库**：`tracing` + `tracing-subscriber` + `tracing-appender`，采用 `EnvFilter` 做级别过滤。
- **输出目标**：控制台（`fmt::layer`）+ 可选的文件滚动（`rolling::daily`），文件通过 `non_blocking` 异步写入并由全局 `WorkerGuard` 持有至进程退出。
- **格式**：支持 text 与 JSON 两种格式，由配置项 `logging.format` 决定，默认 JSON。
- **日志轮转与清理**：按日滚动生成 `ai_orz.log.*` 文件；启动时根据 `logging.retention_days` 删除超过保留期的旧日志。
- **级别控制**：通过环境变量 `RUST_LOG`（`EnvFilter::try_from_default_env()`）覆盖，未设置时默认 `info`。

## 2. 关键文件与位置

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | 定义 `log_info!` / `log_warn!` / `log_error!` / `log_debug!` 四个统一宏及 `sys_*` 兼容别名 |
| `src/pkg/logging.rs` | `init(config)` 初始化 tracing subscriber、文件滚动、JSON/text 层；定义 `LogFields` trait |
| `ai-orz-macros/src/log_fields.rs` | `#[derive(LogFields)]` 过程宏，扫描标注 `#[log_field]` 的字段并生成 `create_log_span` 实现 |
| `common/src/config.rs` | `LoggingConfig { enable_file_log, log_subdir, format, retention_days }` 及默认值 |
| `src/pkg/logging_test.rs` | 针对带上下文日志的单元测试 |
| `docs/design/logging_design.md` | 日志系统设计文档，规定 API、匹配顺序、迁移规范与最佳实践 |

## 3. 架构与设计约定

### 3.1 统一宏入口，零成本抽象
所有业务代码统一使用 `log_info!` / `log_warn!` / `log_error!` / `log_debug!`，不再直接调用 `tracing::*!`。宏在编译期展开为对 `tracing` 的直接调用，无运行时开销。

### 3.2 自动上下文检测（核心机制）
每个宏提供两个分支，按优先级匹配：
1. **无上下文模式**：第一个参数是字符串字面量 → 直接 `tracing::info!(msg, ...)`。
2. **带上下文模式**：第一个参数是非字符串表达式（`&ctx` 或 `ctx.clone()`），第二个是字符串字面量（operation）→ 调用 `ctx.create_log_span(operation, level)` 创建 span 并 enter。

匹配顺序的关键是先用 `$msg:literal` 精确匹配字符串字面量，避免把 `"op", "msg"` 误判为 `(ctx, op)`。

### 3.3 结构化字段自动注入
通过 `#[derive(LogFields)]` 过程宏，结构体上标注 `#[log_field]` 的字段会被自动注入到 tracing span。当前主要应用于 `RequestContext`，自动注入的字段包括 `log_id`、`user_id`、`username`、`organization_id`、`agent_id`、`task_id`、`project_id`、`model_provider_id`、`model_name` 以及传入的 `operation`。

类型处理规则：
- `Option<String>` → `Some` 时输出值，`None` 时输出空串
- `String` → 直接输出
- `Option<T>`（非 String）→ Debug 输出
- 其他类型 → Display 输出

### 3.4 初始化流程
应用启动时 `pkg::init_all(&config)` 会调用 `logging::init(&config)`，根据 `AppConfig::logging` 决定是否启用文件日志、选择 JSON/text 格式、创建按日滚动的 `rolling::daily` 写入器，并将 `WorkerGuard` 放入全局 `OnceCell` 保证程序退出前 flush 完成。

### 3.5 日志路径策略
日志目录由 `config.base_data_path().join(config.logging.log_subdir)` 决定，默认 base path 为 `.ai_orz`，子目录为 `logs`，即默认输出到 `.ai_orz/logs/ai_orz.log.{date}`。

## 4. 约定与约束

- **业务代码必须使用 `log_*` 宏**，不得直接调用 `tracing::*!`（设计文档明确推荐）。
- **带上下文场景必须传 `&ctx` 引用**（或 `ctx.clone()`），operation 必须是字符串字面量（宏匹配要求）。
- **新增上下文字段只需在 struct 上加 `#[log_field]`**，无需修改日志调用点——这是单一数据源约定。
- **bin target 不能重新声明 mod**，必须通过 `ai_orz::run()` 调用 lib 入口，否则 `crate::` 路径在 bin/lib 两个 target 下解析到不同实例导致 trait 不匹配（设计文档中的前置依赖约束）。
- **日志级别**：INFO/WARN/ERROR/DEBUG 四级，TRACE 由 derive 宏生成但未被业务广泛使用；级别过滤通过 `RUST_LOG` 环境变量控制。
- **向后兼容**：`sys_info!` / `sys_warn!` / `sys_error!` / `sys_debug!` 仍可用，等价于无上下文模式的 `log_*`。
- **字符转义**：在 tracing 宏字符串中 `{`、`}` 需写成 `{{`、`}}` 以输出字面花括号。
- **测试覆盖**：`src/pkg/logging_test.rs` 覆盖了中文消息、特殊字符、长消息、多日志同上下文、结构化字段等场景。