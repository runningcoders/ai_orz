use crate::handlers;
use crate::middleware::{
    a2a_auth_middleware, jwt_auth_middleware, request_context_middleware, require_role_middleware,
};
use axum::{
    Router,
    body::{Body, Bytes},
    http::{Request, Response, StatusCode, header},
    routing::{delete, get, patch, post, put},
};
use common::config::AppConfig;
use common::enums::UserRole;
use std::{
    convert::Infallible,
    sync::Arc,
    task::{Context, Poll},
};
use tower_http::services::ServeDir;
use tower_service::Service;

/// SPA 回退服务：静态文件命中时正常返回；未命中且路径无文件扩展名（前端深链，
/// 如 /login、/projects/:id）时返回 200 + index.html 由前端路由接管。
///
/// 注：不用 ServeDir::not_found_service(ServeFile) —— 该组合会以 404 状态码返回
/// index.html 内容，对 SEO/健康探测/部分客户端不友好。
///
/// `/api/` 前缀路径例外：未匹配任何已注册后端路由时**明确返回 404**（而非 SPA 200），
/// 避免 SPA 回退以 200 + 空 body 掩盖接口缺失/路由错配问题（便于快速定位）。
#[derive(Clone)]
struct SpaFallback {
    serve_dir: ServeDir,
    index_html: Bytes,
}

impl Service<Request<Body>> for SpaFallback {
    type Response = Response<Body>;
    type Error = Infallible;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Response<Body>, Infallible>> + Send>,
    >;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // ServeDir 的 io 错误在 call 阶段转为 500 响应，此处直接透传就绪状态
        match <ServeDir as Service<Request<Body>>>::poll_ready(&mut self.serve_dir, cx) {
            Poll::Ready(_) => Poll::Ready(Ok(())),
            Poll::Pending => Poll::Pending,
        }
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let path = req.uri().path();
        // 后端 API 未匹配任何已注册路由时明确返回 404，避免 SPA 回退以 200 掩盖接口缺失
        if path.starts_with("/api/") {
            let resp = Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::empty())
                .expect("static response always builds");
            return Box::pin(std::future::ready(Ok(resp)));
        }
        // 带扩展名的路径视为静态资源，缺失时由 ServeDir 返回真实 404
        let is_file_path = path.rsplit('/').next().is_some_and(|seg| seg.contains('.'));
        if !is_file_path {
            let resp = Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                .body(Body::from(self.index_html.clone()))
                .expect("static response always builds");
            return Box::pin(std::future::ready(Ok(resp)));
        }
        let fut = <ServeDir as Service<Request<Body>>>::call(&mut self.serve_dir, req);
        Box::pin(async move {
            // ServeDir 的 io 错误极罕见，转换为 500 响应以保持 Error = Infallible
            match fut.await {
                Ok(resp) => Ok::<_, Infallible>(resp.map(Body::new)),
                Err(_) => Ok(Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::empty())
                    .expect("static response always builds")),
            }
        })
    }
}

