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
    - src/service/dao/lark/error.rs
    - docs/archive/design-archive/common-error-type.md
---

## 1. 整体方案

项目采用 `common` crate 中定义的**统一错误模型**作为后端与前端（Dioxus WASM）共享的错误契约。核心由三层组成：

- **`ErrorType`**（粗粒度分类）：`Validation / Biz / Auth / Permission / Db / Io / Third / Tool / Runtime / Network / Config / System`，用于过滤、统计、告警。
- **`ErrorCode`**（具体错误码）：通过 `define_error_codes!` 宏在 `common/src/error/code.rs` 中集中声明，每个 variant 绑定一个 `type`、HTTP status 和稳定字符串 code（如 `"invalid_request"`），不携带业务字段。
- **`Error`**（统一结构体）：聚合 `code + error_type + msg + field: Option<ErrorField> + source: Option<Arc<anyhow::Error>>`，实现 `Serialize/Deserialize`，对外序列化时跳过 `source`。

该设计在 `docs/archive/design-archive/common-error-type.md` 中有完整设计文档，并已完成 Phase 1~3 全部落地。

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
| `src/service/dao/lark/error.rs` | DAO 层局部错误类型示例，提供 `LarkResponse.into_result/check`、`From<LarkWsError>` 等，将第三方错误映射为 `ThirdPartyError` |

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
| Domain | 业务规则检查 | 主要错误产生层，按语义选择 `ErrorCode` + `ErrorType`，可附加业务 `field`；工具协议/参数错误 → `ToolParameterInvalid` / `ToolExecutionFailed`；Agent 唤醒失败 → `RuntimeAwakenFailed` |
| DAL | PO↔Entity 转换、组合 DAO | 使用 `err!`/`bail_err!`/`ensure_err!` 包装 DAO 错误，按语义映射为 `Db`/`Biz` 等；持久化错误映射为 `DbQueryFailed`（Db, 500） |
| DAO | 本地 DB CRUD + 外部 API | 建议 DAO 层使用独立 `DaoError`（如 `LarkWsError`），由 DAL 映射为统一 `Error`；DAO 层建议保持纯粹，不直接产生通用 `Error` |
| pkg（`src/pkg/*`） | 业务无感知基础设施 | 通过 `From<E>` 自动转换底层错误（如 `anyhow::Error` → `System`） |

### 3.4 中间件错误处理

- `jwt_auth_middleware`：无 token 或 JWT 无效时，浏览器请求返回 302 重定向到 `/`，API 请求返回 401 JSON。
- `require_role_middleware`：角色权限不足返回 403 JSON。
- 两个中间件均位于 `src/middleware/`，通过 `mod.rs` 统一 re-export。

### 3.5 错误传播链与第三方错误收敛

1. 各层通过 `err!` / `bail_err!` / `ensure_err!` 构造 `common::error::Error`。
2. 借助 `From<E> for Error` 实现，第三方错误可通过 `?` 自动提升。
3. 在 Axum handler 中，`Error` 直接作为返回值，由 `IntoResponse` 实现序列化为 JSON 响应。
4. 中间件（JWT、角色校验）不走统一 `Error`，而是直接返回带 `ApiResponse::<()>::error(...)` 的 `Response`，因为它们是横切关注点，不依赖业务错误码。
5. 非 HTTP 场景（工具执行、AOP 事件、后台任务）通过 `ErrorField.trace_ref` 关联工具调用追踪引用，便于链路回溯。
6. 内部底层异常通过 `with_source` 或 `From<E>` 自动转换挂入 `Error.source`，序列化时被跳过，不泄露给外部；业务上下文通过 `ErrorField` 传递。

**第三方错误 `From<E>` 转换清单（集中在 types.rs）**：
- `sqlx::Error` → `DbQueryFailed`
- `std::io::Error` → `IoError`
- `anyhow::Error` → `Internal`（System）
- `tokio::task::JoinError` → `Internal`（System）
- `reqwest::Error` → `NetworkError`
- `serde_json::Error` → `Internal`
- `sqlx::migrate::MigrateError` → `DbMigrationFailed`
- `bincode` 编解码错误 → `Internal`
- `jsonwebtoken::errors::Error` → `BadRequest`
- `base64::DecodeError` → `InvalidRequest`
- `toml::de::Error` → `ConfigInvalid`

