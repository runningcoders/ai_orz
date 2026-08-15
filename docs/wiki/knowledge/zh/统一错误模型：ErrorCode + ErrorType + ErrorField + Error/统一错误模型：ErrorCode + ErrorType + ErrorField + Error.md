---
kind: error_handling
name: 统一错误模型：ErrorCode + ErrorType + ErrorField + Error
category: error_handling
scope:
    - '**'
source_files:
    - common/src/error/mod.rs
    - common/src/error/types.rs
    - common/src/error/code.rs
    - common/src/error/macros.rs
    - common/src/error/axum.rs
    - common/src/error_test.rs
    - docs/design/common-error-type.md
---

## 1. 系统/方案概述

项目采用在 `common` crate 中集中定义的**统一错误模型**，作为后端 Axum HTTP、Domain/DAL/DAO、工具执行、AOP 事件与前端共享的错误契约。核心由四个层次组成：

- `ErrorType`：粗粒度分类（Validation / Biz / Auth / Permission / Db / Io / Third / Tool / Runtime / Network / Config / System），用于过滤、统计、告警。
- `ErrorCode`：通过 `define_error_codes!` 宏生成的纯单位 enum，每个 variant 绑定一个稳定的字符串 code（如 `"resource_not_found"`）、HTTP status（如 404）和默认 `ErrorType`。
- `ErrorField`：可序列化的结构化业务上下文（`trace_ref` + 动态 `Map<String, Value>`），仅放安全字段，不暴露 token/password/stack trace。
- `Error`：统一错误结构体，携带 `code + error_type + msg + field + source(Arc<anyhow::Error>)`，实现 `std::error::Error`、`Serialize/Deserialize`、`Display`，并提供 `bad_request/not_found/conflict/internal/tool_call_failed/unauthorized/forbidden` 等便捷构造器。

该模型通过 `common/src/error/mod.rs` 暴露为 `pub use {err, bail_err, ensure_err, ErrorCode, Error, ErrorField}`，并在启用 `axum-integration` feature 时提供 `impl IntoResponse for Error`，将错误直接映射为 `{code: i32, error_code: str, message: str, data: null}` JSON 响应。

## 2. 关键文件与包

- `common/src/error/mod.rs` — 模块入口，声明 `Result<T>` 别名并 re-export 所有公共 API。
- `common/src/error/types.rs` — `ErrorType`、`ErrorField`、`Error` 定义及第三方类型到 `Error` 的 `From` 转换（sqlx、io、anyhow、tokio JoinError、reqwest、serde_json、bincode、jsonwebtoken、base64、toml）。
- `common/src/error/code.rs` — `define_error_codes!` 调用点，集中声明全部业务错误码。
- `common/src/error/macros.rs` — `err!` / `bail_err!` / `ensure_err!` / `define_error_codes!` 四个宏，支持 inline json `field: {k:v}`、`source:`、显式 `error_type` 覆盖等多种语法。
- `common/src/error/axum.rs` — `IntoResponse` 实现，将 `Error` 转为 Axum 响应。
- `common/src/error_test.rs` — 针对错误模型的契约测试，验证 code_str、http_status、error_type、field、source 等行为。
- `docs/design/common-error-type.md` — 设计文档，明确分层使用规范与 HTTP 响应格式约定。

## 3. 架构与约定

### 3.1 错误产生层
- **Handler**：参数解析失败 → `InvalidRequest`（Validation, 400）；其他透传 Domain 错误。
- **Domain**：主要错误产生层，按语义选择 `ErrorCode` + `ErrorType`，可附加 `ErrorField`（如 tool_id、control_mode）。
- **DAL/DAO**：DAO 层建议保留独立 `DaoError`，由 DAL 映射为统一 `Error`；持久化错误映射为 `DbQueryFailed`（Db, 500）。
- **工具/运行时**：工具协议/参数错误 → `ToolParameterInvalid` / `ToolExecutionFailed`；Agent 唤醒失败 → `RuntimeAwakenFailed`。

### 3.2 错误传播链
- 内部底层异常通过 `with_source` 或 `From<E>` 自动转换挂入 `Error.source`，序列化时被跳过，不泄露给外部。
- 业务上下文通过 `ErrorField` 传递，支持 `trace_ref` 关联工具调用追踪。
- 上层函数统一返回 `common::error::Result<T>`，通过 `?` 配合 `From` 实现自动提升。

### 3.3 HTTP 适配
- 启用 `axum-integration` 后，任何 `Error` 可直接作为 handler 返回值，`IntoResponse` 根据 `ErrorCode.http_status()` 生成对应 HTTP 状态码，body 固定为 `{code, error_code, message, data: null}`。
- 未识别的 http status 会回退到 500。

### 3.4 宏约定
- `err!(Variant, "msg")`：最简构造。
- `err!(Variant, ErrorType, "msg")`：覆盖默认 `ErrorType`。
- `err!(Variant, "msg", field: {k: v})`：内联 JSON 字段。
- `err!(Variant, "msg", source: e)`：附带底层错误源。
- `bail_err!`：等价于 `return Err(err!(...))`。
- `ensure_err!(cond, Variant, ...)`：条件校验失败即返回错误。

## 4. 约定与约束

- **禁止直接使用裸 `anyhow::Result` 作为对外 API**：业务代码内部统一使用 `common::error::Result`；仅在实现外部 trait 要求时使用 `anyhow::Result`，并在边界处转换为 `Error`。
- **`ErrorField` 只放可公开的安全字段**：不得包含 token、password、secret、完整 SQL、原始 stack trace；内部错误链必须放入 `source`。
- **新增错误码必须通过 `define_error_codes!` 注册**：每个 variant 需声明 `type`、`http`、`code` 三元组，确保 `error_type()`、`http_status()`、`code_str()` 始终一致。
- **HTTP 响应格式固定**：错误响应 body 必须为 `{code: i32, error_code: str, message: str, data: null}`，保持与前端兼容。
- **中间件职责**：认证失败走 `Unauthorized`（Auth, 401），权限不足走 `Forbidden`（Permission, 403），均由 `src/middleware/jwt_auth.rs`、`require_role.rs` 等中间件产生统一 `Error`。
- **测试保障**：`common/src/error_test.rs` 对 `ErrorCode` 元数据、`err!`/`bail_err!`/`ensure_err!`、`ErrorField`、`with_source` 进行契约级断言，新增错误码需同步补充测试。