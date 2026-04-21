use crate::handlers;
use crate::middleware::{jwt_auth_middleware, request_context_middleware};
use axum::{
    routing::{delete, get, post, put}, Router,
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
        .route("/agents/{id}", delete(handlers::hr::agent::delete_agent))
}

fn finance_routes() -> Router {
    Router::new()
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
}
