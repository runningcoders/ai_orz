//! Frontend public config response
//!
//! This DTO is for the public API endpoint that returns public config
//! to the frontend at runtime.

use serde::{Deserialize, Serialize};

/// Public configuration for frontend
///
/// Contains only the configuration that frontend needs to know at runtime.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FrontendPublicConfigResponse {
    /// Base API URL for backend (usually empty for same-domain deployment)
    pub api_base_url: String,
    /// Current server listen address (for development information)
    pub server_listen_addr: String,
    /// Whether this is a production environment
    pub is_production: bool,
}
