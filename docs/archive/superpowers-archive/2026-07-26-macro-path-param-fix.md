# generate_http_handler 宏 path 参数解析修复 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 `generate_http_handler` 宏对 `#[param(source = "path")]`/`#[param(source = "query")]` 属性的解析 bug（`Meta::NameValue` 应改为 `Meta::List` 内层 NameValue），并配套优化 `(true, false)` 分支让纯 path-only 请求结构无需 JSON body 即可工作；同步为 29 个未 derive Default 的 path-only 请求 struct 补 Default；新增宏 trybuild 测试和 HTTP 集成测试覆盖 4 种组合。

**Architecture:**
- 宏修复点：`ai-orz-macros/src/lib.rs` 的 `collect_path_and_query_fields_from_type` 函数第 486-491 行，将 `if let Meta::NameValue(...)` 改为 `if let Meta::List(meta_list) => meta_list.parse_args::<MetaNameValue>()`
- 宏分支优化：`(true, false)` 分支新增"path-only 全字段均为 path 时省去 `Json` 提取器"子分支，使用 `Default::default()` + path 字段赋值
- DTO 改造：29 个 path-only 请求 struct 补 `Default` derive（路径见任务 5）
- 测试基建：新建 `ai-orz-macros/tests/` 目录，使用 `trybuild` 做 macro 生成代码的预期对比；新建 `tests/http_handler_macro.rs` 做 axum 端到端集成测试

**Tech Stack:** Rust + syn 2.0 + quote + axum 0.8 + sqlx + trybuild 1.0（宏测试）+ tokio

---

## 文件结构

### 新建文件

| 文件 | 职责 |
|------|------|
| `ai-orz-macros/tests/macro_codegen.rs` | trybuild 主入口，运行 `tests/ui/*.rs` 的预期对比 |
| `ai-orz-macros/tests/ui/path_only.rs` | 测试用例：纯 path-only 请求 struct |
| `ai-orz-macros/tests/ui/path_only.stderr` | 期望的宏展开报错（trybuild 对比） |
| `ai-orz-macros/tests/ui/query_only.rs` | 测试用例：纯 query-only 请求 struct |
| `ai-orz-macros/tests/ui/path_and_body_mixed.rs` | 测试用例：path + body 混合 |
| `ai-orz-macros/tests/ui/empty_struct.rs` | 测试用例：空 struct |
| `ai-orz-macros/Cargo.toml` | 新增 `[dev-dependencies] trybuild = "1.0"` |
| `tests/http_handler_macro_test.rs` | axum 端到端集成测试 |

### 修改文件

| 文件 | 修改内容 |
|------|---------|
| `ai-orz-macros/src/lib.rs` | 修复 `collect_path_and_query_fields_from_type` 的属性解析 + 优化 `(true, false)` 分支 |
| `common/src/api/agent.rs` | 7 个 path-only struct 补 `Default` |
| `common/src/api/artifact.rs` | 3 个 path-only struct 补 `Default` |
| `common/src/api/mcp_server.rs` | 4 个 path-only struct 补 `Default` |
| `common/src/api/mcp_tool.rs` | 2 个 path-only struct 补 `Default` |
| `common/src/api/message_channel.rs` | 3 个 path-only struct 补 `Default` |
| `common/src/api/model_provider.rs` | 1 个 path-only struct 补 `Default`（DeleteModelProviderRequest） |
| `common/src/api/organization.rs` | 2 个 path-only struct 补 `Default` |
| `common/src/api/project.rs` | 1 个 path-only struct 补 `Default`（GetProjectRequest） |
| `common/src/api/skill.rs` | ~8 个 path-only struct 补 `Default` |
| `common/src/api/tool.rs` | 4 个 path-only struct 补 `Default` |
| `common/src/api/user.rs` | 4 个 path-only struct 补 `Default` |
| `common/src/api/task.rs` | 4 个 path-only struct 补 `Default` |
| `common/src/api/seed.rs` | 1 个 path-only struct 补 `Default`（LoadSeedRequest 是混合，跳过） |
| `docs/archive/design-archive/unified-idl-http-handler.md` | 同步"path-only struct 必须 derive Default"硬约束 |

---

## 任务清单

### Task 0: 准备测试基建（trybuild + 集成测试骨架）

**目的：** 在修复宏之前先建立"先红后绿"的 TDD 基础设施。trybuild 用于精确对比宏展开后的代码，集成测试用于端到端验证 axum 行为。

**Files:**
- Modify: `ai-orz-macros/Cargo.toml`
- Create: `ai-orz-macros/tests/macro_codegen.rs`
- Create: `ai-orz-macros/tests/ui/path_only.rs`
- Create: `ai-orz-macros/tests/ui/path_only.stderr`（修复前应该是失败的，先空着）
- Create: `tests/http_handler_macro_test.rs`

- [ ] **Step 1: 为 ai-orz-macros 添加 trybuild dev-dependency**

修改 `ai-orz-macros/Cargo.toml`，在 `[dependencies]` 之后新增：

```toml
[dev-dependencies]
trybuild = "1.0"
```

- [ ] **Step 2: 创建 trybuild 主入口**

创建 `ai-orz-macros/tests/macro_codegen.rs`：

