//! Public config endpoint for frontend
//!
//! Returns public configuration to frontend at runtime,
//! allowing frontend to dynamically get API base URL etc.

use axum::Extension;
use axum::Json;
use common::api::ApiResponse;
use common::api::FrontendPublicConfigResponse;
use common::constants::RequestContext;

use crate::APP_CONFIG;

/// Get public configuration for frontend
///
/// This endpoint is public (no authentication required) and returns
/// configuration that frontend needs to know at runtime.
pub async fn get_public_config(
    Extension(_ctx): Extension<RequestContext>,
) -> Json<ApiResponse<FrontendPublicConfigResponse>> {
    // Get config from global application state
    let config = APP_CONFIG.get().expect("App config not initialized");

    let resp = FrontendPublicConfigResponse {
        api_base_url: "".to_string(), // Empty means same domain, can be configured if needed
        server_listen_addr: config.server.listen_addr.clone(),
        // In development mode, this can be set to false
        is_production: cfg!(not(debug_assertions)),
    };

    Json(ApiResponse::success(resp))
}
