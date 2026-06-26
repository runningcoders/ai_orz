//! Skill DAO trait definition

pub mod sqlite;
pub mod vector;

#[cfg(test)]
mod sqlite_test;
#[cfg(test)]
mod vector_test;

use common::error::{err, bail_err, Error, Result};
use crate::models::skill::{SkillFile, SkillPo};
use crate::models::vector::VectorIndexParams;
use crate::pkg::RequestContext;
use async_trait::async_trait;
use common::enums::SkillStatus;

/// Skill 查询参数
#[derive(Debug, Clone, Default)]
pub struct SkillQuery {
    pub ids: Option<Vec<String>>, // 按 ID 批量查询
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

// ==================== 基础 SkillDao: 仅负责基础技能数据 ====================

/// Skill DAO trait - 仅负责基础技能数据的 CRUD，不包含向量逻辑
#[async_trait]
pub trait SkillDao: Send + Sync {
    // ========== 基础 CRUD ==========

    /// Insert a new skill
    async fn insert(&self, ctx: RequestContext, skill: &SkillPo) -> Result<()>;

    /// Update an existing skill
    async fn update(&self, ctx: RequestContext, skill: &SkillPo) -> Result<()>;

    /// Soft delete (mark as expired)
    async fn delete_by_id(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// Find skill by id
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<SkillPo>>;

    /// 通用组合查询
    async fn query(&self, ctx: RequestContext, query: SkillQuery)
    -> Result<Vec<SkillPo>>;

    /// List skills by status
    async fn list_by_status(
        &self,
        ctx: RequestContext,
        status: SkillStatus,
    ) -> Result<Vec<SkillPo>>;

    /// List skills by category
    async fn list_by_category(
        &self,
        ctx: RequestContext,
        category: &str,
    ) -> Result<Vec<SkillPo>>;

    /// List skills by author
    async fn list_by_author(
        &self,
        ctx: RequestContext,
        author_id: &str,
    ) -> Result<Vec<SkillPo>>;

    // ========== 业务操作 ==========

    /// Install a published shared skill to an agent as a private draft copy
    async fn install_to_agent(
        &self,
        ctx: RequestContext,
        source_skill: &SkillPo,
        target_agent_id: &str,
    ) -> Result<SkillPo>;

    /// 统一搜索入口（关键词 + 业务过滤，向量搜索由 SkillVectorDao 单独处理）
    async fn search(
        &self,
        ctx: RequestContext,
        search: SkillSearch,
    ) -> Result<Vec<SkillPo>>;

    // ========== 文件操作 ==========

    /// 读取 skill.md 主文件内容
    fn read_main_content(&self, skill: &SkillPo) -> Result<String>;

    /// 写入 skill.md 主文件内容
    fn write_main_content(&self, skill: &SkillPo, content: &str) -> Result<()>;

    /// 列出技能目录下的所有文件（小文件自动预读内容）
    fn list_files(&self, skill: &SkillPo) -> Result<Vec<SkillFile>>;

    /// 读取指定文件名的内容
    fn read_file(&self, skill: &SkillPo, filename: &str) -> Result<String>;

    /// 写入指定文件名的内容
    fn write_file(&self, skill: &SkillPo, filename: &str, content: &str) -> Result<()>;

    /// 写入指定文件名的原始 bytes。
    fn write_file_bytes(
        &self,
        skill: &SkillPo,
        filename: &str,
        bytes: &[u8],
    ) -> Result<()>;

    /// 删除整个技能目录（卸载/删除时调用）
    fn delete_skill_dir(&self, skill: &SkillPo) -> Result<()>;
}

// ==================== SkillVectorDao Trait ====================

/// Skill Vector DAO trait - 仅负责技能向量索引的 CRUD，与基础技能数据解耦
/// 所有方法返回完整的行级结构体，与底层 VectorStore trait 保持一致
#[async_trait]
pub trait SkillVectorDao: Send + Sync {
    /// 插入或更新技能的向量索引
    async fn upsert_vector(
        &self,
        ctx: RequestContext,
        skill_id: &str,
        vector_params: &VectorIndexParams,
    ) -> Result<()>;

    /// 纯向量语义搜索，返回完整的向量行数据 + 相似度距离
    async fn search_vector(
        &self,
        ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
    ) -> Result<Vec<crate::models::vector::VectorSearchHit>>;

    /// 获取指定技能的完整向量行数据（包含元信息）
    async fn get_vector_row(
        &self,
        ctx: RequestContext,
        skill_id: &str,
    ) -> Result<Option<crate::models::vector::VectorRow>>;
}

// ==================== 统一导出 ====================

// 子模块构造函数别名（用于 DAL 层组合）
pub use sqlite::{
    dao as base_dao, init as init_base, new as new_skill_dao,
    new_with_base_path as new_skill_dao_with_base_path,
};
pub use vector::{dao as vector_dao, init as init_vector, new as new_skill_vector_dao};

/// 统一初始化所有 Skill DAO 单例
pub fn init() {
    init_base();
    init_vector();
}

// ========== 向后兼容：旧代码继续使用 `skill::new()` / `skill::dao()` ==========
pub fn new() -> std::sync::Arc<dyn SkillDao> {
    new_skill_dao()
}

pub fn dao() -> std::sync::Arc<dyn SkillDao> {
    base_dao()
}
