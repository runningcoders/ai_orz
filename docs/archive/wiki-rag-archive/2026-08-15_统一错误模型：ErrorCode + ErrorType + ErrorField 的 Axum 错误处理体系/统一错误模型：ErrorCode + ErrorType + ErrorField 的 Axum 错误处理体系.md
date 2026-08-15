> 📦 归档标记（2026-08-15）：被 [统一错误模型：ErrorCode + ErrorType + ErrorField 的跨层错误处理体系](docs/wiki/knowledge/zh/统一错误模型：ErrorCode + ErrorType + ErrorField 的跨层错误处理体系/统一错误模型：ErrorCode + ErrorType + ErrorField 的跨层错误处理体系.md) 取代。保留原因：历史参考，主卡已吸收本卡独有源码锚点与硬约束。生效方案：主卡真实路径作为唯一 RAG 召回目标。
---
kind: error_handling
name: 统一错误模型：ErrorCode + ErrorType + ErrorField 的 Axum 错误处理体系
category: error_handling
scope:
    - '**'
source_files:
    - common/src/error/mod.rs
    - common/src/error/types.rs
    - common/src/error/code.rs
    - common/src/error/macros.rs
    - common/src/error/axum.rs
    - src/middleware/jwt_auth.rs
    - src/middleware/require_role.rs
    - docs/design/common-error-type.md
    - common/src/error_test.rs
---

## 1. 系统/方法概述

仓库采用在 `common` crate 中定义的**统一类型化错误模型**，作为前后端、HTTP API、工具调用与运行时统计共享的错误契约。核心由三层组成：

- **`ErrorType`（粗分类）**：`Validation / Biz / Auth / Permission / Db / Io / Third / Tool / Runtime / Network / Config / System` 等枚举，用于过滤、统计、告警。
- **`ErrorCode`（具体错误码）**：通过 `define_error_codes!` 宏集中声明，每个 variant 绑定一个稳定的字符串 code（如 `"resource_not_found"`）、默认 `ErrorType` 和 HTTP status（如 404）。
- **`Error`（统一错误结构）**：携带 `code + error_type + msg + field: Option<ErrorField> + source: Option<Arc<anyhow::Error>>`，实现 `std::error::Error`、`Serialize/Deserialize`，并提供 `bad_request / db_error / io_error / internal / not_found / conflict / unauthorized / forbidden / tool_call_failed` 等快捷构造器。

`Result<T>` 被重定义为 `std::result::Result<T, common::error::Error>`，全项目内部统一使用单泛型 Result，不再在各层定义独立错误类型。

## 2. 关键文件与包

- `common/src/error/mod.rs`：模块入口，导出 `ErrorCode`、`Error`、`ErrorField`、`ErrorType`、`Result` 以及 `err!` / `bail_err!` / `ensure_err!` 宏。
- `common/src/error/code.rs`：`define_error_codes!` 宏的唯一调用点，集中声明全部错误码及其元数据。
- `common/src/error/types.rs`：`ErrorType`、`ErrorField`、`Error` 的定义及大量 `From<E> for Error` 转换实现（`sqlx::Error`、`std::io::Error`、`anyhow::Error`、`tokio::task::JoinError`、`reqwest::Error`、`serde_json::Error`、`jsonwebtoken::errors::Error`、`base64::DecodeError`、`toml::de::Error` 等），使 `?` 能自动将第三方错误提升为统一 `Error`。
- `common/src/error/macros.rs`：`err!`、`bail_err!`、`ensure_err!`、`define_error_codes!` 四个宏的实现；支持 `field: { key: value }` 内联 JSON、`source:` 附加源错误、显式覆盖 `error_type`。
- `common/src/error/axum.rs`：`impl IntoResponse for Error`，将 `Error` 直接作为 Axum handler 返回值，输出 `{ "code": i32, "error_code": str, "message": str, "data": null }` 并映射到对应 HTTP status。
- `src/middleware/jwt_auth.rs`：认证中间件，未认证时根据请求特征返回 302 重定向（浏览器）或 401 JSON（API）。
- `src/middleware/require_role.rs`：权限中间件，角色不足时返回 403 JSON。
- `docs/design/common-error-type.md`：设计文档，完整描述分层使用规范与 HTTP 响应格式约定。

## 3. 架构与约定

### 3.1 分层职责（来自设计文档）

| 层级 | 职责 | 约定 |
|---|---|---|
| Handler | HTTP 入口、参数转换 | 参数解析失败 → `Validation` + `InvalidRequest`；Domain 错误透传 |
| Domain | 业务规则检查 | 主要错误产生层，按语义选择 `ErrorCode` + `ErrorType`，可附加业务 `field` |
| DAL | PO↔Entity 转换 | DAO 错误映射为统一 `Error`，按语义选择 `Db`/`Biz` 等分类 |
| DAO | 持久化 CRUD | 建议 DAO 层使用独立 `DaoError`，由 DAL 映射为统一 `Error` |

### 3.2 错误传播链

1. 各层通过 `err!` / `bail_err!` / `ensure_err!` 构造 `common::error::Error`。
2. 借助 `From<E> for Error` 实现，第三方错误可通过 `?` 自动提升。
3. 在 Axum handler 中，`Error` 直接作为返回值，由 `IntoResponse` 实现序列化为 JSON 响应。
4. 中间件（JWT、角色校验）不走统一 `Error`，而是直接返回带 `ApiResponse::<()>::error(...)` 的 `Response`，因为它们是横切关注点，不依赖业务错误码。
5. 非 HTTP 场景（工具执行、AOP 事件、后台任务）通过 `ErrorField.trace_ref` 关联工具调用追踪引用，便于链路回溯。

### 3.3 错误码管理

所有错误码集中在 `common/src/error/code.rs` 的 `define_error_codes!` 块中声明，新增错误必须指定 `type`、`http`、`code` 三元组，由宏生成 `ErrorCode` 枚举及 `error_type()` / `http_status()` / `code_str()` 方法，保证每个错误码都有稳定的机器可读标识与默认 HTTP 状态码。

## 4. 约定与约束

- **统一 Result**：全项目内部使用 `common::error::Result<T>`（即 `Result<T, Error>`），不再在各层定义独立错误类型；外部 trait 要求 `anyhow::Result` 时例外。
- **错误码唯一性**：每个 `ErrorCode` variant 对应一个明确错误语义，禁止用纯数字码作为第一形态，避免额外维护号段。
- **字段安全**：`ErrorField` 只放可对外公开的安全字段，不得包含 token、password、secret、完整 SQL、原始 stack trace；内部错误链放在 `source` 且不被序列化。
- **HTTP 响应格式**：Axum 下 `Error` 序列化为 `{ "code": i32, "error_code": str, "message": str, "data": null }`，`code` 保持兼容前端，值为 HTTP status。
- **中间件短路**：认证/鉴权中间件直接返回 `Response`（302 或 401/403 JSON），不抛出 `Error`，因为这是横切逻辑而非业务错误。
- **测试保障**：`common/src/error_test.rs` 对 `ErrorCode` 元数据、`err!` / `bail_err!` / `ensure_err!`、`ErrorField`、`with_source` 进行契约级断言，确保宏行为稳定。
- **向后兼容**：设计文档明确要求保留 `code: i32` 字段兼容前端，新增 `errorCode`、`errorType`、`field` 字段，`data` 保持 `null`。