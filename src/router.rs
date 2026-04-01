use crate::handlers;
use axum::{
    routing::{delete, get, post, put},
    Router,
};
use tower_http::services::ServeDir;

pub fn create_router() -> Router {
    Router::new()
        .nest("/api/v1", api_routes())
        .route("/health", get(handlers::health::health))
        .nest_service("/", ServeDir::new("dist"))
}

fn api_routes() -> Router {
    Router::new()
        .route("/agents", post(handlers::create_agent))
        .route("/agents", get(handlers::list_agents))
        .route("/agents/:id", get(handlers::get_agent))
        .route("/agents/:id", put(handlers::update_agent))
        .route("/agents/:id", delete(handlers::delete_agent))
}
