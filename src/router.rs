use crate::handlers::health;
use axum::{routing::get, Router};

pub fn create_router() -> Router {
    Router::new()
        .nest("/api/v1", api_routes())
        .route("/health", get(health::health))
}

fn api_routes() -> Router {
    Router::new()
    // TODO: 注册 agent、organization、task 路由
}
