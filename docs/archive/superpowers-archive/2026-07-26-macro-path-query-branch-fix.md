# generate_http_handler 宏 (true, true) 分支修复 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 `generate_http_handler` 宏 `(true, true)` 分支（同时含 path 和 query 字段）使其支持无 body 的 GET 请求，并解决 `Query<ParamsTy>` 尝试反序列化全部字段（包括必填 path/body 字段）导致的 400 Bad Request 问题。修复后 10 个生产 path+query struct 应全部可在无 body 时正常工作。

**Architecture:**
- 改用 `axum::extract::RawQuery` 提取原始 query 字符串（不会因缺失字段报错）
- 用 `serde_urlencoded` 解析为 `HashMap<String, String>`，再构建 `serde_json::Value` 对象（含类型推断：bool/number/null/string）
- 对每个 query 字段：调用 `serde_json::from_value`，目标类型由 `params.#ident = parsed` 推导
- 对 `#[serde(flatten)]` query 字段：用整个 `serde_json::Value` 反序列化（如 `PaginationParams` 从 `{limit, offset}` 提取）
- (true, true) 分支拆为两个子分支：
  - **path+query only**（无 body 字段）：`Path + RawQuery`，用 `Default::default()` 构造 params
  - **path+query+body 混合**：`Path + RawQuery + Json`，body 提供基础值，query 覆盖，path 最后覆盖

**Tech Stack:** Rust + syn 2.0 + quote + axum 0.8 (`RawQuery`) + serde_json + serde_urlencoded

---

## 背景与问题分析

### 当前 (true, true) 分支的实现（已存在 bug）

```rust
(true, true) => {
    quote! {
        pub async fn #handler_ident(
            axum::extract::Extension(ctx): axum::extract::Extension<RequestContext>,
            axum::extract::Path(#path_tuple): axum::extract::Path<#path_ty_tuple>,
            axum::extract::Query(query): axum::extract::Query<#params_ty>,  // BUG 1
            axum::Json(mut params): axum::Json<#params_ty>,                  // BUG 2
        ) -> ... {
            #assign_queries  // params.{q} = query.{q}
            #assign_paths    // params.{p} = {p}
            ...
        }
    }
}
```

**Bug 1**：`Query<#params_ty>` 尝试从 query string 反序列化整个 params_ty 的所有字段（包括必填 path 字段如 `server_id: String` 和 body 字段如 `name: String`）。当 query string 缺失这些字段时，axum 返回 400 Bad Request。

**Bug 2**：`Json<#params_ty>` 对空 body 的 GET 请求返回 400 Bad Request（即使有 body 字段，GET 不应强制要求 body）。

### 受影响的 10 个生产 struct

| Struct | 文件 | 字段构成 |
|--------|------|---------|
| `GetModelProviderRequest` | `common/src/api/model_provider.rs:68` | 1 path + 4 query |
| `GetToolRequest` | `common/src/api/tool.rs:47` | 1 path + 4 query |
| `GetTaskRequest` | `common/src/api/task.rs:39` | 1 path + 4 query |
| `ListAgentTasksRequest` | `common/src/api/task.rs:62` | 1 path + 2 query |
| `ListProjectTasksRequest` | `common/src/api/task.rs:76` | 1 path + 2 query |
| `GetProjectRequest` | `common/src/api/project.rs:34` | 1 path + 4 query |
| `GetAgentRequest` | `common/src/api/agent.rs:82` | 1 path + 4 query |
| `ListMcpToolsByServerRequest` | `common/src/api/mcp_tool.rs:27` | 1 path + 3 query（含 flatten pagination）|
| `ListArtifactsRequest` | `common/src/api/artifact.rs:78` | 1 path + 5 query（含 flatten pagination）|
| `ListEventsRequest` | `common/src/api/system.rs:71` | 1 path + 4 query |

### 为什么不沿用旧的"生成临时 Query struct"方案？

之前的尝试：宏生成 `struct __QueryParams { keyword: Option<String>, status: Option<ToolStatus>, ... }`，用 `Query<__QueryParams>` 提取。

**问题（macro hygiene）**：handler 文件可能未导入字段类型（如 `ToolStatus`、`PaginationParams`），导致宏生成的 struct 定义找不到类型。

例：`src/handlers/finance/mcp_tool/list_mcp_tools_by_server.rs` 只导入 `ToolListItem`，不导入 `ToolStatus`。宏生成的 `struct __QueryParams { status: Option<ToolStatus> }` 在该文件作用域中无法解析 `ToolStatus`。

**新方案规避 hygiene 问题**：不生成新 struct，而是在 macro 生成的代码中用 `serde_json::from_value` + 类型推导。类型信息通过 `params.#ident = parsed` 的赋值操作传递给编译器，不需要在 macro 生成代码中显式写出类型名。

---

## 文件结构