pub fn create_router(frontend_dist_dir: &str, config: Arc<AppConfig>) -> Router {
    let config_for_card = config.clone();
    // SPA 回退：前端是 Dioxus 客户端路由，深链（如 /login、/projects/:id）直达或刷新时
    // 静态目录中不存在对应文件，需回退到 index.html 由前端路由接管（200 状态码）
    let index_path = std::path::Path::new(frontend_dist_dir).join("index.html");
    let index_html = std::fs::read_to_string(&index_path).unwrap_or_default();
    let spa_service = SpaFallback {
        serve_dir: ServeDir::new(frontend_dist_dir),
        index_html: Bytes::from(index_html),
    };
    Router::new()
        // Public routes - no JWT authentication required
        .nest("/api/v1", public_routes(config.clone()))
        // Protected routes - require valid JWT token
        .nest("/api/v1", protected_routes(config.clone()))
        // A2A Protocol routes
        // Agent Card: 公开发现端点（无需 JWT，只需 RequestContext）
        .route(
            "/.well-known/agent.json",
            get(handlers::a2a::agent_card::get_agent_card).layer(axum::middleware::from_fn(
                move |req, next| request_context_middleware(config_for_card.clone(), req, next),
            )),
        )
        // JSON-RPC: 双模鉴权端点（P1+P2）
        // a2a_auth（外层）：本地 JWT 优先（既有语义），失败回退联邦凭证 +
        // X-Federation-Caller 声明（建联对端节点调用）；request_context（内层）从
        // 注入的请求头创建 RequestContext
        .route(
            "/a2a",
            post(handlers::a2a::jsonrpc::handle_jsonrpc)
                .layer(axum::middleware::from_fn({
                    let config = config.clone();
                    move |req, next| request_context_middleware(config.clone(), req, next)
                }))
                .layer(axum::middleware::from_fn(a2a_auth_middleware)),
        )
        // SSE 流式端点: tasks/sendSubscribe
        .route(
            "/a2a/subscribe",
            post(handlers::a2a::send_subscribe::handle_send_subscribe)
                .layer(axum::middleware::from_fn({
                    let config = config.clone();
                    move |req, next| request_context_middleware(config.clone(), req, next)
                }))
                .layer(axum::middleware::from_fn(jwt_auth_middleware)),
        )
        // A2A 回调端点（公开，外部 Agent 推送任务更新，无需 JWT）
        .route(
            "/a2a/callback/{task_id}",
            post(handlers::a2a::callback::handle_a2a_callback).layer(axum::middleware::from_fn({
                let config = config.clone();
                move |req, next| request_context_middleware(config.clone(), req, next)
            })),
        )
        // 组网：验证配对码 + 交换凭证（机器侧，调用方是对端节点，无本地用户 JWT）
        // 同前缀 /api/v1/organization/links/* 在 root 层直挂（评审稿 D7；与 JWT nest 双挂载已探针实测）
        // 配对码本身鉴权，不进 protected_routes 的 JWT 链
        .route(
            "/api/v1/organization/links/pairing/verify",
            post(handlers::organization::links::verify_pairing_code_handler).layer(
                axum::middleware::from_fn({
                    let config = config.clone();
                    move |req, next| request_context_middleware(config.clone(), req, next)
                }),
            ),
        )
        // 组网：返回本节点组织目录（机器侧，契约凭证鉴权，响应过 redact!）
        .route(
            "/api/v1/organization/links/directory",
            get(handlers::organization::links::get_directory_handler).layer(
                axum::middleware::from_fn({
                    let config = config.clone();
                    move |req, next| request_context_middleware(config.clone(), req, next)
                }),
            ),
        )
        // 组网：能力发现（机器侧，契约凭证鉴权；P3：连接级白名单 + 可调用 Agent 列表）
        .route(
            "/api/v1/organization/links/capabilities",
            get(handlers::organization::links::get_capabilities_handler).layer(
                axum::middleware::from_fn({
                    let config = config.clone();
                    move |req, next| request_context_middleware(config.clone(), req, next)
                }),
            ),
        )
        // 组网：接收对端推送的目录（机器侧，契约凭证鉴权）
        .route(
            "/api/v1/organization/links/directory/sync",
            post(handlers::organization::links::sync_directory_handler).layer(
                axum::middleware::from_fn({
                    let config = config.clone();
                    move |req, next| request_context_middleware(config.clone(), req, next)
                }),
            ),
        )
        .route("/health", get(handlers::health::health))
        // API Notice 日志：请求结束时打印 method/path/status/duration + 请求/响应体预览
        // （仅覆盖上面已注册的接口路由；必须加在 fallback_service 之前，静态资源不打）
        .layer(axum::middleware::from_fn(
            crate::middleware::api_notice_middleware,
        ))
        .fallback_service(spa_service)
}

/// Public routes - do NOT require JWT authentication
/// These are for initialization, login, etc.
fn public_routes(config: Arc<AppConfig>) -> Router {
    use crate::handlers::organization::auth;
    use crate::handlers::organization::initialize_system;
    use crate::handlers::organization::organizations;

    Router::new()
        // System initialization (only when no organizations exist)
        .route(
            "/organization/initialize/check",
            get(initialize_system::check_initialized_handler),
        )
        .route(
            "/organization/initialize",
            post(initialize_system::initialize_system_handler),
        )
        .route(
            "/organization/initialize/progress",
            get(initialize_system::get_initialize_progress_handler),
        )
        // Login/logout - login issues new JWT token
        .route("/organization/auth/login", post(auth::login::login))
        .route("/organization/auth/logout", post(auth::logout::logout))
        .route(
            "/organization/auth/register",
            post(auth::register_by_invite),
        )
        .route(
            "/organization/auth/invite/validate",
            get(auth::validate_invite_code),
        )
        // List all organizations - public query (for login page selection, no login required)
        .route(
            "/organization/list",
            get(organizations::list_organizations_handler),
        )
        // RequestContext 提取中间件（公开路由也需要 log_id 等上下文）
        .layer(axum::middleware::from_fn(move |req, next| {
            request_context_middleware(config.clone(), req, next)
        }))
}

