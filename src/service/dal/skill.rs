//! Skill DAL 模块
//!
//! 技能数据访问层，提供技能查询和管理能力
//! 负责组合 DAO 完成业务级数据操作，组装完整 Skill 实体（PO + 文件）

use crate::error::AppError;
use crate::models::skill::{Skill, SkillPo, SkillFile};
use crate::models::model_provider::ModelProviderPo;
use crate::models::vector::{VectorIndexParams, SearchMatchInfo, MatchType};
use crate::pkg::request_context::RequestContext;
use crate::service::dao::skill::{SkillDao, SkillVectorDao, SkillQuery, self};
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::model_provider::ModelProviderDao;
use common::enums::{ModelCapability, ModelProviderStatus};
use std::sync::{Arc, OnceLock};

// ==================== 单例管理 ====================

static SKILL_DAL: OnceLock<Arc<dyn SkillDal>> = OnceLock::new();

/// 获取 Skill DAL 单例
pub fn dal() -> Arc<dyn SkillDal> {
    SKILL_DAL.get().cloned().unwrap()
}

/// 初始化 Skill DAL（使用全局单例 DAO）
pub fn init() {
    let _ = SKILL_DAL.set(new(
        skill::dao(),
        skill::vector_dao(),
        crate::service::dao::cortex::dao(),
        crate::service::dao::model_provider::dao(),
    ));
}

/// 创建 Skill DAL（返回 trait 对象）
pub fn new(
    skill_dao: Arc<dyn SkillDao + Send + Sync>,
    skill_vector_dao: Arc<dyn SkillVectorDao + Send + Sync>,
    cortex_dao: Arc<dyn CortexDao + Send + Sync>,
    model_provider_dao: Arc<dyn ModelProviderDao + Send + Sync>,
) -> Arc<dyn SkillDal> {
    Arc::new(SkillDalImpl { skill_dao, skill_vector_dao, cortex_dao, model_provider_dao })
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
    /// 更新技能（仅数据库）
    async fn update(&self, ctx: RequestContext, skill: &Skill) -> Result<(), AppError>;

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

    /// 查询技能的向量索引内容哈希（判断是否需要重索引）
    async fn get_vector_content_hash(
        &self,
        ctx: RequestContext,
        skill_id: &str,
    ) -> Result<Option<String>, AppError>;
}

// ==================== DAL 实现 ====================

/// Skill DAL 基础实现
pub struct SkillDalImpl {
    skill_dao: Arc<dyn SkillDao + Send + Sync>,
    skill_vector_dao: Arc<dyn SkillVectorDao + Send + Sync>,
    cortex_dao: Arc<dyn CortexDao + Send + Sync>,
    model_provider_dao: Arc<dyn ModelProviderDao + Send + Sync>,
}

