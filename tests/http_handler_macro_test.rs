//! generate_http_handler 宏的端到端集成测试
//!
//! 验证宏生成的 axum handler 在 5 种参数组合下的实际 HTTP 行为：
//! 1. 空 struct GET（无 body） - 走 (false, false) 的 is_empty_params 子分支
//! 2. path-only GET（无 body） - 走 (true, false) 的 path-only 优化子分支
//! 3. query-only GET（无 body） - 走 (false, true) 分支
//! 4. path+body 混合 PUT（body 含 path 字段） - 走 (true, false) 的 path+body 子分支
//! 5. path+query 混合 GET（无 body） - 走 (true, true) 分支
//!
//! 此外还验证：
//! - 优先级：path > query > body（path 字段从 URL 提取，覆盖 body 中的同名字段）
//! - query 字段支持 Option 类型（缺失时不报错）
//! - 错误的 Content-Type 不会破坏 path-only GET
//!
//! 运行：`PROTOC=/opt/homebrew/bin/protoc cargo test --test http_handler_macro_test`

use ai_orz::pkg::{RequestContext, storage};
use ai_orz_macros::{Params, generate_http_handler};
use axum::Extension;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use axum::routing::{get, put};
use common::config::{DatabaseConfig, StatsConfig, VectorStoreType};
use common::enums::ToolStatus;
use common::error::Error;
use tower::ServiceExt;

// ==================== 测试基建：全局 storage 初始化 ====================

/// 全局 storage 初始化 cell（用 tokio OnceCell 串行化并发初始化）
static STORAGE_INIT: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();

/// 确保 storage 全局单例只初始化一次
/// 使用 tokio::sync::OnceCell::get_or_init 在并发环境下安全初始化
async fn ensure_storage_initialized() {
    STORAGE_INIT
        .get_or_init(|| async {
            // 用临时目录初始化 storage（InMemory 向量 + 临时 SQLite + 临时 Stats）
            // 用 InMemory 而非 default LanceDb，避免 block_in_place 要求 multi-thread runtime
            let tmp = tempfile::tempdir().expect("创建临时目录失败");
            let db_config = DatabaseConfig {
                vector_store_type: VectorStoreType::InMemory,
                ..Default::default()
            };
            let stats_config = StatsConfig::default();
            storage::init(tmp.path(), &db_config, &stats_config).await;
            // 注意：tmp 目录会被 drop，但 SQLite 文件已打开连接，不影响后续测试
            // 如有问题，可改为 static 保存 TempDir
            std::mem::forget(tmp);
        })
        .await;
}

/// 创建测试用 RequestContext（依赖全局 storage 已初始化）
fn make_test_ctx() -> RequestContext {
    RequestContext::new(Some("test-macro-user".to_string()), None)
}

/// 构造注入了 RequestContext 的测试 Router
async fn make_router<F>(route_fn: F) -> Router
where
    F: FnOnce(Router) -> Router,
{
    ensure_storage_initialized().await;
    let ctx = make_test_ctx();
    // 注意：layer 必须在路由添加之后再应用，否则无法被路由提取到
    // axum 的 layer 是在 layer() 调用时的路由上包装，先加路由再 layer 才能正确传递 Extension
    let router = route_fn(Router::new());
    router.layer(Extension(ctx))
}

// ==================== 公共类型 ====================

// ==================== 测试 1: 空 struct GET（无 body） ====================

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, Params)]
pub struct HealthCheckRequest {}

#[derive(Debug, serde::Serialize)]
pub struct HealthCheckResponse {
    pub ok: bool,
}

#[generate_http_handler]
pub async fn health_check(
    _ctx: RequestContext,
    _params: HealthCheckRequest,
) -> Result<HealthCheckResponse, Error> {
    Ok(HealthCheckResponse { ok: true })
}

#[tokio::test]
async fn test_empty_struct_get_works_without_body() {
    let app = make_router(|r| r.route("/health", get(health_check_handler))).await;

    let req = Request::builder()
        .method(Method::GET)
        .uri("/health")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "empty struct GET 在无 body 时应该工作，响应体: {body_str}"
    );
    assert!(
        body_str.contains("\"ok\":true") || body_str.contains("\"ok\": true"),
        "响应体应包含 ok:true，实际: {body_str}"
    );
}

// ==================== 测试 2: path-only GET（无 body） ====================

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, Params)]
pub struct GetItemRequest {
    #[param(source = "path")]
    pub id: String,
}

#[derive(Debug, serde::Serialize)]
pub struct GetItemResponse {
    pub id: String,
}

#[generate_http_handler]
pub async fn get_item(
    _ctx: RequestContext,
    params: GetItemRequest,
) -> Result<GetItemResponse, Error> {
    Ok(GetItemResponse { id: params.id })
}

#[tokio::test]
async fn test_path_only_get_works_without_body() {
    let app = make_router(|r| r.route("/items/{id}", get(get_item_handler))).await;

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

    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        body_str.contains("abc123"),
        "path id 应该从 URL 提取，实际: {body_str}"
    );
}