/// Protected routes - require valid JWT authentication
/// All requests without valid token will be redirected to / (login page)
///
/// 中间件执行顺序（洋葱模型）：
/// 1. jwt_auth_middleware（外层）- 验证 JWT，将用户信息写入请求头
/// 2. request_context_middleware（内层）- 从请求头提取信息创建 RequestContext
///
/// 这样 RequestContext 就能包含 JWT 注入的用户信息
fn protected_routes(config: Arc<AppConfig>) -> Router {
    Router::new()
        // HR (Human Resources) routes
        .nest("/hr", hr_routes())
        // Finance (模型管理) routes
        .nest("/finance", finance_routes())
        // System (系统管理) routes - 整体要求 Admin 权限（Admin 和 SuperAdmin 可访问）
        // 备份创建/删除/恢复等高危操作在 handler 内部二次校验 SuperAdmin
        .nest(
            "/system",
            system_routes().layer(axum::middleware::from_fn(|req, next| {
                require_role_middleware(UserRole::Admin, req, next)
            })),
        )
        // Organization (组织管理) routes (protected)
        .nest("/organization", organization_protected_routes())
        // Project routes
        .merge(project_routes())
        // Artifact routes
        .nest("/project", artifact_routes())
        // Task routes
        .merge(task_routes())
        // Current user routes - for user profile
        .nest("/user", user_routes())
        // RequestContext 提取中间件（内层，后执行）
        // 从请求头（包含 JWT 注入的用户信息）创建 RequestContext
        .layer(axum::middleware::from_fn(move |req, next| {
            request_context_middleware(config.clone(), req, next)
        }))
        // JWT 认证中间件（外层，先执行）
        // axum 0.8 中后添加的 layer 在更内层；先添加的 layer 在更外层，最先执行
        .layer(axum::middleware::from_fn(jwt_auth_middleware))
}

fn user_routes() -> Router {
    use crate::handlers::user::profile;
    Router::new()
        .route("/me", get(profile::get_current_user_handler))
        .route("/me", put(profile::update_current_user_handler))
}

/// 飞书身份凭证路由（finance domain identity 分级：身份凭证资产 + OAuth device flow + 绑定快照聚合 + 自动绑定）
fn lark_integration_routes() -> Router {
    use crate::handlers::finance::lark_integration as li;
    Router::new()
        .route("/status", get(li::get_status::get_status_handler))
        .route(
            "/credentials",
            post(li::create_credential::create_credential_handler),
        )
        .route(
            "/credentials/{id}",
            put(li::update_credential::update_credential_handler),
        )
        .route(
            "/credentials/{id}",
            delete(li::delete_credential::delete_credential_handler),
        )
        .route(
            "/credentials/default",
            post(li::set_default_credential::set_default_credential_handler),
        )
        .route("/auth/start", post(li::auth_start::auth_start_handler))
        .route(
            "/auth/complete",
            post(li::auth_complete::auth_complete_handler),
        )
        .route("/auth/status", get(li::auth_status::auth_status_handler))
        .route("/auth/logout", post(li::auth_logout::auth_logout_handler))
        .route("/bind/start", post(li::bind_start::bind_start_handler))
        .route("/bind/status", get(li::bind_status::bind_status_handler))
        .route("/bind/cancel", post(li::bind_cancel::bind_cancel_handler))
}

fn github_integration_routes() -> Router {
    use crate::handlers::finance::github_integration as gi;
    Router::new()
        .route("/status", get(gi::get_status::get_status_handler))
        .route(
            "/credentials",
            post(gi::create_credential::create_credential_handler),
        )
        .route(
            "/credentials/{id}",
            put(gi::update_credential::update_credential_handler),
        )
        .route(
            "/credentials/{id}",
            delete(gi::delete_credential::delete_credential_handler),
        )
        .route(
            "/credentials/default",
            post(gi::set_default_credential::set_default_credential_handler),
        )
}

