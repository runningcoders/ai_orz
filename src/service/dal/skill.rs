//! Skill DAL 模块
//!
//! 技能数据访问层，提供技能查询和管理能力
//! 负责组合 DAO 完成业务级数据操作，组装完整 Skill 实体（PO + 文件）

use crate::error::AppError;
use crate::models::skill::{Skill, SkillPo, SkillFile};
use crate::pkg::request_context::RequestContext;
use crate::service::dao::skill::{SkillDao, SkillQuery, SkillSearch, self};
use std::sync::{Arc, OnceLock};

// ==================== 单例管理 ====================

static SKILL_DAL: OnceLock<Arc<dyn SkillDal>> = OnceLock::new();

/// 获取 Skill DAL 单例
pub fn dal() -> Arc<dyn SkillDal> {
    SKILL_DAL.get().cloned().unwrap()
}

/// 初始化 Skill DAL（使用全局单例 DAO）
pub fn init() {
    let _ = SKILL_DAL.set(new(skill::dao()));
}

/// 创建 Skill DAL（返回 trait 对象）
pub fn new(skill_dao: Arc<dyn SkillDao + Send + Sync>) -> Arc<dyn SkillDal> {
    Arc::new(SkillDalImpl { skill_dao })
}

// ==================== DAL 接口 ====================

/// Skill DAL 接口
#[async_trait::async_trait]
pub trait SkillDal: Send + Sync {
    /// 创建新技能（仅数据库）
    async fn create(&self, ctx: RequestContext, po: &SkillPo) -> Result<(), AppError>;

    /// 根据 ID 获取完整技能（PO + 文件列表）
    async fn get_by_id(&self, ctx: RequestContext, id: String) -> Result<Option<Skill>, AppError>;

    /// 根据 ID 获取 PO 数据（不需要文件时用这个）
    async fn get_po_by_id(&self, ctx: RequestContext, id: String) -> Result<Option<SkillPo>, AppError>;

    /// 通用综合查询（返回完整 Skill 实体，包含 PO + 文件列表）
    async fn query(&self, ctx: RequestContext, query: SkillQuery) -> Result<Vec<Skill>, AppError>;

    /// 按状态查询（返回完整 Skill 实体）
    async fn list_by_status(&self, ctx: RequestContext, status: common::enums::SkillStatus) -> Result<Vec<Skill>, AppError>;

    /// 按分类查询（返回完整 Skill 实体）
    async fn list_by_category(&self, ctx: RequestContext, category: &str) -> Result<Vec<Skill>, AppError>;

    /// 按作者查询（返回完整 Skill 实体）
    async fn list_by_author(&self, ctx: RequestContext, author_id: &str) -> Result<Vec<Skill>, AppError>;

    /// 获取 Agent 的所有技能（返回完整 Skill 实体）
    async fn list_for_agent(&self, ctx: RequestContext, agent_id: &str) -> Result<Vec<Skill>, AppError>;

    /// 搜索技能（名称/描述/标签）
    async fn search(&self, ctx: RequestContext, keyword: &str) -> Result<Vec<Skill>, AppError>;

    /// 更新技能元数据（不影响文件）
    async fn update(&self, ctx: RequestContext, po: &SkillPo) -> Result<(), AppError>;

    /// 删除技能（删除数据库记录 + 文件目录）
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<(), AppError>;

    /// 将已发布技能安装到 Agent（原子操作：复制文件 + 创建数据库记录）
    /// 返回安装后新创建的技能 PO
    async fn install_to_agent(
        &self,
        ctx: RequestContext,
        source_skill_id: &str,
        agent_id: &str,
    ) -> Result<SkillPo, AppError>;

    /// 读取技能主文件内容（skill.md）
    fn read_main_content(&self, skill: &SkillPo) -> Result<String, AppError>;

    /// 写入技能主文件内容（skill.md）
    fn write_main_content(&self, skill: &SkillPo, content: &str) -> Result<(), AppError>;

    /// 列出技能的所有文件（小文件自动预读内容）
    fn list_files(&self, skill: &SkillPo) -> Result<Vec<SkillFile>, AppError>;

    /// 读取指定文件内容
    fn read_file(&self, skill: &SkillPo, filename: &str) -> Result<String, AppError>;

    /// 写入文件内容
    fn write_file(&self, skill: &SkillPo, filename: &str, content: &str) -> Result<(), AppError>;
}

// ==================== DAL 实现 ====================

/// Skill DAL 基础实现
pub struct SkillDalImpl {
    skill_dao: Arc<dyn SkillDao + Send + Sync>,
}

#[async_trait::async_trait]
impl SkillDal for SkillDalImpl {
    async fn create(&self, ctx: RequestContext, po: &SkillPo) -> Result<(), AppError> {
        Ok(self.skill_dao.insert(ctx, po).await?)
    }

