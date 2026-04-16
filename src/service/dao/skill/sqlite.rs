//! SQLite implementation of Skill DAO

use async_trait::async_trait;
use crate::error::AppError;
use crate::models::skill::SkillPo;
use crate::pkg::RequestContext;
use common::enums::SkillStatus;
use crate::service::dao::skill::SkillDaoTrait;
use std::sync::{Arc, OnceLock};

static SKILL_DAO: OnceLock<Arc<dyn SkillDaoTrait>> = OnceLock::new();

/// Get Skill DAO singleton
pub fn dao() -> Arc<dyn SkillDaoTrait> {
    SKILL_DAO.get().cloned().unwrap()
}

/// Initialize singleton
pub fn init() {
    let dao = SqliteSkillDao;
    let _ = SKILL_DAO.set(Arc::new(dao));
}

#[derive(Debug, Clone)]
pub struct SqliteSkillDao;

#[async_trait]
impl SkillDaoTrait for SqliteSkillDao {
    async fn insert(&self, ctx: RequestContext, skill: &SkillPo) -> Result<(), AppError> {
        let status_i32 = skill.status.to_i32();
        sqlx::query!(
            r#"
INSERT INTO skills (
    id, name, description, tags, category, parent_skill_id,
    author_id, modifier_id, status, created_at, updated_at, content_path
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            skill.id,
            skill.name,
            skill.description,
            skill.tags,
            skill.category,
            skill.parent_skill_id,
            skill.author_id,
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
        sqlx::query!(
            r#"
UPDATE skills SET
    name = ?, description = ?, tags = ?, category = ?, parent_skill_id = ?,
    author_id = ?, modifier_id = ?, status = ?, updated_at = ?, content_path = ?
WHERE id = ?
            "#,
            skill.name,
            skill.description,
            skill.tags,
            skill.category,
            skill.parent_skill_id,
            skill.author_id,
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
       author_id, modifier_id, status AS "status: SkillStatus",
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
       author_id, modifier_id, status AS "status: SkillStatus",
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
       author_id, modifier_id, status AS "status: SkillStatus",
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
       author_id, modifier_id, status AS "status: SkillStatus",
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
       author_id, modifier_id, status AS "status: SkillStatus",
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
}
