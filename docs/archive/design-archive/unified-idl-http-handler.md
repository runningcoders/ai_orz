# 统一参数 IDL + 自动生成 HTTP handler 设计文档

> 📦 归档标记（2026-08-16）：归档冻结。保留原因：unified-idl-http-handler 设计文档归档冻结，设计决策已沉淀至 wiki 长文。生效方案：见源码和 wiki 长文。

> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 整体分层架构
> - [handler-tool-registration-macro.md](./handler-tool-registration-macro.md) — HTTP Handler 直接注册为内置工具（互补的另一份定义多端复用）
> - [api_protocol_convention.md](./api_protocol_convention.md) — API 协议规范（common DTO 单一事实源，IDL 定义必须符合）
> - 【② Plan 落地】[前端API协议结构重构.md](../plan/前端API协议结构重构.md) — DTO IDL + Handler #[derive(Params)] 对齐
> - 【③ Wiki 长文】[API协议规范.md](docs/wiki/zh/content/架构设计/API协议规范/API协议规范.md)
> - 【④ RAG 卡】[附件存储与DTO协议统一](docs/wiki/knowledge/zh/附件存储与DTO协议统一：AttachmentFinance域资产%20+%20PagedResult%20T%20map全链路%20+%20common%3A%3Aapi单一事实源%20+%20count与query复用WHERE/附件存储与DTO协议统一：AttachmentFinance域资产%20+%20PagedResult%20T%20map全链路%20+%20common%3A%3Aapi单一事实源%20+%20count与query复用WHERE.md) — §红线 5 DTO 仅 common 定义 §IDL derive(Params) 说明位置

## 设计目标

统一接口参数定义，一份结构体定义同时支持：
1. **HTTP API**：自动生成 axum handler，自动从 path/query/body 提取参数
2. **LLM 工具调用**：自动注册为内置工具，自动生成 JSON Schema，自动反序列化参数

做到**一份定义，两端通用**，不需要维护多份参数代码。

## 最终实现方案（稳定版，不需要 nightly）

使用 `#[derive(Params)]` + `#[param(source = "...")]` 标记参数来源：

```rust
// common/src/api/skill.rs
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema, Params)]
pub struct UpdateSkillFileContentParams {
    /// Skill ID
    #[param(source = "path")]
    pub skill_id: String,

    /// File path in skill
    #[param(source = "path")]
    pub filename: String,

    /// New file content (UTF-8 text)
    pub content: String,

    /// Expected last updated timestamp (for optimistic locking), optional
    pub expected_updated_at: Option<i64>,
}
```

- `#[derive(Params)]`：这个 derive 不生成任何代码，只是让 `#[param]` 属性可以被解析
- `#[param(source = "path")]`：标记该参数来自 URL 路径
- `#[param(source = "query")]`：标记该参数来自 URL 查询参数
- 没有 `#[param]` 的字段默认来自 JSON 请求体

不需要 nightly，完全稳定可用！

## 命名规范

遵循项目原有语义约定：
- 请求结构体：`XXXRequest`
- 响应结构体：`XXXResponse`
- 类型别名可以继续使用：`pub type GetSkillResponse = SkillDetail;`

这样 `req/resp` 语义呼应，保持项目原有一致性。

## 字段注解

| 注解方式 | 说明 | 提取方式 |
|------|------|----------|
| `#[param(source = "path")]` | URL 路径参数 | 从 `Path` 提取 |
| `#[param(source = "query")]` | URL 查询参数 | 从 query string 提取 |
| 无注解 | JSON 请求体 | 从 `Json` 提取 |

> **默认规则：** 没有 `#[param]` 注解的字段默认都来自 `body`，整个结构体通过 `Json` 反序列化后，再用 `path`/`query` 覆盖对应字段。

## 宏设计

### 1. `#[derive(Params)]` derive 宏

这是一个空 derive 宏，**不生成任何代码**，只是为了让 `#[param]` 属性可以被 `syn` 解析读取。

### 2. `#[generate_http_handler]` 属性宏

**使用方式：**

```rust
#[generate_http_handler]
```

**作用：**
- 读取函数签名中的参数类型 `Params`
- 通过 `syn` 读取源码文件，查找 `#[param(source = ...)]` 注解，分类收集 `path`/`query`/`body` 字段
- 生成对应的 axum `Path`/`Query`/`Json` extractor 参数
- 组装成完整的 `Params` 结构体（先反序列化 body，然后用 query 覆盖，最后用 path 覆盖）
- 调用用户的核心函数
- 将返回值包装成 `Json<ApiResponse<Output>>`
- 错误分支后续统一通过 `common::error::{ErrorCode, Error}` 映射，保持 HTTP wire format `{ code, message, data }` 兼容，同时让 HTTP handler 与 LLM 工具调用共享同一套错误契约；详见 `docs/archive/design-archive/common-error-type.md`