### 修改文件

| 文件 | 修改内容 |
|------|---------|
| `ai-orz-macros/src/lib.rs` | 1. 修改 `collect_path_and_query_fields_from_type` 返回值新增 `flattened_query_fields`<br>2. 重写 `(true, true)` 分支用 `RawQuery` + `serde_json::Value` 提取 query 字段<br>3. 修复 `(true, false)` 混合分支：从 `Query<#params_ty>` 改为不影响（已正确，无需改） |
| `tests/http_handler_macro_test.rs` | 修复 2 个失败测试 + 新增 6 个针对 path+query 边界的测试 |
| `docs/archive/design-archive/unified-idl-http-handler.md` | 更新 `(true, true)` 分支行为说明、补充修复历史 |

### 不修改文件

- `common/src/api/*.rs`：所有 path+query struct 不需要补 Default（新方案在 path+query only 分支用 Default::default()，已确认 `PaginationParams` impl Default；其他 struct 因 query 字段多为 Option，Default 也已 impl 或可补）
- 现有 `(true, false)`、`(false, true)`、`(false, false)` 分支不动

---

## 任务清单

### Task 1: 编写失败测试锁定当前 bug（TDD red 阶段）

**目的：** 在改宏之前先写测试明确"期望行为"，跑测试确认失败，避免空想。

**Files:**
- Modify: `tests/http_handler_macro_test.rs`

- [ ] **Step 1: 修复已有的 `test_path_and_query_mixed_get_works_without_body` 测试预期**

当前测试已存在但因宏 bug 失败。验证其失败原因确实是 400 而非其他错误。

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --test http_handler_macro_test test_path_and_query_mixed_get_works_without_body 2>&1 | tail -15`
Expected: FAIL，`left: 400, right: 200`，错误消息为"path+query GET 在无 body 时应该工作"

- [ ] **Step 2: 修复已有的 `test_priority_path_greater_than_query_greater_than_body` 测试**

当前测试用 PUT 带 body，但因 `Query<ParamsTy>` 尝试反序列化必填 `id`/`name_body` 字段失败。

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --test http_handler_macro_test test_priority_path_greater_than_query_greater_than_body 2>&1 | tail -15`
Expected: FAIL，`left: 400, right: 200`

- [ ] **Step 3: 新增 path+query 含 enum 类型字段的测试**

在 `tests/http_handler_macro_test.rs` 末尾追加：

```rust
// ==================== 测试 8: path+query 含 enum 类型 ====================

use common::enums::ToolStatus;

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, Params)]
pub struct GetToolStatusRequest {
    #[param(source = "path")]
    pub tool_id: String,
    #[param(source = "query")]
    pub status: Option<ToolStatus>,
}

#[derive(Debug, serde::Serialize)]
pub struct GetToolStatusResponse {
    pub tool_id: String,
    pub status_str: String,
}

#[generate_http_handler]
pub async fn get_tool_status(
    _ctx: RequestContext,
    params: GetToolStatusRequest,
) -> Result<GetToolStatusResponse, Error> {
    Ok(GetToolStatusResponse {
        tool_id: params.tool_id,
        status_str: format!("{:?}", params.status),
    })
}

#[tokio::test]
async fn test_path_and_query_with_enum_type_works() {
    // query 字段是 Option<ToolStatus>（enum），需要 serde 正确反序列化
    let app = make_router(|r| {
        r.route("/tools/{tool_id}/status", get(get_tool_status_handler))
    })
    .await;

    let req = Request::builder()
        .method(Method::GET)
        .uri("/tools/tool-123/status?status=Enabled")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "path+query 含 enum 类型字段应工作"
    );

    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        body_str.contains("tool-123"),
        "tool_id 应从 path 提取，实际: {body_str}"
    );
    assert!(
        body_str.contains("Enabled"),
        "status=Enabled 应从 query 提取并反序列化为 enum，实际: {body_str}"
    );
}

#[tokio::test]
async fn test_path_and_query_with_missing_optional_enum_query() {
    // 缺失 Option<enum> query 字段时不应报错
    let app = make_router(|r| {
        r.route("/tools/{tool_id}/status", get(get_tool_status_handler))
    })
    .await;

    let req = Request::builder()
        .method(Method::GET)
        .uri("/tools/tool-456/status") // 无 ?status=...
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "缺失 Option query 字段应工作"
    );

    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        body_str.contains("None"),
        "缺失 query 时 status 应为 None，实际: {body_str}"
    );
}
```

- [ ] **Step 4: 新增 path+query 含 flatten pagination 的测试**

继续追加：

