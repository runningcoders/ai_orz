//! User-related API request/response DTOs - shared between backend and frontend

use serde::{Deserialize, Serialize};
use crate::enums::UserRole;

/// Current user information response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfoResponse {
    /// User ID
    pub user_id: String,
    /// Username (login name)
    pub username: String,
    /// Display name (optional, can be empty)
    pub display_name: Option<String>,
    /// Email address (optional, can be empty)
    pub email: Option<String>,
    /// Organization ID the user belongs to
    pub organization_id: String,
    /// Role code as integer (1: SuperAdmin, 2: Admin, 3: Member)
    pub role: i32,
    /// Role display name in Chinese
    pub role_name: String,
    /// User status (1: active, 0: inactive)
    pub status: i32,
}

/// Get current user information response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetCurrentUserResponse {
    /// User information data
    pub data: UserInfoResponse,
}

/// Update current user information request
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateCurrentUserRequest {
    /// New display name (None means no change)
    pub display_name: Option<String>,
    /// New email address (None means no change)
    pub email: Option<String>,
    /// New password hash (None means no change)
    pub password_hash: Option<String>,
}

/// Empty success response (used for operations that don't need to return data)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmptyResponse {
    /// Response code (0 means success, non-zero means error)
    pub code: i32,
    /// Response message for human
    pub message: String,
}

/// List users request
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListUsersResponse {
    /// List of users in organization
    pub data: Vec<UserListItem>,
    /// Total count of users
    pub total: u64,
}

/// Single user item in user list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserListItem {
    /// User ID
    pub user_id: String,
    /// Username
    pub username: String,
    /// Display name
    pub display_name: Option<String>,
    /// Email
    pub email: Option<String>,
    /// Role code
    pub role: i32,
    /// Role display name
    pub role_name: String,
    /// User status
    pub status: i32,
    /// Creation timestamp
    pub created_at: i64,
}

/// Create new user request
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateUserRequest {
    /// Username (required, unique in organization)
    pub username: String,
    /// Display name (optional)
    pub display_name: Option<String>,
    /// Email (optional)
    pub email: Option<String>,
    /// Password hash (required)
    pub password_hash: String,
    /// User role (required)
    pub role: i32,
}
