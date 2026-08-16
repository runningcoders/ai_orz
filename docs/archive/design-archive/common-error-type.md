# common 统一错误类型设计

> 📦 归档标记（2026-08-16）：归档冻结。保留原因：common-error-type 设计文档归档冻结，设计决策已沉淀至 wiki 长文。生效方案：见源码和 wiki 长文。

> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 整体分层架构（错误是跨层通用契约）
> - [api_protocol_convention.md](./api_protocol_convention.md) — API 协议规范（错误码是响应协议的核心部分）

> 记录日期：2026-06-25  
> **更新日期：** 2026-06-26  
> **状态：** ✅ 完整实现，重构完成，所有测试通过

## 1. 设计目标

在 `common` crate 中定义统一错误模型，作为前后端、API、工具调用、运行时统计共享的错误契约。

核心目标：

1. **`ErrorCode` 表达具体错误**：一个 enum variant 对应一个确定错误，便于测试、匹配、统计、前端展示。
2. **`ErrorType` 表达错误分类**：粗粒度分类，用于过滤、统计、告警。
3. **`ErrorField` 承载业务上下文**：安全可公开的业务字段，平铺序列化。
4. **`Error` 统一错误结构**：整合 `code + error_type + msg + field + source`。
5. **宏提升易用性**：减少样板代码。

## 2. 核心类型设计