fn generic_token_integration_routes() -> Router {
    use crate::handlers::finance::generic_token_integration as gt;
    Router::new()
        .route("/status", get(gt::get_status::get_status_handler))
        .route(
            "/credentials",
            post(gt::create_credential::create_credential_handler),
        )
        .route(
            "/credentials/{id}",
            patch(gt::update_credential::update_credential_handler),
        )
        .route(
            "/credentials/{id}",
            delete(gt::delete_credential::delete_credential_handler),
        )
        .route(
            "/credentials/default",
            post(gt::set_default_credential::set_default_credential_handler),
        )
}

fn project_routes() -> Router {
    Router::new()
        .route(
            "/projects",
            post(handlers::project::projects::create_project_handler),
        )
        .route(
            "/projects",
            get(handlers::project::projects::list_projects_handler),
        )
        .route(
            "/projects/query",
            post(handlers::project::projects::query_projects_handler),
        )
        .route(
            "/projects/search",
            post(handlers::project::projects::search_projects_handler),
        )
        .route(
            "/projects/{id}",
            get(handlers::project::projects::get_project_handler),
        )
        .route(
            "/projects/{id}",
            put(handlers::project::projects::update_project_handler),
        )
        .route(
            "/projects/{id}/status",
            put(handlers::project::projects::update_project_status_handler),
        )
}

fn task_routes() -> Router {
    Router::new()
        .route("/tasks", post(handlers::project::task::create_task_handler))
        .route("/tasks", get(handlers::project::task::list_tasks_handler))
        .route(
            "/tasks/query",
            post(handlers::project::task::query_tasks_handler),
        )
        .route(
            "/tasks/search",
            post(handlers::project::task::search_tasks_handler),
        )
        .route(
            "/tasks/{id}",
            get(handlers::project::task::get_task_handler),
        )
        .route(
            "/tasks/{id}",
            put(handlers::project::task::update_task_handler),
        )
        .route(
            "/tasks/{id}/status",
            put(handlers::project::task::update_task_status_handler),
        )
        .route(
            "/tasks/{id}/progress",
            put(handlers::project::task::update_task_progress_handler),
        )
        .route(
            "/projects/{project_id}/tasks",
            get(handlers::project::task::list_project_tasks_handler),
        )
        .route(
            "/agents/{agent_id}/tasks",
            get(handlers::project::task::list_agent_tasks_handler),
        )
}

fn artifact_routes() -> Router {
    Router::new()
        .route(
            "/artifacts",
            post(handlers::project::artifact::create_artifact_handler)
                .get(handlers::project::artifact::list_artifacts_handler),
        )
        .route(
            "/artifacts/text",
            post(handlers::project::artifact::create_text_artifact_handler),
        )
        .route(
            "/artifacts/register-from-path",
            post(handlers::project::artifact::register_artifact_from_path_handler),
        )
        .route(
            "/artifacts/{id}",
            get(handlers::project::artifact::get_artifact_handler)
                .delete(handlers::project::artifact::delete_artifact_handler)
                .put(handlers::project::artifact::update_artifact_handler),
        )
        .route(
            "/artifacts/{id}/content",
            get(handlers::project::artifact::get_artifact_content_handler),
        )
}

fn organization_protected_routes() -> Router {
    // Each handler is in its own file in the subdirectory
    use crate::handlers::organization::links;
    use crate::handlers::organization::organization_me;
    use crate::handlers::organization::organizations;
    use crate::handlers::organization::user;

    Router::new()
        // Get/update current user's organization info
        .route(
            "/me",
            get(organization_me::get_current_organization_handler),
        )
        // 组网：签发配对码（用户侧，本端管理员 JWT）
        .route(
            "/links/pairing/issue",
            post(links::issue_pairing_code_handler),
        )
        // 组网：发起建联（用户侧 JWT；服务端出站调对端 verify 交换凭证）
        .route("/links", post(links::create_link_handler))
        // 组网：已建联列表（用户侧 JWT，前端"关联组织"页数据源）
        .route("/links", get(links::list_links_handler))
        // 组网：断联（用户侧，本端管理员 JWT；连接 Revoked + 对端影子降级）
        .route("/links/{peer_org_id}", delete(links::revoke_link_handler))
        .route(
            "/me",
            put(organization_me::update_current_organization_handler),
        )
        .route("/", get(organizations::list_organizations_handler))
        .route(
            "/{organization_id}",
            delete(organizations::delete_organization_handler),
        )
        .route(
            "/{organization_id}",
            get(organizations::get_organization_handler),
        )
        .route(
            "/{organization_id}",
            put(organizations::update_organization_handler),
        )
        .nest(
            "/user",
            Router::new()
                .route("/", post(user::create_user_handler))
                .route(
                    "/me/list",
                    get(user::list_users_by_current_organization_handler),
                )
                .route(
                    "/{org_id}/list",
                    get(user::list_users_by_organization_handler),
                )
                .route("/update", put(user::update_user_handler))
                .route(
                    "/username/{username}",
                    get(user::get_user_by_username_handler),
                )
                .route("/id/{user_id}", delete(user::delete_user_handler)),
        )
}

