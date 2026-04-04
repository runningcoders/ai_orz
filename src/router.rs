use crate::handlers;
use axum::{
    routing::{delete, get, post, put},
    Router,
};
use tower_http::services::ServeDir;

pub fn create_router(frontend_dist_dir: &str) -> Router {
    Router::new()
        .nest("/api/v1", api_routes())
        .route("/health", get(handlers::health::health))
        .fallback_service(ServeDir::new(frontend_dist_dir))
}

fn api_routes() -> Router {
    Router::new()
        // HR (Human Resources) routes
        .nest("/hr", hr_routes())
        // Finance (模型管理) routes
        .nest("/finance", finance_routes())
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
        .route("/model-providers", post(handlers::create_model_provider))
        .route("/model-providers", get(handlers::list_model_providers))
        .route("/model-providers/{id}", get(handlers::get_model_provider))
        .route("/model-providers/{id}", put(handlers::update_model_provider))
        .route("/model-providers/{id}", delete(handlers::delete_model_provider))
}
