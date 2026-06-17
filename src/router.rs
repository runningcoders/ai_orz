use crate::handlers;
use crate::middleware::{jwt_auth_middleware, request_context_middleware};
use axum::{
    Router,
    routing::{delete, get, post, put},
};
use common::config::AppConfig;
use std::sync::Arc;
use tower_http::services::ServeDir;

pub fn create_router(frontend_dist_dir: &str, config: Arc<AppConfig>) -> Router {
    Router::new()
        // Public routes - no JWT authentication required
        .nest("/api/v1", public_routes())
        // Protected routes - require valid JWT token
        .nest("/api/v1", protected_routes())
        .route("/health", get(handlers::health::health))
        // RequestContext 提取必须在 JWT 认证之前运行
        // JWT 认证会验证 token 后更新 RequestContext 中的用户信息
        .layer(axum::middleware::from_fn(move |req, next| {
            request_context_middleware(config.clone(), req, next)
        }))
        .fallback_service(ServeDir::new(frontend_dist_dir))
}

/// Public routes - do NOT require JWT authentication
/// These are for initialization, login, etc.
fn public_routes() -> Router {
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
        // Get organization basic info - public query (no login required)
        .route(
            "/organization/{org_id}",
            get(organization::get_organization::get_organization),
        )
        // List all organizations - public query (for login page selection, no login required)
        .route(
            "/organization/list",
            get(organization::list_organizations::list_organizations),
        )
}

/// Protected routes - require valid JWT authentication
/// All requests without valid token will be redirected to / (login page)
fn protected_routes() -> Router {
    Router::new()
        // HR (Human Resources) routes
        .nest("/hr", hr_routes())
        // Finance (模型管理) routes
        .nest("/finance", finance_routes())
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
        // Add JWT authentication middleware to all protected routes
        .layer(axum::middleware::from_fn(jwt_auth_middleware))
}

fn user_routes() -> Router {
    use crate::handlers::user::profile;
    Router::new()
        .route("/me", get(profile::get_current_user::get_current_user))
        .route(
            "/me",
            put(profile::update_current_user::update_current_user),
        )
}

fn project_routes() -> Router {
    Router::new()
        .route(
            "/projects",
            post(handlers::project::project::create_project),
        )
        .route("/projects", get(handlers::project::project::list_projects))
        .route(
            "/projects/{id}",
            get(handlers::project::project::get_project),
        )
        .route(
            "/projects/{id}",
            put(handlers::project::project::update_project),
        )
        .route(
            "/projects/{id}/status",
            put(handlers::project::project::update_project_status),
        )
}

fn task_routes() -> Router {
    Router::new()
        .route("/tasks", post(handlers::project::task::create_task))
        .route("/tasks/{id}", get(handlers::project::task::get_task))
        .route("/tasks/{id}", put(handlers::project::task::update_task))
        .route(
            "/tasks/{id}/status",
            put(handlers::project::task::update_task_status),
        )
        .route(
            "/projects/{project_id}/tasks",
            get(handlers::project::task::list_project_tasks),
        )
        .route(
            "/agents/{agent_id}/tasks",
            get(handlers::project::task::list_agent_tasks),
        )
}

fn artifact_routes() -> Router {
    Router::new()
        .route(
            "/artifacts",
            post(handlers::project::artifact::create_artifact)
                .get(handlers::project::artifact::list_artifacts),
        )
        .route(
            "/artifacts/{id}",
            get(handlers::project::artifact::get_artifact)
                .delete(handlers::project::artifact::delete_artifact),
        )
        .route(
            "/artifacts/{id}/content",
            get(handlers::project::artifact::get_artifact_content)
                .put(handlers::project::artifact::update_artifact_content),
        )
}