#[async_trait::async_trait]
impl SkillDal for SkillDalImpl {
    async fn create(&self, ctx: RequestContext, po: &SkillPo) -> Result<(), AppError> {
        // 1. 先保存基础技能数据
        self.skill_dao.insert(ctx.clone(), po).await?;

        // 2. 查询可用的 Embedding 能力的 ModelProvider
        if let Some(provider) = self.model_provider_dao.get_default_embedding_provider(ctx.clone()).await? {
            // 创建 Cortex（这是同步方法，不需要 await）
            let cortex = self.cortex_dao.create_cortex_trait(
                ctx.clone(),
                &provider,
                vec![],
            )?;

            // 生成向量（调用 CortexTrait 的方法）
            let content = format!("{} {}", po.name, po.description);
            let content_hash = sha256::digest(&content);
            let vectors = cortex.embeddings(&[content]).await?;
            
            // 构建向量索引参数
            let vector_params = VectorIndexParams {
                vector: vectors.into_iter().next().unwrap_or_default(),
                content_hash,
                model_provider_id: provider.id.clone(),
                embedding_model: provider.model_name.clone(),
                expire_at: None,
            };

            // 保存向量索引
            // 注意：测试环境可能没有 vss0 扩展，此时向量索引会失败
            // 降级策略：忽略错误，不影响核心功能
            if let Err(e) = self.skill_vector_dao.upsert_vector(
                ctx,
                &po.id,
                &vector_params,
            ).await {
                tracing::warn!("保存技能向量索引失败（可能 vss0 扩展未安装）: {}", e);
            }
        }

        Ok(())
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
        use std::collections::HashSet;

        // Step 1: 查询是否有可用的 Embedding Provider
        let providers = self.model_provider_dao.query(
            ctx.clone(),
            crate::service::dao::model_provider::ModelProviderQuery {
                capability: Some(ModelCapability::Embedding),
                status: Some(ModelProviderStatus::Normal),
                limit: Some(1),
                ..Default::default()
            },
        ).await?;

        let mut vector_scores: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
        let mut vector_skill_ids: HashSet<String> = HashSet::new();

        // Step 2: 如果有 Embedding Provider，执行向量搜索
        if let Some(provider) = providers.first() {
            // 创建 Cortex
            let cortex = self.cortex_dao.create_cortex_trait(
                ctx.clone(),
                provider,
                vec![],
            )?;
            
            // 生成查询向量
            let vectors = cortex.embeddings(&[keyword.to_string()]).await?;
            let query_vector = vectors.into_iter().next().unwrap_or_default();
            
            // 向量搜索（前 50 条）
            // 注意：只保留距离小于阈值的结果（余弦距离 0-2，0 是完全相同）
            match self.skill_vector_dao.search_vector(
                ctx.clone(),
                &query_vector,
                50,
            ).await {
                Ok(vector_results) => {
                    // 距离阈值：只保留足够相似的结果（0.8 是比较宽松的阈值）
                    const VECTOR_DISTANCE_THRESHOLD: f32 = 0.8;
                    let filtered_results: Vec<(String, f32)> = vector_results
                        .into_iter()
                        .filter(|hit| hit.distance < VECTOR_DISTANCE_THRESHOLD)
                        .map(|hit| (hit.row.id, hit.distance))
                        .collect();
                    
                    vector_skill_ids = filtered_results.iter().map(|(id, _)| id.clone()).collect();
                    vector_scores = filtered_results.into_iter().collect();
                }
                Err(e) => {
                    // 向量搜索失败（可能是 vss0 扩展未安装），降级到纯关键词搜索
                    tracing::warn!("向量搜索失败，降级到关键词搜索: {}", e);
                }
            }
        }

        // Step 3: 执行关键词搜索（补充向量没覆盖的结果）
        let keyword_query = SkillQuery {
            keyword: Some(keyword.to_string()),
            ..Default::default()
        };
        let keyword_pos = self.skill_dao.query(ctx.clone(), keyword_query).await?;

        // Step 4: 如果有向量搜索，获取向量匹配的完整 PO
        let mut all_pos = keyword_pos.clone();
        if !vector_skill_ids.is_empty() {
            // 关键词搜索可能没覆盖到向量匹配的结果，需要额外获取
            let mut ids_to_fetch: Vec<String> = vector_skill_ids
                .into_iter()
                .filter(|id| !keyword_pos.iter().any(|po| po.id == *id))
                .collect();
            
            if !ids_to_fetch.is_empty() {
                // 分批获取（避免 SQL 太长）
                ids_to_fetch.sort();
                ids_to_fetch.dedup();
                
                for chunk in ids_to_fetch.chunks(20) {
                    let chunk_ids: Vec<String> = chunk.to_vec();
                    let chunk_query = SkillQuery {
                        ids: Some(chunk_ids),
                        ..Default::default()
                    };
                    let chunk_pos = self.skill_dao.query(ctx.clone(), chunk_query).await?;
                    all_pos.extend(chunk_pos);
                }
            }
        }

        // Step 5: 去重
        all_pos.sort_by(|a, b| a.id.cmp(&b.id));
        all_pos.dedup_by(|a, b| a.id == b.id);

        // Step 6: 构建 Skill 对象并排序
        let mut skills = Vec::with_capacity(all_pos.len());
        for po in all_pos {
            let files = self.skill_dao.list_files(&po)?;
            let match_info = if let Some(distance) = vector_scores.get(&po.id) {
                SearchMatchInfo {
                    match_type: MatchType::Hybrid,
                    vector_distance: Some(*distance),
                    ..Default::default()
                }
            } else {
                SearchMatchInfo {
                    match_type: MatchType::Keyword,
                    ..Default::default()
                }
            };
            skills.push(Skill {
                po,
                files,
                search_match: Some(match_info),
            });
        }

        // Step 7: 排序（向量距离优先，距离越小越好；纯关键词按原有顺序）
        if !vector_scores.is_empty() {
            skills.sort_by(|a, b| {
                let dist_a = a.search_match.as_ref().and_then(|m| m.vector_distance).unwrap_or(f32::MAX);
                let dist_b = b.search_match.as_ref().and_then(|m| m.vector_distance).unwrap_or(f32::MAX);
                dist_a.partial_cmp(&dist_b).unwrap_or(std::cmp::Ordering::Equal)
            });
        }

        Ok(skills)
    }