```rust
// ==================== 测试 9: path+query 含 #[serde(flatten)] pagination ====================

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, Params)]
pub struct ListArtifactsTestRequest {
    #[param(source = "path")]
    pub project_id: String,
    #[param(source = "query")]
    pub file_type: Option<String>,
    #[serde(flatten)]
    #[param(source = "query")]
    pub pagination: common::api::PaginationParams,
}

#[derive(Debug, serde::Serialize)]
pub struct ListArtifactsTestResponse {
    pub project_id: String,
    pub file_type: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[generate_http_handler]
pub async fn list_artifacts_test(
    _ctx: RequestContext,
    params: ListArtifactsTestRequest,
) -> Result<ListArtifactsTestResponse, Error> {
    Ok(ListArtifactsTestResponse {
        project_id: params.project_id,
        file_type: params.file_type,
        limit: params.pagination.limit,
        offset: params.pagination.offset,
    })
}

#[tokio::test]
async fn test_path_and_query_with_flattened_pagination_works() {
    // pagination 字段使用 #[serde(flatten)]，query string 中是 limit/offset 而非 pagination[limit]
    let app = make_router(|r| {
        r.route("/projects/{project_id}/artifacts", get(list_artifacts_test_handler))
    })
    .await;

    let req = Request::builder()
        .method(Method::GET)
        .uri("/projects/proj-123/artifacts?file_type=txt&limit=10&offset=20")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "path+query 含 flatten pagination 应工作"
    );

    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains("proj-123"), "project_id 应从 path 提取");
    assert!(body_str.contains("txt"), "file_type=txt 应从 query 提取");
    assert!(body_str.contains("10"), "limit=10 应从 flatten query 提取");
    assert!(body_str.contains("20"), "offset=20 应从 flatten query 提取");
}

#[tokio::test]
async fn test_path_and_query_with_flattened_pagination_missing() {
    // 缺失 flatten pagination 字段时不应报错（PaginationParams impl Default）
    let app = make_router(|r| {
        r.route("/projects/{project_id}/artifacts", get(list_artifacts_test_handler))
    })
    .await;

    let req = Request::builder()
        .method(Method::GET)
        .uri("/projects/proj-456/artifacts") // 无任何 query
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "缺失 flatten pagination 字段应工作"
    );
}
```

- [ ] **Step 5: 跑全部新增测试确认全部失败**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --test http_handler_macro_test 2>&1 | tail -30`
Expected: 至少 6 个测试 FAIL（400 错误），证明当前宏对 path+query 完全失效

- [ ] **Step 6: Commit**

```bash
git add tests/http_handler_macro_test.rs
git commit -m "test(macro): 新增 path+query 边界测试锁定当前 bug

新增 4 个测试覆盖 path+query 各种边界：
- enum 类型 query 字段（Option<ToolStatus>）
- 缺失 Option<enum> query 字段
- flatten pagination query 字段
- 缺失 flatten pagination 字段

这些测试在当前宏实现下全部失败（400 Bad Request），
为后续 RawQuery 方案修复提供 TDD red 基线。"
```

---

### Task 2: 修改宏 collect 函数追踪 flattened query 字段

**目的：** 让宏能识别 `#[serde(flatten)]` 标注的 query 字段，分别存放，以便后续生成不同的提取代码。

**Files:**
- Modify: `ai-orz-macros/src/lib.rs` (函数 `collect_path_and_query_fields_from_type` 第 480-574 行)

- [ ] **Step 1: 修改函数签名和返回值**

将 `collect_path_and_query_fields_from_type` 的返回值从 `(Vec<(Ident, Type)>, Vec<(Ident, Type)>, usize)` 改为 `(Vec<(Ident, Type)>, Vec<(Ident, Type)>, Vec<(Ident, Type)>, usize)`，新增的 `Vec<(Ident, Type)>` 是 flattened query 字段列表。

```rust
fn collect_path_and_query_fields_from_type(
    path: syn::Path,
) -> (Vec<(Ident, Type)>, Vec<(Ident, Type)>, Vec<(Ident, Type)>, usize) {
    // ...
    let mut path_fields = Vec::new();
    let mut query_fields = Vec::new();
    let mut flattened_query_fields = Vec::new();  // 新增
    // ...
}
```

- [ ] **Step 2: 修改字段收集循环识别 #[serde(flatten)]**

在原循环中（处理 `#[param(source = "query")]` 分支），同时检查 `#[serde(flatten)]`：

```rust
"query" => {
    if let Some(ident) = &field.ident {
        // 检查是否有 #[serde(flatten)] 属性
        let is_flattened = field.attrs.iter().any(|attr| {
            if attr.path().is_ident("serde") {
                if let Meta::List(meta_list) = &attr.meta {
                    let tokens_str = meta_list.tokens.to_string();
                    return tokens_str.contains("flatten");
                }
            }
            false
        });

        if is_flattened {
            flattened_query_fields.push((ident.clone(), field.ty.clone()));
        } else {
            query_fields.push((ident.clone(), field.ty.clone()));
        }
    }
}
```

- [ ] **Step 3: 修改返回值**

