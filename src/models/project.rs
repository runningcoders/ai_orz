//! Project 模块
//!
//! 包含：
//! - ProjectPo - 持久化对象（只在 DAO/DAL 层使用）
//! - Project - 业务实体（Domain 层使用，包含聚合关系和业务方法）

use crate::pkg::request_context::{EnrichContext, RequestContextBuilder};
use common::constants::utils;
use common::enums::project::ProjectStatus;
use common::models::{ModelCallStats, ProjectStats};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// ProjectPo 项目持久化对象
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ProjectPo {
    /// 项目 ID
    pub id: String,
    /// 项目名称
    pub name: String,
    /// 项目详细描述
    pub description: String,
    /// 项目运作流程描述，各角色协作方式（可选，为空使用默认流程）
    pub workflow: Option<String>,
    /// 用户对项目的指导建议，Agent 执行时参考（可选）
    pub guidance: Option<String>,
    /// 项目状态
    pub status: ProjectStatus,
    /// 优先级（数值越大优先级越高）
    pub priority: i32,
    /// 标签列表（JSON 数组字符串）
    pub tags: String,
    /// 根用户 ID：这个项目最终归属哪个用户
    pub root_user_id: String,
    /// 负责人 Agent ID（PMO 推进项目），可为空
    pub owner_agent_id: Option<String>,
    /// 开始时间戳（毫秒），可为空
    pub start_at: Option<i64>,
    /// 截止时间戳（毫秒），可为空
    pub due_at: Option<i64>,
    /// 结束时间戳（毫秒），可为空
    pub end_at: Option<i64>,
    /// 创建者用户 ID（可能是 Agent 创建）
    pub created_by: String,
    /// 最后修改者用户 ID
    pub modified_by: String,
    /// 创建时间戳（毫秒）
    pub created_at: i64,
    /// 更新时间戳（毫秒）
    pub updated_at: i64,
}

/// Project 业务实体
///
/// 聚合所有相关信息：项目基本信息 + 任务列表
/// 这是 Domain 层返回给上层的类型
#[derive(Debug, Clone)]
pub struct Project {
    /// 底层持久化对象
    pub po: ProjectPo,
    /// 搜索匹配元信息（搜索场景下由 DAL 层填充）
    pub search_match: Option<crate::models::vector::SearchMatchInfo>,
    /// 统计数据（由 DAL 层按需注入）
    pub stats: Option<ProjectStats>,
    /// 模型调用统计数据（由 DAL 层按需注入）
    pub model_call_stats: Option<ModelCallStats>,
}

impl Project {
    /// 从 PO 创建 Project
    pub fn from_po(po: ProjectPo) -> Self {
        Self {
            po,
            search_match: None,
            stats: None,
            model_call_stats: None,
        }
    }

    /// 创建新的 Project
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        name: String,
        description: String,
        workflow: Option<String>,
        guidance: Option<String>,
        priority: i32,
        tags: Vec<String>,
        root_user_id: String,
        owner_agent_id: Option<String>,
        start_at: Option<i64>,
        due_at: Option<i64>,
        end_at: Option<i64>,
        created_by: String,
    ) -> Self {
        Self {
            po: ProjectPo::new(
                id,
                name,
                description,
                workflow,
                guidance,
                priority,
                tags,
                root_user_id,
                owner_agent_id,
                start_at,
                due_at,
                end_at,
                created_by,
            ),
            search_match: None,
            stats: None,
            model_call_stats: None,
        }
    }

    /// 转换为 PO（消耗 self）
    pub fn into_po(self) -> ProjectPo {
        self.po
    }

    /// 获取项目 ID
    pub fn id(&self) -> &str {
        &self.po.id
    }

    /// 获取项目名称
    pub fn name(&self) -> &str {
        &self.po.name
    }

    /// 获取项目状态
    pub fn status(&self) -> ProjectStatus {
        self.po.status
    }

    /// 获取项目优先级
    pub fn priority(&self) -> i32 {
        self.po.priority
    }

    /// 判断项目是否已完成
    pub fn is_completed(&self) -> bool {
        matches!(self.po.status, ProjectStatus::Completed)
    }

    /// 判断项目是否已归档
    pub fn is_archived(&self) -> bool {
        matches!(self.po.status, ProjectStatus::Archived)
    }

    /// 启动项目
    pub fn start(&mut self) {
        self.po.status = ProjectStatus::InProgress;
        self.po.start_at = Some(utils::current_timestamp());
    }

    /// 完成项目
    pub fn complete(&mut self) {
        self.po.status = ProjectStatus::Completed;
        self.po.end_at = Some(utils::current_timestamp());
    }
}

impl ProjectPo {
    /// 创建新的 ProjectPo
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        name: String,
        description: String,
        workflow: Option<String>,
        guidance: Option<String>,
        priority: i32,
        tags: Vec<String>,
        root_user_id: String,
        owner_agent_id: Option<String>,
        start_at: Option<i64>,
        due_at: Option<i64>,
        end_at: Option<i64>,
        created_by: String,
    ) -> Self {
        let now = utils::current_timestamp();
        // tags 序列化为 JSON 字符串存储
        let tags_json = serde_json::to_string(&tags).unwrap_or_default();
        Self {
            id,
            name,
            description,
            workflow,
            guidance,
            status: ProjectStatus::default(),
            priority,
            tags: tags_json,
            root_user_id,
            owner_agent_id,
            start_at,
            due_at,
            end_at,
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
}

impl crate::pkg::request_context::EnrichContext for ProjectPo {
    fn enrich(
        &self,
        builder: crate::pkg::request_context::RequestContextBuilder,
    ) -> crate::pkg::request_context::RequestContextBuilder {
        builder
            .project_id(self.id.clone())
            .try_agent_id(self.owner_agent_id.clone())
    }
}

impl EnrichContext for Project {
    fn enrich(&self, builder: RequestContextBuilder) -> RequestContextBuilder {
        self.po.enrich(builder)
    }
}

// ==================== Vectorizable 实现 ====================

impl crate::models::vector::Vectorizable for ProjectPo {
    fn vectorize_text(&self) -> String {
        // ProjectPo 向量化：name + description + workflow + guidance
        // workflow 和 guidance 可能为 NULL/空，跳过空值避免多余换行
        let mut parts: Vec<&str> = vec![&self.name, &self.description];
        if let Some(w) = &self.workflow
            && !w.trim().is_empty()
        {
            parts.push(w.as_str());
        }
        if let Some(g) = &self.guidance
            && !g.trim().is_empty()
        {
            parts.push(g.as_str());
        }
        parts.join("\n")
    }

    fn vector_collection() -> &'static str {
        "projects"
    }
}

impl crate::models::vector::Vectorizable for Project {
    fn vectorize_text(&self) -> String {
        self.po.vectorize_text()
    }

    fn vector_collection() -> &'static str {
        "projects"
    }
}
