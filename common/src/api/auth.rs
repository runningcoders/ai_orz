//! Authentication (login/logout) related API request/response DTOs - shared between backend and frontend

use serde::{Deserialize, Serialize};

/// 登录请求
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoginRequest {
    /// 用户名
    pub username: String,
    /// 密码哈希（前端已经 bcrypt 哈希）
    pub password_hash: String,
    /// 组织 ID
    pub organization_id: String,
}

/// 登录响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    /// 用户 ID
    pub user_id: String,
    /// 用户名
    pub username: String,
    /// 组织 ID
    pub organization_id: String,
}

/// 登出响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogoutResponse {
    /// 是否登出成功
    pub success: bool,
}
