//! Skill DAO trait definition

pub mod sqlite;

use crate::error::AppError;
use crate::models::skill::{SkillPo, SkillFile};
use crate::models::vector::{VectorIndexParams, SearchResult};
use crate::pkg::RequestContext;
use common::enums::SkillStatus;
use async_trait::async_trait;

/// Skill 查询参数
#[derive(Debug, Clone, Default)]
pub struct SkillQuery {
    pub ids: Option<Vec<String>>,           // 按 ID 批量查询
    pub status: Option<SkillStatus>,
    pub exclude_status: Option<SkillStatus>,
    pub category: Option<String>,
    pub author_id: Option<String>,
    pub keyword: Option<String>,
    pub limit: Option<usize>,
}

/// ✅ 技能搜索统一入参（关键词搜索 + 向量语义搜索共用）
#[derive(Debug, Clone, Default)]
pub struct SkillSearch {
    /// 关键词搜索查询（用于传统 LIKE 匹配）
    pub keyword: Option<String>,
    /// 查询向量（用于向量语义搜索，DAL 层填充）
    pub query_vector: Option<Vec<f32>>,
    /// 返回 Top K 结果（向量搜索专用）
    pub top_k: Option<i32>,
    /// ✅ 业务过滤条件（直接复用 SkillQuery）
    pub filters: SkillQuery,
}

/// Skill DAO trait
#[async_trait]
pub trait SkillDao: Send + Sync {
    // ========== 基础 CRUD ==========

    /// Insert a new skill
    async fn insert(&self, ctx: RequestContext, skill: &SkillPo) -> Result<(), AppError>;

    /// Update an existing skill
    async fn update(&self, ctx: RequestContext, skill: &SkillPo) -> Result<(), AppError>;

    /// Soft delete (mark as expired)
    async fn delete_by_id(&self, ctx: RequestContext, id: &str) -> Result<(), AppError>;

    // ========== 向量增强方法 ==========

    /// Insert a new skill with vector index
    async fn insert_with_vector(
        &self,
        ctx: RequestContext,
        skill: &SkillPo,
        vector_params: &VectorIndexParams,
    ) -> Result<(), AppError>;

    /// Update an existing skill with vector index
    async fn update_with_vector(
        &self,
        ctx: RequestContext,
        skill: &SkillPo,
        vector_params: &VectorIndexParams,
    ) -> Result<(), AppError>;

    /// ✅ 查询技能的向量索引内容哈希（DAL 判断是否需要重索引）
    async fn get_vector_content_hash(
        &self,
        ctx: RequestContext,
        skill_id: &str,
    ) -> Result<Option<String>, AppError>;

    // ========== 查询与搜索 ==========

    /// Find skill by id
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<SkillPo>, AppError>;

    /// 通用组合查询
    async fn query(&self, ctx: RequestContext, query: SkillQuery) -> Result<Vec<SkillPo>, AppError>;

    /// List skills by status
    async fn list_by_status(&self, ctx: RequestContext, status: SkillStatus) -> Result<Vec<SkillPo>, AppError>;

    /// List skills by category
    async fn list_by_category(&self, ctx: RequestContext, category: &str) -> Result<Vec<SkillPo>, AppError>;

    /// List skills by author
    async fn list_by_author(&self, ctx: RequestContext, author_id: &str) -> Result<Vec<SkillPo>, AppError>;

    /// 搜索技能（支持关键词搜索 OR 向量语义搜索）
    /// - 有 keyword 但无 query_vector: 走 SQL LIKE 模糊匹配
    /// - 有 query_vector 但无 keyword: 走向量语义搜索
    /// - 两者都有: 执行混合搜索策略（可扩展）
    async fn search(
        &self,
        ctx: RequestContext,
        search: SkillSearch,
    ) -> Result<Vec<SearchResult<SkillPo>>, AppError>;

    // ========== 业务操作 ==========

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

    // ========== 文件操作 ==========

    /// 读取 skill.md 主文件内容
    fn read_main_content(&self, skill: &SkillPo) -> Result<String, AppError>;

    /// 写入 skill.md 主文件内容
    fn write_main_content(&self, skill: &SkillPo, content: &str) -> Result<(), AppError>;

    /// 列出技能目录下的所有文件（小文件自动预读内容）
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