fn hr_routes() -> Router {
    Router::new()
        .route("/agents", post(handlers::hr::agent::create_agent_handler))
        .route("/agents", get(handlers::hr::agent::list_agents_handler))
        .route(
            "/agents/query",
            post(handlers::hr::agent::query_agents_handler),
        )
        .route(
            "/agents/search",
            post(handlers::hr::agent::search_agents_handler),
        )
        .route(
            "/agents/reception",
            get(handlers::hr::agent::get_reception_agent_handler),
        )
        .route(
            "/agents/external",
            post(handlers::hr::agent::create_external_agent_handler),
        )
        .route("/agents/{id}", get(handlers::hr::agent::get_agent_handler))
        .route(
            "/agents/{id}",
            put(handlers::hr::agent::update_agent_handler),
        )
        .route(
            "/agents/{id}/status",
            put(handlers::hr::agent::update_agent_status_handler),
        )
        .route(
            "/agents/{id}",
            delete(handlers::hr::agent::delete_agent_handler),
        )
        .route(
            "/agents/runtime-list",
            get(handlers::hr::agent::runtime_list_handler),
        )
        .route(
            "/agents/{id}/runtime-status",
            get(handlers::hr::agent::runtime_status_handler),
        )
        .route(
            "/agents/{id}/cancel-thinking",
            post(handlers::hr::agent::cancel_thinking_handler),
        )
        .route(
            "/agents/{agent_id}/tool-packs",
            get(handlers::hr::agent::list_installed_tool_packs_handler),
        )
        .route(
            "/agents/{agent_id}/tool-packs/{tag}",
            post(handlers::hr::agent::install_tool_pack_handler),
        )
        .route(
            "/agents/{agent_id}/tool-packs/{tag}",
            delete(handlers::hr::agent::uninstall_tool_pack_handler),
        )
        .route(
            "/agents/{agent_id}/skill-packs",
            get(handlers::hr::agent::list_installed_skill_packs_handler),
        )
        .route(
            "/agents/{agent_id}/skill-packs/{tag}",
            post(handlers::hr::agent::install_skill_pack_handler),
        )
        .route(
            "/agents/{agent_id}/skill-packs/{tag}",
            delete(handlers::hr::agent::uninstall_skill_pack_handler),
        )
        .route(
            "/agents/{agent_id}/sync-packs",
            post(handlers::hr::agent::sync_agent_packs_handler),
        )
        .route(
            "/agents/{agent_id}/tools/{tool_id}/bind",
            post(handlers::finance::tool::bind_tool_to_agent::bind_tool_to_agent_handler),
        )
        .route(
            "/agents/{agent_id}/tools/{tool_id}/bind",
            delete(handlers::finance::tool::unbind_tool_from_agent::unbind_tool_from_agent_handler),
        )
        .route("/skills", post(handlers::hr::skill::create_skill_handler))
        .route("/skills", get(handlers::hr::skill::list_skills_handler))
        .route(
            "/skills/query",
            post(handlers::hr::skill::query_skills_handler),
        )
        .route(
            "/skills/search",
            post(handlers::hr::skill::search_skills_handler),
        )
        .route(
            "/skills/tags",
            get(handlers::hr::skill::list_skill_tags_handler),
        )
        .route("/skills/{id}", get(handlers::hr::skill::get_skill_handler))
        .route(
            "/skills/{id}",
            put(handlers::hr::skill::update_skill_handler),
        )
        .route(
            "/skills/{id}",
            delete(handlers::hr::skill::delete_skill_handler),
        )
        .route(
            "/agents/{agent_id}/skills",
            get(handlers::hr::skill::list_agent_skills_handler),
        )
        .route(
            "/agents/{agent_id}/skills/expired",
            get(handlers::hr::skill::list_expired_agent_skills_handler),
        )
        .route(
            "/agents/{agent_id}/skills/{skill_id}",
            post(handlers::hr::skill::install_skill_to_agent_handler)
                .delete(handlers::hr::skill::uninstall_skill_from_agent_handler),
        )
        .route(
            "/skills/{skill_id}/restore",
            post(handlers::hr::skill::restore_skill_handler),
        )
        .route(
            "/skills/{skill_id}/files",
            get(handlers::hr::skill::list_skill_files_handler),
        )
        .route(
            "/skills/{skill_id}/files/{*filename}",
            get(handlers::hr::skill::get_skill_file_content_handler),
        )
        .route(
            "/skills/{skill_id}/files/{*filename}",
            put(handlers::hr::skill::update_skill_file_content_handler),
        )
        .route(
            "/agents/search_memory",
            post(handlers::hr::agent::search_memory_handler),
        )
        .route(
            "/agents/query_memory",
            post(handlers::hr::agent::query_memory_handler),
        )
        .route(
            "/agents/recommend_seed_nodes",
            post(handlers::hr::agent::recommend_seed_nodes_handler),
        )
        .route(
            "/agents/memories/{memory_id}",
            delete(handlers::hr::agent::delete_memory_handler),
        )
}

