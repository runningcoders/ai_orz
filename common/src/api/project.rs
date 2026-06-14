//! Project related API request/response DTOs - shared between backend and frontend

use crate::enums::ProjectStatus;
use serde::{Deserialize, Serialize};

/// 创建 Project 请求
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateProjectRequest {
    /// 项目名称
    pub name: String,
    /// 项目描述
    pub description: Option<String>,
    /// 优先级（数值越大优先级越高）
    pub priority: Option<i32>,
    /// 标签列表
    pub tags: Option<Vec<String>>,
}

/// 创建 Project 响应
pub type CreateProjectResponse = GetProjectResponse;

/// Project 列表查询参数
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ListProjectsQuery {
    /// 根用户 ID；为空时默认使用当前登录用户
    pub root_user_id: Option<String>,
    /// 可选状态筛选
    pub status: Option<ProjectStatus>,
    /// 返回数量限制
    pub limit: Option<usize>,
}

/// Project 列表项响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectListItem {
    /// Project ID
    pub id: String,
    /// 项目名称
    pub name: String,
    /// 项目描述
    pub description: Option<String>,
    /// 项目状态
    pub status: i32,
    /// 优先级
    pub priority: i32,
    /// 标签列表
    pub tags: Vec<String>,
    /// 根用户 ID
    pub root_user_id: String,
    /// 负责人 Agent ID
    pub owner_agent_id: Option<String>,
    /// 创建时间戳
    pub created_at: i64,
    /// 更新时间戳
    pub updated_at: i64,
}

/// 获取 Project 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetProjectResponse {
    /// Project ID
    pub id: String,
    /// 项目名称
    pub name: String,
    /// 项目描述
    pub description: Option<String>,
    /// 项目运作流程描述
    pub workflow: Option<String>,
    /// 用户指导建议
    pub guidance: Option<String>,
    /// 项目状态
    pub status: i32,
    /// 优先级
    pub priority: i32,
    /// 标签列表
    pub tags: Vec<String>,
    /// 根用户 ID
    pub root_user_id: String,
    /// 负责人 Agent ID
    pub owner_agent_id: Option<String>,
    /// 开始时间戳
    pub start_at: Option<i64>,
    /// 截止时间戳
    pub due_at: Option<i64>,
    /// 结束时间戳
    pub end_at: Option<i64>,
    /// 创建时间戳
    pub created_at: i64,
    /// 更新时间戳
    pub updated_at: i64,
}

/// 更新 Project 请求
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateProjectRequest {
    /// 项目名称
    pub name: Option<String>,
    /// 项目描述
    pub description: Option<String>,
    /// 优先级
    pub priority: Option<i32>,
    /// 标签列表
    pub tags: Option<Vec<String>>,
}

/// 更新 Project 响应
pub type UpdateProjectResponse = GetProjectResponse;

/// 更新 Project 状态请求
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateProjectStatusRequest {
    /// 目标项目状态。
    ///
    /// 状态流转合法性由 Project Domain 校验；软删除不通过状态接口暴露。
    pub status: ProjectStatus,
}

/// 更新 Project 状态响应
pub type UpdateProjectStatusResponse = GetProjectResponse;
