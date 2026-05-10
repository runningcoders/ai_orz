//! SQLite implementation of Skill DAO

use async_trait::async_trait;
use crate::error::AppError;
use crate::models::skill::{SkillPo, SkillFile};
use crate::models::vector::{VectorIndexParams, SearchResult, SearchMatchInfo, MatchType};
use crate::pkg::RequestContext;
use common::enums::skill::SkillAuthorType;
use common::enums::SkillStatus;
use crate::service::dao::skill::{SkillDao, SkillQuery, SkillSearch};
use std::sync::{Arc, OnceLock};
use std::path::PathBuf;
use std::collections::HashMap;

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

    async fn insert_with_vector(
        &self,
        ctx: RequestContext,
        skill: &SkillPo,
        vector_params: &VectorIndexParams,
    ) -> Result<(), AppError> {
        // 1. 先插入业务数据
        self.insert(ctx.clone(), skill).await?;

        // 2. 再插入向量索引
        let vector_store = ctx.vector_store();
        vector_store.upsert(
            "skills",
            &skill.id,
            &vector_params.vector,
            &vector_params.content_hash,
            &vector_params.embedding_model,
            vector_params.expire_at,
        ).await?;

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

    async fn update_with_vector(
        &self,
        ctx: RequestContext,
        skill: &SkillPo,
        vector_params: &VectorIndexParams,
    ) -> Result<(), AppError> {
        // 1. 先更新业务数据
        self.update(ctx.clone(), skill).await?;

        // 2. 再更新向量索引
        let vector_store = ctx.vector_store();
        vector_store.upsert(
            "skills",
            &skill.id,
            &vector_params.vector,
            &vector_params.content_hash,
            &vector_params.embedding_model,
            vector_params.expire_at,
        ).await?;

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

    async fn query(&self, ctx: RequestContext, query: SkillQuery) -> Result<Vec<SkillPo>, AppError> {
        let mut builder = sqlx::QueryBuilder::new(
            r#"SELECT id, name, description, tags, category, parent_skill_id, author_id, author_type, modifier_id, status, created_at, updated_at, content_path FROM skills WHERE 1=1"#
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
            builder.push(" AND status != ").push_bind(*exclude_status as i32);
        }

        // 分类过滤
        if let Some(category) = &query.category {
            builder.push(" AND category = ").push_bind(category);
        }

        // 作者过滤
        if let Some(author_id) = &query.author_id {
            builder.push(" AND author_id = ").push_bind(author_id);
        }

        // 关键词搜索 (name 或 description)
        if let Some(keyword) = &query.keyword {
            let like_pattern = format!("%{}%", keyword);
            builder.push(" AND (name LIKE ").push_bind(like_pattern.clone());
            builder.push(" OR description LIKE ").push_bind(like_pattern).push(")");
        }

        // 排序
        builder.push(" ORDER BY updated_at DESC");

        // 限制数量
        if let Some(limit) = query.limit {
            builder.push(" LIMIT ").push_bind(limit as i64);
        }

        // 执行查询
        let rows = builder.build_query_as()
            .fetch_all(ctx.db_pool())
            .await?;

        Ok(rows)
    }

    /// ✅ 统一搜索入口（支持关键词/向量/混合三种策略）
    async fn search(
        &self,
        ctx: RequestContext,
        search: SkillSearch,
    ) -> Result<Vec<SearchResult<SkillPo>>, AppError> {
        // ========== 策略路由：根据入参选择搜索模式 ==========
        match (search.keyword.as_ref(), search.query_vector.as_ref()) {
            // ---------- 模式1：仅关键词搜索 ----------
            (Some(keyword), None) => {
                // 复用通用查询，走 SQL LIKE 匹配
                let skills = self.query(ctx, SkillQuery {
                    keyword: Some(keyword.to_string()),
                    exclude_status: Some(SkillStatus::Expired),
                    ..search.filters
                }).await?;

                // 包装为统一结果格式
                Ok(skills.into_iter()
                    .map(|skill| SearchResult {
                        entity: skill,
                        match_info: SearchMatchInfo {
                            match_type: MatchType::Keyword,
                            vector_distance: None,
                            keyword_fields: Some(vec!["name".to_string(), "description".to_string()]),
                            embedding_model: None,
                            indexed_at: None,
                            content_hash: None,
                        },
                    })
                    .collect())
            }

            // ---------- 模式2：仅向量语义搜索 ----------
            (None, Some(query_vector)) => {
                // ========== 第一阶段：向量检索获取候选 ID 列表 ==========
                let vector_store = ctx.vector_store();
                let search_results = vector_store.search(
                    "skills",
                    query_vector,
                    search.top_k.unwrap_or(20),
                ).await?;

                if search_results.is_empty() {
                    return Ok(Vec::new());
                }

                // 提取 ID 列表和 distance 映射
                let skill_ids: Vec<String> = search_results.iter().map(|(id, _)| id.clone()).collect();
                let distance_map: HashMap<String, f32> = search_results.into_iter().collect();

                // ========== 第二阶段：按业务条件过滤 ==========
                let skills = self.query(ctx, SkillQuery {
                    ids: Some(skill_ids),
                    exclude_status: Some(SkillStatus::Expired),
                    ..search.filters
                }).await?;

                // ========== 第三阶段：组合结果，按相似度排序 ==========
                let mut results: Vec<_> = skills.into_iter()
                    .map(|skill| SearchResult {
                        entity: skill.clone(),
                        match_info: SearchMatchInfo {
                            match_type: MatchType::Vector,
                            vector_distance: Some(distance_map.get(&skill.id).copied().unwrap_or(1.0)),
                            keyword_fields: None,
                            embedding_model: Some(String::new()), // TODO: 从元数据表填充
                            indexed_at: Some(0), // TODO: 从元数据表填充
                            content_hash: Some(String::new()), // TODO: 从元数据表填充
                        },
                    })
                    .collect();

                results.sort_by(|a, b| a.match_info.vector_distance.unwrap_or(1.0)
                    .partial_cmp(&b.match_info.vector_distance.unwrap_or(1.0)).unwrap());
                Ok(results)
            }

            // ---------- 模式3：混合搜索（关键词 + 向量取交集） ----------
            (Some(keyword), Some(query_vector)) => {
                // TODO: 实现混合排序策略（RRF / Borda Count / 加权和）
                // 目前降级为仅向量搜索（后续可扩展）
                let mut fallback_search = search.clone();
                fallback_search.keyword = None;
                self.search(ctx, fallback_search).await
            }

            // ---------- 模式4：无有效入参，返回空 ----------
            (None, None) => {
                Ok(Vec::new())
            }
        }
    }

    /// ✅ 查询技能的向量索引内容哈希（DAL 判断是否需要重索引）
    async fn get_vector_content_hash(
        &self,
        ctx: RequestContext,
        skill_id: &str,
    ) -> Result<Option<String>, AppError> {
        ctx.vector_store()
            .get_content_hash("skills", skill_id)
            .await
            .map_err(|e| AppError::Internal(format!("Vector store error: {}", e)))
    }

    async fn list_by_status(
        &self,
        ctx: RequestContext,
        status: SkillStatus,
    ) -> Result<Vec<SkillPo>, AppError> {
        // 语法糖：调用通用查询
        self.query(ctx, SkillQuery {
            status: Some(status),
            ..Default::default()
        }).await
    }

    async fn list_by_category(
        &self,
        ctx: RequestContext,
        category: &str,
    ) -> Result<Vec<SkillPo>, AppError> {
        // 语法糖：调用通用查询
        self.query(ctx, SkillQuery {
            category: Some(category.to_string()),
            ..Default::default()
        }).await
    }

    async fn list_by_author(
        &self,
        ctx: RequestContext,
        author_id: &str,
    ) -> Result<Vec<SkillPo>, AppError> {
        // 语法糖：调用通用查询
        self.query(ctx, SkillQuery {
            author_id: Some(author_id.to_string()),
            ..Default::default()
        }).await
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

        // 原子操作：先拷贝文件，再插入数据库（确保数据库记录存在时文件一定已就绪）
        self.copy_skill_dir(source_skill, &new_skill)?;

        // Insert the new skill into database
        self.insert(ctx.clone(), &new_skill).await?;

        Ok(new_skill)
    }

    // ===== 文件操作方法 =====

    fn read_main_content(&self, skill: &SkillPo) -> Result<String, AppError> {
        let path = self.main_content_path(skill);
        if !path.exists() {
            return Ok(String::new());
        }
        let content = std::fs::read_to_string(path)?;
        Ok(content)
    }

    fn write_main_content(&self, skill: &SkillPo, content: &str) -> Result<(), AppError> {
        let dir = self.skill_dir(skill);
        std::fs::create_dir_all(&dir)?;
        let path = self.main_content_path(skill);
        std::fs::write(path, content)?;
        Ok(())
    }

    fn list_files(&self, skill: &SkillPo) -> Result<Vec<SkillFile>, AppError> {
        const SMALL_FILE_THRESHOLD: u64 = 64 * 1024; // 64KB 以下的文件直接预读内容
        let dir = self.skill_dir(skill);
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut files = Vec::new();
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_file() {
                let filename = entry.file_name().to_string_lossy().to_string();
                let metadata = entry.metadata()?;
                let file_size = metadata.len();

                // 小文件直接读取内容，大文件留空让上层按需读取
                let content = if file_size <= SMALL_FILE_THRESHOLD {
                    Some(std::fs::read_to_string(entry.path())?)
                } else {
                    None
                };

                files.push(SkillFile { filename, file_size, content });
            }
        }

        // 按文件名排序，让 skill.md 排在前面
        files.sort_by(|a, b| a.filename.cmp(&b.filename));
        Ok(files)
    }

    fn read_file(&self, skill: &SkillPo, filename: &str) -> Result<String, AppError> {
        let path = self.file_path(skill, filename);
        if !path.exists() {
            return Err(AppError::NotFound(format!("File not found: {}", filename)));
        }
        let content = std::fs::read_to_string(path)?;
        Ok(content)
    }

    fn write_file(&self, skill: &SkillPo, filename: &str, content: &str) -> Result<(), AppError> {
        let dir = self.skill_dir(skill);
        std::fs::create_dir_all(&dir)?;
        let path = self.file_path(skill, filename);
        std::fs::write(path, content)?;
        Ok(())
    }

    fn delete_skill_dir(&self, skill: &SkillPo) -> Result<(), AppError> {
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
        crate::config::get().base_data_path().join(&skill.content_path)
    }

    /// 获取技能主文件（skill.md）的完整路径
    fn main_content_path(&self, skill: &SkillPo) -> PathBuf {
        self.skill_dir(skill).join("skill.md")
    }

    /// 获取技能中指定文件的完整路径
    fn file_path(&self, skill: &SkillPo, filename: &str) -> PathBuf {
        self.skill_dir(skill).join(filename)
    }

    /// 递归拷贝技能目录（用于 install_to_agent）
    fn copy_skill_dir(&self, source: &SkillPo, target: &SkillPo) -> Result<(), AppError> {
        let source_dir = self.skill_dir(source);
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