//! User-related API request/response DTOs - shared between backend and frontend

use serde::{Deserialize, Serialize};

/// 当前用户信息响应
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

/// 获取当前用户信息响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetCurrentUserResponse {
    /// 用户信息数据
    pub data: UserInfoResponse,
}

/// 更新当前用户信息请求
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateCurrentUserRequest {
    /// 新显示名称（None 表示不修改）
    pub display_name: Option<String>,
    /// 新邮箱地址（None 表示不修改）
    pub email: Option<String>,
    /// 新密码哈希（None 表示不修改）
    pub password_hash: Option<String>,
}

/// 更新当前用户信息响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCurrentUserResponse {
    /// 更新后的用户信息
    pub data: UserInfoResponse,
}

/// 用户列表项
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// 列出用户响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListUsersResponse {
    /// 组织内用户列表
    pub data: Vec<UserListItem>,
    /// 用户总数
    pub total: u64,
}

/// 创建新用户请求
#[derive(Debug, Clone, Deserialize, Serialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// 更新用户请求
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateUserRequest {
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteUserResponse {
    /// 是否删除成功
    pub success: bool,
}

/// 空成功响应（用于不需要返回数据的操作）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmptyResponse {
    /// 响应码（0 表示成功，非零表示错误）
    pub code: i32,
    /// 响应消息，给人看的
    pub message: String,
}
