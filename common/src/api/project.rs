//! Project related API request/response DTOs - shared between backend and frontend

use crate::api::PaginationParams;
use crate::enums::ProjectStatus;
use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 创建 Project 请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct CreateProjectRequest {
    /// 项目名称
    pub name: String,
    /// 项目描述
    pub description: Option<String>,
    /// 优先级（数值越大优先级越高）
    pub priority: Option<i32>,
    /// 标签列表
    pub tags: Option<Vec<String>>,
    /// 负责人 Agent ID（可选）
    ///
    /// 由上层（handler）按需组合：默认对话框场景不创建 project；
    /// A2A tasks/send 场景由 handler 调 `resolve_agent(ctx)` 拿到 agent 后透传。
    /// handler 层纯粹透传，不在此处调 resolve_agent。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_agent_id: Option<String>,
}

/// 创建 Project 响应
pub type CreateProjectResponse = GetProjectResponse;

/// 获取 Project 请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetProjectRequest {
    /// Project ID
    #[param(source = "path")]
    pub id: String,
    /// 是否加载统计信息（事件次数汇总）
    #[param(source = "query")]
    pub with_stats: Option<bool>,
    /// 是否加载模型调用统计（token + 时序趋势）
    #[param(source = "query")]
    pub with_model_call_stats: Option<bool>,
    /// 统计时间范围起始（毫秒时间戳）
    #[param(source = "query")]
    pub stats_time_start: Option<i64>,
    /// 统计时间范围结束（毫秒时间戳）
    #[param(source = "query")]
    pub stats_time_end: Option<i64>,
    /// 时序查询粒度：hourly / daily
    #[param(source = "query")]
    pub stats_interval: Option<String>,
}

/// 获取 Project 列表请求（语法糖：只接受分页参数，内部固定 root_user_id=ctx.uid() + 排除 status=0 + priority DESC, created_at DESC）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListProjectsRequest {
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    #[param(source = "query")]
    pub pagination: PaginationParams,
}

/// Project 列表项响应
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
    /// 项目统计数据（按需返回）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<crate::models::ProjectStats>,
    /// 模型调用统计数据（按需返回）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_call_stats: Option<crate::models::ModelCallStats>,
}

/// 从详情响应构造列表项（用于按需加载场景，避免全量 list_projects）
impl From<&GetProjectResponse> for ProjectListItem {
    fn from(p: &GetProjectResponse) -> Self {
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            description: p.description.clone(),
            status: p.status,
            priority: p.priority,
            tags: p.tags.clone(),
            root_user_id: p.root_user_id.clone(),
            owner_agent_id: p.owner_agent_id.clone(),
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

/// 更新 Project 请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateProjectRequest {
    /// Project ID
    #[param(source = "path")]
    pub id: String,
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
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateProjectStatusRequest {
    /// Project ID
    #[param(source = "path")]
    pub id: String,
    /// 目标项目状态。
    ///
    /// 状态流转合法性由 Project Domain 校验；软删除不通过状态接口暴露。
    pub status: ProjectStatus,
}

/// 更新 Project 状态响应
pub type UpdateProjectStatusResponse = GetProjectResponse;

/// 获取 Project 列表响应
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ListProjectsResponse {
    /// Project 列表
    pub projects: Vec<ProjectListItem>,
}

/// Project 通用查询请求（POST body，支持完整查询能力）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ProjectQueryRequest {
    /// 按 ID 批量查询
    pub ids: Option<Vec<String>>,
    /// 关键词搜索
    pub keyword: Option<String>,
    /// 根用户 ID
    pub root_user_id: Option<String>,
    /// 状态列表（OR 语义）
    pub status_in: Option<Vec<ProjectStatus>>,
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// Project 列表项响应别名（前端兼容）
pub type ListProjectsResponseItem = ProjectListItem;
