//! SQLite implementation of Skill DAO

use async_trait::async_trait;
use crate::error::AppError;
use crate::models::skill::SkillPo;
use crate::pkg::RequestContext;
use common::enums::skill::SkillAuthorType;
use common::enums::SkillStatus;
use crate::service::dao::skill::SkillDao;
use std::sync::{Arc, OnceLock};

// ==================== 工厂方法 + 单例 ====================

static SKILL_DAO: OnceLock<Arc<dyn SkillDao>> = OnceLock::new();

/// 创建一个全新的 Skill DAO 实例（用于测试）
pub fn new() -> Arc<dyn SkillDao> {
    Arc::new(SkillDaoSqliteImpl)
}

/// Get Skill DAO singleton
pub fn dao() -> Arc<dyn SkillDao> {
    SKILL_DAO.get().cloned().unwrap()
}

/// Initialize singleton
pub fn init() {
    let _ = SKILL_DAO.set(new());
}

// ==================== 实现 ====================

#[derive(Debug, Clone)]
struct SkillDaoSqliteImpl;

#[async_trait]
impl SkillDao for SkillDaoSqliteImpl {
    async fn insert(&self, ctx: RequestContext, skill: &SkillPo) -> Result<(), AppError> {
        let status_i32 = skill.status.to_i32();
        let author_type_i32 = skill.author_type.to_i32();
        sqlx::query!(
            r#"
INSERT INTO skills (
    id, name, description, tags, category, parent_skill_id,
    author_id, author_type, modifier_id, status, created_at, updated_at, content_path
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            skill.id,
            skill.name,
            skill.description,
            skill.tags,
            skill.category,
            skill.parent_skill_id,
            skill.author_id,
            author_type_i32,
            skill.modifier_id,
            status_i32,
            skill.created_at,
            skill.updated_at,
            skill.content_path
        )
        .execute(ctx.db_pool())
        .await?;
        Ok(())
    }

    async fn update(&self, ctx: RequestContext, skill: &SkillPo) -> Result<(), AppError> {
        let now = chrono::Utc::now().timestamp_millis();
        let status_i32 = skill.status.to_i32();
        let author_type_i32 = skill.author_type.to_i32();
        sqlx::query!(
            r#"
UPDATE skills SET
    name = ?, description = ?, tags = ?, category = ?, parent_skill_id = ?,
    author_id = ?, author_type = ?, modifier_id = ?, status = ?, updated_at = ?, content_path = ?
WHERE id = ?
            "#,
            skill.name,
            skill.description,
            skill.tags,
            skill.category,
            skill.parent_skill_id,
            skill.author_id,
            author_type_i32,
            skill.modifier_id,
            status_i32,
            now,
            skill.content_path,
            skill.id
        )
        .execute(ctx.db_pool())
        .await?;
        Ok(())
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<SkillPo>, AppError> {
        let skill = sqlx::query_as!(
            SkillPo,
            r#"
SELECT id, name, description, tags, category, parent_skill_id,
       author_id, author_type AS "author_type: SkillAuthorType", modifier_id, status AS "status: SkillStatus",
       created_at, updated_at, content_path
FROM skills WHERE id = ?
            "#,
            id
        )
        .fetch_optional(ctx.db_pool())
        .await?;
        Ok(skill)
    }

    async fn list_by_status(
        &self,
        ctx: RequestContext,
        status: SkillStatus,
    ) -> Result<Vec<SkillPo>, AppError> {
        let status_i32 = status.to_i32();
        let skills = sqlx::query_as!(
            SkillPo,
            r#"
SELECT id, name, description, tags, category, parent_skill_id,
       author_id, author_type AS "author_type: SkillAuthorType", modifier_id, status AS "status: SkillStatus",
       created_at, updated_at, content_path
FROM skills WHERE status = ? ORDER BY updated_at DESC
            "#,
            status_i32
        )
        .fetch_all(ctx.db_pool())
        .await?;
        Ok(skills)
    }

    async fn list_by_category(
        &self,
        ctx: RequestContext,
        category: &str,
    ) -> Result<Vec<SkillPo>, AppError> {
        let skills = sqlx::query_as!(
            SkillPo,
            r#"
SELECT id, name, description, tags, category, parent_skill_id,
       author_id, author_type AS "author_type: SkillAuthorType", modifier_id, status AS "status: SkillStatus",
       created_at, updated_at, content_path
FROM skills WHERE category = ? ORDER BY updated_at DESC
            "#,
            category
        )
        .fetch_all(ctx.db_pool())
        .await?;
        Ok(skills)
    }

    async fn list_by_author(
        &self,
        ctx: RequestContext,
        author_id: &str,
    ) -> Result<Vec<SkillPo>, AppError> {
        let skills = sqlx::query_as!(
            SkillPo,
            r#"
SELECT id, name, description, tags, category, parent_skill_id,
       author_id, author_type AS "author_type: SkillAuthorType", modifier_id, status AS "status: SkillStatus",
       created_at, updated_at, content_path
FROM skills WHERE author_id = ? ORDER BY updated_at DESC
            "#,
            author_id
        )
        .fetch_all(ctx.db_pool())
        .await?;
        Ok(skills)
    }

    async fn search(&self, ctx: RequestContext, keyword: &str) -> Result<Vec<SkillPo>, AppError> {
        let pattern = format!("%{}%", keyword);
        let skills = sqlx::query_as!(
            SkillPo,
            r#"
SELECT id, name, description, tags, category, parent_skill_id,
       author_id, author_type AS "author_type: SkillAuthorType", modifier_id, status AS "status: SkillStatus",
       created_at, updated_at, content_path
FROM skills
WHERE (name LIKE ? OR description LIKE ?) AND status != 0
ORDER BY updated_at DESC
            "#,
            pattern,
            pattern
        )
        .fetch_all(ctx.db_pool())
        .await?;
        Ok(skills)
    }

    async fn delete_by_id(&self, ctx: RequestContext, id: &str) -> Result<(), AppError> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query!(
            r#"
UPDATE skills SET status = 0, updated_at = ? WHERE id = ?
            "#,
            now,
            id
        )
        .execute(ctx.db_pool())
        .await?;
        Ok(())
    }

    async fn install_to_agent(
        &self,
        ctx: RequestContext,
        source_skill: &SkillPo,
        target_agent_id: &str,
    ) -> Result<SkillPo, AppError> {
        // Source skill must be Published (shared) to be installed
        if source_skill.status != SkillStatus::Published {
            return Err(AppError::BadRequest(format!(
                "Only published skills can be installed, current status is {:?}",
                source_skill.status
            )));
        }

        // Generate new unique skill id internally (using v7 uuid for time ordering)
        let new_skill_id = uuid::Uuid::now_v7().to_string();

        // Calculate relative content path for agent-owned draft skill
        // Format: agents/{agent_id}/skills/{skill_id}
        let content_path = format!("agents/{}/skills/{}", target_agent_id, new_skill_id);

        // Create new skill record: copy metadata from source, set new id, agent as author, draft status
        let new_skill = SkillPo::new(
            new_skill_id,
            source_skill.name.clone(),
            source_skill.description.clone(),
            source_skill.parse_tags(),
            source_skill.category.clone(),
            source_skill.id.clone(), // parent_skill_id points to original
            target_agent_id.to_string(), // author is the agent
            SkillAuthorType::Agent, // author type is Agent
            content_path, // content path calculated internally
        );
        // new_skill is already Draft by default

        // Insert the new skill into database
        self.insert(ctx.clone(), &new_skill).await?;

        Ok(new_skill)
    }
}
