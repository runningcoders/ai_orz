//! Task 模块
//!
//! 包含：
//! - TaskPo - 持久化对象（只在 DAO/DAL 层使用）
//! - Task - 业务实体（Domain 层使用，包含聚合关系和业务方法）

use crate::models::vector::{SearchMatchInfo, VectorPayload, Vectorizable};
use crate::pkg::request_context::{EnrichContext, RequestContextBuilder};
use common::api::ArtifactDetail;
use common::constants::utils;
use common::enums::{AssigneeType, TaskStatus};
use common::models::{ModelCallStats, TaskStats};
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
    /// 任务进度（0-100）
    pub progress: i32,
    /// 创建者用户 ID（可能是 Agent 创建）
    pub created_by: String,
    /// 最后修改者用户 ID
    pub modified_by: String,
    /// 创建时间戳（毫秒）
    pub created_at: i64,
    /// 更新时间戳（毫秒）
    pub updated_at: i64,
    /// 执行计划（Agent Loop 规划阶段产出）
    pub execution_plan: Option<String>,
    /// 执行结果（Agent Loop 执行阶段产出）
    pub execution_result: Option<String>,
}

/// Task 业务实体
///
/// 聚合所有相关信息：任务基本信息 + 产物列表
/// 这是 Domain 层返回给上层的类型
#[derive(Debug, Clone)]
pub struct Task {
    /// 底层持久化对象
    pub po: TaskPo,
    /// 搜索匹配元信息（搜索场景下由 DAL 层填充）
    pub search_match: Option<SearchMatchInfo>,
    /// 统计数据（由 DAL 层按需注入）
    pub stats: Option<TaskStats>,
    /// 模型调用统计数据（由 DAL 层按需注入）
    pub model_call_stats: Option<ModelCallStats>,
    /// 产物列表（由 Domain 层按需注入）
    pub artifacts: Option<Vec<ArtifactDetail>>,
}

impl Task {
    /// 从 PO 创建 Task
    pub fn from_po(po: TaskPo) -> Self {
        Self {
            po,
            search_match: None,
            stats: None,
            model_call_stats: None,
            artifacts: None,
        }
    }

    /// 创建新的 Task
    #[allow(clippy::too_many_arguments)]
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
            search_match: None,
            stats: None,
            model_call_stats: None,
            artifacts: None,
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
        self.po.start_at = Some(utils::current_timestamp_ms());
    }

    /// 完成任务
    pub fn complete(&mut self) {
        self.po.status = TaskStatus::Completed;
        self.po.progress = 100;
        self.po.end_at = Some(utils::current_timestamp_ms());
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

    /// 获取任务进度
    pub fn progress(&self) -> i32 {
        self.po.progress
    }

    /// 设置任务进度（0-100，超出范围会被截断）
    pub fn set_progress(&mut self, progress: i32) {
        self.po.progress = progress.clamp(0, 100);
        self.po.updated_at = utils::current_timestamp_ms();
    }

    /// 生成 Prompt 用的摘要字符串
    ///
    /// 用于在 Prompt 中注入任务上下文，让 Agent 感知当前消息所属的具体任务。
    /// 仅包含关键字段，避免冗长；前置依赖任务 ID 一并带出，
    /// Agent 可据此自行 `get_task(前置ID, with_artifacts=true)` 查询前置任务状态与产物
    /// （DAG 协作中前置任务的执行结果/方案文档常记录在产物里，需阅读后方可接力）。
    pub fn to_prompt_summary(&self) -> String {
        let mut s = String::from("【任务上下文】\n");
        s.push_str(&format!("- 任务ID: {}\n", self.po.id));
        s.push_str(&format!("- 任务标题: {}\n", self.po.title));
        if !self.po.description.is_empty() {
            s.push_str(&format!("- 任务描述: {}\n", self.po.description));
        }
        s.push_str(&format!("- 任务状态: {:?}\n", self.po.status));
        s.push_str(&format!(
            "- 分配给: {:?}({})\n",
            self.po.assignee_type, self.po.assignee_id
        ));
        s.push_str(&format!("- 任务进度: {}%\n", self.po.progress));
        let deps = self.po.get_dependencies();
        if !deps.is_empty() {
            s.push_str(&format!("- 前置依赖任务: {}\n", deps.join(", ")));
            s.push_str("  （前置任务的执行结果与产出物常记录在产物里；需要时可用 get_task(前置任务ID, with_artifacts=true) 查看其状态、执行结果与产物清单，并阅读所依赖的方案/实现产物）\n");
        }
        s
    }
}

impl TaskPo {
    /// 创建新的 TaskPo
    #[allow(clippy::too_many_arguments)]
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
        let now = utils::current_timestamp_ms();
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
            progress: 0,
            created_by: created_by.clone(),
            modified_by: created_by,
            created_at: now,
            updated_at: now,
            execution_plan: None,
            execution_result: None,
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
        self.updated_at = utils::current_timestamp_ms();
    }

    /// 重置思考深度（用户回复后）
    pub fn reset_thinking_depth(&mut self) {
        self.thinking_depth = 0;
        self.updated_at = utils::current_timestamp_ms();
    }
}

impl crate::pkg::request_context::EnrichContext for TaskPo {
    fn enrich(
        &self,
        builder: crate::pkg::request_context::RequestContextBuilder,
    ) -> crate::pkg::request_context::RequestContextBuilder {
        builder
            .task_id(self.id.clone())
            .try_project_id(self.project_id.clone())
    }
}

impl EnrichContext for Task {
    fn enrich(&self, builder: RequestContextBuilder) -> RequestContextBuilder {
        self.po.enrich(builder)
    }
}

impl Vectorizable for TaskPo {
    fn vectorize_text(&self) -> String {
        format!("{}\n{}", self.title, self.description)
    }

    fn vector_collection() -> &'static str {
        "tasks"
    }

    fn vector_id(&self) -> &str {
        &self.id
    }

    fn vector_payload(&self) -> VectorPayload {
        VectorPayload {
            project_id: self.project_id.clone(),
            // assignee_type 非 Agent（如 User）时语义不符，保守不落
            agent_id: (self.assignee_type == AssigneeType::Agent).then(|| self.assignee_id.clone()),
            tags: Some(self.tags.clone()),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(dependencies: Vec<String>) -> Task {
        Task::new(
            "task-main".to_string(),
            "实现导出功能".to_string(),
            "导出为 CSV".to_string(),
            2,
            vec![],
            None,
            None,
            None,
            dependencies,
            "root-user".to_string(),
            AssigneeType::Agent,
            "agent-1".to_string(),
            Some("proj-1".to_string()),
            "creator".to_string(),
        )
    }

    #[test]
    fn test_prompt_summary_with_dependencies_lists_ids_and_hint() {
        let task = make_task(vec!["task-pre-a".to_string(), "task-pre-b".to_string()]);
        let summary = task.to_prompt_summary();
        assert!(summary.contains("- 前置依赖任务: task-pre-a, task-pre-b"));
        assert!(summary.contains("get_task(前置任务ID, with_artifacts=true)"));
        assert!(summary.contains("产物清单"));
    }

    #[test]
    fn test_prompt_summary_without_dependencies_omits_line() {
        let task = make_task(vec![]);
        let summary = task.to_prompt_summary();
        assert!(!summary.contains("前置依赖任务"));
        assert!(!summary.contains("with_artifacts"));
    }
}