#[tokio::test]
async fn test_path_only_get_ignores_content_type_header() {
    // 即使前端误发 Content-Type: application/json，path-only GET 也不应解析 body
    let app = make_router(|r| r.route("/items/{id}", get(get_item_handler))).await;

    let req = Request::builder()
        .method(Method::GET)
        .uri("/items/xyz789")
        .header("Content-Type", "application/json")
        .body(Body::empty()) // 没有 body 内容
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "path-only GET 即使带 Content-Type header 也应工作（无 body）"
    );
}

// ==================== 测试 3: query-only GET（无 body） ====================

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, Params)]
pub struct ListItemsRequest {
    #[param(source = "query")]
    pub limit: Option<u32>,
    #[param(source = "query")]
    pub offset: Option<u32>,
}

#[derive(Debug, serde::Serialize)]
pub struct ListItemsResponse {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[generate_http_handler]
pub async fn list_items(
    _ctx: RequestContext,
    params: ListItemsRequest,
) -> Result<ListItemsResponse, Error> {
    Ok(ListItemsResponse {
        limit: params.limit,
        offset: params.offset,
    })
}

#[tokio::test]
async fn test_query_only_get_works_with_query_string() {
    let app = make_router(|r| r.route("/items", get(list_items_handler))).await;

    let req = Request::builder()
        .method(Method::GET)
        .uri("/items?limit=10&offset=20")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "query-only GET 应该从 query string 提取参数"
    );

    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        body_str.contains("10"),
        "limit=10 应该被提取，实际: {body_str}"
    );
    assert!(
        body_str.contains("20"),
        "offset=20 应该被提取，实际: {body_str}"
    );
}

#[tokio::test]
async fn test_query_only_get_works_with_missing_optional_query_params() {
    // query 参数都是 Option，缺失时不应报错
    let app = make_router(|r| r.route("/items", get(list_items_handler))).await;

    let req = Request::builder()
        .method(Method::GET)
        .uri("/items") // 无任何 query
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "query-only GET 在 query 参数缺失时应仍工作"
    );
}

// ==================== 测试 4: path+body 混合 PUT ====================

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, Params)]
pub struct UpdateItemRequest {
    #[param(source = "path")]
    pub id: String,
    pub name: String,
}

#[derive(Debug, serde::Serialize)]
pub struct UpdateItemResponse {
    pub id: String,
    pub name: String,
}

#[generate_http_handler]
pub async fn update_item(
    _ctx: RequestContext,
    params: UpdateItemRequest,
) -> Result<UpdateItemResponse, Error> {
    Ok(UpdateItemResponse {
        id: params.id,
        name: params.name,
    })
}

#[tokio::test]
async fn test_path_and_body_mixed_put_path_overrides_body() {
    // path 字段优先级 > body 字段，path 值应该覆盖 body 中的同名字段
    let app = make_router(|r| r.route("/items/{id}", put(update_item_handler))).await;

    // body 中的 id 是 "from_body"，但 URL path 是 "from_path"
    let body = r#"{"id":"from_body","name":"hello"}"#;
    let req = Request::builder()
        .method(Method::PUT)
        .uri("/items/from_path")
        .header("Content-Type", "application/json")
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "path+body PUT 应该工作（body 含 path 字段）"
    );

    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        body_str.contains("from_path"),
        "id 应取自 URL path 而非 body，实际: {body_str}"
    );
    assert!(
        body_str.contains("hello"),
        "name 应取自 body，实际: {body_str}"
    );
}

// ==================== 测试 4b: path+body 混合 PUT，body 可缺 path 字段 ====================
// serde(default) 让 body 反序列化不强校验缺失字段，id 由 URL path 补充覆盖

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, Params)]
#[serde(default)]
pub struct UpdateItemLooseRequest {
    #[param(source = "path")]
    pub id: String,
    pub name: String,
}

#[derive(Debug, serde::Serialize)]
pub struct UpdateItemLooseResponse {
    pub id: String,
    pub name: String,
}

#[generate_http_handler]
pub async fn update_item_loose(
    _ctx: RequestContext,
    params: UpdateItemLooseRequest,
) -> Result<UpdateItemLooseResponse, Error> {
    Ok(UpdateItemLooseResponse {
        id: params.id,
        name: params.name,
    })
}

#[tokio::test]
async fn test_path_and_body_mixed_put_allows_missing_path_field_in_body() {
    // body 缺 path 字段（id）不应报错：serde(default) 兜底后由 URL path 覆盖
    let app = make_router(|r| r.route("/items-loose/{id}", put(update_item_loose_handler))).await;

    let body = r#"{"name":"hello"}"#;
    let req = Request::builder()
        .method(Method::PUT)
        .uri("/items-loose/from_path")
        .header("Content-Type", "application/json")
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "body 缺 path 字段时应工作（serde(default) + path 覆盖）"
    );

    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        body_str.contains("from_path"),
        "id 应取自 URL path 补充，实际: {body_str}"
    );
    assert!(
        body_str.contains("hello"),
        "name 应取自 body，实际: {body_str}"
    );
}

