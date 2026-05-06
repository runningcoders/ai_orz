//! Skill DAO trait definition

pub mod sqlite;

use crate::error::AppError;
use crate::models::skill::{SkillPo, SkillFile};
use crate::pkg::RequestContext;
use common::enums::SkillStatus;
use async_trait::async_trait;

/// Skill 查询参数
#[derive(Debug, Clone, Default)]
pub struct SkillQuery {
    pub status: Option<SkillStatus>,
    pub exclude_status: Option<SkillStatus>,
    pub category: Option<String>,
    pub author_id: Option<String>,
    pub keyword: Option<String>,
    pub limit: Option<usize>,
}

/// Skill DAO trait
#[async_trait]
pub trait SkillDao: Send + Sync {
    /// Insert a new skill
    async fn insert(&self, ctx: RequestContext, skill: &SkillPo) -> Result<(), AppError>;

    /// Update an existing skill
    async fn update(&self, ctx: RequestContext, skill: &SkillPo) -> Result<(), AppError>;

    /// Find skill by id
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<SkillPo>, AppError>;

    /// 通用查询
    async fn query(&self, ctx: RequestContext, query: SkillQuery) -> Result<Vec<SkillPo>, AppError>;

    /// List skills by status
    async fn list_by_status(&self, ctx: RequestContext, status: SkillStatus) -> Result<Vec<SkillPo>, AppError>;

    /// List skills by category
    async fn list_by_category(&self, ctx: RequestContext, category: &str) -> Result<Vec<SkillPo>, AppError>;

    /// List skills by author
    async fn list_by_author(&self, ctx: RequestContext, author_id: &str) -> Result<Vec<SkillPo>, AppError>;
    /// Search skills by keyword in name or description
    async fn search(&self, ctx: RequestContext, keyword: &str) -> Result<Vec<SkillPo>, AppError>;

    /// Soft delete (mark as expired)
    async fn delete_by_id(&self, ctx: RequestContext, id: &str) -> Result<(), AppError>;

    /// Install a published shared skill to an agent as a private draft copy
    /// 
    /// - source_skill: the source shared skill to install (already validated by upper layer)
    /// - target_agent_id: which agent to install to (will be the author of the new copy)
    /// 
    /// Atomic operation: copies all skill files + creates database record
    async fn install_to_agent(
        &self,
        ctx: RequestContext,
        source_skill: &SkillPo,
        target_agent_id: &str,
    ) -> Result<SkillPo, AppError>;

    // ===== 文件操作方法 =====

    /// 读取 skill.md 主文件内容
    fn read_main_content(&self, skill: &SkillPo) -> Result<String, AppError>;

    /// 写入 skill.md 主文件内容
    fn write_main_content(&self, skill: &SkillPo, content: &str) -> Result<(), AppError>;

    /// 列出技能目录下的所有文件（返回文件名和大小，不读取内容）
    fn list_files(&self, skill: &SkillPo) -> Result<Vec<SkillFile>, AppError>;

    /// 读取指定文件名的内容
    fn read_file(&self, skill: &SkillPo, filename: &str) -> Result<String, AppError>;

    /// 写入指定文件名的内容
    fn write_file(&self, skill: &SkillPo, filename: &str, content: &str) -> Result<(), AppError>;

    /// 删除整个技能目录（卸载/删除时调用）
    fn delete_skill_dir(&self, skill: &SkillPo) -> Result<(), AppError>;
}

pub use sqlite::{dao, init, new};

#[cfg(test)]
mod sqlite_test;