**生成的代码示例：**

用户写：
```rust
#[register_handler_tool(
    id = "update_skill_file_content",
    name = "update_skill_file_content",
    description = "Create or update skill file content",
    params = "common::api::UpdateSkillFileContentParams",
)]
#[generate_http_handler]
pub async fn update_skill_file_content(
    ctx: RequestContext,
    params: UpdateSkillFileContentParams,
) -> Result<(), AppError> {
    // ... core logic ...
}
```

宏生成：
```rust
pub async fn update_skill_file_content_handler(
    Extension(ctx): Extension<RequestContext>,
    Path((skill_id, filename)): Path<(String, String)>,
    Json(mut params): Json<UpdateSkillFileContentParams>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    params.skill_id = skill_id;
    params.filename = filename;
    let result = update_skill_file_content(ctx, params).await?;
    Ok(Json(ApiResponse::success(result)))
}
```

### 3. `#[register_handler_tool]` 属性宏（保持不变）

自动注册为 LLM 内置工具：
- 生成工厂结构体
- 程序启动自动注册到全局工具注册表
- 自动生成 JSON Schema 存入数据库
- 支持 `Result<Value, AppError>` 和 `Result<impl Serialize, AppError>` 两种返回类型

## 处理规则

### 多个 `#[path]` 字段

```rust
struct Params {
    #[path] project_id: String,
    #[path] artifact_id: String,
}
```

生成：
```rust
Path((project_id, artifact_id)): Path<(String, String)>,
...
params.project_id = project_id;
params.artifact_id = artifact_id;
```

### 混合来源

```rust
struct Params {
    #[path] skill_id: String,
    #[path] filename: String,
    #[body] content: String,
    #[body] expected_updated_at: Option<i64>,
}
```

生成：
```rust
Path((skill_id, filename)): Path<(String, String)>,
Json(mut params): Json<Params>,
...
params.skill_id = skill_id;
params.filename = filename;
// content 和 expected_updated_at 已经在 Json 提取了
```

### 整个 body

```rust
struct Params {
    #[path] id: String,
    // 其他字段都在 body
    name: String,
    email: String,
}
```
**注意：** 没有 `#[body]` 注解的字段默认在哪里？
- 如果有 `#[path]`/`#[query]`，剩下的默认都在 `#[body]`
- 如果全部字段都没有注解，默认整个结构体从 `Json` 提取

## 支持的组合

| 组合 | 生成的 axum extractor | 优先级 |
|------|------------------------|--------|
| 有 `path` + 有 `query` | `Path(...)` + `RawQuery` (+ `Json(...)` 当有 body 字段时) | path > query > body |
| 有 `path` + 无 `query` | `Path(...)` + `Json(...)` | path > body |
| 无 `path` + 有 `query` | `Query(...)` | query 就是全部 |
| 无 `path` + 无 `query` | `Json(...)` (所有参数都在 body)| body 就是全部 |
| **空 struct**（零命名字段） | 仅 `Extension(ctx)` | 无需 extractor |

**子分支自动判定：**
- 当所有非 path 字段都是 query 字段（无 body 字段）时，宏走"path+query only"分支：仅 `Path + RawQuery`，无 Json 提取器，params 用 `Default::default()` 构造空实例后用 query/path 值填充。典型场景：`GET /items/{id}?verbose=true`。
- 当存在 body 字段（非 path 非 query）时，宏走"path+query+body 混合"分支：`Path + RawQuery + Json`，body 提供基础值，query 覆盖 body 同名字段，path 最后覆盖。典型场景：`PUT /items/{id}?verbose=true` body `{"name":"...","body_field":"..."}`。

### Query 字段提取实现细节（path+query 分支）

为避免 macro hygiene 问题（handler 文件可能未导入 query 字段类型如 `ToolStatus`），宏不生成临时 Query struct，而是：

1. 用 `axum::extract::RawQuery` 提取原始 query 字符串（不会因缺失字段报错）
2. `serde_urlencoded` 解析为 `HashMap<String, String>`
3. 构建 `serde_json::Value` 对象，按值内容推断类型（bool / number / null / string）
4. 对非 `#[serde(flatten)]` query 字段：`serde_json::from_value(query_value.get(name).cloned())` 反序列化，目标类型由 `params.{ident} = parsed` 赋值推导
5. 对 `#[serde(flatten)]` query 字段（如 `pagination: PaginationParams`）：用整个 `query_value` 反序列化

