//! Organization-related API request/response DTOs - shared between backend and frontend

use serde::{Deserialize, Serialize};

/// 系统初始化请求 - 创建第一个组织和超级管理员
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InitializeSystemRequest {
    /// 组织名称
    pub organization_name: String,
    /// 超级管理员用户名
    pub admin_username: String,
    /// 超级管理员密码（前端已哈希）
    pub admin_password_hash: String,
    /// 组织描述（可选）
    pub description: Option<String>,
    /// 超级管理员显示名称（可选）
    pub admin_display_name: Option<String>,
    /// 超级管理员邮箱（可选）
    pub admin_email: Option<String>,
}

/// 系统初始化响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeSystemResponse {
    /// 组织 ID
    pub organization_id: String,
    /// 超级管理员用户 ID
    pub user_id: String,
}

/// 检查初始化状态响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckInitializedResponse {
    /// 系统是否已初始化（至少有一个组织）
    pub initialized: bool,
}

/// 组织列表项（用于登录页选择）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationListItem {
    /// 组织 ID
    pub organization_id: String,
    /// 组织名称
    pub name: String,
    /// 组织描述（可选）
    pub description: Option<String>,
}

/// 列出所有组织响应（登录页选择用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListOrganizationsResponse {
    /// 组织列表
    pub data: Vec<OrganizationListItem>,
    /// 总数
    pub total: u64,
}

/// 组织基础信息响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationInfoResponse {
    /// 组织 ID
    pub organization_id: String,
    /// 组织名称
    pub name: String,
    /// 组织描述（可选）
    pub description: Option<String>,
    /// 外部访问 Base URL（可选）
    pub base_url: Option<String>,
    /// 组织状态（1: 活跃, 0: 非活跃）
    pub status: i32,
    /// 创建时间戳
    pub created_at: i64,
}

/// 获取当前组织信息响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetCurrentOrganizationResponse {
    /// 组织信息数据
    pub data: OrganizationInfoResponse,
}

/// 更新当前组织信息请求
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateCurrentOrganizationRequest {
    /// 新组织名称（None 表示不修改）
    pub name: Option<String>,
    /// 新组织描述（None 表示不修改）
    pub description: Option<String>,
    /// 新外部访问 Base URL（None 表示不修改）
    pub base_url: Option<String>,
}

/// 更新当前组织信息响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCurrentOrganizationResponse {
    /// 更新后的组织信息
    pub data: OrganizationInfoResponse,
}

/// 删除组织响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteOrganizationResponse {
    /// 是否删除成功
    pub success: bool,
}
