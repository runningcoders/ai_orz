//! Skill DAO trait definition

pub mod sqlite;

use crate::error::AppError;
use crate::models::skill::SkillPo;
use crate::pkg::RequestContext;
use common::enums::SkillStatus;
use async_trait::async_trait;

/// Skill DAO trait
#[async_trait]
pub trait SkillDao: Send + Sync {
    /// Insert a new skill
    async fn insert(&self, ctx: RequestContext, skill: &SkillPo) -> Result<(), AppError>;

    /// Update an existing skill
    async fn update(&self, ctx: RequestContext, skill: &SkillPo) -> Result<(), AppError>;

    /// Find skill by id
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<SkillPo>, AppError>;

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
    async fn install_to_agent(
        &self,
        ctx: RequestContext,
        source_skill: &SkillPo,
        target_agent_id: &str,
    ) -> Result<SkillPo, AppError>;
}

pub use sqlite::{dao, init, new};

#[cfg(test)]
mod sqlite_test;