### 3.6 宏语法与快捷构造器

- **宏语法**（支持多种参数组合）：
  - `err!(Variant, "msg")`：最简构造
  - `err!(Variant, ErrorType, "msg")`：覆盖默认 `ErrorType`
  - `err!(Variant, "msg", field: {k: v})`：内联 JSON 字段
  - `err!(Variant, "msg", source: e)`：附带底层错误源
  - `bail_err!`：等价于 `return Err(err!(...))`
  - `ensure_err!(cond, Variant, ...)`：条件校验失败即返回错误
- **快捷构造器**：`Error::bad_request/unauthorized/not_found/conflict/internal/tool_call_failed/db_error/io_error/payload_too_large`。

### 3.7 未覆盖场景

- 仓库中未发现全局 `catch_all` 错误处理器或 panic/recover 策略；异常路径主要通过 `Result<T, Error>` 向上传播并由 Axum handler 或中间件显式处理。
- 前端 Dioxus 应用通过 `common` crate 共享相同错误枚举，但前端侧的具体 UI 错误展示逻辑不在本卡片范围内。

## 4. 约束与约定

1. `ErrorCode` 必须是纯单位 enum variant，不携带业务字段；业务上下文一律放入 `ErrorField`。
2. `ErrorField` 只放可对外公开的安全字段，不得包含 token、password、secret、完整 SQL、原始 stack trace。
3. 新增错误码必须通过 `define_error_codes!` 在 `common/src/error/code.rs` 中声明，同时指定 `type`、`http` 状态码和稳定 `code` 字符串。
4. 所有 `From<E>` 转换必须明确映射到合适的 `ErrorCode` + `ErrorType`，禁止吞掉错误信息。
5. 中间件中的认证/鉴权错误不走 `Error::IntoResponse`，而应直接构造 `ApiResponse` 并返回对应 `StatusCode`。
6. 设计文档 `docs/archive/design-archive/common-error-type.md` 规定整个项目已完成统一错误模型重构，业务代码内部全部使用 `common::error::{Error, Result}`。
7. **禁止直接使用裸 `anyhow::Result` 作为对外 API**：业务代码内部统一使用 `common::error::Result`；仅在实现外部 trait 要求时使用 `anyhow::Result`，并在边界处转换为 `Error`。
8. **分层职责边界**：DAO 不调 DAO、DAL 不调 DAL、Domain 不调 Domain；错误应在各自层产生并按语义映射，禁止跨层混用错误类型。
9. **错误码唯一性**：每个 `ErrorCode` variant 对应一个明确错误语义，禁止用纯数字码作为第一形态。
10. **panic 仅用于编译期/宏级不可恢复错误**：`ai-orz-macros` 中的 `panic!` 用于缺失必需属性（如 `id/name/description/params`），运行时业务逻辑不使用 panic，全部走 `Result` 传播。
11. **测试覆盖**：`common/src/error_test.rs` 对 `ErrorCode` 元数据、`err!` / `bail_err!` / `ensure_err!`、`ErrorField`、`with_source` 进行契约级断言，新增错误码需同步补充测试。设计文档记录 Phase 2 完成“将所有项目文件从 `Result<T, E>` 双泛型迁移到 `Result<T>` 单泛型”，当前代码库已统一使用 `common::error::Result`。
12. **向后兼容**：保留 `code: i32` 字段兼容前端，新增 `errorCode`、`errorType`、`field` 字段，`data` 保持 `null`。
13. **全项目统一 Result**：所有业务代码使用 `common::error::Result<T>`（即 `Result<T, Error>`），不再在各层定义独立错误类型；外部 trait 要求 `anyhow::Result` 时例外。