```rust
return (path_fields, query_fields, flattened_query_fields, total_named_fields);
```

最末尾的 panic 处也对应修改。

- [ ] **Step 4: 修改调用处解构**

在 `generate_http_handler` 中：

```rust
let (path_fields, query_fields, flattened_query_fields, total_named_fields) =
    collect_path_and_query_fields_from_type(params_ty_path);
```

新增提取 flattened query 字段 idents/types：

```rust
let flattened_query_idents: Vec<Ident> = flattened_query_fields
    .iter()
    .map(|(ident, _)| ident.clone())
    .collect();
let has_flattened_query = !flattened_query_idents.is_empty();
```

- [ ] **Step 5: 调整 (true, true) 之外的分支**

`(true, false)`、`(false, true)`、`(false, false)` 分支不使用 query 提取，但需要兼容新签名。这些分支的代码逻辑不变，但需要确认 `flattened_query_idents` 在它们中不被错误使用。

由于 `has_query` 仅检查 `query_fields.is_empty()`，原来含 flatten 的字段也算 query。现在拆分后 `has_query` 应该是 `!query_fields.is_empty() || !flattened_query_fields.is_empty()`：

```rust
let has_query = !query_idents.is_empty() || !flattened_query_idents.is_empty();
```

- [ ] **Step 6: 验证编译通过**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo check -p ai-orz-macros 2>&1 | tail -10`
Expected: PASS（可能有 warning，不应有 error）

- [ ] **Step 7: 验证原有 (false, true) 分支仍工作**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --test http_handler_macro_test test_query_only_get_works_with_query_string 2>&1 | tail -10`
Expected: PASS（query-only 测试不应受影响）

- [ ] **Step 8: Commit**

```bash
git add ai-orz-macros/src/lib.rs
git commit -m "refactor(macro): collect_path_and_query_fields 追踪 flattened query

将函数返回值从 (path, query, total) 改为
(path, query, flattened_query, total)，分别存放
#[serde(flatten)] query 字段，为后续 (true, true)
分支按 flatten 生成不同提取代码做准备。"
```

---

### Task 3: 重写 (true, true) 分支用 RawQuery + serde_json::Value 提取

**目的：** 用 hygiene-safe 方式从 query string 提取 query 字段，不引用 handler 文件作用域之外的自定义类型。

**Files:**
- Modify: `ai-orz-macros/src/lib.rs` ((true, true) 分支第 337-379 行)

- [ ] **Step 1: 设计 query 提取的代码生成模板**

宏生成的代码模式（在测试中手动验证后再写入宏）：

```rust
// 1. 提取 RawQuery
let raw_query_str: Option<&str> = raw_query.0.as_deref();

// 2. 解析为 HashMap<String, String> 并构建 serde_json::Value（带类型推断）
let query_value: serde_json::Value = if let Some(qs) = raw_query_str {
    let query_map: std::collections::HashMap<String, String> =
        serde_urlencoded::from_str(qs).unwrap_or_default();
    let mut obj = serde_json::Map::new();
    for (k, v) in &query_map {
        let parsed: serde_json::Value = if v == "true" {
            serde_json::Value::Bool(true)
        } else if v == "false" {
            serde_json::Value::Bool(false)
        } else if v == "null" {
            serde_json::Value::Null
        } else if let Ok(n) = v.parse::<i64>() {
            serde_json::Value::Number(n.into())
        } else if let Ok(n) = v.parse::<f64>() {
            serde_json::Number::from_f64(n)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::String(v.clone()))
        } else {
            serde_json::Value::String(v.clone())
        };
        obj.insert(k.clone(), parsed);
    }
    serde_json::Value::Object(obj)
} else {
    serde_json::Value::Object(serde_json::Map::new())
};

// 3. 提取非 flatten query 字段（按字段名）
// 对每个 query_field: params.{ident} = serde_json::from_value(query_value.get("{name}").cloned().unwrap_or(Null))

// 4. 提取 flatten query 字段（用整个 query_value 反序列化）
// 对每个 flattened_query_field: params.{ident} = serde_json::from_value(query_value.clone())
```

- [ ] **Step 2: 重写 (true, true) 分支**

将原 `(true, true)` 分支替换为：

