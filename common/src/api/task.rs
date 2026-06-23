//! Task related API request/response DTOs - shared between backend and frontend

use crate::enums::{AssigneeType, TaskStatus};
use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 创建 Task 请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct CreateTaskRequest {
    /// 任务标题
    pub title: String,
    /// 任务描述
    pub description: Option<String>,
    /// 优先级（数值越大优先级越高）
    pub priority: Option<i32>,
    /// 标签列表
    pub tags: Option<Vec<String>>,
    /// 根用户 ID；为空时默认使用当前登录用户
    pub root_user_id: Option<String>,
    /// 分配对象类型；为空时默认 Agent
    pub assignee_type: Option<AssigneeType>,
    /// 分配对象 ID
    pub assignee_id: String,
    /// 所属项目 ID
    pub project_id: Option<String>,
    /// 截止时间戳
    pub due_at: Option<i64>,
    /// 前置任务 ID 列表
    pub dependencies: Option<Vec<String>>,
}

/// 创建 Task 响应
pub type CreateTaskResponse = GetTaskResponse;

/// 获取 Task 请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetTaskRequest {
    /// Task ID
    #[param(source = "path")]
    pub id: String,
}

/// 获取 Task 列表（按 Agent）请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListAgentTasksRequest {
    /// Agent ID
    #[param(source = "path")]
    pub agent_id: String,
    /// 可选状态筛选
    #[param(source = "query")]
    pub status: Option<TaskStatus>,
    /// 返回数量限制
    #[param(source = "query")]
    pub limit: Option<usize>,
}

/// 获取 Task 列表（按 Project）请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListProjectTasksRequest {
    /// Project ID
    #[param(source = "path")]
    pub project_id: String,
    /// 可选状态筛选
    #[param(source = "query")]
    pub status: Option<TaskStatus>,
    /// 返回数量限制
    #[param(source = "query")]
    pub limit: Option<usize>,
}

/// Task 列表项响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TaskListItem {
    /// Task ID
    pub id: String,
    /// 任务标题
    pub title: String,
    /// 任务描述
    pub description: Option<String>,
    /// 任务状态
    pub status: i32,
    /// 优先级
    pub priority: i32,
    /// 标签列表
    pub tags: Vec<String>,
    /// 根用户 ID
    pub root_user_id: String,
    /// 分配对象类型
    pub assignee_type: i32,
    /// 分配对象 ID
    pub assignee_id: String,
    /// 所属项目 ID
    pub project_id: Option<String>,
    /// 当前思考深度
    pub thinking_depth: i64,
    /// 创建时间戳
    pub created_at: i64,
    /// 更新时间戳
    pub updated_at: i64,
}

/// 获取 Task 响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetTaskResponse {
    /// Task ID
    pub id: String,
    /// 任务标题
    pub title: String,
    /// 任务描述
    pub description: Option<String>,
    /// 任务状态
    pub status: i32,
    /// 优先级
    pub priority: i32,
    /// 标签列表
    pub tags: Vec<String>,
    /// 截止时间戳
    pub due_at: Option<i64>,
    /// 开始时间戳
    pub start_at: Option<i64>,
    /// 结束时间戳
    pub end_at: Option<i64>,
    /// 前置任务 ID 列表
    pub dependencies: Vec<String>,
    /// 根用户 ID
    pub root_user_id: String,
    /// 分配对象类型
    pub assignee_type: i32,
    /// 分配对象 ID
    pub assignee_id: String,
    /// 所属项目 ID
    pub project_id: Option<String>,
    /// 当前思考深度
    pub thinking_depth: i64,
    /// 创建者用户 ID
    pub created_by: String,
    /// 最后修改者用户 ID
    pub modified_by: String,
    /// 创建时间戳
    pub created_at: i64,
    /// 更新时间戳
    pub updated_at: i64,
}

/// 更新 Task 请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateTaskRequest {
    /// Task ID
    #[param(source = "path")]
    pub id: String,
    /// 任务标题
    pub title: Option<String>,
    /// 任务描述
    pub description: Option<String>,
    /// 优先级
    pub priority: Option<i32>,
    /// 标签列表
    pub tags: Option<Vec<String>>,
    /// 截止时间戳
    pub due_at: Option<i64>,
    /// 前置任务 ID 列表
    pub dependencies: Option<Vec<String>>,
}

/// 更新 Task 响应
pub type UpdateTaskResponse = GetTaskResponse;

/// 更新 Task 状态请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateTaskStatusRequest {
    /// Task ID
    #[param(source = "path")]
    pub id: String,
    /// 目标任务状态。
    ///
    /// 状态流转合法性由 Project Domain 的 Task 子能力校验。
    pub status: TaskStatus,
}

/// 更新 Task 状态响应
pub type UpdateTaskStatusResponse = GetTaskResponse;