// ==================== 测试 5: path+query 混合 GET（无 body） ====================

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, Params)]
pub struct GetItemDetailRequest {
    #[param(source = "path")]
    pub id: String,
    #[param(source = "query")]
    pub verbose: Option<bool>,
}

#[derive(Debug, serde::Serialize)]
pub struct GetItemDetailResponse {
    pub id: String,
    pub verbose: bool,
}

#[generate_http_handler]
pub async fn get_item_detail(
    _ctx: RequestContext,
    params: GetItemDetailRequest,
) -> Result<GetItemDetailResponse, Error> {
    Ok(GetItemDetailResponse {
        id: params.id,
        verbose: params.verbose.unwrap_or(false),
    })
}

#[tokio::test]
async fn test_path_and_query_mixed_get_works_without_body() {
    let app = make_router(|r| r.route("/items/{id}/detail", get(get_item_detail_handler))).await;

    let req = Request::builder()
        .method(Method::GET)
        .uri("/items/abc123/detail?verbose=true")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "path+query GET 在无 body 时应该工作"
    );

    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        body_str.contains("abc123"),
        "id 应该从 path 提取，实际: {body_str}"
    );
    assert!(
        body_str.contains("true"),
        "verbose=true 应该从 query 提取，实际: {body_str}"
    );
}

// ==================== 测试 6: 优先级 path > query > body ====================

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, Params)]
pub struct PriorityTestRequest {
    #[param(source = "path")]
    pub id: String,
    #[param(source = "query")]
    pub name: Option<String>,
    pub name_body: String, // body 字段，用于验证 query 优先级
}

#[derive(Debug, serde::Serialize)]
pub struct PriorityTestResponse {
    pub id: String,
    pub name: Option<String>,
    pub name_body: String,
}

#[generate_http_handler]
pub async fn priority_test(
    _ctx: RequestContext,
    params: PriorityTestRequest,
) -> Result<PriorityTestResponse, Error> {
    Ok(PriorityTestResponse {
        id: params.id,
        name: params.name,
        name_body: params.name_body,
    })
}

#[tokio::test]
async fn test_priority_path_greater_than_query_greater_than_body() {
    // path > query > body 优先级验证
    let app = make_router(|r| {
        r.route("/priority/{id}", get(priority_test_handler))
            .route("/priority/{id}", put(priority_test_handler))
    })
    .await;

    // PUT：body 提供 name_body 字段，query 提供 name 字段，path 提供 id 字段
    let body = r#"{"id":"from_body","name":"from_body","name_body":"body_value"}"#;
    let req = Request::builder()
        .method(Method::PUT)
        .uri("/priority/from_path?name=from_query")
        .header("Content-Type", "application/json")
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        body_str.contains("from_path"),
        "path 优先级最高：id 应来自 URL，实际: {body_str}"
    );
    assert!(
        body_str.contains("from_query"),
        "query 优先级高于 body：name 应来自 query，实际: {body_str}"
    );
    assert!(
        body_str.contains("body_value"),
        "body 字段 name_body 应来自 body，实际: {body_str}"
    );
}

// ==================== 测试 7: ApiResponse 包装结构验证 ====================

#[tokio::test]
async fn test_response_is_wrapped_in_api_response() {
    // 宏生成的 handler 应将返回值包装在 ApiResponse::success() 中
    let app = make_router(|r| r.route("/items/{id}", get(get_item_handler))).await;

    let req = Request::builder()
        .method(Method::GET)
        .uri("/items/wrapped123")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    // ApiResponse 序列化后应包含 success 和 data 字段
    assert!(
        body_str.contains("success"),
        "响应应包含 ApiResponse.success 字段，实际: {body_str}"
    );
    assert!(
        body_str.contains("wrapped123"),
        "响应应包含 data.id 字段，实际: {body_str}"
    );
}

// ==================== 测试 8: path+query 含 enum 类型 ====================

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
    let app =
        make_router(|r| r.route("/tools/{tool_id}/status", get(get_tool_status_handler))).await;

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
    let app =
        make_router(|r| r.route("/tools/{tool_id}/status", get(get_tool_status_handler))).await;

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
        r.route(
            "/projects/{project_id}/artifacts",
            get(list_artifacts_test_handler),
        )
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
        r.route(
            "/projects/{project_id}/artifacts",
            get(list_artifacts_test_handler),
        )
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

// ==================== 测试 10: path+query+body 混合 PUT ====================

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
    let app = make_router(|r| r.route("/items/{id}", put(mixed_override_handler))).await;

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
    let app = make_router(|r| r.route("/items/{item_id}/numbers", get(get_number_handler))).await;

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