**关键优势**：宏生成代码不引用任何自定义类型名（如 `ToolStatus`），全部通过类型推导完成反序列化，因此 handler 文件无需为 query 字段类型额外 `use` 导入。

### 空 struct GET 端点

GET 请求通常没有 body，如果 params 类型没有任何命名字段（如 `CheckInitializedRequest {}`、`GetAllQueueStatsRequest {}`），宏会跳过 `Json` 提取器，直接用 `Default::default()` 构造空 params：

```rust
// 用户写：
#[generate_http_handler]
pub async fn check_initialized(
    ctx: RequestContext,
    _params: CheckInitializedRequest,  // 空 struct
) -> Result<CheckInitializedResponse> { ... }

// 宏生成（无 Json extractor，GET 请求不会 400）：
pub async fn check_initialized_handler(
    Extension(ctx): Extension<RequestContext>,
) -> Result<Json<ApiResponse<CheckInitializedResponse>>, Error> {
    let params = CheckInitializedRequest::default();
    let result = check_initialized(ctx, params).await?;
    Ok(Json(ApiResponse::success(result)))
}
```

> **背景**：早期版本宏对所有结构体都生成 `Json` 提取器，导致 GET 端点（无 body）被 axum 拒绝并返回 400 Bad Request。空 struct 分支解决了这个问题。

## 优先级规则

后赋值覆盖先赋值：
1. 先从 body 反序列化得到基础值
2. 然后用 query 字段覆盖
3. 最后用 path 字段覆盖

优先级：`path > query > body`，这样即使不同位置有同名字段，一定按 URL 里的值为准，符合 HTTP 约定。

## 实现步骤

### 第一步：定义 proc-macro 在 `ai-orz-macros`

1. 实现 `generate_http_handler` 属性宏
2. 解析函数签名，获取参数类型 `Params`
3. 解析 `Params` 结构体的字段注解
4. 分类收集 `path_fields: Vec<(Ident, Type)>` / `query_fields` / `body_fields`
5. 生成 handler 函数代码

### 第二步：处理不同提取器的签名生成

根据字段分类生成正确的 axum extractor 参数：

- 如果有 N 个 path 字段 → `Path<(T1, T2, ...)>`
- 如果有 query 字段 → `Query<Params>` 或者 `Query<(...)>` （推荐整个 Params 提取，然后注入 path）
- 如果有 body 字段 → `Json<Params>`

> **实现选择：** 我们总是先创建 `Params` 实例，然后从各个提取器注入字段。这样不管怎么混合都能正确工作。

### 第三步：集成到项目

1. 更新设计文档
2. 测试一个实际例子（比如 `list_skill_files`）验证可用

## 使用示例

### 完整例子：update_skill_file_content

**common/src/api/update_skill_file_content.rs:**
```rust
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct UpdateSkillFileContentParams {
    /// Skill ID
    #[path]
    pub skill_id: String,

    /// File path in skill
    #[path]
    pub filename: String,

    /// New file content (UTF-8 text)
    pub content: String,

    /// Expected last updated timestamp (for optimistic locking), optional
    pub expected_updated_at: Option<i64>,
}
```

**src/handlers/hr/skill/update_skill_file_content.rs:**
```rust
use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::UpdateSkillFileContentParams;
use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;

/// Update skill file content
#[register_handler_tool(
    id = "update_skill_file_content",
    name = "update_skill_file_content",
    description = "Create or update a text file in a skill. If the file doesn't exist, it will be created. If it exists, it will be overwritten.",
    params = "common::api::UpdateSkillFileContentParams",
)]
#[generate_http_handler]
pub async fn update_skill_file_content(
    ctx: RequestContext,
    params: UpdateSkillFileContentParams,
) -> Result<(), AppError> {
    domain()
        .skill_manage()
        .update_skill_file_content(
            ctx,
            &params.skill_id,
            &params.filename,
            &params.content,
            params.expected_updated_at,
        )
        .await?;

    Ok(())
}
```

**就这么多！** 不需要写 handler，宏自动生成 `update_skill_file_content_handler`，router 直接用。

## 对比原来的写法

**原来需要写：**
```rust
pub async fn update_skill_file_content_handler(
    Extension(ctx): Extension<RequestContext>,
    Path((skill_id, filename)): Path<(String, String)>,
    Json(req): Json<UpdateSkillFileContentRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    domain()
        .skill_manage()
        .update_skill_file_content(
            ctx,
            &skill_id,
            &filename,
            &req.content,
            req.expected_updated_at,
        )
        .await?;

    Ok(Json(ApiResponse::<()>::ok()))
}
```

