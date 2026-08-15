> 📦 归档标记（2026-08-15）：被 [统一错误模型：ErrorCode + ErrorType + ErrorField 的跨层错误处理体系](docs/wiki/knowledge/zh/统一错误模型：ErrorCode + ErrorType + ErrorField 的跨层错误处理体系/统一错误模型：ErrorCode + ErrorType + ErrorField 的跨层错误处理体系.md) 取代。保留原因：历史参考，主卡已吸收本卡独有源码锚点与硬约束。生效方案：主卡真实路径作为唯一 RAG 召回目标。
---
kind: error_handling
name: 统一错误模型：common::error 与 ErrorCode/ErrorType/ErrorField
category: error_handling
scope:
    - '**'
source_files:
    - common/src/error/mod.rs
    - common/src/error/code.rs
    - common/src/error/types.rs
    - common/src/error/macros.rs
    - common/src/error/axum.rs
    - src/middleware/jwt_auth.rs
    - src/service/dao/lark/error.rs
    - docs/design/common-error-type.md
---

## 1. 系统/方案概述

项目采用在 `common` crate 中定义的**统一类型化错误模型**，作为后端服务、DAO/DAL、A2A 回调、工具执行以及前端共享的错误契约。核心由四个层次组成：

- `ErrorType`：粗粒度分类（Validation / Biz / Auth / Permission / Db / Io / Third / Tool / Runtime / Network / Config / System），用于过滤、统计、告警。
- `ErrorCode`：通过 `define_error_codes!` 宏生成的纯单位 enum，每个 variant 绑定一个稳定的字符串码（如 `"resource_not_found"`）、HTTP 状态码和默认 `ErrorType`。
- `ErrorField`：结构化业务上下文，内部使用 `serde_json::Map<String, Value>` 平铺序列化，并支持 `trace_ref: Option<ToolCallTraceRef>` 字段以携带工具调用追踪引用。
- `Error`：统一错误结构体，包含 `code + error_type + msg + field + source`，实现 `Serialize/Deserialize` 与 `std::error::Error`，并提供 `bad_request/unauthorized/not_found/conflict/internal/tool_call_failed/db_error/io_error/payload_too_large` 等便捷构造器。

该模块通过 Cargo feature 开关集成第三方库的 `From<E>` 转换：`sqlx`、`tokio`、`reqwest`、`serde_json`、`bincode`、`jsonwebtoken`、`base64`、`toml`，使各层可直接用 `?` 将底层错误提升为统一 `Error`。

HTTP 适配层在 `common/src/error/axum.rs` 中实现 `IntoResponse for Error`，将 `Error` 直接转为 `(StatusCode, Json<...>)`，响应体格式固定为 `{ code: i32, error_code: &str, message: String, data: null }`，其中 `code` 取自 `ErrorCode::http_status()`。

## 2. 关键文件与包

- `common/src/error/mod.rs`：模块入口，重导出 `ErrorCode`、`Error`、`ErrorField`、`ErrorType`、`Result<T>` 及 `err!/bail_err!/ensure_err!/define_error_codes!` 宏。
- `common/src/error/code.rs`：集中声明所有 `ErrorCode` 变体及其元数据（type/http/code）。
- `common/src/error/types.rs`：`ErrorType`、`ErrorField`、`Error` 定义与所有 `From<E>` 转换实现。
- `common/src/error/macros.rs`：`err!`、`bail_err!`、`ensure_err!`、`define_error_codes!` 宏实现。
- `common/src/error/axum.rs`：`impl IntoResponse for Error`，Axum 路由层零样板错误返回。
- `src/middleware/jwt_auth.rs`：认证中间件，失败时根据请求特征返回 302 重定向或 401 JSON（兼容旧 `ApiResponse` 格式）。
- `src/service/dao/lark/error.rs`：DAO 层局部错误类型示例，提供 `LarkResponse.into_result/check`、`From<LarkWsError>` 等，将第三方错误映射为 `ThirdPartyError`。
- `docs/design/common-error-type.md`：设计文档，明确分层职责、HTTP 响应格式与迁移阶段。

## 3. 架构与约定

### 3.1 错误产生与传播路径