```rust
(true, true) => {
    let path_tuple = quote! { ( #( #path_idents, )* ) };
    let path_ty_tuple = quote! { ( #( #path_types, )* ) };
    let assign_paths = quote! { #( params.#path_idents = #path_idents; )* };

    // 为非 flatten query 字段生成提取代码
    let extract_query_fields = {
        let query_idents_str: Vec<String> = query_idents.iter().map(|i| i.to_string()).collect();
        quote! {
            #(
                if let Some(__v) = __query_value.get(#query_idents_str) {
                    if let Ok(__parsed) = serde_json::from_value(__v.clone()) {
                        params.#query_idents = __parsed;
                    }
                }
            )*
        }
    };

    // 为 flatten query 字段生成提取代码
    let extract_flattened_query_fields = {
        quote! {
            #(
                if let Ok(__parsed) = serde_json::from_value(__query_value.clone()) {
                    params.#flattened_query_idents = __parsed;
                }
            )*
        }
    };

    // 判断是否所有非 path 字段都是 query（即无 body 字段）
    let total_query_fields = query_fields.len() + flattened_query_fields.len();
    if total_named_fields == path_fields.len() + total_query_fields {
        // path+query only GET：无 Json 提取器
        quote! {
            #item_fn

            pub async fn #handler_ident(
                axum::extract::Extension(ctx): axum::extract::Extension<RequestContext>,
                axum::extract::Path(#path_tuple): axum::extract::Path<#path_ty_tuple>,
                axum::extract::RawQuery(__raw_query): axum::extract::RawQuery,
            ) -> ::std::result::Result<axum::Json<common::api::ApiResponse<#output_ty>>, common::error::Error> {
                let mut params = <#params_ty as ::std::default::Default>::default();

                // 解析 query string 并构建 serde_json::Value（带类型推断）
                let __query_value: serde_json::Value = if let Some(__qs) = __raw_query.0.as_deref() {
                    let __query_map: std::collections::HashMap<String, String> =
                        serde_urlencoded::from_str(__qs).unwrap_or_default();
                    let mut __obj = serde_json::Map::new();
                    for (__k, __v) in &__query_map {
                        let __parsed: serde_json::Value = if __v == "true" {
                            serde_json::Value::Bool(true)
                        } else if __v == "false" {
                            serde_json::Value::Bool(false)
                        } else if __v == "null" {
                            serde_json::Value::Null
                        } else if let Ok(__n) = __v.parse::<i64>() {
                            serde_json::Value::Number(__n.into())
                        } else if let Ok(__n) = __v.parse::<f64>() {
                            serde_json::Number::from_f64(__n)
                                .map(serde_json::Value::Number)
                                .unwrap_or(serde_json::Value::String(__v.clone()))
                        } else {
                            serde_json::Value::String(__v.clone())
                        };
                        __obj.insert(__k.clone(), __parsed);
                    }
                    serde_json::Value::Object(__obj)
                } else {
                    serde_json::Value::Object(serde_json::Map::new())
                };

                // 提取非 flatten query 字段
                #extract_query_fields
                // 提取 flatten query 字段
                #extract_flattened_query_fields
                // 提取 path 字段（path 优先级最高）
                #assign_paths

                let __result = #core_ident(ctx, params).await?;
                Ok(axum::Json(common::api::ApiResponse::success(__result)))
            }
        }
    } else {
        // path + query + body 混合：保留 Json 提取器
        quote! {
            #item_fn

            pub async fn #handler_ident(
                axum::extract::Extension(ctx): axum::extract::Extension<RequestContext>,
                axum::extract::Path(#path_tuple): axum::extract::Path<#path_ty_tuple>,
                axum::extract::RawQuery(__raw_query): axum::extract::RawQuery,
                axum::Json(mut params): axum::Json<#params_ty>,
            ) -> ::std::result::Result<axum::Json<common::api::ApiResponse<#output_ty>>, common::error::Error> {
                // 解析 query string 并构建 serde_json::Value
                let __query_value: serde_json::Value = if let Some(__qs) = __raw_query.0.as_deref() {
                    let __query_map: std::collections::HashMap<String, String> =
                        serde_urlencoded::from_str(__qs).unwrap_or_default();
                    let mut __obj = serde_json::Map::new();
                    for (__k, __v) in &__query_map {
                        let __parsed: serde_json::Value = if __v == "true" {
                            serde_json::Value::Bool(true)
                        } else if __v == "false" {
                            serde_json::Value::Bool(false)
                        } else if __v == "null" {
                            serde_json::Value::Null
                        } else if let Ok(__n) = __v.parse::<i64>() {
                            serde_json::Value::Number(__n.into())
                        } else if let Ok(__n) = __v.parse::<f64>() {
                            serde_json::Number::from_f64(__n)
                                .map(serde_json::Value::Number)
                                .unwrap_or(serde_json::Value::String(__v.clone()))
                        } else {
                            serde_json::Value::String(__v.clone())
                        };
                        __obj.insert(__k.clone(), __parsed);
                    }
                    serde_json::Value::Object(__obj)
                } else {
                    serde_json::Value::Object(serde_json::Map::new())
                };

                // 优先级：path > query > body
                // 先用 query 覆盖 body 字段
                #extract_query_fields
                #extract_flattened_query_fields
                // 最后用 path 覆盖
                #assign_paths

                let __result = #core_ident(ctx, params).await?;
                Ok(axum::Json(common::api::ApiResponse::success(__result)))
            }
        }
    }
}
```

