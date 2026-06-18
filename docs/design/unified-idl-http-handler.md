# 统一参数 IDL + 自动生成 HTTP handler 设计文档

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
| 有 `path` + 有 `query` | `Path(...)` + `Query(...)` + `Json(...)` | path > query > body |
| 有 `path` + 无 `query` | `Path(...)` + `Json(...)` | path > body |
| 无 `path` + 有 `query` | `Query(...)` | query 就是全部 |
| 无 `path` + 无 `query` | `Json(...)` (所有参数都在 body)| body 就是全部 |

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
 | 2026-06-21 | 初始设计文档完成，完整支持 `#[param(source = "path")]` `#[param(source = "query")]`，最终方案采用 `#[derive(Params)]` + `#[param]`，不需要 nightly，完全稳定可用。优先级 `path > query > body` | AI Orz |