| 层级 | 职责 | 约定 |
|---|---|---|
| Handler（`src/handlers/*`） | HTTP 入口、参数解析 | 参数校验失败 → `Validation` + `InvalidRequest`；Domain 错误透传 |
| Domain（`src/service/domain/*`） | 业务规则检查 | 主要错误产生层，按语义选择 `ErrorCode` + `ErrorType`，可附加 `ErrorField` |
| DAL（`src/service/dal/*`） | PO ↔ Entity 转换、组合 DAO | 使用 `err!`/`bail_err!`/`ensure_err!` 包装 DAO 错误，按语义映射为 `Db`/`Biz` 等 |
| DAO（`src/service/dao/*`） | 本地 DB CRUD + 外部 API 出站 | 建议 DAO 层使用独立 `DaoError`（如 `LarkWsError`），由 DAL 映射为统一 `Error` |
| pkg（`src/pkg/*`） | 业务无感知基础设施 | 通过 `From<E>` 自动转换底层错误（如 `anyhow::Error` → `System`） |

### 3.2 错误构造约定

- 优先使用 `err!(Variant, "msg", ...)` 而非手写 `Error::new(...)`。
- 条件校验使用 `ensure_err!(cond, Variant, "msg")`，避免嵌套 if。
- 提前返回使用 `bail_err!(Variant, "msg")`，等价于 `return Err(err!(...))`。
- 需要附带结构化上下文时使用 `field: { key: value }` 语法（宏内联 JSON）。
- 需要保留底层错误链时使用 `source: $source` 参数。
- 快捷构造器：`Error::bad_request/unauthorized/not_found/conflict/internal/tool_call_failed/db_error/io_error/payload_too_large`。

### 3.3 HTTP 响应约定

- Axum handler 返回 `common::error::Result<T>` 即可，`Error` 通过 `IntoResponse` 自动转换为 JSON 错误体。
- 认证中间件例外：JWT 验证失败对浏览器请求返回 302 重定向到 `/`，对 API 请求返回 401 JSON（使用 `ApiResponse::<()>::error` 保持前端兼容）。
- 错误响应体固定字段：`code`（HTTP 状态码整数）、`error_code`（稳定字符串码）、`message`、`data`（null）。

### 3.4 第三方错误收敛

`types.rs` 集中实现了多种第三方错误的 `From<E>` 转换：
- `sqlx::Error` → `DbQueryFailed`
- `std::io::Error` → `IoError`
- `anyhow::Error` → `Internal`（System）
- `tokio::task::JoinError` → `Internal`（System）
- `reqwest::Error` → `NetworkError`
- `serde_json::Error` → `Internal`
- `sqlx::migrate::MigrateError` → `DbMigrationFailed`
- `bincode` 编解码错误 → `Internal`
- `jsonwebtoken::errors::Error` → `BadRequest`（Bad Request）
- `base64::DecodeError` → `InvalidRequest`
- `toml::de::Error` → `ConfigInvalid`

DAO 层（如 Lark）还维护局部错误类型并通过 `From` 或 `into_result/check` 方法统一上抛。

## 4. 约束与规范

- **错误码集中管理**：新增错误必须通过 `define_error_codes!` 宏在 `code.rs` 中声明，不得散落手写 `Error::new(ErrorCode::..., ...)`。
- **`ErrorField` 仅放安全字段**：设计文档明确要求不得放入 token、password、secret、完整 SQL、原始 stack trace；内部错误链应放在 `source` 而非 `field`。
- **分层职责边界**：DAO 不调 DAO、DAL 不调 DAL、Domain 不调 Domain；错误应在各自层产生并按语义映射，禁止跨层混用错误类型。
- **HTTP 状态码由 `ErrorCode` 驱动**：`http_status` 是枚举元数据的一部分，不应在 handler 中硬编码状态码。
- **认证中间件双模式**：Cookie 请求失败 → 302；Bearer 请求失败 → 401 JSON，这是唯一绕过统一 `Error` 响应的特例。
- **panic 仅用于编译期/宏级不可恢复错误**：`ai-orz-macros` 中的 `panic!` 用于缺失必需属性（如 `id/name/description/params`），运行时业务逻辑不使用 panic，全部走 `Result` 传播。
- **测试覆盖**：设计文档记录 Phase 2 完成“将所有项目文件从 `Result<T, E>` 双泛型迁移到 `Result<T>` 单泛型”，当前代码库已统一使用 `common::error::Result`。