    async fn get_by_id(&self, ctx: RequestContext, id: String) -> Result<Option<Skill>, AppError> {
        let Some(po) = self.skill_dao.find_by_id(ctx, &id).await? else {
            return Ok(None);
        };
        let files = self.skill_dao.list_files(&po)?;
        Ok(Some(Skill { po, files, search_match: None }))
    }

    async fn get_po_by_id(&self, ctx: RequestContext, id: String) -> Result<Option<SkillPo>, AppError> {
        Ok(self.skill_dao.find_by_id(ctx, &id).await?)
    }

    async fn query(&self, ctx: RequestContext, query: SkillQuery) -> Result<Vec<Skill>, AppError> {
        let pos = self.skill_dao.query(ctx, query).await?;
        let mut skills = Vec::with_capacity(pos.len());
        for po in pos {
            let files = self.skill_dao.list_files(&po)?;
            skills.push(Skill { po, files, search_match: None });
        }
        Ok(skills)
    }

    async fn list_by_status(&self, ctx: RequestContext, status: common::enums::SkillStatus) -> Result<Vec<Skill>, AppError> {
        self.query(ctx, SkillQuery { status: Some(status), ..Default::default() }).await
    }

    async fn list_by_category(&self, ctx: RequestContext, category: &str) -> Result<Vec<Skill>, AppError> {
        self.query(ctx, SkillQuery { category: Some(category.to_string()), ..Default::default() }).await
    }

    async fn list_by_author(&self, ctx: RequestContext, author_id: &str) -> Result<Vec<Skill>, AppError> {
        self.query(ctx, SkillQuery { author_id: Some(author_id.to_string()), ..Default::default() }).await
    }

    async fn list_for_agent(&self, ctx: RequestContext, agent_id: &str) -> Result<Vec<Skill>, AppError> {
        self.query(ctx, SkillQuery { author_id: Some(agent_id.to_string()), ..Default::default() }).await
    }

    async fn search(&self, ctx: RequestContext, keyword: &str) -> Result<Vec<Skill>, AppError> {
        // 调用 DAO 统一 search 入口（仅关键词模式）
        let results = self.skill_dao.search(ctx, SkillSearch {
            keyword: Some(keyword.to_string()),
            query_vector: None,
            top_k: None,
            filters: SkillQuery::default(),
        }).await?;
        
        let mut skills = Vec::with_capacity(results.len());
        for result in results {
            let files = self.skill_dao.list_files(&result.entity)?;
            skills.push(Skill {
                po: result.entity,
                files,
                search_match: Some(result.match_info),
            });
        }
        Ok(skills)
    }

    async fn update(&self, ctx: RequestContext, po: &SkillPo) -> Result<(), AppError> {
        Ok(self.skill_dao.update(ctx, po).await?)
    }

    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<(), AppError> {
        // 先获取 PO（用于删除文件时获取 content_path）
        let Some(po) = self.skill_dao.find_by_id(ctx.clone(), id).await? else {
            return Ok(()); // 不存在就返回成功
        };
        // 先删文件，再删数据库记录
        self.skill_dao.delete_skill_dir(&po)?;
        self.skill_dao.delete_by_id(ctx, id).await?;
        Ok(())
    }

    async fn install_to_agent(
        &self,
        ctx: RequestContext,
        source_skill_id: &str,
        agent_id: &str,
    ) -> Result<SkillPo, AppError> {
        // 先获取源技能 PO
        let source_skill = self.skill_dao.find_by_id(ctx.clone(), source_skill_id).await?
                .ok_or_else(|| AppError::NotFound("Skill not found".to_string()))?;
        // 调用 DAO 原子安装
        Ok(self.skill_dao.install_to_agent(ctx, &source_skill, agent_id).await?)
    }

    fn read_main_content(&self, skill: &SkillPo) -> Result<String, AppError> {
        Ok(self.skill_dao.read_main_content(skill)?)
    }

    fn write_main_content(&self, skill: &SkillPo, content: &str) -> Result<(), AppError> {
        Ok(self.skill_dao.write_main_content(skill, content)?)
    }

    fn list_files(&self, skill: &SkillPo) -> Result<Vec<SkillFile>, AppError> {
        Ok(self.skill_dao.list_files(skill)?)
    }

    fn read_file(&self, skill: &SkillPo, filename: &str) -> Result<String, AppError> {
        Ok(self.skill_dao.read_file(skill, filename)?)
    }

    fn write_file(&self, skill: &SkillPo, filename: &str, content: &str) -> Result<(), AppError> {
        Ok(self.skill_dao.write_file(skill, filename, content)?)
    }
}