**现在只需要写：** 核心逻辑，宏生成上面这段。

代码量减少一半，参数定义统一，维护方便。

## 依赖

需要在 `common` 添加 `schemars` dependency，已经计划添加了。

 ## 更新记录

 | 日期 | 更新内容 | 作者 |
 |------|----------|------|
 | 2026-07-27 | 新增「2026-07-27: 修复 3 个 GET struct 字段位置注解不一致」修复历史，记录宏解析 bug 掩盖的 struct 注解错误 | AI Orz |
 | 2026-06-25 | 补充 common 统一错误类型目标：后续错误分支统一映射到 `common::error::{ErrorCode, Error}`，HTTP 与 LLM 工具调用共享错误契约，wire format 继续兼容 `{ code, message, data }` | Hermes |
 | 2026-06-21 | 初始设计文档完成，完整支持 `#[param(source = "path")]` `#[param(source = "query")]`，最终方案采用 `#[derive(Params)]` + `#[param]`，不需要 nightly，完全稳定可用。优先级 `path > query > body` | AI Orz |

## 修复历史

### 2026-07-26: (true, true) 分支用 RawQuery 替代 Query<ParamsTy>

**问题**：原 `(true, true)` 分支生成 `Path + Query<ParamsTy> + Json<ParamsTy>`，存在两个 bug：
1. `Query<ParamsTy>` 尝试从 query string 反序列化所有字段（含必填 path 字段如 `id: String`），缺失字段时返回 400
2. `Json<ParamsTy>` 对无 body 的 GET 请求返回 400

**影响**：10 个生产 path+query struct（`GetAgentRequest`、`ListMcpToolsByServerRequest`、`ListArtifactsRequest` 等）全部受影响，GET 请求无法工作。

**修复方案**：
- 用 `RawQuery` 提取原始 query 字符串（不会因缺失字段报错）
- 用 `serde_json::Value` 中间表示 + 类型推断反序列化各 query 字段
- 拆为两个子分支：path+query only（无 Json 提取器）/ path+query+body 混合（保留 Json）

**为什么不用临时 Query struct 方案**：宏生成 `struct __QueryParams { status: Option<ToolStatus>, ... }` 会遇到 macro hygiene 问题——handler 文件可能未导入 `ToolStatus`，导致宏生成代码在该作用域中找不到类型。新方案通过 `params.{ident} = parsed` 类型推导规避此问题。

**测试覆盖**：15 个集成测试覆盖 path+query only GET、path+query+body 混合 PUT、enum 类型、flatten pagination、数值类型、缺失 Option 字段等边界场景。

### 2026-07-27: 修复 3 个 GET struct 字段位置注解不一致

**问题**：宏 `(true, true)` 分支修复后（`Meta::List` 正确解析 `#[param]`），暴露出 3 个 struct 的字段位置注解与后端路由/前端调用不一致：

1. **`ListArtifactsRequest`**：`project_id` 标为 `#[param(source = "path")]`，但路由 `/api/v1/project/artifacts` 无 path 段，前端调用 `?project_id=xxx`（query string）。宏生成 `Path<(project_id,)>` 提取器但路由无 path 段，返回 400。
2. **`ListMessagesRequest`**：7 个字段全部无 `#[param]` 注解（默认 body），但路由是 `GET /api/v1/finance/messages`，前端调用 `?project_id=xxx&limit=20`。宏生成 `Json<ListMessagesRequest>` 提取器，GET 请求无 body 返回 400。
3. **`QueryToolCallEntriesRequest`**：9 个字段全部无 `#[param]` 注解，但路由是 `GET /api/v1/finance/tool-call-entries`，前端调用 `?call_id=xxx&agent_id=xxx`。同上问题。

**根因**：这些 bug 之前被宏的 `Meta::NameValue` 解析错误掩盖——所有 `#[param]` 失效，struct 走 `(false, false)` 的 `Json` 分支，GET 请求因无 body 一直返回 400。修复宏后才暴露出真实的注解错误。

**修复方案**：
- `ListArtifactsRequest.project_id`：`path` → `query`
- `ListMessagesRequest` 7 个字段：补 `#[param(source = "query")]`
- `QueryToolCallEntriesRequest` 9 个字段：补 `#[param(source = "query")]`

**验证**：15 个宏集成测试 + 781 个 lib 测试全部通过；前端 Artifacts 列表页、消息列表/历史加载、工具调用记录查询 3 个功能恢复正常。

**教训**：宏的解析 bug 会掩盖 struct 定义本身的问题。修复宏的解析逻辑后，应系统审查所有使用 `#[param]` 的 struct，确认字段位置注解与路由/前端调用一致。