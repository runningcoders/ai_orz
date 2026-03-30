use crate::error::ApiResponse;
use axum::Json;

pub async fn health() -> Json<ApiResponse<HealthData>> {
    Json(ApiResponse::success(HealthData {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }))
}

#[derive(serde::Serialize)]
pub struct HealthData {
    status: String,
    version: String,
}