- [ ] **Step 3: 确认 ai-orz-macros Cargo.toml 已有 serde_urlencoded 依赖**

Run: `grep serde_urlencoded ai-orz-macros/Cargo.toml || echo "需要添加"`
Expected: 输出 "需要添加" 或显示已有依赖

如果未添加，在 `ai-orz-macros/Cargo.toml` 的 `[dependencies]` 新增：

```toml
serde_urlencoded = "0.7"
serde_json = "1"
```

注意：宏 crate 在生成代码中引用 `serde_urlencoded` 和 `serde_json`，必须确保被引用的 crate 在最终编译的 crate（如 `ai_orz`）中可见。这两个 crate 在主项目已使用，无需在 ai-orz-macros 的 Cargo.toml 中声明也能工作（因为宏生成代码在调用方作用域中编译）。如果出现"cannot find"错误，则在 ai-orz-macros/Cargo.toml 中补依赖。

- [ ] **Step 4: 验证宏编译通过**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo check -p ai-orz-macros 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: 验证主项目编译通过**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo check 2>&1 | tail -15`
Expected: PASS（不应有 error）

- [ ] **Step 6: 跑 Task 1 的失败测试确认现在通过**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --test http_handler_macro_test 2>&1 | tail -25`
Expected: 所有 9 个测试 PASS（含原 7 个 + 新增 4 个 = 共 11 个，但去重后 9 个；具体数量以实际为准）

- [ ] **Step 7: Commit**

```bash
git add ai-orz-macros/src/lib.rs ai-orz-macros/Cargo.toml
git commit -m "fix(macro): (true, true) 分支支持无 body 的 path+query GET

用 RawQuery + serde_json::Value 替代 Query<ParamsTy>，规避：
1. Query<ParamsTy> 反序列化必填 path/body 字段失败（400）
2. Json<ParamsTy> 拒绝空 body GET（400）
3. 临时 Query struct 的 macro hygiene 问题

新方案：
- RawQuery 提取原始 query 字符串（不会因缺失字段报错）
- serde_urlencoded 解析为 HashMap，再构建 serde_json::Value（带类型推断）
- 非 flatten 字段：from_value(query_value.get(name))
- flatten 字段：from_value(query_value.clone())（如 PaginationParams）
- 类型信息通过 params.{ident} = parsed 赋值推导，不引用作用域外类型

子分支：
- path+query only（无 body 字段）：Path + RawQuery + Default::default()
- path+query+body 混合：Path + RawQuery + Json，path > query > body 优先级"
```

---

### Task 4: 扩展测试覆盖优先级边界 case

**目的：** 验证 (true, true) 混合分支的 path > query > body 优先级正确，且 query 完全覆盖 body 中的同名字段。

**Files:**
- Modify: `tests/http_handler_macro_test.rs`

- [ ] **Step 1: 修复 `test_priority_path_greater_than_query_greater_than_body` 测试**

当前测试已有但失败。验证它在 Task 3 修复后通过：

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --test http_handler_macro_test test_priority_path_greater_than_query_greater_than_body 2>&1 | tail -10`
Expected: PASS（path > query > body 优先级正确）

- [ ] **Step 2: 新增 query 覆盖 body 同名字段的测试**

在 `tests/http_handler_macro_test.rs` 末尾追加：

```rust
// ==================== 测试 10: query 覆盖 body 同名字段 ====================

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, Params)]
pub struct MixedOverrideRequest {
    #[param(source = "path")]
    pub id: String,
    #[param(source = "query")]
    pub name: Option<String>,
    pub name: String, // body 字段，与 query 同名
}

#[derive(Debug, serde::Serialize)]
pub struct MixedOverrideResponse {
    pub id: String,
    pub name: String,
}

#[generate_http_handler]
pub async fn mixed_override(
    _ctx: RequestContext,
    params: MixedOverrideRequest,
) -> Result<MixedOverrideResponse, Error> {
    Ok(MixedOverrideResponse {
        id: params.id,
        name: params.name,
    })
}
```

注意：Rust 不允许同名字段，所以需要将 body 字段命名为 `name_from_body`，query 字段为 `name`，response 显示 `query 覆盖 body` 还是相反。重新设计：

```rust
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, Params)]
pub struct MixedOverrideRequest {
    #[param(source = "path")]
    pub id: String,
    #[param(source = "query")]
    pub query_name: Option<String>,
    pub body_name: String,
}

#[derive(Debug, serde::Serialize)]
pub struct MixedOverrideResponse {
    pub id: String,
    pub query_name: Option<String>,
    pub body_name: String,
}

#[generate_http_handler]
pub async fn mixed_override(
    _ctx: RequestContext,
    params: MixedOverrideRequest,
) -> Result<MixedOverrideResponse, Error> {
    Ok(MixedOverrideResponse {
        id: params.id,
        query_name: params.query_name,
        body_name: params.body_name,
    })
}