### 2.1 ErrorType：错误分类

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorType {
    /// 输入验证错误：参数缺失、格式非法。
    Validation,
    /// 业务规则错误：状态不允许、资源不存在、业务冲突。
    Biz,
    /// 认证错误：未登录、token 无效。
    Auth,
    /// 权限错误：身份有效，但无权执行。
    Permission,
    /// 数据库错误：SQL、连接、事务错误。
    Db,
    /// 文件/IO 错误：本地文件读写、对象存储。
    Io,
    /// 第三方服务错误：模型供应商、MCP、消息平台。
    Third,
    /// 工具错误：工具协议、参数、执行失败。
    Tool,
    /// Runtime 编排错误：Agent 唤醒、上下文组装失败。
    Runtime,
    /// 配置错误：缺少配置、配置非法。
    Config,
    /// 系统内部错误：未预期异常、兜底。
    System,
}
```

分类边界：

| ErrorType | 典型来源 |
|-----------|----------|
| `Validation` | Handler / 参数解析 |
| `Biz` | Domain / 业务规则 |
| `Auth` | middleware / 认证 |
| `Permission` | Domain / 权限检查 |
| `Db` | DAO / 持久化 |
| `Io` | pkg / 文件存储 |
| `Third` | 第三方适配器 |
| `Tool` | Runtime / 工具执行 |
| `Runtime` | Runtime Domain |
| `Config` | 启动 / 工厂初始化 |
| `System` | 任何层兜底 |

### 2.2 ErrorCode：具体错误码

`ErrorCode` 是具体、稳定、可匹配的错误码，一个 variant 对应一个明确错误语义。

由 `define_error_codes!` 宏生成结构和元数据方法：

```rust
// 宏调用示例：
define_error_codes! {
    general {
        InvalidRequest {
            type: Validation,
            http: 400,
            code: "invalid_request",
        }
        Unauthorized {
            type: Auth,
            http: 401,
            code: "unauthorized",
        }
        Forbidden {
            type: Permission,
            http: 403,
            code: "forbidden",
        }
        ResourceNotFound {
            type: Biz,
            http: 404,
            code: "resource_not_found",
        }
        ResourceConflict {
            type: Biz,
            http: 409,
            code: "resource_conflict",
        }
        DbQueryFailed {
            type: Db,
            http: 500,
            code: "db_query_failed",
        }
        ThirdPartyUnavailable {
            type: Third,
            http: 502,
            code: "third_party_unavailable",
        }
        ToolAutoModeNotSupported {
            type: Tool,
            http: 400,
            code: "tool_auto_mode_not_supported",
        }
        RuntimeAwakenFailed {
            type: Runtime,
            http: 500,
            code: "runtime_awaken_failed",
        }
        Internal {
            type: System,
            http: 500,
            code: "internal",
        }
    }
}
```

生成内容：

- `pub enum ErrorCode { ... }` 纯单位 variant，不携带业务字段
- `impl ErrorCode { fn code(&self) -> &'static str; ... }`
- `impl ErrorCode { fn error_type(&self) -> ErrorType; ... }`
- `impl ErrorCode { fn http_status_code(&self) -> u16; ... }`
- `impl ErrorCode { fn field(&self) -> serde_json::Map<String, Value>; ... }`（对于单位 variant 返回空 map）

设计原则：

- 不用纯数字码作为第一形态，避免额外维护号段
- 如果未来需要数字码，可以后加映射，不影响当前设计
- `ErrorCode` 自带默认 `ErrorType` 和 HTTP status，但 `Error` 结构仍保留 `error_type` 字段，允许特殊场景覆盖

### 2.3 ErrorField：业务上下文

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ErrorField {
    #[serde(flatten)]
    pub values: serde_json::Map<String, serde_json::Value>,
}

impl ErrorField {
    pub fn new() -> Self { ... }
    pub fn with<V: Serialize>(mut self, key: impl Into<String>, value: V) -> Self { ... }
    pub fn insert<V: Serialize>(&mut self, key: impl Into<String>, value: V) { ... }
    pub fn is_empty(&self) -> bool { ... }
}
```

使用示例：

```rust
let field = ErrorField::new()
    .with("tool_id", tool_id)
    .with("control_mode", "auto");
```

JSON 输出：

```json
{
  "toolId": "tool_123",
  "controlMode": "auto"
}
```

设计原则：

- 只放可对外公开的安全字段
- 不得放 token、password、secret、完整 SQL、原始 stack trace
- 内部错误链放在 `source`，不进入 `field`

### 2.4 Error：统一错误结构

```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Error {
    /// 具体错误码
    pub code: ErrorCode,
    /// 错误粗分类（默认由 code 推导，可覆盖）
    pub error_type: ErrorType,
    /// 安全可展示消息
    pub msg: String,
    /// 业务上下文
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<ErrorField>,
    /// 内部错误链，不序列化对外
    #[serde(skip_serializing)]
    pub source: Option<anyhow::Error>,
}

pub type Result<T> = std::result::Result<T, Error>;
```

构造方法：

```rust
impl Error {
    /// 从 ErrorCode 创建，自动推导 error_type
    pub fn new(code: ErrorCode, msg: impl Into<String>) -> Self { ... }
    /// 显式指定 error_type，覆盖默认
    pub fn typed(code: ErrorCode, error_type: ErrorType, msg: impl Into<String>) -> Self { ... }
    /// 添加业务上下文
    pub fn with_field(mut self, field: ErrorField) -> Self { ... }
    /// 附加内部错误源
    pub fn with_source<E>(mut self, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static;
    { ... }
    /// 获取稳定字符串错误码
    pub fn code(&self) -> &'static str { ... }
    /// 获取默认 HTTP 状态码
    pub fn http_status_code(&self) -> u16 { ... }
    /// 获取结构化业务字段
    pub fn field(&self) -> Option<&ErrorField> { ... }
}
```

## 3. 宏设计

### 3.1 基础构造 `err!`

```rust
// 最简
err!(InvalidRequest, "invalid request parameter");

// 带 field
err!(
    ToolAutoModeNotSupported,
    "HTTP Tool only supports Manual control mode",
    ErrorField::new()
        .with("tool_protocol", "http")
        .with("control_mode", "auto")
);

// 显式覆盖 error_type
err!(
    ThirdPartyUnavailable,
    Config,
    "missing model provider configuration"
);
```

### 3.2 提前返回 `bail_err!`

```rust
bail_err!(ResourceNotFound, "task not found");

bail_err!(
    ResourceNotFound,
    "task not found",
    ErrorField::new().with("task_id", task_id)
);
```

### 3.3 条件校验 `ensure_err!`

```rust
ensure_err!(
    tool.control_mode == ControlMode::Manual,
    ToolAutoModeNotSupported,
    "HTTP Tool only supports Manual control mode",
    ErrorField::new()
        .with("tool_id", tool.id)
        .with("control_mode", tool.control_mode)
);
```

## 4. 分层使用规范

| 层级 | 职责 | 使用建议 |
|------|------|----------|
| **Handler** | HTTP 入口、参数转换 | 参数解析错误 → `Validation` + `InvalidRequest`；Domain 错误透传转换 |
| **Domain** | 业务规则检查 | 主要错误产生层，按语义选择 `ErrorCode` + `ErrorType`，可附加业务 `field` |
| **DAL** | PO → Entity 转换、组合 DAO | DAO 错误映射为统一 `Error`，按语义选择 `Db`/`Biz` 等分类 |
| **DAO** | 持久化 CRUD | 建议 DAO 层使用独立 `DaoError`，由 DAL 映射为统一 `Error`，保持 DAO 纯粹 |

## 5. HTTP 响应格式

最终 JSON 格式：

```json
{
  "code": 400,
  "errorCode": "tool_auto_mode_not_supported",
  "errorType": "tool",
  "message": "HTTP Tool only supports Manual control mode",
  "field": {
    "toolId": "tool_123",
    "controlMode": "auto"
  },
  "data": null
}

兼容说明：

- 保留 `code: i32` 字段兼容前端，值为 HTTP status code
- 新增 `errorCode`（字符串具体错误码）、`errorType`（字符串分类）、`field`（业务上下文）
- `data` 保持 `null`，与成功响应结构对齐

## 6. 分阶段落地计划（已完成）

> 本小节为开发期落地记录；设计现状以 [统一错误模型 wiki 长文](docs/wiki/zh/content/功能模块/系统管理/统一错误与异常处理模型.md) 为准。

### Phase 1：完成 common 类型定义
- 定义 `ErrorType`、`ErrorCode`（基础变体）、`ErrorField`、`Error`、`Result`
- 实现 `define_error_codes!` 宏
- 实现 `err!`/`bail_err!`/`ensure_err!`
- 单元测试

### Phase 2：全项目迁移统一错误模型
- 将所有项目文件从 `Result<T, E>` 双泛型迁移到 `Result<T>` 单泛型
- 删除 `ToolExecutionError` 定义，统一使用 `common::error::Error`
- 将 `ToolCallTraceRef` 集成到 `ErrorField` 中，保留工具执行追踪能力
- 修复所有 trait 实现中外部要求使用 `anyhow::Result` 的冲突
- 修复所有错误转换问题，添加 `From<E>` 实现支持 `?` 自动转换

### Phase 3：验证测试
- 修复所有测试错误，保证测试语法正确
- 所有 490 个单元测试全部通过
- 保持 API 兼容，序列化格式满足前端期望

### 迁移总结：
- 整个项目已完成统一错误模型重构，业务代码内部全部使用 `common::error::{Error, Result}`
- 实现外部 trait 时遵循外部 trait 要求，使用 `anyhow::Result`
- `trace_ref` 保存在 `ErrorField` 中，可通过 `ErrorField::with("trace_ref", trace_ref)` 添加
- 所有第三方错误可自动转换为 `common::error::Error`，通过 `?` 自动转换