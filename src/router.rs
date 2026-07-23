use crate::handlers;
use crate::middleware::{jwt_auth_middleware, request_context_middleware, require_role_middleware};
use axum::{
    Router,
    routing::{delete, get, post, put},
};
use common::config::AppConfig;
use common::enums::UserRole;
use std::sync::Arc;
use tower_http::services::ServeDir;

pub fn create_router(frontend_dist_dir: &str, config: Arc<AppConfig>) -> Router {
    let config_for_card = config.clone();
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
        // JSON-RPC: JWT 保护端点
        .route(
            "/a2a",
            post(handlers::a2a::jsonrpc::handle_jsonrpc)
                .layer(axum::middleware::from_fn(jwt_auth_middleware))
                .layer(axum::middleware::from_fn({
                    let config = config.clone();
                    move |req, next| {
                        request_context_middleware(config.clone(), req, next)
                    }
                })),
        )
        // SSE 流式端点: tasks/sendSubscribe
        .route(
            "/a2a/subscribe",
            post(handlers::a2a::send_subscribe::handle_send_subscribe)
                .layer(axum::middleware::from_fn(jwt_auth_middleware))
                .layer(axum::middleware::from_fn({
                    let config = config.clone();
                    move |req, next| {
                        request_context_middleware(config.clone(), req, next)
                    }
                })),
        )
        // A2A 回调端点（公开，外部 Agent 推送任务更新，无需 JWT）
        .route(
            "/a2a/callback/{task_id}",
            post(handlers::a2a::callback::handle_a2a_callback)
                .layer(axum::middleware::from_fn({
                    let config = config.clone();
                    move |req, next| {
                        request_context_middleware(config.clone(), req, next)
                    }
                })),
        )
        .route("/health", get(handlers::health::health))
        .fallback_service(ServeDir::new(frontend_dist_dir))
}

/// Public routes - do NOT require JWT authentication
/// These are for initialization, login, etc.
fn public_routes(config: Arc<AppConfig>) -> Router {
    use crate::handlers::organization::auth;
    use crate::handlers::organization::initialize_system;
    use crate::handlers::organization::organization;

    Router::new()
        // System initialization (only when no organizations exist)
        .route(
            "/organization/initialize/check",
            get(initialize_system::check_initialized),
        )
        .route(
            "/organization/initialize",
            post(initialize_system::initialize_system),
        )
        // Login/logout - login issues new JWT token
        .route("/organization/auth/login", post(auth::login::login))
        .route("/organization/auth/logout", post(auth::logout::logout))
        // List all organizations - public query (for login page selection, no login required)
        .route(
            "/organization/list",
            get(organization::list_organizations_handler),
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
        // JWT 认证中间件（外层，先执行）
        .layer(axum::middleware::from_fn(jwt_auth_middleware))
        // RequestContext 提取中间件（内层，后执行）
        // 从请求头（包含 JWT 注入的用户信息）创建 RequestContext
        .layer(axum::middleware::from_fn(move |req, next| {
            request_context_middleware(config.clone(), req, next)
        }))
}

fn user_routes() -> Router {
    use crate::handlers::user::profile;
    Router::new()
        .route("/me", get(profile::get_current_user_handler))
        .route("/me", put(profile::update_current_user_handler))
}

fn project_routes() -> Router {
    Router::new()
        .route(
            "/projects",
            post(handlers::project::project::create_project_handler),
        )
        .route(
            "/projects",
            get(handlers::project::project::list_projects_handler),
        )
        .route(
            "/projects/{id}",
            get(handlers::project::project::get_project_handler),
        )
        .route(
            "/projects/{id}",
            put(handlers::project::project::update_project_handler),
        )
        .route(
            "/projects/{id}/status",
            put(handlers::project::project::update_project_status_handler),
        )
}

fn task_routes() -> Router {
    Router::new()
        .route("/tasks", post(handlers::project::task::create_task_handler))
        .route("/tasks", get(handlers::project::task::list_tasks_handler))
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
            "/artifacts/{id}",
            get(handlers::project::artifact::get_artifact_handler)
                .delete(handlers::project::artifact::delete_artifact_handler),
        )
        .route(
            "/artifacts/{id}/content",
            get(handlers::project::artifact::get_artifact_content_handler)
                .put(handlers::project::artifact::update_artifact_content_handler),
        )
}

fn organization_protected_routes() -> Router {
    // Each handler is in its own file in the subdirectory
    use crate::handlers::organization::organization;
    use crate::handlers::organization::organization_me;
    use crate::handlers::organization::user;

    Router::new()
        // Get/update current user's organization info
        .route(
            "/me",
            get(organization_me::get_current_organization_handler),
        )
        .route(
            "/me",
            put(organization_me::update_current_organization_handler),
        )
        .route("/", get(organization::list_organizations_handler))
        .route(
            "/{organization_id}",
            delete(organization::delete_organization_handler),
        )
        .route(
            "/{organization_id}",
            get(organization::get_organization_handler),
        )
        .route(
            "/{organization_id}",
            put(organization::update_organization_handler),
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
        .route("/agents/search", get(handlers::hr::agent::search_agents_handler))
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
            "/skills/search",
            get(handlers::hr::skill::search_skills_handler),
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
            "/agents/{agent_id}/skills/{skill_id}",
            post(handlers::hr::skill::install_skill_to_agent_handler),
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
}

fn finance_routes() -> Router {
    Router::new()
        .route(
            "/attachments/upload",
            post(handlers::finance::attachment::upload_attachment),
        )
        .route(
            "/attachments/text",
            post(handlers::finance::attachment::create_text_attachment),
        )
        .route(
            "/attachments",
            get(handlers::finance::attachment::list_attachments),
        )
        .route(
            "/attachments/{id}/content",
            get(handlers::finance::attachment::get_attachment_content),
        )
        .route(
            "/attachments/{id}/content",
            put(handlers::finance::attachment::update_attachment_content),
        )
        .route(
            "/attachments/{id}",
            get(handlers::finance::attachment::get_attachment),
        )
        .route(
            "/attachments/{id}",
            delete(handlers::finance::attachment::delete_attachment),
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
            post(handlers::finance::tool::debug_call_tool::debug_call_tool_handler),
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
        .route("/logs", get(handlers::system::logs::query_logs::handler))
        // AOP queue monitoring routes
        .route("/aop/stats", get(handlers::system::aop::get_all_queue_stats))
        .route(
            "/aop/{consumer}/stats",
            get(handlers::system::aop::get_queue_stats),
        )
        .route(
            "/aop/{consumer}/events",
            get(handlers::system::aop::list_events),
        )
        .route(
            "/aop/{consumer}/events/{event_id}",
            get(handlers::system::aop::get_event),
        )
}
