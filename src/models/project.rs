//! Project 持久化对象
//!
//! 对应 SQL 建表语句：migrations/20260411000000_initial.sql

use common::constants::utils;
use common::enums::project::ProjectStatus;
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

impl ProjectPo {
    /// 创建新的 ProjectPo
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