    async fn update(&self, ctx: RequestContext, skill: &Skill) -> Result<(), AppError> {
        // 1. 先更新基础技能数据
        self.skill_dao.update(ctx.clone(), &skill.po).await?;

        // 2. 查询可用的 Embedding 能力的 ModelProvider
        let providers = self.model_provider_dao.query(
            ctx.clone(),
            crate::service::dao::model_provider::ModelProviderQuery {
                capability: Some(ModelCapability::Embedding),
                status: Some(ModelProviderStatus::Normal),
                limit: Some(1),
                ..Default::default()
            },
        ).await?;

        // 3. 如果有可用的 Embedding Provider，更新向量
        if let Some(provider) = providers.first() {
            let content = format!("{} {}", skill.po.name, skill.po.description);
            let new_hash = sha256::digest(&content);
            
            // 检查内容是否变化（如果没变就不需要重索引）
            let old_row = self.skill_vector_dao.get_vector_row(ctx.clone(), &skill.po.id).await?;
            let old_hash = old_row.map(|r| r.meta.content_hash);
            
            if old_hash.as_deref() != Some(&new_hash) {
                // 创建 Cortex
                let cortex = self.cortex_dao.create_cortex_trait(
                    ctx.clone(),
                    provider,
                    vec![],
                )?;

                // 生成向量
                let vectors = cortex.embeddings(&[content]).await?;
                
                // 构建向量索引参数
                let vector_params = VectorIndexParams {
                    vector: vectors.into_iter().next().unwrap_or_default(),
                    content_hash: new_hash,
                    model_provider_id: provider.id.clone(),
                    embedding_model: provider.model_name.clone(),
                    expire_at: None,
                };

                // 更新向量索引
                // 注意：测试环境可能没有 vss0 扩展，此时向量索引会失败
                // 降级策略：忽略错误，不影响核心功能
                if let Err(e) = self.skill_vector_dao.upsert_vector(
                    ctx,
                    &skill.po.id,
                    &vector_params,
                ).await {
                    tracing::warn!("更新技能向量索引失败（可能 vss0 扩展未安装）: {}", e);
                }
            }
        }

        Ok(())
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

    async fn get_vector_content_hash(
        &self,
        ctx: RequestContext,
        skill_id: &str,
    ) -> Result<Option<String>, AppError> {
        let row = self.skill_vector_dao.get_vector_row(ctx, skill_id).await?;
        Ok(row.map(|r| r.meta.content_hash))
    }
}
