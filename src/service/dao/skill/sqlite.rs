//! SQLite implementation of Skill DAO

use std::path::{Path, PathBuf};
use std::sync::Arc;

use common::error::{Error, Result};
use crate::models::skill::{SkillFile, SkillPo};
use crate::pkg::RequestContext;
use crate::service::dao::skill::{SkillDao, SkillQuery, SkillSearch};
use async_trait::async_trait;
use common::enums::SkillStatus;
use common::enums::skill::SkillAuthorType;
use common::{err, bail_err};

// ==================== 单例模式 ====================

static SKILL_DAO_INSTANCE: std::sync::OnceLock<Arc<dyn SkillDao + Send + Sync>> =
    std::sync::OnceLock::new();

/// 获取全局单例 Skill DAO
pub fn dao() -> Arc<dyn SkillDao + Send + Sync> {
    SKILL_DAO_INSTANCE
        .get()
        .expect("Skill DAO not initialized. Call dao::skill::init() first.")
        .clone()
}

/// 初始化 Skill DAO 单例
pub fn init() {
    let _ = SKILL_DAO_INSTANCE.set(new());
}

/// 创建新的 Skill DAO 实例
pub fn new() -> Arc<dyn SkillDao + Send + Sync> {
    Arc::new(SkillDaoSqliteImpl { base_path: None })
}

/// 创建使用指定 base_path 的 Skill DAO 实例（测试专用）。
pub fn new_with_base_path(base_path: PathBuf) -> Arc<dyn SkillDao + Send + Sync> {
    Arc::new(SkillDaoSqliteImpl {
        base_path: Some(base_path),
    })
}

// ==================== 实现 ====================

#[derive(Debug, Clone)]
struct SkillDaoSqliteImpl {
    base_path: Option<PathBuf>,
}

