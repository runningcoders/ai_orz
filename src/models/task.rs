//! Task 模块
//!
//! 包含：
//! - TaskPo - 持久化对象（只在 DAO/DAL 层使用）
//! - Task - 业务实体（Domain 层使用，包含聚合关系和业务方法）

use common::constants::utils;
use common::enums::{AssigneeType, TaskStatus};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// TaskPo 任务持久化对象
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TaskPo {
    /// 任务 ID
    pub id: String,
    /// 任务标题
    pub title: String,
    /// 任务详细描述
    pub description: String,
    /// 任务状态
    pub status: TaskStatus,
    /// 优先级（数值越大优先级越高）
    pub priority: i32,
    /// 标签列表（JSON 数组字符串）
    pub tags: String,
    /// 截止时间戳（毫秒），可为空
    pub due_at: Option<i64>,
    /// 开始时间戳（毫秒），可为空
    pub start_at: Option<i64>,
    /// 结束时间戳（毫秒），可为空
    pub end_at: Option<i64>,
    /// 前置任务 ID 列表（JSON 数组字符串），可为空表示无依赖
    pub dependencies: Option<String>,
    /// 根用户 ID：这个任务最终为哪个用户服务，所有派生任务继承此字段
    pub root_user_id: String,
    /// 分配对象类型
    pub assignee_type: AssigneeType,
    /// 分配对象 ID
    pub assignee_id: String,
    /// 所属项目 ID，可为空
    pub project_id: Option<String>,
    /// 当前思考深度（轮次）
    pub thinking_depth: i64,
    /// 创建者用户 ID（可能是 Agent 创建）
    pub created_by: String,
    /// 最后修改者用户 ID
    pub modified_by: String,
    /// 创建时间戳（毫秒）
    pub created_at: i64,
    /// 更新时间戳（毫秒）
    pub updated_at: i64,
}

/// Task 业务实体
///
/// 聚合所有相关信息：任务基本信息 + 产物列表
/// 这是 Domain 层返回给上层的类型
#[derive(Debug, Clone)]
pub struct Task {
    /// 底层持久化对象
    pub po: TaskPo,
}

impl Task {
    /// 从 PO 创建 Task
    pub fn from_po(po: TaskPo) -> Self {
        Self { po }
    }

    /// 创建新的 Task
    pub fn new(
        id: String,
        title: String,
        description: String,
        priority: i32,
        tags: Vec<String>,
        due_at: Option<i64>,
        start_at: Option<i64>,
        end_at: Option<i64>,
        dependencies: Vec<String>,
        root_user_id: String,
        assignee_type: AssigneeType,
        assignee_id: String,
        project_id: Option<String>,
        created_by: String,
    ) -> Self {
        Self {
            po: TaskPo::new(
                id,
                title,
                description,
                priority,
                tags,
                due_at,
                start_at,
                end_at,
                dependencies,
                root_user_id,
                assignee_type,
                assignee_id,
                project_id,
                created_by,
            ),
        }
    }

    /// 转换为 PO
    pub fn into_po(self) -> TaskPo {
        self.po
    }

    /// 获取任务 ID
    pub fn id(&self) -> &str {
        &self.po.id
    }

    /// 获取任务标题
    pub fn title(&self) -> &str {
        &self.po.title
    }

    /// 获取任务状态
    pub fn status(&self) -> TaskStatus {
        self.po.status
    }

    /// 获取所属项目 ID
    pub fn project_id(&self) -> Option<&str> {
        self.po.project_id.as_deref()
    }

    /// 判断任务是否已完成
    pub fn is_completed(&self) -> bool {
        matches!(self.po.status, TaskStatus::Completed)
    }

    /// 判断任务是否已开始
    pub fn is_started(&self) -> bool {
        self.po.start_at.is_some()
    }

    /// 启动任务
    pub fn start(&mut self) {
        self.po.status = TaskStatus::InProgress;
        self.po.start_at = Some(utils::current_timestamp());
    }

    /// 完成任务
    pub fn complete(&mut self) {
        self.po.status = TaskStatus::Completed;
        self.po.end_at = Some(utils::current_timestamp());
    }

    /// 取消任务
    pub fn cancel(&mut self) {
        self.po.status = TaskStatus::Cancelled;
    }

    /// 增加思考深度（每次思考调用）
    pub fn increment_thinking_depth(&mut self) {
        self.po.increment_thinking_depth();
    }

    /// 重置思考深度（用户回复后）
    pub fn reset_thinking_depth(&mut self) {
        self.po.reset_thinking_depth();
    }

    /// 获取当前思考深度
    pub fn thinking_depth(&self) -> i64 {
        self.po.thinking_depth
    }
}

impl TaskPo {
    /// 创建新的 TaskPo
    pub fn new(
        id: String,
        title: String,
        description: String,
        priority: i32,
        tags: Vec<String>,
        due_at: Option<i64>,
        start_at: Option<i64>,
        end_at: Option<i64>,
        dependencies: Vec<String>,
        root_user_id: String,
        assignee_type: AssigneeType,
        assignee_id: String,
        project_id: Option<String>,
        created_by: String,
    ) -> Self {
        let now = utils::current_timestamp();
        // tags 序列化为 JSON 字符串存储
        let tags_json = serde_json::to_string(&tags).unwrap_or_default();
        // dependencies 序列化为 JSON 字符串存储，如果为空则存 None
        let dependencies_json = if dependencies.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&dependencies).unwrap_or_default())
        };
        Self {
            id,
            title,
            description,
            status: TaskStatus::default(),
            priority,
            tags: tags_json,
            due_at,
            start_at,
            end_at,
            dependencies: dependencies_json,
            root_user_id,
            assignee_type,
            assignee_id,
            project_id,
            thinking_depth: 0,
            created_by: created_by.clone(),
            modified_by: created_by,
            created_at: now,
            updated_at: now,
        }
    }

    /// 反序列化得到标签列表
    pub fn get_tags(&self) -> Vec<String> {
        serde_json::from_str(&self.tags).unwrap_or_default()
    }

    /// 反序列化得到依赖任务 ID 列表
    pub fn get_dependencies(&self) -> Vec<String> {
        match &self.dependencies {
            Some(deps) => serde_json::from_str(deps).unwrap_or_default(),
            None => Vec::new(),
        }
    }

    /// 增加思考深度（每次思考调用）
    pub fn increment_thinking_depth(&mut self) {
        self.thinking_depth += 1;
        self.updated_at = utils::current_timestamp();
    }

    /// 重置思考深度（用户回复后）
    pub fn reset_thinking_depth(&mut self) {
        self.thinking_depth = 0;
        self.updated_at = utils::current_timestamp();
    }
}
