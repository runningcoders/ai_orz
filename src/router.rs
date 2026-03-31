use crate::handlers;
use axum::{
    routing::{delete, get, post, put},
    Router,
};

pub fn create_router() -> Router {
    Router::new()
        .nest("/api/v1", api_routes())
        .route("/health", get(handlers::health::health))
}

fn api_routes() -> Router {
    Router::new()
        .route("/agents", post(handlers::create_agent))
        .route("/agents", get(handlers::list_agents))
        .route("/agents/:id", get(handlers::get_agent))
        .route("/agents/:id", put(handlers::update_agent))
        .route("/agents/:id", delete(handlers::delete_agent))
}