#[async_trait]
impl SkillDao for SkillDaoSqliteImpl {
    async fn insert(&self, ctx: RequestContext, skill: &SkillPo) -> Result<()> {
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

    async fn update(&self, ctx: RequestContext, skill: &SkillPo) -> Result<()> {
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

    async fn delete_by_id(&self, ctx: RequestContext, id: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp_millis();
        let deleted_status = SkillStatus::Expired.to_i32();
        sqlx::query!(
            "UPDATE skills SET status = ?, updated_at = ? WHERE id = ?",
            deleted_status,
            now,
            id
        )
        .execute(ctx.db_pool())
        .await?;
        Ok(())
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<SkillPo>> {
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

    async fn query(
        &self,
        ctx: RequestContext,
        query: SkillQuery,
    ) -> Result<Vec<SkillPo>> {
        let mut builder = sqlx::QueryBuilder::new(
            r#"SELECT id, name, description, tags, category, parent_skill_id, author_id, author_type, modifier_id, status, created_at, updated_at, content_path FROM skills WHERE 1=1"#,
        );

        // ✅ 按 ID 批量查询（向量搜索的核心过滤）
        if let Some(ids) = &query.ids {
            builder.push(" AND id IN (");
            let mut separated = builder.separated(", ");
            for id in ids {
                separated.push_bind(id);
            }
            separated.push_unseparated(")");
        }

        // 状态过滤
        if let Some(status) = &query.status {
            builder.push(" AND status = ").push_bind(*status as i32);
        }

        // 排除状态过滤
        if let Some(exclude_status) = &query.exclude_status {
            builder
                .push(" AND status != ")
                .push_bind(*exclude_status as i32);
        }

        // 分类过滤
        if let Some(category) = &query.category {
            builder.push(" AND category = ").push_bind(category);
        }

        // 作者过滤
        if let Some(author_id) = &query.author_id {
            builder.push(" AND author_id = ").push_bind(author_id);
        }

        // 父技能 ID 过滤（用于幂等检查已安装副本）
        if let Some(parent_skill_id) = &query.parent_skill_id {
            builder
                .push(" AND parent_skill_id = ")
                .push_bind(parent_skill_id);
        }

        // tag 过滤（OR 语义：包含任一 tag 即可命中）
        if let Some(tags) = &query.tags {
            if !tags.is_empty() {
                builder.push(" AND EXISTS (SELECT 1 FROM json_each(tags) WHERE json_each.value IN (");
                let mut separated = builder.separated(", ");
                for tag in tags {
                    separated.push_bind(tag);
                }
                separated.push_unseparated("))");
            }
        }

        // 关键词搜索 (name / description / tags)
        if let Some(keyword) = &query.keyword {
            let like_pattern = format!("%{}%", keyword);
            builder
                .push(" AND (name LIKE ")
                .push_bind(like_pattern.clone());
            builder
                .push(" OR description LIKE ")
                .push_bind(like_pattern.clone())
                .push(" OR tags LIKE ")
                .push_bind(like_pattern)
                .push(")");
        }

        // 排序
        builder.push(" ORDER BY updated_at DESC");

        // 限制数量
        if let Some(limit) = query.limit {
            builder.push(" LIMIT ").push_bind(limit as i64);
        }

        // 执行查询
        let rows = builder
            .build_query_as::<SkillPo>()
            .fetch_all(ctx.db_pool())
            .await?;

        Ok(rows)
    }

    async fn list_by_status(
        &self,
        ctx: RequestContext,
        status: SkillStatus,
    ) -> Result<Vec<SkillPo>> {
        let status_i32 = status.to_i32();
        let skills = sqlx::query_as!(
            SkillPo,
            r#"
SELECT id, name, description, tags, category, parent_skill_id,
       author_id, author_type AS "author_type: SkillAuthorType", modifier_id, status AS "status: SkillStatus",
       created_at, updated_at, content_path
FROM skills WHERE status = ?
ORDER BY updated_at DESC
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
    ) -> Result<Vec<SkillPo>> {
        let skills = sqlx::query_as!(
            SkillPo,
            r#"
SELECT id, name, description, tags, category, parent_skill_id,
       author_id, author_type AS "author_type: SkillAuthorType", modifier_id, status AS "status: SkillStatus",
       created_at, updated_at, content_path
FROM skills WHERE category = ?
ORDER BY updated_at DESC
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
    ) -> Result<Vec<SkillPo>> {
        let skills = sqlx::query_as!(
            SkillPo,
            r#"
SELECT id, name, description, tags, category, parent_skill_id,
       author_id, author_type AS "author_type: SkillAuthorType", modifier_id, status AS "status: SkillStatus",
       created_at, updated_at, content_path
FROM skills WHERE author_id = ?
ORDER BY updated_at DESC
            "#,
            author_id
        )
        .fetch_all(ctx.db_pool())
        .await?;
        Ok(skills)
    }

    async fn install_to_agent(
        &self,
        ctx: RequestContext,
        source_skill: &SkillPo,
        target_agent_id: &str,
    ) -> Result<SkillPo> {
        // Source skill must be Published (shared) to be installed
        if source_skill.status != SkillStatus::Published {
            bail_err!(InvalidRequest, "Only published skills can be installed, current status is {:?}", source_skill.status);
        }

        // Generate new unique skill id internally (using v7 uuid for time ordering)
        let new_skill_id = uuid::Uuid::now_v7().to_string();

        // Calculate relative content path for agent-owned draft skill
        // Format: agents/{agent_id}/skills/{skill_id}
        let content_path = format!("agents/{}/skills/{}", target_agent_id, new_skill_id);

        // Create new skill record: copy metadata from source, set new id, agent as author, draft status
        let new_skill = SkillPo::new(
            new_skill_id.clone(),
            source_skill.name.clone(),
            source_skill.description.clone(),
            source_skill.parse_tags(),
            source_skill.category.clone(),
            source_skill.id.clone(), // parent_skill_id points to original (String not Option)
            target_agent_id.to_string(), // author is the agent
            SkillAuthorType::Agent,  // author type is Agent
            content_path,            // content path calculated internally
        );
        // new_skill is already Draft by default

        // 原子操作：先拷贝文件，再插入数据库（确保数据库记录存在时文件一定已就绪）
        self.copy_skill_dir(source_skill, &new_skill)?;

        // Insert the new skill into database
        self.insert(ctx.clone(), &new_skill).await?;

        Ok(new_skill)
    }

    async fn search(
        &self,
        ctx: RequestContext,
        search: SkillSearch,
    ) -> Result<Vec<SkillPo>> {
        // ✅ 只做参数转换，直接转发到 query（DAO 层不碰向量搜索）
        let mut query = search.filters;

        // 如果 search 有关键词，补充到 query
        if search.keyword.is_some() {
            query.keyword = search.keyword;
        }

        // 向量搜索相关的 query_vector/top_k：DAO 层直接忽略
        // （向量搜索由 DAL 层调 SkillVectorDao 单独处理）

        // 转发到已有的 query 方法
        self.query(ctx, query).await
    }

    // ========== 文件操作 ==========

    fn read_main_content(&self, skill: &SkillPo) -> Result<String> {
        let path = self.main_content_path(skill);
        if !path.exists() {
            return Ok(String::new());
        }
        let content = std::fs::read_to_string(path)?;
        Ok(content)
    }

    fn write_main_content(&self, skill: &SkillPo, content: &str) -> Result<()> {
        let dir = self.skill_dir(skill);
        std::fs::create_dir_all(&dir)?;
        let path = self.main_content_path(skill);
        std::fs::write(path, content)?;
        Ok(())
    }

    fn list_files(&self, skill: &SkillPo) -> Result<Vec<SkillFile>> {
        const SMALL_FILE_THRESHOLD: u64 = 64 * 1024; // 64KB 以下的文件直接预读内容
        let dir = self.skill_dir(skill);
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut files = Vec::new();
        self.collect_files(&dir, &dir, &mut files, SMALL_FILE_THRESHOLD)?;

        // 按文件名排序，让 skill.md 排在前面
        files.sort_by(|a, b| a.filename.cmp(&b.filename));
        Ok(files)
    }

    fn read_file(&self, skill: &SkillPo, filename: &str) -> Result<String> {
        let path = self.file_path(skill, filename);
        if !path.exists() {
            bail_err!(ResourceNotFound, "File not found: {}", filename);
        }
        let content = std::fs::read_to_string(path)?;
        Ok(content)
    }

    fn write_file(&self, skill: &SkillPo, filename: &str, content: &str) -> Result<()> {
        self.write_file_bytes(skill, filename, content.as_bytes())
    }

    fn write_file_bytes(
        &self,
        skill: &SkillPo,
        filename: &str,
        bytes: &[u8],
    ) -> Result<()> {
        let path = self.file_path(skill, filename);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, bytes)?;
        Ok(())
    }

    fn delete_skill_dir(&self, skill: &SkillPo) -> Result<()> {
        let dir = self.skill_dir(skill);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        Ok(())
    }
}

// ===== SkillDaoSqliteImpl 辅助方法 =====

impl SkillDaoSqliteImpl {
    /// 获取技能的完整目录路径
    fn skill_dir(&self, skill: &SkillPo) -> PathBuf {
        self.base_path
            .clone()
            .unwrap_or_else(|| crate::config::get().base_data_path())
            .join(&skill.content_path)
    }

    /// 获取技能主文件（skill.md）的完整路径
    fn main_content_path(&self, skill: &SkillPo) -> PathBuf {
        self.skill_dir(skill).join("skill.md")
    }

    /// 获取技能中指定文件的完整路径
    fn file_path(&self, skill: &SkillPo, filename: &str) -> PathBuf {
        self.skill_dir(skill).join(filename)
    }

    fn collect_files(
        &self,
        base_dir: &Path,
        current_dir: &Path,
        files: &mut Vec<SkillFile>,
        small_file_threshold: u64,
    ) -> Result<()> {
        for entry in std::fs::read_dir(current_dir)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            let path = entry.path();
            if file_type.is_dir() {
                self.collect_files(base_dir, &path, files, small_file_threshold)?;
                continue;
            }

            if !file_type.is_file() {
                continue;
            }

            let filename = path
                .strip_prefix(base_dir)
                .map_err(|e| err!(Internal, "计算技能文件相对路径失败: {e}").with_source(e))?
                .to_string_lossy()
                .replace('\\', "/");
            let metadata = entry.metadata()?;
            let file_size = metadata.len();

            // 小文件直接读取内容；非 UTF-8 小文件保留为 None，避免列表接口因二进制附件失败。
            let content = if file_size <= small_file_threshold {
                std::fs::read_to_string(&path).ok()
            } else {
                None
            };

            files.push(SkillFile {
                filename,
                file_size,
                content,
            });
        }

        Ok(())
    }

    /// 递归拷贝技能目录（用于 install_to_agent）
    fn copy_skill_dir(&self, src: &SkillPo, target: &SkillPo) -> Result<()> {
        let source_dir = self.skill_dir(src);
        let target_dir = self.skill_dir(target);

        // 如果源目录不存在，创建空的目标目录即可（新建技能可能还没有文件）
        if !source_dir.exists() {
            std::fs::create_dir_all(&target_dir)?;
            return Ok(());
        }

        // 创建目标目录
        std::fs::create_dir_all(&target_dir)?;

        // 遍历源目录并拷贝所有文件
        for entry in std::fs::read_dir(&source_dir)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_file() {
                let target_file = target_dir.join(entry.file_name());
                std::fs::copy(entry.path(), target_file)?;
            }
            // 暂时不递归子目录，需要时再扩展
        }

        Ok(())
    }
}