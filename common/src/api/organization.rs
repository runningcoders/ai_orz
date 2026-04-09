//! Organization-related API request/response DTOs - shared between backend and frontend

use serde::{Deserialize, Serialize};

/// Organization basic information response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationInfoResponse {
    /// Organization ID
    pub organization_id: String,
    /// Organization name
    pub name: String,
    /// Organization description (optional)
    pub description: Option<String>,
    /// Base URL for external access (optional)
    pub base_url: Option<String>,
    /// Organization status (1: active, 0: inactive)
    pub status: i32,
    /// Creation timestamp
    pub created_at: i64,
}

/// Get current organization information response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetCurrentOrganizationResponse {
    /// Organization information data
    pub data: OrganizationInfoResponse,
}

/// Update current organization information request
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateOrganizationRequest {
    /// New organization name (None means no change)
    pub name: Option<String>,
    /// New organization description (None means no change)
    pub description: Option<String>,
    /// New base URL (None means no change)
    pub base_url: Option<String>,
}

/// System initialization request - create first organization and super admin
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InitializeSystemRequest {
    /// Organization name
    pub organization_name: String,
    /// Admin username
    pub admin_username: String,
    /// Admin password (hashed on frontend)
    pub admin_password_hash: String,
    /// Organization description (optional)
    pub description: Option<String>,
    /// Admin display name (optional)
    pub admin_display_name: Option<String>,
    /// Admin email (optional)
    pub admin_email: Option<String>,
}

/// Check initialization status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckInitializedResponse {
    /// Whether system has been initialized (has at least one organization)
    pub initialized: bool,
}

/// List all organizations response (for login page selection)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListOrganizationsResponse {
    /// List of organizations
    pub data: Vec<OrganizationListItem>,
}

/// Single organization item in list (for login selection)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationListItem {
    /// Organization ID
    pub organization_id: String,
    /// Organization name
    pub name: String,
    /// Organization description (optional)
    pub description: Option<String>,
}

/// Login request
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoginRequest {
    /// Organization ID to login to
    pub organization_id: String,
    /// Username
    pub username: String,
    /// Password hash (computed on frontend)
    pub password_hash: String,
}

/// Login response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    /// Login success flag
    pub success: bool,
    /// Message if login failed
    pub message: Option<String>,
}

/// Logout request
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LogoutRequest {}

/// Logout response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogoutResponse {
    /// Logout success flag
    pub success: bool,
}