fn organization_protected_routes() -> Router {
    // Each handler is in its own file in the subdirectory
    use crate::handlers::organization::organization;
    use crate::handlers::organization::organization_me;
    use crate::handlers::organization::user;

    Router::new()
        // Get/update current user's organization info
        .route("/me", get(organization_me::get_current_organization::get_current_organization))
        .route("/me", put(organization_me::update_current_organization::update_current_organization))
        .route("/update", put(organization::update_organization::update_organization))
        .route("/{org_id}", delete(organization::delete_organization::delete_organization))
        .nest("/user", Router::new()
            .route("/", post(user::create_user::create_user))
            .route("/me/list", get(user::list_users_by_current_organization::list_users_by_current_organization))
            .route("/{org_id}/list", get(user::list_users_by_organization::list_users_by_organization))
            .route("/update", put(user::update_user::update_user))
            .route("/username/{username}", get(user::get_user_by_username::get_user_by_username))
            .route("/id/{user_id}", delete(user::delete_user::delete_user))
        )
}

fn hr_routes() -> Router {
    Router::new()
        .route("/agents", post(handlers::hr::agent::create_agent))
        .route("/agents", get(handlers::hr::agent::list_agents))
        .route("/agents/{id}", get(handlers::hr::agent::get_agent))
        .route("/agents/{id}", put(handlers::hr::agent::update_agent))
        .route(
            "/agents/{id}/status",
            put(handlers::hr::agent::update_agent_status),
        )
        .route("/agents/{id}", delete(handlers::hr::agent::delete_agent))
        .route("/skills", post(handlers::hr::skill::create_skill))
        .route("/skills", get(handlers::hr::skill::list_skills))
        .route("/skills/search", get(handlers::hr::skill::search_skills))
        .route("/skills/{id}", get(handlers::hr::skill::get_skill))
        .route("/skills/{id}", put(handlers::hr::skill::update_skill))
        .route("/skills/{id}", delete(handlers::hr::skill::delete_skill))
        .route(
            "/agents/{agent_id}/skills",
            get(handlers::hr::skill::list_agent_skills),
        )
        .route(
            "/agents/{agent_id}/skills/{skill_id}",
            post(handlers::hr::skill::install_skill_to_agent),
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
            post(handlers::finance::model_provider::create_model_provider),
        )
        .route(
            "/model-providers",
            get(handlers::finance::model_provider::list_model_providers),
        )
        .route(
            "/model-providers/{id}",
            get(handlers::finance::model_provider::get_model_provider),
        )
        .route(
            "/model-providers/{id}",
            put(handlers::finance::model_provider::update_model_provider),
        )
        .route(
            "/model-providers/{id}/test",
            post(handlers::finance::model_provider::test_model_provider_connection),
        )
        .route(
            "/model-providers/{id}/call",
            post(handlers::finance::model_provider::call_model),
        )
        .route(
            "/model-providers/{id}",
            delete(handlers::finance::model_provider::delete_model_provider),
        )
        .route(
            "/message-channels",
            post(handlers::finance::message_channel::create_message_channel),
        )
        .route(
            "/message-channels",
            get(handlers::finance::message_channel::list_message_channels),
        )
        .route(
            "/message-channels/{id}",
            get(handlers::finance::message_channel::get_message_channel),
        )
        .route(
            "/message-channels/{id}",
            put(handlers::finance::message_channel::update_message_channel),
        )
        .route(
            "/message-channels/{id}/status",
            put(handlers::finance::message_channel::update_message_channel_status),
        )
        .route(
            "/message-channels/{id}/test",
            post(handlers::finance::message_channel::test_message_channel_connection),
        )
        .route(
            "/message-channels/{id}",
            delete(handlers::finance::message_channel::delete_message_channel),
        )
        .route("/tools", post(handlers::finance::tool::create_tool))
        .route("/tools", get(handlers::finance::tool::list_tools))
        .route("/tools/{id}", get(handlers::finance::tool::get_tool))
        .route("/tools/{id}", put(handlers::finance::tool::update_tool))
        .route(
            "/tools/{id}/status",
            put(handlers::finance::tool::update_tool_status),
        )
        .route(
            "/tools/{id}/agent-bind",
            post(handlers::finance::tool::bind_tool_to_agent),
        )
        .route(
            "/tools/{id}/agent-bind",
            delete(handlers::finance::tool::unbind_tool_from_agent),
        )
        .route("/tools/{id}", delete(handlers::finance::tool::delete_tool))
}