```rust
//! 宏生成代码的 trybuild 测试
//!
//! 通过对比 `tests/ui/*.rs` 和 `*.stderr` 文件，
//! 验证 `generate_http_handler` 宏在不同 `#[param(source = ...)]` 组合下的展开行为。
//!
//! 运行：`cargo test -p ai-orz-macros --test macro_codegen`

#[test]
fn macro_codegen_ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/parse_path_attr.rs");
    // 后续 task 会添加更多 ui 测试文件
}
```

- [ ] **Step 3: 创建第一个 ui 测试用例（验证 bug 修复前的行为）**

创建 `ai-orz-macros/tests/ui/parse_path_attr.rs`：

```rust
//! 验证 `#[param(source = "path")]` 能被宏正确识别为 path 字段
//!
//! 修复前：宏解析失败，所有字段被当作 body，此测试 compile_fail
//! 修复后：宏解析成功，生成 `Path` 提取器，此测试应 PASS

use ai_orz_macros::generate_http_handler;

pub struct PathOnlyRequest {
    #[param(source = "path")]
    pub id: String,
}

#[generate_http_handler]
pub async fn handler(
    ctx: (),
    params: PathOnlyRequest,
) -> Result<(), ()> {
    let _ = (ctx, params);
    Ok(())
}

fn main() {}
```

注意：这个测试在 Task 1 修复前应该 compile_fail（验证 bug 存在），修复后改为 pass。

- [ ] **Step 4: 创建 axum 集成测试骨架**

创建 `tests/http_handler_macro_test.rs`：

```rust
//! generate_http_handler 宏的端到端集成测试
//!
//! 验证宏生成的 axum handler 在 4 种参数组合下的实际 HTTP 行为：
//! 1. 空 struct GET（无 body）
//! 2. path-only GET（无 body）
//! 3. query-only GET（无 body）
//! 4. path+body 混合 PUT（body 含 path 字段）
//!
//! 运行：`cargo test --test http_handler_macro_test`

use axum::body::Body;
use axum::http::{Request, Method, StatusCode};
use axum::routing::{get, post, put, delete, Router};
use tower::ServiceExt;

// 测试用 handler 会在后续 Task 中按需添加

#[tokio::test]
async fn test_skeleton_compiles() {
    // 仅验证测试基建可编译
    assert_eq!(1 + 1, 2);
}
```

- [ ] **Step 5: 验证 trybuild 框架可运行**

Run: `cd ai-orz-macros && cargo test --test macro_codegen 2>&1 | tail -20`
Expected: 编译失败（因为 `parse_path_attr.rs` 引用了不存在的 `param` macro 或类似错误），但 trybuild 框架本身能运行

- [ ] **Step 6: 修正测试用例使 trybuild 真正运行**

如果 Step 5 失败，可能需要调整测试用例。改用 `t.pass("tests/ui/parse_path_attr.rs")` 替代 `compile_fail`，让 trybuild 在修复后期望成功编译：

```rust
#[test]
fn macro_codegen_ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/parse_path_attr.rs");
}
```

- [ ] **Step 7: Commit**

```bash
git add ai-orz-macros/Cargo.toml ai-orz-macros/tests/
git add tests/http_handler_macro_test.rs
git commit -m "test(macro): 添加 trybuild 测试基建和集成测试骨架"
```

---

### Task 1: 修复宏的属性解析（Meta::NameValue → Meta::List）

**目的：** 修复 `collect_path_and_query_fields_from_type` 函数对 `#[param(source = "path")]` 的解析，让 `path_fields` 和 `query_fields` 真正被填充。

**Files:**
- Modify: `ai-orz-macros/src/lib.rs` (第 486-541 行附近)

- [ ] **Step 1: 先写期望宏展开后能编译通过的 trybuild 测试**

修改 `ai-orz-macros/tests/macro_codegen.rs`：

```rust
#[test]
fn macro_codegen_ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/parse_path_attr.rs");
}
```

Run: `cd ai-orz-macros && cargo test --test macro_codegen 2>&1 | tail -30`
Expected: FAIL，因为宏解析不到 `#[param]`，`path_only.rs` 中的 `PathOnlyRequest` 被当作 body-only，缺少 `Path` 提取器。trybuild 会输出实际编译错误。

- [ ] **Step 2: 修复宏的属性解析逻辑**

修改 `ai-orz-macros/src/lib.rs` 第 482-541 行的 `for field in &item_struct.fields { ... }` 循环，将 `Meta::NameValue` 替换为 `Meta::List` + `parse_args::<MetaNameValue>()`：

```rust
for field in &item_struct.fields {
    for attr in &field.attrs {
        if attr.path().is_ident("param") {
            // `#[param(source = "path")]` 整体形式是 Meta::List：
            // attr.path = "param"，tokens = `source = "path"`
            // 用 parse_args::<MetaNameValue>() 解析 tokens 中的 `name = value` 形式
            if let Meta::List(meta_list) = &attr.meta {
                if let Ok(nv) = meta_list.parse_args::<MetaNameValue>() {
                    if nv.path.is_ident("source") {
                        if let syn::Expr::Lit(syn::ExprLit {
                            lit: Lit::Str(s),
                            ..
                        }) = &nv.value
                        {
                            match s.value().as_str() {
                                "path" => {
                                    if let Some(ident) = &field.ident {
                                        path_fields.push((
                                            ident.clone(),
                                            field.ty.clone(),
                                        ));
                                    }
                                }
                                "query" => {
                                    if let Some(ident) = &field.ident {
                                        query_fields.push((
                                            ident.clone(),
                                            field.ty.clone(),
                                        ));
                                    }
                                }
                                "body" => {
                                    // body is default, no need to collect
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 3: 验证 trybuild 测试通过**

Run: `cd ai-orz-macros && cargo test --test macro_codegen 2>&1 | tail -30`
Expected: PASS（宏现在能正确识别 `#[param(source = "path")]`）

- [ ] **Step 4: 验证主项目编译未引入新错误**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo check --release 2>&1 | tail -20`
Expected: PASS（仅可能有 warning，不应有 error）

- [ ] **Step 5: 跑现有测试套件确保未破坏现有行为**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --lib seed_handler_test 2>&1 | tail -15`
Expected: PASS（seed handler 4 个测试仍通过）

- [ ] **Step 6: Commit**

```bash
git add ai-orz-macros/src/lib.rs ai-orz-macros/tests/
git commit -m "fix(macro): 修复 #[param(source = ...)] 属性解析

将 collect_path_and_query_fields_from_type 中的 Meta::NameValue
改为 Meta::List + parse_args::<MetaNameValue>()，让宏真正识别
#[param(source = \"path\")] 和 #[param(source = \"query\")] 标注。

之前 bug 导致所有 path/query 字段被静默忽略，全部从 body 提取。
修复后 path+body 混合请求零破坏，path-only 请求需 Task 2 优化。"
```

---

### Task 2: 优化 (true, false) 分支让 path-only 请求无需 JSON body

**目的：** 修复后 `(true, false)` 分支仍生成 `Json` 提取器，对纯 path-only 请求（前端无 body）会失败。需要新增"path-only 全字段均为 path 时省去 Json 提取器"子分支。

**Files:**
- Modify: `ai-orz-macros/src/lib.rs` (第 372-398 行附近，`(true, false)` 分支)

- [ ] **Step 1: 编写集成测试验证 path-only GET 在无 body 时失败**

修改 `tests/http_handler_macro_test.rs`：

```rust
use ai_orz_macros::generate_http_handler;
use axum::body::Body;
use axum::http::{Request, Method, StatusCode};
use axum::routing::get;
use axum::Router;
use tower::ServiceExt;

// 测试用 DTO：纯 path-only，derive Default 以支持宏的 path-only 优化分支
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, ai_orz_macros::Params)]
struct GetItemRequest {
    #[param(source = "path")]
    pub id: String,
}

#[derive(Debug, serde::Serialize)]
struct GetItemResponse {
    pub id: String,
}

#[generate_http_handler]
pub async fn get_item(
    _ctx: (),
    params: GetItemRequest,
) -> Result<GetItemResponse, ()> {
    Ok(GetItemResponse { id: params.id })
}

fn build_test_router() -> Router {
    Router::new().route("/items/{id}", get(get_item_handler))
}

#[tokio::test]
async fn test_path_only_get_works_without_body() {
    let app = build_test_router();
    let req = Request::builder()
        .method(Method::GET)
        .uri("/items/abc123")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "path-only GET 在无 body 时应该工作（修复后）"
    );
}
```

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --test http_handler_macro_test test_path_only_get_works_without_body 2>&1 | tail -20`
Expected: FAIL（当前 `(true, false)` 分支仍生成 `Json` 提取器，无 body 时 axum 0.8 返回 4xx）

- [ ] **Step 2: 优化宏的 (true, false) 分支**

修改 `ai-orz-macros/src/lib.rs` 第 372-398 行 `(true, false)` 分支：

```rust
(true, false) => {
    let path_tuple = quote! {
        ( #( #path_idents, )* )
    };
    let path_ty_tuple = quote! {
        ( #( #path_types, )* )
    };
    let assign_paths = quote! {
        #(
            params.#path_idents = #path_idents;
        )*
    };

    // 当所有命名字段都是 path 参数时，无需 Json body 提取器
    // 典型场景：GET /items/{id}, DELETE /items/{id}
    // 要求 Params: Default（用 Default::default() 构造空实例，再用 path 值覆盖）
    if total_named_fields == path_fields.len() {
        quote! {
            #item_fn

            pub async fn #handler_ident(
                axum::extract::Extension(ctx): axum::extract::Extension<RequestContext>,
                axum::extract::Path(#path_tuple): axum::extract::Path<#path_ty_tuple>,
            ) -> ::std::result::Result<axum::Json<common::api::ApiResponse<#output_ty>>, common::error::Error> {
                let mut params = <#params_ty as ::std::default::Default>::default();
                #assign_paths
                let result = #core_ident(ctx, params).await?;
                Ok(axum::Json(common::api::ApiResponse::success(result)))
            }
        }
    } else {
        // path + body 混合：保留 Json 提取器，path 字段后覆盖 body 字段（优先级 path > body）
        quote! {
            #item_fn

            pub async fn #handler_ident(
                axum::extract::Extension(ctx): axum::extract::Extension<RequestContext>,
                axum::extract::Path(#path_tuple): axum::extract::Path<#path_ty_tuple>,
                axum::Json(mut params): axum::Json<#params_ty>,
            ) -> ::std::result::Result<axum::Json<common::api::ApiResponse<#output_ty>>, common::error::Error> {
                #assign_paths
                let result = #core_ident(ctx, params).await?;
                Ok(axum::Json(common::api::ApiResponse::success(result)))
            }
        }
    }
}
```

- [ ] **Step 3: 验证集成测试通过**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --test http_handler_macro_test test_path_only_get_works_without_body 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 4: 验证 path+body 混合仍工作（添加混合测试）**

在 `tests/http_handler_macro_test.rs` 末尾添加：

```rust
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, ai_orz_macros::Params)]
struct UpdateItemRequest {
    #[param(source = "path")]
    pub id: String,
    pub name: String,
}

#[derive(Debug, serde::Serialize)]
struct UpdateItemResponse {
    pub id: String,
    pub name: String,
}

#[generate_http_handler]
pub async fn update_item(
    _ctx: (),
    params: UpdateItemRequest,
) -> Result<UpdateItemResponse, ()> {
    Ok(UpdateItemResponse { id: params.id, name: params.name })
}

#[tokio::test]
async fn test_path_and_body_mixed_put_works_with_body_containing_path_field() {
    let app = Router::new()
        .route("/items/{id}", axum::routing::put(update_item_handler));

    // 前端 PUT 调用：body 中同时包含 path 字段和 body 字段（依赖现状）
    let body = r#"{"id":"abc123","name":"hello"}"#;
    let req = Request::builder()
        .method(Method::PUT)
        .uri("/items/abc123")
        .header("Content-Type", "application/json")
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
```

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --test http_handler_macro_test test_path_and_body_mixed_put_works_with_body_containing_path_field 2>&1 | tail -20`
Expected: PASS（path 字段优先级 > body，path 值覆盖 body 值）

- [ ] **Step 5: Commit**

```bash
git add ai-orz-macros/src/lib.rs tests/http_handler_macro_test.rs
git commit -m "fix(macro): path-only 请求不再要求 JSON body

(true, false) 分支新增子分支：当所有命名字段都是 path 参数时
省去 Json 提取器，使用 Default::default() + path 赋值。

典型场景：GET /items/{id}, DELETE /items/{id} 前端不发 body，
修复前因宏 bug + Json 提取器要求 body 而失败，修复后正常工作。

path+body 混合分支保留 Json(mut params) 提取器，path 字段
后赋值覆盖 body 字段，保证 path > body 优先级。"
```

---

### Task 3: 添加 query-only 和 empty struct 集成测试覆盖

**目的：** 补全 4 种组合的测试矩阵，防止未来回归。

**Files:**
- Modify: `tests/http_handler_macro_test.rs`

- [ ] **Step 1: 添加 query-only GET 测试**

在 `tests/http_handler_macro_test.rs` 末尾添加：

```rust
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, ai_orz_macros::Params)]
struct ListItemsRequest {
    #[param(source = "query")]
    pub limit: Option<u32>,
    #[param(source = "query")]
    pub offset: Option<u32>,
}

#[derive(Debug, serde::Serialize)]
struct ListItemsResponse {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[generate_http_handler]
pub async fn list_items(
    _ctx: (),
    params: ListItemsRequest,
) -> Result<ListItemsResponse, ()> {
    Ok(ListItemsResponse { limit: params.limit, offset: params.offset })
}

#[tokio::test]
async fn test_query_only_get_works_with_query_string() {
    let app = Router::new().route("/items", get(list_items_handler));

    let req = Request::builder()
        .method(Method::GET)
        .uri("/items?limit=10&offset=20")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains("10"), "limit 应该从 query 提取");
    assert!(body_str.contains("20"), "offset 应该从 query 提取");
}
```

- [ ] **Step 2: 添加 empty struct GET 测试**

在 `tests/http_handler_macro_test.rs` 末尾添加：

```rust
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, ai_orz_macros::Params)]
struct HealthCheckRequest {}

#[derive(Debug, serde::Serialize)]
struct HealthCheckResponse {
    pub ok: bool,
}

#[generate_http_handler]
pub async fn health_check(
    _ctx: (),
    _params: HealthCheckRequest,
) -> Result<HealthCheckResponse, ()> {
    Ok(HealthCheckResponse { ok: true })
}

#[tokio::test]
async fn test_empty_struct_get_works_without_body() {
    let app = Router::new().route("/health", get(health_check_handler));

    let req = Request::builder()
        .method(Method::GET)
        .uri("/health")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
```

- [ ] **Step 3: 添加 path+query 混合 GET 测试**

```rust
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, ai_orz_macros::Params)]
struct GetItemDetailRequest {
    #[param(source = "path")]
    pub id: String,
    #[param(source = "query")]
    pub verbose: Option<bool>,
}

#[derive(Debug, serde::Serialize)]
struct GetItemDetailResponse {
    pub id: String,
    pub verbose: bool,
}

#[generate_http_handler]
pub async fn get_item_detail(
    _ctx: (),
    params: GetItemDetailRequest,
) -> Result<GetItemDetailResponse, ()> {
    Ok(GetItemDetailResponse {
        id: params.id,
        verbose: params.verbose.unwrap_or(false),
    })
}

#[tokio::test]
async fn test_path_and_query_mixed_get_works() {
    let app = Router::new().route("/items/{id}/detail", get(get_item_detail_handler));

    let req = Request::builder()
        .method(Method::GET)
        .uri("/items/abc123/detail?verbose=true")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains("abc123"), "id 应该从 path 提取");
    assert!(body_str.contains("true"), "verbose 应该从 query 提取");
}
```

- [ ] **Step 4: 跑全部集成测试**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --test http_handler_macro_test 2>&1 | tail -25`
Expected: 5 个测试全部 PASS

- [ ] **Step 5: Commit**

```bash
git add tests/http_handler_macro_test.rs
git commit -m "test(macro): 补全 4 种 path/query/body 组合的集成测试

新增测试：
- query-only GET（验证 query 字符串提取）
- empty struct GET（验证无 body 路径）
- path+query 混合 GET（验证两种来源组合）
- 此前已有的 path-only GET 和 path+body PUT

合计 5 个端到端测试，覆盖宏所有 (has_path, has_query) 分支。"
```

---

### Task 4: 扩展 trybuild 测试覆盖宏展开细节

**目的：** trybuild 测试比集成测试更精确，能验证宏生成的代码细节（如是否生成 `Path` 提取器）。这一步扩展 trybuild 测试矩阵。

**Files:**
- Modify: `ai-orz-macros/tests/macro_codegen.rs`
- Create: `ai-orz-macros/tests/ui/query_only.rs`
- Create: `ai-orz-macros/tests/ui/path_and_body_mixed.rs`
- Create: `ai-orz-macros/tests/ui/empty_struct.rs`

- [ ] **Step 1: 扩展 macro_codegen.rs 覆盖所有 4 种组合**

修改 `ai-orz-macros/tests/macro_codegen.rs`：

```rust
//! 宏生成代码的 trybuild 测试
//!
//! 通过对比 `tests/ui/*.rs` 编译结果，验证宏在不同参数组合下的展开行为。
//! 修复后所有用例应 PASS（生成正确的 axum extractor 组合）。

#[test]
fn macro_codegen_ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/path_only.rs");
    t.pass("tests/ui/query_only.rs");
    t.pass("tests/ui/path_and_body_mixed.rs");
    t.pass("tests/ui/empty_struct.rs");
}
```

- [ ] **Step 2: 创建 path_only 测试用例**

创建 `ai-orz-macros/tests/ui/path_only.rs`：

```rust
//! path-only 请求 struct：所有字段都是 `#[param(source = "path")]`
//! 期望宏生成 `Extension + Path<...>`，无 Json 提取器

use ai_orz_macros::{generate_http_handler, Params};

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, Params)]
struct GetItemRequest {
    #[param(source = "path")]
    pub id: String,
}

#[derive(Debug, serde::Serialize)]
struct GetItemResponse {
    pub id: String,
}

// 注意：这里需要 RequestContext 类型，否则宏无法编译
// 在测试中通过 type alias 简化
type RequestContext = ();

#[generate_http_handler]
pub async fn get_item(
    _ctx: RequestContext,
    params: GetItemRequest,
) -> Result<GetItemResponse, ()> {
    Ok(GetItemResponse { id: params.id })
}

fn main() {
    // 验证 handler 函数被生成
    let _ = get_item_handler;
}
```

- [ ] **Step 3: 创建 query_only 测试用例**

创建 `ai-orz-macros/tests/ui/query_only.rs`：

```rust
//! query-only 请求 struct：所有字段都是 `#[param(source = "query")]`
//! 期望宏生成 `Extension + Query<Params>`

use ai_orz_macros::{generate_http_handler, Params};

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, Params)]
struct ListItemsRequest {
    #[param(source = "query")]
    pub limit: Option<u32>,
}

#[derive(Debug, serde::Serialize)]
struct ListItemsResponse {
    pub limit: Option<u32>,
}

type RequestContext = ();

#[generate_http_handler]
pub async fn list_items(
    _ctx: RequestContext,
    params: ListItemsRequest,
) -> Result<ListItemsResponse, ()> {
    Ok(ListItemsResponse { limit: params.limit })
}

fn main() {
    let _ = list_items_handler;
}
```

- [ ] **Step 4: 创建 path_and_body_mixed 测试用例**

创建 `ai-orz-macros/tests/ui/path_and_body_mixed.rs`：

```rust
//! path + body 混合请求：部分字段是 path，部分是 body
//! 期望宏生成 `Extension + Path<...> + Json<mut Params>`

use ai_orz_macros::{generate_http_handler, Params};

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, Params)]
struct UpdateItemRequest {
    #[param(source = "path")]
    pub id: String,
    pub name: String,
}

#[derive(Debug, serde::Serialize)]
struct UpdateItemResponse {
    pub id: String,
    pub name: String,
}

type RequestContext = ();

#[generate_http_handler]
pub async fn update_item(
    _ctx: RequestContext,
    params: UpdateItemRequest,
) -> Result<UpdateItemResponse, ()> {
    Ok(UpdateItemResponse { id: params.id, name: params.name })
}

fn main() {
    let _ = update_item_handler;
}
```

- [ ] **Step 5: 创建 empty_struct 测试用例**

创建 `ai-orz-macros/tests/ui/empty_struct.rs`：

```rust
//! 空请求 struct：无字段
//! 期望宏生成 `Extension` only，无 Path/Query/Json 提取器

use ai_orz_macros::{generate_http_handler, Params};

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, Params)]
struct HealthCheckRequest {}

#[derive(Debug, serde::Serialize)]
struct HealthCheckResponse {
    pub ok: bool,
}

type RequestContext = ();

#[generate_http_handler]
pub async fn health_check(
    _ctx: RequestContext,
    _params: HealthCheckRequest,
) -> Result<HealthCheckResponse, ()> {
    Ok(HealthCheckResponse { ok: true })
}

fn main() {
    let _ = health_check_handler;
}
```

- [ ] **Step 6: 跑 trybuild 测试套件**

Run: `cd ai-orz-macros && cargo test --test macro_codegen 2>&1 | tail -30`
Expected: 4 个用例全部 PASS（修复宏后所有组合应正确展开）

- [ ] **Step 7: Commit**

```bash
git add ai-orz-macros/tests/
git commit -m "test(macro): 扩展 trybuild 测试覆盖 4 种 path/query/body 组合

新增 ui 测试用例：
- path_only.rs：验证 path-only struct 生成 Path 提取器
- query_only.rs：验证 query-only struct 生成 Query 提取器
- path_and_body_mixed.rs：验证混合 struct 生成 Path + Json(mut) 提取器
- empty_struct.rs：验证空 struct 仅生成 Extension

trybuild 测试比集成测试更精确，能验证宏生成的代码细节。"
```

---

### Task 5: 为 29 个 path-only 请求 struct 补 Default derive

**目的：** Task 2 的 path-only 优化分支要求 `Params: Default`。当前 38 个 path-only struct 中 9 个已 derive Default，29 个未 derive，需要逐一补全。

**Files:**
- Modify: `common/src/api/agent.rs`
- Modify: `common/src/api/artifact.rs`
- Modify: `common/src/api/mcp_server.rs`
- Modify: `common/src/api/mcp_tool.rs`
- Modify: `common/src/api/message_channel.rs`
- Modify: `common/src/api/model_provider.rs`
- Modify: `common/src/api/organization.rs`
- Modify: `common/src/api/project.rs`
- Modify: `common/src/api/skill.rs`
- Modify: `common/src/api/tool.rs`
- Modify: `common/src/api/user.rs`
- Modify: `common/src/api/task.rs`

- [ ] **Step 1: 编写脚本批量查找需要补 Default 的 struct**

Run: `grep -l '#\[param(source = "path")\]' common/src/api/*.rs | xargs -I {} grep -l 'derive(Debug, Clone, Deserialize' {}`
Expected: 列出所有受影响文件

- [ ] **Step 2: 为 agent.rs 补 Default（7 个 struct）**

修改 `common/src/api/agent.rs`，为以下 struct 在 derive 中加 `Default`：

```rust
// 找到这些 struct 的 derive 行，加 Default：
// GetAgentRequest, UpdateAgentRequest（path+body 混合，可省但加上更安全）,
// UpdateAgentStatusRequest（混合）, DeleteAgentRequest,
// InstallToolPackRequest（path+body 混合）, UninstallToolPackRequest（混合）,
// ListInstalledToolPacksRequest（path-only）, InstallSkillPackRequest（混合）,
// UninstallSkillPackRequest（混合）, ListInstalledSkillPacksRequest（path-only）
```

注意：path+body 混合 struct 走的是 `(true, false)` 的非 path-only 分支（保留 Json 提取器），不要求 Default，但加上无害。**仅 path-only 必须补**：
- `DeleteAgentRequest`
- `ListInstalledToolPacksRequest`
- `ListInstalledSkillPacksRequest`

为这 3 个 struct 的 derive 添加 `Default`：

```rust
// 修改前：#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
// 修改后：#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
```

- [ ] **Step 3: 为 artifact.rs 补 Default（3 个 path-only struct）**

修改 `common/src/api/artifact.rs`，为以下 struct 补 Default：
- `GetArtifactRequest`
- `DeleteArtifactRequest`
- `GetArtifactContentRequest`

- [ ] **Step 4: 为 mcp_server.rs 补 Default（4 个 path-only struct）**

修改 `common/src/api/mcp_server.rs`：
- `GetMcpServerRequest`
- `DeleteMcpServerRequest`
- `UpdateMcpServerRequest`（path+body 混合，但保险起见也加）
- `UpdateMcpServerStatusRequest`（path+body 混合）

仅 path-only 必须补：`GetMcpServerRequest`, `DeleteMcpServerRequest`
其余混合 struct 视情况补。

- [ ] **Step 5: 为 mcp_tool.rs 补 Default**

修改 `common/src/api/mcp_tool.rs`：
- `SyncMcpToolsRequest`（path-only，必补）
- `ListMcpToolsByServerRequest`（path-only，必补）

- [ ] **Step 6: 为 message_channel.rs 补 Default**

修改 `common/src/api/message_channel.rs`：
- `GetMessageChannelRequest`（path-only，必补）
- `DeleteMessageChannelRequest`（path-only，必补）
- `TestMessageChannelConnectionRequest`（path-only，必补）

- [ ] **Step 7: 为 model_provider.rs 补 Default**

修改 `common/src/api/model_provider.rs`：
- `DeleteModelProviderRequest`（path-only，必补）

- [ ] **Step 8: 为 organization.rs 补 Default**

修改 `common/src/api/organization.rs`：
- `GetOrganizationRequest`（path-only，必补）
- `DeleteOrganizationRequest`（path-only，必补）

- [ ] **Step 9: 为 project.rs 补 Default**

修改 `common/src/api/project.rs`：
- `GetProjectRequest`（path-only，必补）

- [ ] **Step 10: 为 skill.rs 补 Default（最多，约 8 个 path-only）**

修改 `common/src/api/skill.rs`：
- `GetSkillRequest`
- `DeleteSkillRequest`
- `ListSkillFilesRequest`
- `GetSkillFileContentRequest`
- `ListAgentSkillsRequest`
- `InstallSkillPackRequest`（混合，但保险起见也加）
- `UninstallSkillPackRequest`（混合）
- `ListInstalledSkillPacksRequest`（path-only）

- [ ] **Step 11: 为 tool.rs 补 Default**

修改 `common/src/api/tool.rs`：
- `DeleteToolRequest`（path-only，必补）
- `BindToolToAgentRequest`（path+path 混合，必补）
- `UnbindToolFromAgentRequest`（path+path 混合，必补）

- [ ] **Step 12: 为 user.rs 补 Default**

修改 `common/src/api/user.rs`：
- `GetUserByUsernameRequest`（path-only，必补）
- `GetUserRequest`（path-only，必补）
- `DeleteUserRequest`（path-only，必补）
- `ListUsersByOrganizationRequest`（path-only，必补）

- [ ] **Step 13: 为 task.rs 补 Default**

修改 `common/src/api/task.rs`：
- `GetTaskRequest`（path-only，必补）
- `ListAgentTasksRequest`（path-only，必补）
- `ListProjectTasksRequest`（path-only，必补）

- [ ] **Step 14: 验证编译通过**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo check --release 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 15: 跑 seed handler 集成测试确认未破坏**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --lib seed_handler_test 2>&1 | tail -10`
Expected: 4 个测试全部 PASS

- [ ] **Step 16: Commit**

```bash
git add common/src/api/
git commit -m "feat(api): 为 29 个 path-only 请求 struct 补 Default derive

配合 Task 2 的宏 path-only 优化分支（使用 Default::default()
构造空实例后用 Path 字段赋值），所有 path-only 请求 struct
现在都 derive Default。

修改的 struct 分布：
- agent.rs: DeleteAgentRequest, ListInstalledToolPacksRequest,
  ListInstalledSkillPacksRequest
- artifact.rs: GetArtifactRequest, DeleteArtifactRequest,
  GetArtifactContentRequest
- mcp_server.rs: GetMcpServerRequest, DeleteMcpServerRequest
- mcp_tool.rs: SyncMcpToolsRequest, ListMcpToolsByServerRequest
- message_channel.rs: GetMessageChannelRequest,
  DeleteMessageChannelRequest, TestMessageChannelConnectionRequest
- model_provider.rs: DeleteModelProviderRequest
- organization.rs: GetOrganizationRequest, DeleteOrganizationRequest
- project.rs: GetProjectRequest
- skill.rs: GetSkillRequest, DeleteSkillRequest,
  ListSkillFilesRequest, GetSkillFileContentRequest,
  ListAgentSkillsRequest, ListInstalledSkillPacksRequest
- tool.rs: DeleteToolRequest, BindToolToAgentRequest,
  UnbindToolFromAgentRequest
- user.rs: GetUserByUsernameRequest, GetUserRequest,
  DeleteUserRequest, ListUsersByOrganizationRequest
- task.rs: GetTaskRequest, ListAgentTasksRequest,
  ListProjectTasksRequest"
```

---

### Task 6: 启动后端做全量端到端回归测试

**目的：** 修复宏后必须验证全项目所有 handler 在真实 HTTP 调用下仍工作。重点验证 path-only GET/DELETE 在前端无 body 调用下的行为。

**Files:**
- 无修改文件，仅手动测试脚本

- [ ] **Step 1: 启动 release 后端**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo build --release 2>&1 | tail -5`
Expected: PASS

Run: 启动 `./target/release/ai_orz`（非阻塞，web_server 类型）

- [ ] **Step 2: 登录获取 token**

```bash
curl -s -c /tmp/ai_orz_cookies.txt -X POST http://localhost:3000/api/v1/organization/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password_hash":"admin123","organization_id":"<YOUR_ORG_ID>"}'
```

将返回的 token 保存到 `/tmp/ai_orz_token.txt`。

- [ ] **Step 3: 验证 path-only GET 接口（无 body）**

测试 representative path-only 接口（覆盖各域）：

```bash
TOKEN=$(cat /tmp/ai_orz_token.txt)
AUTH="Authorization: Bearer $TOKEN"

# HR 域 path-only GET
echo "=== GET /agents/{id} ==="
curl -s -b /tmp/ai_orz_cookies.txt -H "$AUTH" \
  http://localhost:3000/api/v1/hr/agents/<some_agent_id> | head -3

echo "=== GET /skills/{id} ==="
curl -s -b /tmp/ai_orz_cookies.txt -H "$AUTH" \
  http://localhost:3000/api/v1/hr/skills/<some_skill_id> | head -3

# Finance 域 path-only GET
echo "=== GET /attachments/{id} ==="
curl -s -b /tmp/ai_orz_cookies.txt -H "$AUTH" \
  http://localhost:3000/api/v1/finance/attachments/<some_attachment_id> | head -3

# Organization 域 path-only GET
echo "=== GET /organizations/{id} ==="
curl -s -b /tmp/ai_orz_cookies.txt -H "$AUTH" \
  http://localhost:3000/api/v1/organization/organizations/<your_org_id> | head -3

# System 域 path-only GET
echo "=== GET /seed/file/{name} ==="
curl -s -b /tmp/ai_orz_cookies.txt -H "$AUTH" \
  http://localhost:3000/api/v1/system/seed/file/default.json | head -3
```

Expected: 所有接口返回 200 + JSON 数据（修复后无需 body）

- [ ] **Step 4: 验证 path-only DELETE 接口（无 body）**

找一个可删除的测试实体（如临时创建的 attachment/skill）：

```bash
echo "=== DELETE /attachments/{id} ==="
curl -s -b /tmp/ai_orz_cookies.txt -H "$AUTH" \
  -X DELETE http://localhost:3000/api/v1/finance/attachments/<test_attachment_id>
```

Expected: 200 成功删除

- [ ] **Step 5: 验证 query-only GET 接口**

```bash
echo "=== GET /agents?limit=10 ==="
curl -s -b /tmp/ai_orz_cookies.txt -H "$AUTH" \
  http://localhost:3000/api/v1/hr/agents?limit=10 | head -3
```

Expected: 200 + 分页数据

- [ ] **Step 6: 验证 path+body PUT 接口（前端 body 仍含 path 字段）**

```bash
echo "=== PUT /agents/{id} (body 含 id 字段) ==="
curl -s -b /tmp/ai_orz_cookies.txt -H "$AUTH" -H "Content-Type: application/json" \
  -X PUT http://localhost:3000/api/v1/hr/agents/<agent_id> \
  -d '{"id":"<agent_id>","name":"更新后的名字","description":"测试"}' | head -3
```

Expected: 200 + 更新后的数据，path 字段优先级 > body 字段（即 id 取自 URL 而非 body）

- [ ] **Step 7: 验证空 struct GET 接口**

```bash
echo "=== GET /organization/initialize/check ==="
curl -s http://localhost:3000/api/v1/organization/initialize/check

echo "=== GET /system/health-metrics ==="
curl -s -b /tmp/ai_orz_cookies.txt -H "$AUTH" \
  http://localhost:3000/api/v1/system/health-metrics | head -3
```

Expected: 200 + 响应数据

- [ ] **Step 8: 跑完整集成测试套件**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --test http_handler_macro_test 2>&1 | tail -10`
Expected: 所有测试 PASS

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --lib 2>&1 | tail -5`
Expected: 现有测试不破坏

- [ ] **Step 9: 记录测试结果**

如果有任何端点返回错误，记录下来并修复（可能是漏 derive Default，或 path 参数命名不匹配 axum 路由）。

- [ ] **Step 10: Commit 测试结果记录**

如果有 bug 修复，按修复内容 commit；如果全部通过：

```bash
git log -3 --oneline
# 确认前面 5 个 Task 的 commit 都已推送
```

---

### Task 7: 更新设计文档同步宏修复后的行为

**目的：** `docs/archive/design-archive/unified-idl-http-handler.md` 当前文档描述与实现一致（修复前两者不一致），需要补充"path-only struct 必须 derive Default"的硬约束说明。

**Files:**
- Modify: `docs/archive/design-archive/unified-idl-http-handler.md`

- [ ] **Step 1: 阅读现有文档相关章节**

Run: Read `docs/archive/design-archive/unified-idl-http-handler.md` 全文，重点查看：
- 第 51-59 行的 `#[param(source = ...)]` 说明
- 第 180-188 行的"支持的组合"表
- 第 213-220 行的优先级规则

- [ ] **Step 2: 在"支持的组合"表后新增"path-only 约束"小节**

在第 188 行后插入：

```markdown
### 硬约束：path-only struct 必须 derive Default

宏对 `(has_path=true, has_query=false)` 分支的子分支判断：
当请求 struct 的所有命名字段都是 `#[param(source = "path")]`（即 `total_named_fields == path_fields.len()`）时，
宏走"path-only 优化分支"，不生成 `Json` 提取器，而是：

1. 用 `<Params as Default>::default()` 构造空实例
2. 从 `axum::extract::Path` 提取路径参数
3. 用路径值覆盖 params 对应字段

因此所有 path-only 请求 struct 必须 derive `Default`。
对 path+body 混合 struct，宏仍走 `Json(mut params) + Path` 分支，
Default 不是必需的（但加上无害）。

判定规则：
- 所有字段都是 `#[param(source = "path")]` → path-only 优化分支（要求 Default）
- 含 path 字段 + 其他字段（无 #[param] 或 body） → path+body 混合分支（不要求 Default）
- 无 path 字段 → 走 `(false, has_query)` 分支
```

- [ ] **Step 3: 更新优先级规则说明（如需）**

查看第 213-220 行的优先级规则，确认是否需要更新"path-only 时无 body，body 优先级不适用"。

- [ ] **Step 4: 在文档末尾新增"修复历史"小节**

```markdown
## 修复历史

### 2026-07-26: path 参数解析 bug 修复

**问题**：`collect_path_and_query_fields_from_type` 用 `Meta::NameValue` 匹配
`#[param(source = "path")]`，但实际属性形式是 `Meta::List`（`attr.path = "param"`，
`tokens = source = "path"`），导致所有 `#[param]` 标注被静默忽略，
所有 path/query 字段被当作 body 字段处理。

**修复**：改用 `Meta::List + parse_args::<MetaNameValue>()` 正确解析属性。
配套优化 `(true, false)` 分支：path-only 全字段都是 path 时省去 `Json` 提取器。

**影响**：
- path-only GET/DELETE 修复前可能失败（前端无 body 时 axum 拒绝），修复后正常工作
- path+body PUT 修复前后均工作（前端 body 含 path 字段，被 path 覆盖）
- query-only GET 修复后 query 字符串真正生效
- 空 struct GET 无变化（一直工作）

**测试**：新增 `tests/http_handler_macro_test.rs` 覆盖 5 种组合，
新增 `ai-orz-macros/tests/` trybuild 测试覆盖 4 种 ui 用例。
```

- [ ] **Step 5: Commit 文档更新**

```bash
git add docs/archive/design-archive/unified-idl-http-handler.md
git commit -m "docs: 同步宏 path 参数解析修复后的行为说明

- 新增"path-only struct 必须 derive Default"硬约束说明
- 新增"修复历史"章节记录本次 bug 修复的影响范围
- 确认文档与实现一致（修复前文档承诺的 Path + Json 模式从未触发）"
```

---

### Task 8: 推送和总结

- [ ] **Step 1: 推送所有 commit**

```bash
git push
```

- [ ] **Step 2: 总结修复成果**

输出总结，包括：
- 修复的 bug 描述
- 修复前后行为对比表
- 影响范围（多少 struct、多少 handler）
- 新增测试覆盖
- 文档更新内容

---

## 自查清单（Self-Review）

在执行前请确认：

- [ ] 所有 trybuild 测试在 Task 1 修复前 FAIL，修复后 PASS（验证 TDD 红→绿流程）
- [ ] 所有 path-only struct 在 Task 5 补 Default 后编译通过
- [ ] 集成测试覆盖 5 种组合：path-only / query-only / empty / path+body / path+query
- [ ] Task 6 端到端测试至少覆盖 HR/Finance/Organization/System 四个域的 representative 接口
- [ ] 文档同步说明 path-only Default 硬约束
- [ ] 没有引入新的 backward-compatibility 破坏（path+body 混合分支保留 Json 提取器，前端调用方式不变）
