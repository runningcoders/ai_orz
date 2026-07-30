//! 通用后台任务 API DTO
//!
//! 统一管理所有后台异步任务（初始化、向量重建、seed 导出/导入等）的进度查询。

use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 后台任务状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// 等待开始
    Pending,
    /// 运行中
    Running,
    /// 已完成
    Completed,
    /// 已失败
    Failed,
}

/// 后台任务类型标识（前端按此区分展示文案）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    /// 系统初始化
    InitializeSystem,
    /// 向量索引重建
    RebuildVectors,
    /// Seed 导出
    SeedSave,
    /// Seed 导入
    SeedLoad,
    /// 应用默认 Seed
    SeedApplyDefault,
}

impl TaskType {
    /// 转换为字符串标识
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InitializeSystem => "initialize_system",
            Self::RebuildVectors => "rebuild_vectors",
            Self::SeedSave => "seed_save",
            Self::SeedLoad => "seed_load",
            Self::SeedApplyDefault => "seed_apply_default",
        }
    }
}

/// 任务进度快照（从任务对象读取的当前状态）
///
/// 通用接口返回此结构。业务 handler 可在此基础上装饰为各自的响应 DTO。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TaskProgressSnapshot {
    /// 任务 ID
    pub task_id: String,
    /// 任务类型字符串
    pub task_type: String,
    /// 任务状态
    pub status: TaskStatus,
    /// 当前步骤编号（1-based，0 表示尚未开始）
    pub current_step: usize,
    /// 总步骤数
    pub total_steps: usize,
    /// 当前步骤描述（人类可读）
    pub step_message: String,
    /// 开始时间戳（毫秒）
    pub started_at: i64,
    /// 结束时间戳（毫秒，运行中为 None）
    pub finished_at: Option<i64>,
    /// 失败时的错误信息
    pub error: Option<String>,
    /// 任务结果（完成时，JSON 序列化的业务结果）
    pub result: Option<serde_json::Value>,
}

/// 异步提交响应（统一返回 task_id）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TaskIdResponse {
    /// 任务 ID
    pub task_id: String,
}

/// 进度查询请求（task_id 从 path 提取）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetTaskProgressRequest {
    /// 任务 ID
    #[param(source = "path")]
    pub task_id: String,
}

/// 后台任务列表查询请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListBackgroundTasksRequest {
    /// 按任务类型筛选（可选，字符串匹配 task_type 字段）
    #[param(source = "query")]
    pub task_type: Option<String>,
    /// 按状态筛选（可选）
    #[param(source = "query")]
    pub status: Option<TaskStatus>,
}

/// 后台任务列表响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListBackgroundTasksResponse {
    /// 任务进度快照列表（按 started_at 降序）
    pub tasks: Vec<TaskProgressSnapshot>,
    /// 总数
    pub total: usize,
}

/// 清理已完成任务请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct CleanupTasksRequest {
    /// 每个类型保留的最近已完成任务数量（默认 10）
    #[param(source = "query")]
    pub max_count: Option<usize>,
}

/// 清理已完成任务响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CleanupTasksResponse {
    /// 清理的任务数量
    pub cleaned: usize,
}
