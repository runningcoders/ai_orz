//! Authentication (login/logout) related API request/response DTOs - shared between backend and frontend

use serde::{Deserialize, Serialize};

/// 登录请求
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoginRequest {
    /// 用户名
    pub username: String,
    /// 密码（明文传输，服务端唯一哈希点）
    pub password: String,
    /// 组织 ID
    pub organization_id: String,
}

/// 登录响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    /// 用户 ID
    pub user_id: String,
    /// 用户名（登录名）
    pub username: String,
    /// 显示名称（用户注册时填写的，可能为空）
    pub display_name: String,
    /// 组织 ID
    pub organization_id: String,
    /// JWT token（供 API 工具/代码调用使用，浏览器场景通过 Cookie 自动携带）
    pub token: String,
}

/// 登出请求
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LogoutRequest {}

/// 登出响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogoutResponse {
    /// 是否登出成功
    pub success: bool,
}

/// 邀请码注册请求（公开接口，不需要登录）
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RegisterByInviteRequest {
    /// 组织邀请码（管理员在组织管理中生成）
    pub invite_code: String,
    /// 用户名（全局唯一）
    pub username: String,
    /// 密码（明文传输，服务端唯一哈希点）
    pub password: String,
    /// 显示名称（可选）
    pub display_name: Option<String>,
}

/// 邀请码有效性校验请求（GET query 参数）
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct InviteCodeValidateRequest {
    /// 组织邀请码
    #[serde(default)]
    pub invite_code: String,
}

/// 邀请码有效性校验响应（前端注册表单输入邀请码时校验）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteCodeValidateResponse {
    /// 邀请码是否有效
    pub valid: bool,
    /// 组织 ID（有效时返回，方便前端展示组织名）
    pub organization_id: Option<String>,
    /// 组织名称（有效时返回）
    pub organization_name: Option<String>,
}