#[tokio::test]
async fn test_mixed_path_query_body_all_extracted_correctly() {
    // path+query+body 混合 PUT：每个字段从对应来源提取
    let app = make_router(|r| {
        r.route("/items/{id}", put(mixed_override_handler))
    })
    .await;

    let body = r#"{"id":"from_body","query_name":"from_body","body_name":"body_value"}"#;
    let req = Request::builder()
        .method(Method::PUT)
        .uri("/items/path_id?query_name=from_query")
        .header("Content-Type", "application/json")
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains("path_id"), "id 应取自 path");
    assert!(body_str.contains("from_query"), "query_name 应取自 query");
    assert!(body_str.contains("body_value"), "body_name 应取自 body");
}
```

- [ ] **Step 3: 新增 path+query 含数值类型字段的测试**

```rust
// ==================== 测试 11: path+query 含数值类型 ====================

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, Params)]
pub struct GetNumberRequest {
    #[param(source = "path")]
    pub item_id: String,
    #[param(source = "query")]
    pub count: Option<u32>,
    #[param(source = "query")]
    pub rate: Option<f64>,
}

#[derive(Debug, serde::Serialize)]
pub struct GetNumberResponse {
    pub item_id: String,
    pub count: Option<u32>,
    pub rate: Option<f64>,
}

#[generate_http_handler]
pub async fn get_number(
    _ctx: RequestContext,
    params: GetNumberRequest,
) -> Result<GetNumberResponse, Error> {
    Ok(GetNumberResponse {
        item_id: params.item_id,
        count: params.count,
        rate: params.rate,
    })
}

#[tokio::test]
async fn test_path_and_query_with_numeric_types_works() {
    // query 字段是数值类型（u32, f64），需要 serde_json::Value 正确推断
    let app = make_router(|r| {
        r.route("/items/{item_id}/numbers", get(get_number_handler))
    })
    .await;

    let req = Request::builder()
        .method(Method::GET)
        .uri("/items/item-789/numbers?count=42&rate=3.14")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains("item-789"), "item_id 应从 path 提取");
    assert!(body_str.contains("42"), "count=42 应从 query 提取");
    assert!(body_str.contains("3.14"), "rate=3.14 应从 query 提取");
}
```

- [ ] **Step 4: 跑全部测试**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --test http_handler_macro_test 2>&1 | tail -30`
Expected: 所有测试 PASS

- [ ] **Step 5: Commit**

```bash
git add tests/http_handler_macro_test.rs
git commit -m "test(macro): 扩展 path+query 边界测试覆盖优先级和数值类型

新增 2 个测试：
- path+query+body 混合 PUT：验证三种来源各取其值
- path+query 含数值类型字段（u32, f64）

至此 (true, true) 分支测试覆盖矩阵：
- path+query only GET（无 body）
- path+query+body 混合 PUT
- enum 类型 query 字段
- flatten pagination query 字段
- 缺失 Option query 字段
- 缺失 flatten pagination 字段
- 数值类型 query 字段
- path > query > body 优先级"
```

---

### Task 5: 验证 10 个生产 path+query struct 编译通过

**目的：** 确保新宏对所有生产 path+query struct 都能正确生成代码，不引入编译错误。

**Files:**
- 无修改文件，仅验证

- [ ] **Step 1: 跑全量编译检查**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo check --release 2>&1 | tail -20`
Expected: PASS（所有 handler 文件能正确生成 handler 函数）

- [ ] **Step 2: 跑全量测试套件验证未破坏现有行为**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --lib 2>&1 | tail -15`
Expected: PASS（特别是 seed_handler_test 4 个测试仍通过）

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --test http_handler_macro_test 2>&1 | tail -15`
Expected: 所有 11 个测试 PASS

- [ ] **Step 3: 抽样验证生产 handler 编译产物**

随便选一个 path+query handler 文件，检查宏展开后是否能编译：

Run: `PROTOC=/opt/homebrew/bin/protoc cargo expand --lib --path src/handlers/finance/mcp_tool/list_mcp_tools_by_server.rs 2>&1 | grep -A 30 "list_mcp_tools_by_server_handler" | head -50`
Expected: 宏生成的 handler 函数体包含 `axum::extract::RawQuery` 和 `serde_json::from_value` 调用

如 `cargo expand` 不可用，跳过此步。

- [ ] **Step 4: 记录测试结果**

如所有测试通过，无需 commit（仅验证）。
如有失败，记录失败用例并修复（可能是某个生产 struct 的字段类型未 impl Default 或 serde::Deserialize）。

---

### Task 6: 更新设计文档同步宏修复后的行为

**目的：** 让 `docs/archive/design-archive/unified-idl-http-handler.md` 反映 (true, true) 分支的新行为，移除"path+query GET 需带 body"的限制说明（如有）。

**Files:**
- Modify: `docs/archive/design-archive/unified-idl-http-handler.md`

- [ ] **Step 1: 阅读现有文档相关章节**

Run: Read `docs/archive/design-archive/unified-idl-http-handler.md`
重点查看：
- 第 51-59 行 `#[param(source = ...)]` 说明
- 第 180-188 行 "支持的组合" 表
- 第 213-220 行 优先级规则

