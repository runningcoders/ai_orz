//! User-related API request/response DTOs - shared between backend and frontend

use crate::api::PaginationParams;
use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Get current user info request
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetCurrentUserRequest {}

/// 当前用户信息响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UserInfoResponse {
    /// 用户 ID
    pub user_id: String,
    /// 用户名（登录名）
    pub username: String,
    /// 显示名称（可选，可以为空）
    pub display_name: Option<String>,
    /// 邮箱地址（可选，可以为空）
    pub email: Option<String>,
    /// 用户所属组织 ID
    pub organization_id: String,
    /// 用户角色代码（整数形式，1: SuperAdmin, 2: Admin, 3: Member）
    pub role: i32,
    /// 用户角色中文显示名称
    pub role_name: String,
    /// 用户状态（1: 启用, 0: 禁用）
    pub status: i32,
    /// 用户自述偏好（声明式画像，Markdown 自由文本，None 表示未设置）
    pub preferences: Option<String>,
}

/// 获取当前用户信息响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetCurrentUserResponse {
    /// 用户信息数据
    pub data: UserInfoResponse,
}

/// 更新当前用户信息请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateCurrentUserRequest {
    /// 新显示名称（None 表示不修改）
    pub display_name: Option<String>,
    /// 新邮箱地址（None 表示不修改）
    pub email: Option<String>,
    /// 新密码哈希（None 表示不修改）
    pub password_hash: Option<String>,
    /// 新偏好自述（None 表示不修改；Agent 上下文调用时该字段被忽略，仅限用户本人修改）
    pub preferences: Option<String>,
}

/// 更新当前用户信息响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UpdateCurrentUserResponse {
    /// 更新后的用户信息
    pub data: UserInfoResponse,
}

/// Get user by username request
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetUserByUsernameRequest {
    /// Username to query
    #[param(source = "path")]
    pub username: String,
}

/// Get user by username response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetUserByUsernameResponse {
    /// User information if found
    pub user: Option<UserInfoResponse>,
}

/// 用户列表项
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UserListItem {
    /// 用户 ID
    pub user_id: String,
    /// 用户名
    pub username: String,
    /// 显示名称
    pub display_name: Option<String>,
    /// 邮箱
    pub email: Option<String>,
    /// 用户角色代码
    pub role: i32,
    /// 用户角色中文显示名称
    pub role_name: String,
    /// 用户状态
    pub status: i32,
    /// 创建时间戳
    pub created_at: i64,
}

/// List users by organization request (specified organization ID in path)
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListUsersByOrganizationRequest {
    /// Organization ID to query
    #[param(source = "path")]
    pub organization_id: String,
}

/// List users by current organization request (no parameters needed, gets organization from auth context)
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListUsersByCurrentOrganizationRequest {}

/// 列出用户响应
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ListUsersResponse {
    /// 组织内用户列表
    pub data: Vec<UserListItem>,
    /// 用户总数
    pub total: u64,
}

/// User 通用查询请求（POST body）
///
/// 支持完整查询条件 + 分页，query 是核心查询能力。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct UserQueryRequest {
    /// 按组织 ID 查询
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// User list item alias (frontend compatibility)
pub type ListUsersResponseItem = UserListItem;

/// Create organization user request alias (frontend compatibility)
pub type CreateOrganizationUserRequest = CreateUserRequest;

/// Create organization user response alias (frontend compatibility)
pub type CreateOrganizationUserResponse = CreateUserResponse;

/// 创建新用户请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct CreateUserRequest {
    /// 用户名（必填，组织内唯一）
    pub username: String,
    /// 显示名称（可选）
    pub display_name: Option<String>,
    /// 邮箱（可选）
    pub email: Option<String>,
    /// 密码哈希（必填）
    pub password_hash: String,
    /// 用户角色（必填）
    pub role: i32,
}

/// 创建新用户响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateUserResponse {
    /// 用户 ID
    pub user_id: String,
    /// 用户名
    pub username: String,
    /// 显示名称
    pub display_name: Option<String>,
    /// 邮箱
    pub email: Option<String>,
    /// 用户角色
    pub role: i32,
}

/// Get user request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetUserRequest {
    /// User ID
    #[param(source = "path")]
    pub user_id: String,
}

/// Delete user request
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteUserRequest {
    /// User ID
    #[param(source = "path")]
    pub user_id: String,
}

/// 更新用户请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateUserRequest {
    /// User ID
    #[param(source = "path")]
    pub user_id: String,
    /// 显示名称（None 表示不修改）
    pub display_name: Option<String>,
    /// 邮箱（None 表示不修改）
    pub email: Option<String>,
    /// 用户角色（None 表示不修改）
    pub role: Option<i32>,
    /// 用户状态（None 表示不修改）
    pub status: Option<i32>,
    /// 密码哈希（None 表示不修改）
    pub password_hash: Option<String>,
}

/// 更新用户响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UpdateUserResponse {
    /// 用户 ID
    pub user_id: String,
    /// 用户名
    pub username: String,
    /// 显示名称
    pub display_name: Option<String>,
    /// 邮箱
    pub email: Option<String>,
    /// 用户角色
    pub role: i32,
    /// 用户状态
    pub status: i32,
}

/// 删除用户响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeleteUserResponse {
    /// 是否删除成功
    pub success: bool,
}