fn finance_routes() -> Router {
    Router::new()
        .nest("/identity/lark", lark_integration_routes())
        .nest("/identity/github", github_integration_routes())
        .nest("/identity/generic-token", generic_token_integration_routes())
        .route(
            "/attachments/upload",
            post(handlers::finance::attachment::upload_attachment),
        )
        .route(
            "/attachments/text",
            post(handlers::finance::attachment::create_text_attachment::create_text_attachment_handler),
        )
        .route(
            "/attachments",
            get(handlers::finance::attachment::list_attachments::list_attachments_handler),
        )
        .route(
            "/attachments/{id}/content",
            get(handlers::finance::attachment::get_attachment_content::get_attachment_content_handler),
        )
        .route(
            "/attachments/{id}/content",
            put(handlers::finance::attachment::update_attachment_content::update_attachment_content_handler),
        )
        .route(
            "/attachments/{id}",
            get(handlers::finance::attachment::get_attachment::get_attachment_handler),
        )
        .route(
            "/attachments/{id}",
            delete(handlers::finance::attachment::delete_attachment::delete_attachment_handler),
        )
        .route(
            "/model-providers",
            post(handlers::finance::model_provider::create_model_provider::create_model_provider_handler),
        )
        .route(
            "/model-providers",
            get(handlers::finance::model_provider::list_model_providers::list_model_providers_handler),
        )
        .route(
            "/model-providers/{id}",
            get(handlers::finance::model_provider::get_model_provider::get_model_provider_handler),
        )
        .route(
            "/model-providers/{id}",
            put(handlers::finance::model_provider::update_model_provider::update_model_provider_handler),
        )
        .route(
            "/model-providers/{id}/test",
            post(handlers::finance::model_provider::test_connection::test_model_provider_connection_handler),
        )
        .route(
            "/model-providers/{id}/switch",
            post(handlers::finance::model_provider::switch_embedding::switch_embedding_provider_handler),
        )
        .route(
            "/model-providers/rebuild-progress",
            get(handlers::finance::model_provider::rebuild_progress::get_rebuild_progress_handler),
        )
        .route(
            "/model-providers/{id}/call",
            post(handlers::finance::model_provider::call_model::call_model_handler),
        )
        .route(
            "/model-providers/{id}",
            delete(handlers::finance::model_provider::delete_model_provider::delete_model_provider_handler),
        )
        .route(
            "/messages/agents",
            post(handlers::finance::message::send_message_to_agent_handler),
        )
        .route(
            "/messages",
            get(handlers::finance::message::list_messages_handler),
        )
        .route(
            "/messages/search",
            post(handlers::finance::message::search_messages_handler),
        )
        .route(
            "/messages/sse",
            get(handlers::finance::message::subscribe_sse_handler),
        )
        .route(
            "/message-channels",
            post(handlers::finance::message_channel::create_message_channel::create_message_channel_handler),
        )
        .route(
            "/message-channels",
            get(handlers::finance::message_channel::list_message_channels::list_message_channels_handler),
        )
        .route(
            "/message-channels/{id}",
            get(handlers::finance::message_channel::get_message_channel::get_message_channel_handler),
        )
        .route(
            "/message-channels/{id}",
            put(handlers::finance::message_channel::update_message_channel::update_message_channel_handler),
        )
        .route(
            "/message-channels/{id}/status",
            put(handlers::finance::message_channel::update_message_channel_status::update_message_channel_status_handler),
        )
        .route(
            "/message-channels/{id}/test",
            post(handlers::finance::message_channel::test_message_channel_connection::test_message_channel_connection_handler),
        )
        .route(
            "/message-channels/{id}",
            delete(handlers::finance::message_channel::delete_message_channel::delete_message_channel_handler),
        )
        .route(
            "/mcp-servers",
            post(handlers::finance::mcp_server::create_mcp_server_handler),
        )
        .route(
            "/mcp-servers",
            get(handlers::finance::mcp_server::list_mcp_servers_handler),
        )
        .route(
            "/mcp-servers/{id}",
            get(handlers::finance::mcp_server::get_mcp_server_handler),
        )
        .route(
            "/mcp-servers/{id}",
            put(handlers::finance::mcp_server::update_mcp_server_handler),
        )
        .route(
            "/mcp-servers/{id}/status",
            put(handlers::finance::mcp_server::update_mcp_server_status_handler),
        )
        .route(
            "/mcp-servers/{id}",
            delete(handlers::finance::mcp_server::delete_mcp_server_handler),
        )
        .route(
            "/mcp-servers/{server_id}/tools/sync",
            post(handlers::finance::mcp_tool::sync_mcp_tools_handler),
        )
        .route(
            "/mcp-servers/{server_id}/tools",
            get(handlers::finance::mcp_tool::list_mcp_tools_by_server_handler),
        )
        .route("/tools", post(handlers::finance::tool::create_tool::create_tool_handler))
        .route("/tools", get(handlers::finance::tool::list_tools::list_tools_handler))
        .route(
            "/tools/query",
            post(handlers::finance::tool::query_tools::query_tools_handler),
        )
        .route(
            "/tools/search",
            post(handlers::finance::tool::search_tools::search_tools_handler),
        )
        .route(
            "/tools/tags",
            get(handlers::finance::tool::list_tool_tags::list_tool_tags_handler),
        )
        .route(
            "/tool-call-entries",
            get(handlers::finance::tool::query_tool_call_entries_handler),
        )
        .route(
            "/tool-call-entries/{call_id}",
            get(handlers::finance::tool::get_tool_call_entry_handler),
        )
        .route("/tools/{id}", get(handlers::finance::tool::get_tool::get_tool_handler))
        .route("/tools/{id}", put(handlers::finance::tool::update_tool::update_tool_handler))
        .route(
            "/tools/{id}/status",
            put(handlers::finance::tool::update_tool_status::update_tool_status_handler),
        )
        .route(
            "/tools/{id}/debug-call",
            post(handlers::finance::tool::debug_call_tool::debug_call_tool_handler)
                .layer(axum::middleware::from_fn(|req, next| {
                    require_role_middleware(common::enums::UserRole::Admin, req, next)
                })),
        )
        .route(
            "/agents/{agent_id}/tools/{tool_id}/bind",
            post(handlers::finance::tool::bind_tool_to_agent::bind_tool_to_agent_handler),
        )
        .route(
            "/agents/{agent_id}/tools/{tool_id}/bind",
            delete(handlers::finance::tool::unbind_tool_from_agent::unbind_tool_from_agent_handler),
        )
        .route("/tools/{id}", delete(handlers::finance::tool::delete_tool::delete_tool_handler))
}