- [ ] **Step 2: 更新"支持的组合"表**

在第 183 行 `有 path + 有 query` 行后补充说明：

```markdown
| 有 `path` + 有 `query` | `Path(...)` + `RawQuery` (+ `Json(...)` 当有 body 字段时) | path > query > body |

**子分支自动判定：**
- 当所有非 path 字段都是 query 字段（无 body 字段）时，宏走"path+query only"分支：仅 `Path + RawQuery`，无 Json 提取器，params 用 `Default::default()` 构造空实例后用 query/path 值填充。典型场景：`GET /items/{id}?verbose=true`。
- 当存在 body 字段（非 path 非 query）时，宏走"path+query+body 混合"分支：`Path + RawQuery + Json`，body 提供基础值，query 覆盖 body 同名字段，path 最后覆盖。典型场景：`PUT /items/{id}?verbose=true` body `{"name":"...","body_field":"..."}`。
```

- [ ] **Step 3: 新增 "Query 字段提取实现细节" 小节**

在"支持的组合"小节后追加：

```markdown
### Query 字段提取实现细节

为避免 macro hygiene 问题（handler 文件可能未导入 query 字段类型如 `ToolStatus`），宏不生成临时 Query struct，而是：

1. 用 `axum::extract::RawQuery` 提取原始 query 字符串
2. `serde_urlencoded` 解析为 `HashMap<String, String>`
3. 构建 `serde_json::Value` 对象，按值内容推断类型（bool / number / null / string）
4. 对非 `#[serde(flatten)]` query 字段：`serde_json::from_value(query_value.get(name).cloned())` 反序列化，目标类型由 `params.{ident} = parsed` 赋值推导
5. 对 `#[serde(flatten)]` query 字段（如 `pagination: PaginationParams`）：用整个 `query_value` 反序列化

**关键优势**：宏生成代码不引用任何自定义类型名（如 `ToolStatus`），全部通过类型推导完成反序列化，因此 handler 文件无需为 query 字段类型额外 `use` 导入。
```

- [ ] **Step 4: 在文档末尾新增 "修复历史" 小节**

```markdown
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
```

- [ ] **Step 5: Commit 文档更新**

```bash
git add docs/archive/design-archive/unified-idl-http-handler.md
git commit -m "docs: 同步 (true, true) 宏分支 RawQuery 修复方案

- 更新支持的组合表：补充子分支自动判定逻辑
- 新增 Query 字段提取实现细节小节
- 新增修复历史记录 RawQuery 方案及替代临时 struct 的原因"
```

---

### Task 7: 最终回归测试与提交

- [ ] **Step 1: 跑全量测试套件**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test 2>&1 | tail -30`
Expected: 所有测试 PASS

- [ ] **Step 2: 跑宏测试专项**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --test http_handler_macro_test 2>&1 | tail -20`
Expected: 所有 11+ 测试 PASS

- [ ] **Step 3: 跑 seed handler 测试**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --lib seed_handler_test 2>&1 | tail -10`
Expected: 4 个测试 PASS

- [ ] **Step 4: 推送所有 commit**

```bash
git log --oneline -8
git push
```

- [ ] **Step 5: 输出最终总结**

输出修复成果总结，包括：
- 修复的 bug 描述（(true, true) 分支两个 400 错误场景）
- 影响范围（10 个生产 struct）
- 修复方案（RawQuery + serde_json::Value 类型推导）
- 测试覆盖矩阵（11 个测试覆盖各种边界）
- 文档更新内容

---

## 自查清单（Self-Review）

执行前请确认：

- [ ] Task 1 中所有新增测试在 Task 3 修复前 FAIL，修复后 PASS（验证 TDD red→green）
- [ ] Task 3 中宏生成代码不引用任何 handler 文件作用域外的自定义类型名
- [ ] Task 4 测试覆盖矩阵完整：path+query only / path+query+body / enum / flatten / 数值 / 缺失 Option
- [ ] Task 5 所有 10 个生产 path+query struct 编译通过
- [ ] Task 6 文档同步说明子分支判定逻辑和 RawQuery 方案
- [ ] 没有引入新的 backward-compatibility 破坏（path+body 混合分支保留 Json 提取器，前端调用方式不变）
- [ ] macro hygiene 问题确实被规避（不生成临时 struct，全部用类型推导）
