> 📦 归档标记（2026-08-15）：被 [基于 tracing 的结构化日志系统（宏 + 文件滚动 + JSONL 查询）](docs/wiki/knowledge/zh/基于 tracing 的结构化日志系统（宏 + 文件滚动 + JSONL 查询）/基于 tracing 的结构化日志系统（宏 + 文件滚动 + JSONL 查询）.md) 取代。保留原因：历史参考，主卡已吸收本卡独有源码锚点与硬约束。生效方案：主卡真实路径作为唯一 RAG 召回目标。
---
kind: logging_system
name: 基于 tracing 的结构化日志系统（宏 + 文件滚动 + JSONL 工具调用追踪）
category: logging_system
scope:
    - '**'
source_files:
    - src/pkg/logging.rs
    - ai-orz-macros/src/log_fields.rs
    - ai-orz-macros/src/lib.rs
    - common/src/config.rs
    - docs/design/logging_design.md
    - src/pkg/tool_tracing/mod.rs
    - src/pkg/tool_tracing/logger.rs
    - src/pkg/tool_tracing/entry.rs
---

## 1. 系统与框架

项目采用 `tracing` 作为统一日志基础设施，通过 `tracing-subscriber` 组装输出层，使用 `tracing-appender` 的 `rolling::daily` 实现按日滚动文件输出。日志同时写入控制台与文件，支持文本与 JSON 两种格式，默认启用 JSON 格式以便分析。

- **核心库**：`tracing`, `tracing-subscriber`, `tracing-appender`
- **过滤机制**：`EnvFilter`，从环境变量读取（默认 `info` 级别），可通过 `RUST_LOG` 等覆盖
- **全局初始化入口**：`src/pkg/logging.rs` 中的 `init(&AppConfig)`
- **配置来源**：`common/src/config.rs` 的 `LoggingConfig`（`enable_file_log`、`log_subdir`、`format`、`retention_days`），路径由 `AppConfig::log_dir()` 解析到 `{base_data_path}/logs`

## 2. 关键文件与包

| 文件 | 职责 |
|---|---|
| `src/pkg/logging.rs` | 日志子系统初始化、控制台/文件双 sink、JSON 格式切换、旧日志清理、`LogFields` trait 定义 |
| `ai-orz-macros/src/log_fields.rs` | `#[derive(LogFields)]` 过程宏，扫描 `#[log_field]` 字段生成 `create_log_span` |
| `ai-orz-macros/src/lib.rs` | 导出 `LogFields` derive；文档中说明日志宏定义位于 `src/lib.rs`（当前仓库未包含该宏定义源码，但通过设计文档可知存在统一的 `log_info!` / `log_warn!` / `log_error!` / `log_debug!` 四个宏及 `sys_*` 别名） |
| `common/src/config.rs` | `LoggingConfig` 结构体与默认值（默认 `json` 格式、30 天保留、`logs` 子目录） |
| `src/pkg/tool_tracing/mod.rs` | 工具调用追踪模块入口 |
| `src/pkg/tool_tracing/logger.rs` | 基于 `DailyJsonlWriter` 的每日 JSONL 文件持久化工具调用记录，提供查询 API |
| `src/pkg/tool_tracing/entry.rs` | `ToolCallEntry` 结构体与外部协议工具的 trace 脱敏函数 `redact_trace_values_for_tool` |

## 3. 架构与设计约定

### 3.1 日志宏与上下文自动注入

日志调用统一通过宏完成（零成本抽象），宏根据第一个参数是否为字符串字面量自动区分两种模式：
- **无上下文模式**：`log_info!("message")`，直接透传到 `tracing::info!`
- **带上下文模式**：`log_info!(&ctx, "operation", fields...)`，通过 `RequestContext.create_log_span(operation, level)` 创建命名 span，再进入 span 后输出消息

`LogFields` derive 宏会扫描结构体上标注 `#[log_field]` 的字段，为每个字段类型选择合适格式化方式：
- `String` → `%self.field.as_str()`
- `Option<String>` → `%self.field.as_deref().unwrap_or("")`
- `Option<T>`（非 String）→ `?self.field`（Debug 输出）
- 其他类型 → `%self.field`（依赖 Display）

生成的 span 名称固定为 `request`，并附带所有标注字段以及 `operation = %operation`。

### 3.2 输出层配置

`init` 流程：
1. 读取 `config.logging.format`，判断是否 JSON
2. 构建 `EnvFilter`（默认 `info`）
3. 若 `enable_file_log == true`：
   - 确保 `config.log_dir()` 存在
   - 若 `retention_days > 0`，启动时清理超过保留期的 `ai_orz.log.*` 文件
   - 使用 `rolling::daily(logs_dir, "ai_orz.log")` 创建按日滚动的文件 appender
   - 通过 `non_blocking` 异步写入，`WorkerGuard` 通过 `OnceCell` 全局持有保证退出前 flush
4. 分别构建 console layer 和 file layer（均开启 `with_target(true)`、`with_file(true)`、`with_line_number(true)`），注册到 `tracing_subscriber::registry()`
5. 若禁用文件日志，仅注册 console layer

### 3.3 工具调用追踪（独立于应用日志）

`tool_tracing` 模块是独立于 `tracing` 应用日志的专用追踪子系统，用于持久化工具调用的输入/输出/错误：
- 存储路径：`{base_data_path}/tools/{tool_id}/call_trace/{YYYYMMDD}.jsonl`
- 每条记录是一个 `ToolCallEntry`，包含 `call_id`、`tool_id`、`started_at`、`finished_at`、`duration_ms`、`input`、`output`、`error`、`status`、`metadata`
- 通过 `DailyJsonlWriter` 按日追加写入
- 对外部协议工具（HTTP/MCP）的 input/output/error 默认脱敏为 `[REDACTED]`，内置工具保留原值
- 提供 `query_calls(ToolCallQuery)` 支持按 call_id、agent_id、project_id、task_id、tool_id、status、时间范围查询，限制最大返回 100 条

## 4. 约定与约束

- **统一宏调用**：业务代码应使用 `log_info!` / `log_warn!` / `log_error!` / `log_debug!`，不直接调用 `tracing::*!`（设计文档明确规范）
- **带上下文必须传引用**：`log_info!(&ctx, "operation", ...)`，宏通过方法调用语法处理借用，避免 ctx 所有权转移
- **operation 必须是字符串字面量**：宏使用 `$op:literal` 匹配器区分上下文模式，不能传变量
- **日志字段单一数据源**：在 struct 定义处用 `#[log_field]` 标注需要注入的字段，新增字段只需加注解
- **日志级别控制**：通过环境变量（`RUST_LOG` 等）配合 `EnvFilter` 动态调整，默认 `info`
- **JSON 格式默认**：`LoggingConfig::default()` 中 `format` 默认为 `"json"`，便于结构化分析
- **文件保留策略**：`retention_days` 默认 30，0 表示不清理；启动时清理以 `ai_orz.log.` 开头的过期文件
- **bin target 唯一性**：设计文档要求只有一份编译产物（main.rs 通过 `ai_orz::run()` 复用 lib），否则 derive 宏生成的 `crate::pkg::logging::LogFields` 在 bin/target 下解析到不同实例导致 trait 不匹配
- **工具调用脱敏**：HTTP/MCP 协议的 tool call trace 默认对 input/output/error 脱敏，防止敏感数据泄露
- **日志路径集中管理**：所有日志文件位于 `AppConfig::base_data_path()` 下的 `logs` 子目录，通过 `config.log_dir()` 访问