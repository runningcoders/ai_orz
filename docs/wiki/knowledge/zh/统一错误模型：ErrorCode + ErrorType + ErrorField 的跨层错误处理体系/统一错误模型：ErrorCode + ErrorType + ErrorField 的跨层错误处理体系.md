---
kind: error_handling
name: 统一错误模型：ErrorCode + ErrorType + ErrorField 的跨层错误处理体系
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
    - src/middleware/jwt_auth.rs
    - src/middleware/require_role.rs
    - docs/design/common-error-type.md
---

## 1. 整体方案

项目采用 `common` crate 中定义的**统一错误模型**作为后端与前端（Dioxus WASM）共享的错误契约。核心由三层组成：

- **`ErrorType`**（粗粒度分类）：`Validation / Biz / Auth / Permission / Db / Io / Third / Tool / Runtime / Network / Config / System`，用于过滤、统计、告警。
- **`ErrorCode`**（具体错误码）：通过 `define_error_codes!` 宏在 `common/src/error/code.rs` 中集中声明，每个 variant 绑定一个 `type`、HTTP status 和稳定字符串 code（如 `"invalid_request"`），不携带业务字段。
- **`Error`**（统一结构体）：聚合 `code + error_type + msg + field: Option<ErrorField> + source: Option<Arc<anyhow::Error>>`，实现 `Serialize/Deserialize`，对外序列化时跳过 `source`。

该设计在 `docs/design/common-error-type.md` 中有完整设计文档，并已完成 Phase 1~3 全部落地。

## 2. 关键文件与位置

| 文件 | 职责 |
|---|---|
| `common/src/error/mod.rs` | 模块入口，重导出 `ErrorCode`、`Error`、`ErrorField`、`Result<T>` 以及 `err!`/`bail_err!`/`ensure_err!` 宏 |
| `common/src/error/types.rs` | `ErrorType`、`ErrorField`、`Error` 定义及 `From<E>` 自动转换（sqlx/io/anyhow/tokio/reqwest/jsonwebtoken/base64/bincode/toml） |
| `common/src/error/code.rs` | `define_error_codes!` 调用，集中声明所有错误码及其 HTTP 状态码 |
| `common/src/error/macros.rs` | `err!`、`bail_err!`、`ensure_err!`、`define_error_codes!` 四个宏的实现 |
| `common/src/error/axum.rs` | `impl IntoResponse for Error`（feature `axum-integration`），将 `Error` 转为 `{ code, error_code, message, data }` JSON |
| `src/middleware/jwt_auth.rs` | 认证失败按请求类型返回 302 或 401 JSON |
| `src/middleware/require_role.rs` | 权限不足返回 403 JSON |
| `common/src/error_test.rs` | 对错误模型的契约测试（code_str / http_status / macro 行为） |

## 3. 架构与约定

### 3.1 错误构造与传播

- 业务层使用 `err!(Variant, "msg")` 构造错误，用 `bail_err!` 提前返回，用 `ensure_err!` 做条件校验；三者均支持可选的 `field:` 注入结构化上下文和 `source:` 附加底层错误链。
- `ErrorField` 仅承载可公开的安全字段（如 `trace_ref`、工具参数），内部错误链通过 `source` 保存且不序列化到响应。
- 各第三方库错误通过 `From<E>` 自动映射为 `Error`，使 `?` 运算符可直接向上层传递：`sqlx::Error → DbQueryFailed`、`std::io::Error → IoError`、`jsonwebtoken::errors::Error → JwtInvalid`、`reqwest::Error → NetworkError`、`serde_json::Error → Internal` 等。
- 函数签名统一使用 `common::error::Result<T>`（即 `Result<T, common::error::Error>`），不再在各层维护独立 `Result<T, E>` 双泛型。

### 3.2 HTTP 响应格式

- 当 `Error` 直接作为 Axum handler 返回值时，`IntoResponse` 实现将其序列化为：
  ```json
  { "code": 400, "error_code": "invalid_request", "message": "...", "data": null }
  ```
  其中 `code` 是 i32 类型的 HTTP 状态码，`error_code` 是稳定字符串码，`data` 固定为 `null`，以兼容前端期望。
- 中间件（`jwt_auth_middleware`、`require_role_middleware`）不走 `Error::IntoResponse`，而是直接返回 `(StatusCode, Json(ApiResponse::<()>::error(...)))`，仍遵循统一的 `ApiResponse` 结构。

### 3.3 分层使用规范（来自设计文档）

| 层级 | 职责 | 约定 |
|---|---|---|
| Handler | HTTP 入口、参数解析 | 参数解析错误 → `Validation` + `InvalidRequest`；Domain 错误透传 |
| Domain | 业务规则检查 | 主要错误产生层，按语义选择 `ErrorCode` + `ErrorType`，可附加业务 `field` |
| DAL | PO↔Entity 转换、组合 DAO | DAO 错误映射为统一 `Error`，按语义选择分类 |
| DAO | 本地 DB CRUD + 外部 API | 建议保持纯粹，DAO 错误由 DAL 映射 |

### 3.4 中间件错误处理

- `jwt_auth_middleware`：无 token 或 JWT 无效时，浏览器请求返回 302 重定向到 `/`，API 请求返回 401 JSON。
- `require_role_middleware`：角色权限不足返回 403 JSON。
- 两个中间件均位于 `src/middleware/`，通过 `mod.rs` 统一 re-export。

### 3.5 未覆盖场景

- 仓库中未发现全局 `catch_all` 错误处理器或 panic/recover 策略；异常路径主要通过 `Result<T, Error>` 向上传播并由 Axum handler 或中间件显式处理。
- 前端 Dioxus 应用通过 `common` crate 共享相同错误枚举，但前端侧的具体 UI 错误展示逻辑不在本卡片范围内。

## 4. 约束与约定

- `ErrorCode` 必须是纯单位 enum variant，不携带业务字段；业务上下文一律放入 `ErrorField`。
- `ErrorField` 只放可对外公开的安全字段，不得包含 token、password、secret、完整 SQL、原始 stack trace。
- 新增错误码必须通过 `define_error_codes!` 在 `common/src/error/code.rs` 中声明，同时指定 `type`、`http` 状态码和稳定 `code` 字符串。
- 所有 `From<E>` 转换必须明确映射到合适的 `ErrorCode` + `ErrorType`，禁止吞掉错误信息。
- 中间件中的认证/鉴权错误不走 `Error::IntoResponse`，而应直接构造 `ApiResponse` 并返回对应 `StatusCode`。
- 设计文档 `docs/design/common-error-type.md` 规定整个项目已完成统一错误模型重构，业务代码内部全部使用 `common::error::{Error, Result}`。