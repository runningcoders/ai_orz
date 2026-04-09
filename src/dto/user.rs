//! User DTO - 用户相关数据传输对象

use serde::{Deserialize, Serialize};

/// 当前用户信息响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfoResponse {
    /// 用户 ID
    pub user_id: String,
    /// 用户名
    pub username: String,
    /// 显示名称
    pub display_name: Option<String>,
    /// 邮箱
    pub email: Option<String>,
    /// 所属组织 ID
    pub organization_id: String,
    /// 角色编号
    pub role: i32,
    /// 角色名称
    pub role_name: String,
    /// 用户状态
    pub status: i32,
}

/// 获取当前用户信息响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetCurrentUserResponse {
    /// 用户信息数据
    pub data: UserInfoResponse,
}

/// 更新当前用户信息请求
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateCurrentUserRequest {
    /// 新的显示名称（None 表示不修改）
    pub display_name: Option<String>,
    /// 新的邮箱（None 表示不修改）
    pub email: Option<String>,
    /// 新的密码哈希（None 表示不修改）
    pub password_hash: Option<String>,
}

/// 空响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmptyResponse {
    /// 响应码
    pub code: i32,
    /// 响应消息
    pub message: String,
}
