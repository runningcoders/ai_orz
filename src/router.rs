use crate::handlers;
use crate::middleware::{jwt_auth_middleware, request_context_middleware};
use axum::{
    routing::{delete, get, post, put},
    Router,
};
use tower_http::services::ServeDir;

pub fn create_router(frontend_dist_dir: &str) -> Router {
    Router::new()
        .nest("/api/v1", api_routes())
        .route("/health", get(handlers::health::health))
        // RequestContext 提取必须在 JWT 认证之前运行
        // JWT 认证会验证 token 后更新 RequestContext 中的用户信息
        .layer(axum::middleware::from_fn(request_context_middleware))
        .layer(axum::middleware::from_fn(jwt_auth_middleware))
        .fallback_service(ServeDir::new(frontend_dist_dir))
}

fn api_routes() -> Router {
    Router::new()
        // HR (Human Resources) routes
        .nest("/hr", hr_routes())
        // Finance (模型管理) routes
        .nest("/finance", finance_routes())
        // Organization (组织管理) routes - contains login/logout
        .nest("/organization", organization_routes())
}

fn organization_routes() -> Router {
    // Each handler is in its own file in the subdirectory
    use crate::handlers::organization::auth;
    use crate::handlers::organization::initialize_system;
    use crate::handlers::organization::organization;
    use crate::handlers::organization::user;

    Router::new()
        .route("/initialize/check", get(initialize_system::check_initialized))
        .route("/initialize", post(initialize_system::initialize_system))
        .route("/auth/login", post(auth::login::login))
        .route("/auth/logout", post(auth::logout::logout))
        .route("/list", get(organization::list_organizations::list_organizations))
        .route("/{org_id}", get(organization::get_organization::get_organization))
        .route("/update", put(organization::update_organization::update_organization))
        .route("/{org_id}", delete(organization::delete_organization::delete_organization))
        .nest("/user", Router::new()
            .route("/", post(user::create_user::create_user))
            .route("/{username}", get(user::get_user_by_username::get_user_by_username))
            .route("/{org_id}/list", get(user::list_users_by_organization::list_users_by_organization))
            .route("/update", put(user::update_user::update_user))
            .route("/{user_id}", delete(user::delete_user::delete_user))
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
        .route("/model-providers", post(handlers::finance::model_provider::create_model_provider))
        .route("/model-providers", get(handlers::finance::model_provider::list_model_providers))
        .route("/model-providers/{id}", get(handlers::finance::model_provider::get_model_provider))
        .route("/model-providers/{id}", put(handlers::finance::model_provider::update_model_provider))
        .route("/model-providers/{id}/test", post(handlers::finance::model_provider::test_model_provider_connection))
        .route("/model-providers/{id}/call", post(handlers::finance::model_provider::call_model))
        .route("/model-providers/{id}", delete(handlers::finance::model_provider::delete_model_provider))
}