fn system_routes() -> Router {
    Router::new()
        .route(
            "/cron-triggers",
            post(handlers::system::cron_trigger::create_cron_trigger_handler),
        )
        .route(
            "/cron-triggers",
            get(handlers::system::cron_trigger::list_cron_triggers_handler),
        )
        .route(
            "/cron-triggers/{trigger_id}",
            get(handlers::system::cron_trigger::get_cron_trigger_handler),
        )
        .route(
            "/cron-triggers/{trigger_id}",
            put(handlers::system::cron_trigger::update_cron_trigger_handler),
        )
        .route(
            "/cron-triggers/{trigger_id}",
            delete(handlers::system::cron_trigger::delete_cron_trigger_handler),
        )
        .route(
            "/cron-triggers/{trigger_id}/pause",
            post(handlers::system::cron_trigger::pause_cron_trigger_handler),
        )
        .route(
            "/cron-triggers/{trigger_id}/resume",
            post(handlers::system::cron_trigger::resume_cron_trigger_handler),
        )
        // Backup routes - 创建/列出/删除/恢复脚本
        .route(
            "/backups",
            post(handlers::system::backup::create_backup_handler)
                .get(handlers::system::backup::list_backups_handler),
        )
        .route(
            "/backups/{version}",
            delete(handlers::system::backup::delete_backup_handler),
        )
        .route(
            "/backups/{version}/restore",
            post(handlers::system::backup::restore_backup_handler),
        )
        // Log query route - 查询应用日志（Admin/SuperAdmin 可访问）
        .route(
            "/logs",
            get(handlers::system::logs::query_logs::query_logs_handler),
        )
        // Log stats aggregation routes - 日志统计聚合（级别分布 + 时序）
        .route(
            "/logs/stats/level-distribution",
            get(handlers::system::logs::log_stats::get_log_level_distribution_handler),
        )
        .route(
            "/logs/stats/time-series",
            get(handlers::system::logs::log_stats::get_log_time_series_handler),
        )
        // AOP queue monitoring routes
        .route(
            "/aop/stats",
            get(handlers::system::aop::get_all_queue_stats_handler),
        )
        .route(
            "/aop/{consumer}/stats",
            get(handlers::system::aop::get_queue_stats_handler),
        )
        .route(
            "/aop/{consumer}/events",
            get(handlers::system::aop::list_events_handler),
        )
        .route(
            "/aop/{consumer}/events/{event_id}",
            get(handlers::system::aop::get_event_handler),
        )
        // AOP realtime stats routes
        .route(
            "/aop/stats/overview",
            get(handlers::system::aop_stats::get_stats_overview_handler),
        )
        .route(
            "/aop/stats/time-series",
            get(handlers::system::aop_stats::get_stats_time_series_handler),
        )
        .route(
            "/aop/stats/distribution",
            get(handlers::system::aop_stats::get_stats_distribution_handler),
        )
        // Health metrics aggregation route - 系统健康指标聚合（HUD 仪表盘墙用）
        .route(
            "/health/metrics",
            get(handlers::system::health_metrics::get_health_metrics_handler),
        )
        // Tool log storage routes - 工具日志存储监控与清理（① 运行时输出层治理）
        .route(
            "/storage/tool-logs",
            get(handlers::system::storage::tool_log_stats::get_tool_log_storage_handler),
        )
        .route(
            "/storage/tool-logs/cleanup",
            post(handlers::system::storage::tool_log_cleanup::cleanup_tool_logs_handler),
        )
        // Seed routes - 配置迁移（导出/导入/diff）
        .nest(
            "/seed",
            Router::new()
                .route("/list", get(handlers::system::seed::list_seeds_handler))
                .route(
                    "/file/{name}",
                    get(handlers::system::seed::get_seed_file_handler),
                )
                .route(
                    "/file/{name}",
                    delete(handlers::system::seed::delete_seed_file_handler),
                )
                .route("/save", post(handlers::system::seed::save_seed_handler))
                .route(
                    "/load/{name}",
                    post(handlers::system::seed::load_seed_handler),
                )
                .route("/diff/{name}", post(handlers::system::seed::diff_handler))
                .route(
                    "/diff-files",
                    post(handlers::system::seed::diff_files_handler),
                )
                .route("/default", get(handlers::system::seed::get_default_handler))
                .route(
                    "/apply-default",
                    post(handlers::system::seed::apply_default_handler),
                ),
        )
        // 通用后台任务进度查询
        .route(
            "/tasks/{task_id}/progress",
            get(handlers::system::task_progress::get_task_progress_handler),
        )
        // 后台任务管理
        .route(
            "/tasks",
            get(handlers::system::task_list::list_tasks_handler),
        )
        .route(
            "/tasks/cleanup",
            post(handlers::system::task_cleanup::cleanup_tasks_handler),
        )
        // 统一后台进程管理（shell_list / shell_status / shell_kill 双露工具的 HTTP 面）
        .route(
            "/processes",
            get(handlers::system::process::shell_list::shell_list_handler),
        )
        .route(
            "/processes/{pid}",
            get(handlers::system::process::shell_status::shell_status_handler),
        )
        .route(
            "/processes/{pid}/kill",
            post(handlers::system::process::shell_kill::shell_kill_handler),
        )
}
