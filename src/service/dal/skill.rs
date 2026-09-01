//! Skill DAL 模块
//!
//! 技能数据访问层，提供技能查询和管理能力
//! 负责组合 DAO 完成业务级数据操作，组装完整 Skill 实体（PO + 文件）

use crate::models::skill::{Skill, SkillFile, SkillPo};
use crate::models::vector::{MatchType, SearchMatchInfo, Vectorizable};
use crate::pkg::request_context::RequestContext;
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::model_provider::ModelProviderDao;
use crate::service::dao::skill::{self, SkillDao, SkillQuery, SkillSearch, SkillVectorDao};
use common::enums::SkillStatus;
use common::error::{Result, err};
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
    Arc::new(SkillDalImpl {
        skill_dao,
        skill_vector_dao,
        cortex_dao,
        model_provider_dao,
    })
}

// ==================== DAL 接口 ====================

/// Skill DAL 接口
#[async_trait::async_trait]
pub trait SkillDal: Send + Sync {
    /// 创建新技能（仅数据库）
    async fn create(&self, ctx: RequestContext, po: &SkillPo) -> Result<()>;

    /// 根据 ID 获取完整技能（PO + 文件列表）
    async fn get_by_id(&self, ctx: RequestContext, id: String) -> Result<Option<Skill>>;

    /// 根据 ID 获取 PO 数据（不需要文件时用这个）
    async fn get_po_by_id(&self, ctx: RequestContext, id: String) -> Result<Option<SkillPo>>;

    /// 通用综合查询（返回完整 Skill 实体，包含 PO + 文件列表）
    async fn query(
        &self,
        ctx: RequestContext,
        query: SkillQuery,
    ) -> Result<common::api::PagedResult<Skill>>;

    /// 按状态查询（返回完整 Skill 实体）
    async fn list_by_status(
        &self,
        ctx: RequestContext,
        status: common::enums::SkillStatus,
    ) -> Result<Vec<Skill>>;

    /// 按分类查询（返回完整 Skill 实体）
    async fn list_by_category(&self, ctx: RequestContext, category: &str) -> Result<Vec<Skill>>;

    /// 按作者查询（返回完整 Skill 实体）
    async fn list_by_author(&self, ctx: RequestContext, author_id: &str) -> Result<Vec<Skill>>;

    /// 获取 Agent 的所有技能（返回完整 Skill 实体）
    async fn list_for_agent(&self, ctx: RequestContext, agent_id: &str) -> Result<Vec<Skill>>;

    /// 获取 Agent 当前所有标记为 Expired 的技能副本（用于详情页「已过期技能」
    /// 虚拟分组展示 + 「恢复」按钮）。与 list_for_agent 的结果集天然互斥。
    async fn list_expired_for_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<Skill>>;

    /// 搜索技能（名称/描述/标签）
    async fn search(
        &self,
        ctx: RequestContext,
        search: SkillSearch,
    ) -> Result<common::api::PagedResult<Skill>>;

    /// 更新技能元数据（不影响文件）
    /// 更新技能（仅数据库）
    async fn update(&self, ctx: RequestContext, skill: &Skill) -> Result<()>;

    /// 删除技能（删除数据库记录 + 文件目录）
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 将已发布技能安装到 Agent（原子操作：复制文件 + 创建数据库记录）
    /// 返回安装后新创建的完整 Skill 业务实体
    async fn install_to_agent(
        &self,
        ctx: RequestContext,
        source_skill_id: &str,
        agent_id: &str,
    ) -> Result<Skill>;

    /// 读取技能主文件内容（skill.md）
    fn read_main_content(&self, skill: &SkillPo) -> Result<String>;

    /// 写入技能主文件内容（skill.md）
    fn write_main_content(&self, skill: &SkillPo, content: &str) -> Result<()>;

    /// 列出技能的所有文件（小文件自动预读内容）
    fn list_files(&self, skill: &SkillPo) -> Result<Vec<SkillFile>>;

    /// 读取指定文件内容
    fn read_file(&self, skill: &SkillPo, filename: &str) -> Result<String>;

    /// 写入文件内容
    fn write_file(&self, skill: &SkillPo, filename: &str, content: &str) -> Result<()>;

    /// 写入文件 bytes
    fn write_file_bytes(&self, skill: &SkillPo, filename: &str, bytes: &[u8]) -> Result<()>;

    fn skill_dir(&self, skill: &SkillPo) -> std::path::PathBuf;

    fn file_abs_path(&self, skill: &SkillPo, filename: &str) -> std::path::PathBuf;

    /// 删除技能目录内的指定文件（禁删 skill.md 主文件；canonicalize 校验在 skill_dir 下）。
    ///
    /// 不存在的文件视为成功（幂等）。
    fn delete_file(&self, skill: &SkillPo, filename: &str) -> Result<()>;

    /// 查询技能的向量索引内容哈希（判断是否需要重索引）
    async fn get_vector_content_hash(
        &self,
        ctx: RequestContext,
        skill_id: &str,
    ) -> Result<Option<String>>;

    /// 按 tag 查询已发布技能（用于技能包安装）
    async fn list_published_by_tag(&self, ctx: RequestContext, tag: &str) -> Result<Vec<Skill>>;

    /// 列出所有已发布技能的 distinct tags
    async fn list_tags(&self, ctx: RequestContext) -> Result<Vec<String>>;

    /// 查询指定 Agent 已有的技能副本（通过 author_id 和 parent_skill_id 列表）
    /// 如果 parent_skill_ids 为空，返回空 Vec
    async fn find_agent_skill_copies(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        parent_skill_ids: &[String],
    ) -> Result<Vec<Skill>>;

    /// 🔄 重建所有技能的向量索引
    ///
    /// 清空向量集合后，查询全量技能，逐条重新生成 embedding 并 upsert。
    /// 单条失败不影响整体，用 log_warn! 记录。
    async fn rebuild_vectors(&self, ctx: RequestContext) -> Result<()>;
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
    async fn create(&self, ctx: RequestContext, po: &SkillPo) -> Result<()> {
        // 1. 先保存基础技能数据
        self.skill_dao.insert(ctx.clone(), po).await?;

        // 2. 向量索引自动维护（失败仅 warn 降级，不影响主流程）
        match self.try_build_skill_vector_params(ctx.clone(), po).await {
            Ok(Some(vec_params)) => {
                if let Err(e) = self
                    .skill_vector_dao
                    .upsert_vector(ctx.clone(), &po.id, &vec_params)
                    .await
                {
                    log_warn!(
                        &ctx,
                        "vector_index",
                        skill_id = %po.id,
                        error = ?e,
                        "技能向量索引写入失败，已降级"
                    );
                }
            }
            Ok(None) => {
                log_debug!(
                    &ctx,
                    "vector_index",
                    skill_id = %po.id,
                    "无可用 Embedding Provider，跳过技能向量索引"
                );
            }
            Err(e) => {
                log_warn!(
                    &ctx,
                    "vector_index",
                    skill_id = %po.id,
                    error = ?e,
                    "技能向量化失败，已降级"
                );
            }
        }

        Ok(())
    }

    async fn get_by_id(&self, ctx: RequestContext, id: String) -> Result<Option<Skill>> {
        let Some(po) = self.skill_dao.find_by_id(ctx, &id).await? else {
            return Ok(None);
        };
        let files = self.skill_dao.list_files(&po)?;
        Ok(Some(Skill {
            po,
            files,
            search_match: None,
        }))
    }

    async fn get_po_by_id(&self, ctx: RequestContext, id: String) -> Result<Option<SkillPo>> {
        Ok(self.skill_dao.find_by_id(ctx, &id).await?)
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: SkillQuery,
    ) -> Result<common::api::PagedResult<Skill>> {
        let page = self.skill_dao.query(ctx, query).await?;
        let mut skills = Vec::with_capacity(page.items.len());
        for po in page.items {
            let files = self.skill_dao.list_files(&po)?;
            skills.push(Skill {
                po,
                files,
                search_match: None,
            });
        }
        Ok(common::api::PagedResult {
            items: skills,
            total: page.total,
        })
    }

    async fn list_by_status(
        &self,
        ctx: RequestContext,
        status: common::enums::SkillStatus,
    ) -> Result<Vec<Skill>> {
        let page = self
            .query(
                ctx,
                SkillQuery {
                    status: Some(status),
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    async fn list_by_category(&self, ctx: RequestContext, category: &str) -> Result<Vec<Skill>> {
        let page = self
            .query(
                ctx,
                SkillQuery {
                    category: Some(category.to_string()),
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    async fn list_by_author(&self, ctx: RequestContext, author_id: &str) -> Result<Vec<Skill>> {
        let page = self
            .query(
                ctx,
                SkillQuery {
                    author_id: Some(author_id.to_string()),
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    async fn list_for_agent(&self, ctx: RequestContext, agent_id: &str) -> Result<Vec<Skill>> {
        // 排除 Expired：过期技能是安装时间戳/包版本演进的 tombstone，
        // 不应出现在 Agent 详情页的可用技能栏里（恢复走 restore_skill 单条变更）。
        let page = self
            .query(
                ctx,
                SkillQuery {
                    author_id: Some(agent_id.to_string()),
                    exclude_status: Some(SkillStatus::Expired),
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    async fn list_expired_for_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<Skill>> {
        // 只返回 Expired：与 list_for_agent 天然互斥，用于「过期技能虚拟包」显示。
        let page = self
            .query(
                ctx,
                SkillQuery {
                    author_id: Some(agent_id.to_string()),
                    status: Some(SkillStatus::Expired),
                    exclude_status: None,
                    pagination: common::api::PaginationParams {
                        limit: None,
                        offset: None,
                    },
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    async fn search(
        &self,
        ctx: RequestContext,
        search: SkillSearch,
    ) -> Result<common::api::PagedResult<Skill>> {
        // 提前捕获 pagination，避免 search 被 move 后无法访问
        let pagination = search.filters.pagination.clone();
        // 向量距离阈值（可配置，默认 0.8）
        let vector_distance_threshold = search.vector_distance_threshold.unwrap_or(0.8);

        // Step 1: 准备向量搜索结果容器
        let mut vector_scores: std::collections::HashMap<String, f32> =
            std::collections::HashMap::new();
        let mut vector_skill_ids: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        // Step 2: 如果有关键词，尝试向量搜索
        if search.keyword.is_some()
            && let Some(provider) = self
                .model_provider_dao
                .get_default_embedding_provider(ctx.clone())
                .await?
        {
            // 生成查询向量
            if let Some(keyword) = &search.keyword {
                let query_vector_params = self
                    .cortex_dao
                    .embed_text_for_search(ctx.clone(), &provider, keyword)
                    .await?;
                let query_vector = query_vector_params.vector;

                // 向量搜索（前 20 条，与 search LIMIT 20 上限对齐）
                // 注意：只保留距离小于阈值的结果（余弦距离 0-2，0 是完全相同）
                match self
                    .skill_vector_dao
                    .search_vector(ctx.clone(), &query_vector, 20)
                    .await
                {
                    Ok(vector_results) => {
                        // 过滤距离小于阈值的结果
                        let filtered_results: Vec<(String, f32)> = vector_results
                            .into_iter()
                            .filter(|hit| hit.distance < vector_distance_threshold)
                            .map(|hit| (hit.row.id, hit.distance))
                            .collect();

                        vector_skill_ids =
                            filtered_results.iter().map(|(id, _)| id.clone()).collect();
                        vector_scores = filtered_results.into_iter().collect();
                    }
                    Err(e) => {
                        // 向量搜索失败（可能是 vss0 扩展未安装），降级到纯关键词搜索
                        log_warn!(
                            &ctx,
                            "vector_search",
                            "技能向量搜索失败，降级到关键词搜索: {}",
                            e
                        );
                    }
                }
            }
        }

        // Step 3: 执行 FTS5 关键词搜索（DAO 返回 Vec<(Po, fts_rank)>）
        let keyword_results = self.skill_dao.search(ctx.clone(), search.clone()).await?;

        // 提取 fts_rank 并转换为 Vec<Po> 便于聚合
        let mut fts_ranks: std::collections::HashMap<String, f32> =
            std::collections::HashMap::new();
        let keyword_pos: Vec<SkillPo> = keyword_results
            .into_iter()
            .map(|(po, rank)| {
                if let Some(r) = rank {
                    fts_ranks.insert(po.id.clone(), r);
                }
                po
            })
            .collect();

        // Step 4: 聚合结果（如果有向量结果，用通用 query 批量获取，避免 N+1）
        let mut all_pos = keyword_pos.clone();

        if !vector_skill_ids.is_empty() {
            let ids_to_fetch: Vec<String> = vector_skill_ids
                .into_iter()
                .filter(|id| !keyword_pos.iter().any(|po| po.id == *id))
                .collect();

            if !ids_to_fetch.is_empty() {
                // 分批获取（避免 SQL 太长）
                let mut ids_to_fetch = ids_to_fetch;
                ids_to_fetch.sort();
                ids_to_fetch.dedup();

                for chunk in ids_to_fetch.chunks(20) {
                    let chunk_ids: Vec<String> = chunk.to_vec();
                    let chunk_query = SkillQuery {
                        ids: Some(chunk_ids),
                        ..Default::default()
                    };
                    let chunk_pos = self.skill_dao.query(ctx.clone(), chunk_query).await?;
                    all_pos.extend(chunk_pos.items);
                }
            }
        }

        // Step 5: 去重
        all_pos.sort_by(|a, b| a.id.cmp(&b.id));
        all_pos.dedup_by(|a, b| a.id == b.id);

        // Step 6: 构建 Skill 对象，附加 SearchMatchInfo（三态匹配）
        let mut skills = Vec::with_capacity(all_pos.len());
        for po in all_pos {
            let files = self.skill_dao.list_files(&po)?;
            let has_vector = vector_scores.contains_key(&po.id);
            let has_keyword = fts_ranks.contains_key(&po.id);
            let match_info = if has_vector && has_keyword {
                // 双命中：向量 + 关键词
                Some(SearchMatchInfo {
                    match_type: MatchType::Hybrid,
                    vector_distance: vector_scores.get(&po.id).copied(),
                    fts_rank: fts_ranks.get(&po.id).copied(),
                    ..Default::default()
                })
            } else if has_vector {
                // 仅向量命中
                Some(SearchMatchInfo {
                    match_type: MatchType::Vector,
                    vector_distance: vector_scores.get(&po.id).copied(),
                    ..Default::default()
                })
            } else if has_keyword {
                // 仅关键词命中
                Some(SearchMatchInfo {
                    match_type: MatchType::Keyword,
                    fts_rank: fts_ranks.get(&po.id).copied(),
                    ..Default::default()
                })
            } else {
                None
            };
            skills.push(Skill {
                po,
                files,
                search_match: match_info,
            });
        }

        // Step 7: 综合排序（Hybrid 优先 → Vector 次之 → Keyword/None 最后）
        //    组内排序：Hybrid/Vector 按向量距离升序，Keyword 按 fts_rank 升序（BM25 越小越相关）
        skills.sort_by(|a, b| {
            let a_type = a.search_match.as_ref().map(|m| m.match_type);
            let b_type = b.search_match.as_ref().map(|m| m.match_type);
            let order_a = match a_type {
                Some(MatchType::Hybrid) => 0,
                Some(MatchType::Vector) => 1,
                _ => 2,
            };
            let order_b = match b_type {
                Some(MatchType::Hybrid) => 0,
                Some(MatchType::Vector) => 1,
                _ => 2,
            };
            order_a.cmp(&order_b).then_with(|| match (a_type, b_type) {
                (Some(MatchType::Hybrid), Some(MatchType::Hybrid))
                | (Some(MatchType::Vector), Some(MatchType::Vector)) => {
                    let a_dist = a
                        .search_match
                        .as_ref()
                        .and_then(|m| m.vector_distance)
                        .unwrap_or(f32::MAX);
                    let b_dist = b
                        .search_match
                        .as_ref()
                        .and_then(|m| m.vector_distance)
                        .unwrap_or(f32::MAX);
                    a_dist
                        .partial_cmp(&b_dist)
                        .unwrap_or(std::cmp::Ordering::Equal)
                }
                _ => {
                    let a_rank = a
                        .search_match
                        .as_ref()
                        .and_then(|m| m.fts_rank)
                        .unwrap_or(f32::MAX);
                    let b_rank = b
                        .search_match
                        .as_ref()
                        .and_then(|m| m.fts_rank)
                        .unwrap_or(f32::MAX);
                    a_rank
                        .partial_cmp(&b_rank)
                        .unwrap_or(std::cmp::Ordering::Equal)
                }
            })
        });

        // Step 8: 应用 search 上限（与 DAO LIMIT 20 对齐，避免内存聚合后结果过多）
        skills.truncate(20);

        // Step 9: 内存分页（DAO 已按 LIMIT 20 截断，这里基于聚合后的全量做 offset/limit 分页）
        let total = skills.len();
        let offset = pagination.offset.unwrap_or(0);
        let limit = pagination.limit.unwrap_or(20);
        let items = skills.into_iter().skip(offset).take(limit).collect();

        Ok(common::api::PagedResult { items, total })
    }

    async fn update(&self, ctx: RequestContext, skill: &Skill) -> Result<()> {
        // 1. 先更新基础技能数据
        self.skill_dao.update(ctx.clone(), &skill.po).await?;

        // 2. 向量索引自动维护：内容变化时重新索引（失败仅 warn 降级）
        let old_hash = self
            .get_vector_content_hash(ctx.clone(), &skill.po.id)
            .await?;
        let content = skill.po.vectorize_text();
        let new_hash = sha256::digest(&content);

        if old_hash.as_deref() != Some(&new_hash) {
            match self
                .try_build_skill_vector_params(ctx.clone(), &skill.po)
                .await
            {
                Ok(Some(vec_params)) => {
                    if let Err(e) = self
                        .skill_vector_dao
                        .upsert_vector(ctx.clone(), &skill.po.id, &vec_params)
                        .await
                    {
                        log_warn!(
                            &ctx,
                            "vector_index",
                            skill_id = %skill.po.id,
                            error = ?e,
                            "技能向量索引更新失败，已降级"
                        );
                    }
                }
                Ok(None) => {
                    log_debug!(
                        &ctx,
                        "vector_index",
                        skill_id = %skill.po.id,
                        "无可用 Embedding Provider，跳过技能向量索引"
                    );
                }
                Err(e) => {
                    log_warn!(
                        &ctx,
                        "vector_index",
                        skill_id = %skill.po.id,
                        error = ?e,
                        "技能向量化失败，已降级"
                    );
                }
            }
        }

        Ok(())
    }

    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()> {
        // 先获取 PO（用于删除文件时获取 content_path）
        let Some(po) = self.skill_dao.find_by_id(ctx.clone(), id).await? else {
            return Ok(()); // 不存在就返回成功
        };
        // 先删文件，再删数据库记录
        self.skill_dao.delete_skill_dir(&po)?;
        self.skill_dao.delete_by_id(ctx.clone(), id).await?;

        // 删除时清理向量索引（与 Tool DAL 一致，best-effort 不影响主流程）
        let _ = self.skill_vector_dao.delete_vector(ctx, id).await;

        Ok(())
    }

    async fn install_to_agent(
        &self,
        ctx: RequestContext,
        source_skill_id: &str,
        agent_id: &str,
    ) -> Result<Skill> {
        let ctx = ctx.to_builder().agent_id(agent_id).build();
        // 先获取源技能（PO + 文件列表，文件列表含每个文件的元信息）
        let source_po = self
            .skill_dao
            .find_by_id(ctx.clone(), source_skill_id)
            .await?
            .ok_or_else(|| err!(ResourceNotFound, "Skill not found"))?;
        let source_files = self.skill_dao.list_files(&source_po)?;

        // ===== 原地更新策略（解决 Expired 副本堆积） =====
        // 含 Expired 在内：只要 (author_id=agent_id, parent_skill_id=source_id) 存在任意状态副本，
        // 就复用其 ID，执行「状态重置（Expired→Draft）+ 元数据覆盖 + 字节级文件 diff 覆盖写」。
        // 这样 agent 技能过期后点"同步/重建"直接恢复为最新版本，不会出现重复行堆积。
        let existing = self
            .skill_dao
            .query(
                ctx.clone(),
                SkillQuery {
                    author_id: Some(agent_id.to_string()),
                    parent_skill_id: Some(source_skill_id.to_string()),
                    ..Default::default()
                },
            )
            .await?;

        if let Some(mut existing_po) = existing.items.into_iter().next() {
            // 1. 目标状态：统一 Draft（Expired 恢复、其他状态保持当前业务期望的可编辑状态）
            let target_status = if matches!(existing_po.status, SkillStatus::Published) {
                SkillStatus::Published
            } else {
                SkillStatus::Draft
            };
            let is_status_change = existing_po.status != target_status;
            if is_status_change {
                existing_po.status = target_status;
            }

            // 2. 元数据以源为准（name/description/tags/category 覆盖；保留 updated_at 重置到本次）
            // 仅在字段实际不同时置 changed，避免无谓 UPDATE
            let mut is_meta_change = false;
            if existing_po.name != source_po.name && !source_po.name.is_empty() {
                existing_po.name = source_po.name.clone();
                is_meta_change = true;
            }
            if existing_po.description != source_po.description {
                existing_po.description = source_po.description.clone();
                is_meta_change = true;
            }
            if existing_po.tags != source_po.tags {
                existing_po.tags = source_po.tags.clone();
                is_meta_change = true;
            }
            if existing_po.category != source_po.category {
                existing_po.category = source_po.category.clone();
                is_meta_change = true;
            }

            // 3. 文件字节级 diff：只覆盖有变化的文件（减少不必要 IO）。
            // 源路径 → 副本绝对路径，读取两侧字节比较。
            let mut any_file_changed = false;
            for sf in &source_files {
                let src_path = self.skill_dao.file_abs_path(&source_po, &sf.filename);
                // 源文件不存在（极小概率，list_files 刚拿到但瞬时被删）→ skip
                let src_bytes = match std::fs::read(&src_path) {
                    Ok(b) => b,
                    Err(_) => continue,
                };
                let dst_path = self.skill_dao.file_abs_path(&existing_po, &sf.filename);
                let dst_bytes = std::fs::read(&dst_path).ok();
                let dst_match = dst_bytes.as_ref().map(|b| b == &src_bytes).unwrap_or(false);
                if !dst_match {
                    any_file_changed = true;
                    self.skill_dao
                        .write_file_bytes(&existing_po, &sf.filename, &src_bytes)?;
                }
            }

            // 4. 若有变更 → 统一 UPDATE PO + 重置 updated_at + 刷新向量索引
            let needs_update = is_status_change || is_meta_change || any_file_changed;
            if needs_update {
                existing_po.updated_at = common::constants::utils::current_timestamp_ms();
                let updated_skill = Skill {
                    po: existing_po.clone(),
                    files: vec![],
                    search_match: None,
                };
                // 走 self.update() 复用"向量索引 hash 变化才重算"的逻辑，避免无谓 embed
                self.update(ctx.clone(), &updated_skill).await?;
            }

            // 组装返回实体（重新 list_files，确保字节落盘后与展示一致）
            let files = self.skill_dao.list_files(&existing_po)?;
            return Ok(Skill {
                po: existing_po,
                files,
                search_match: None,
            });
        }

        // 无副本：走 DAO 原子安装（复制文件 + 创建 DB 记录）
        let installed_po = self
            .skill_dao
            .install_to_agent(ctx.clone(), &source_po, agent_id)
            .await?;
        // 新创建的技能：由于直接走 DAO.install_to_agent 没有调用 DAL 的 create() → 不会自动建向量索引，
        // 这里补一次向量索引（create 流程等价，失败仅 warn 降级）。
        if let Ok(Some(vec_params)) = self
            .try_build_skill_vector_params(ctx.clone(), &installed_po)
            .await
        {
            let _ = self
                .skill_vector_dao
                .upsert_vector(ctx.clone(), &installed_po.id, &vec_params)
                .await;
        }
        let files = self.skill_dao.list_files(&installed_po)?;
        Ok(Skill {
            po: installed_po,
            files,
            search_match: None,
        })
    }

    fn read_main_content(&self, skill: &SkillPo) -> Result<String> {
        self.skill_dao.read_main_content(skill)
    }

    fn write_main_content(&self, skill: &SkillPo, content: &str) -> Result<()> {
        self.skill_dao.write_main_content(skill, content)
    }

    fn list_files(&self, skill: &SkillPo) -> Result<Vec<SkillFile>> {
        self.skill_dao.list_files(skill)
    }

    fn read_file(&self, skill: &SkillPo, filename: &str) -> Result<String> {
        self.skill_dao.read_file(skill, filename)
    }

    fn write_file(&self, skill: &SkillPo, filename: &str, content: &str) -> Result<()> {
        self.skill_dao.write_file(skill, filename, content)
    }

    fn write_file_bytes(&self, skill: &SkillPo, filename: &str, bytes: &[u8]) -> Result<()> {
        self.skill_dao.write_file_bytes(skill, filename, bytes)
    }

    fn skill_dir(&self, skill: &SkillPo) -> std::path::PathBuf {
        self.skill_dao.skill_dir(skill)
    }

    fn file_abs_path(&self, skill: &SkillPo, filename: &str) -> std::path::PathBuf {
        self.skill_dao.file_abs_path(skill, filename)
    }

    fn delete_file(&self, skill: &SkillPo, filename: &str) -> Result<()> {
        self.skill_dao.delete_file(skill, filename)
    }

    async fn get_vector_content_hash(
        &self,
        ctx: RequestContext,
        skill_id: &str,
    ) -> Result<Option<String>> {
        let row = self.skill_vector_dao.get_vector_row(ctx, skill_id).await?;
        Ok(row.map(|r| r.meta.content_hash))
    }

    async fn list_published_by_tag(&self, ctx: RequestContext, tag: &str) -> Result<Vec<Skill>> {
        let page = self
            .query(
                ctx,
                SkillQuery {
                    tags: Some(vec![tag.to_string()]),
                    status: Some(SkillStatus::Published),
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    async fn list_tags(&self, ctx: RequestContext) -> Result<Vec<String>> {
        self.skill_dao.list_distinct_tags(ctx).await
    }

    async fn find_agent_skill_copies(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        parent_skill_ids: &[String],
    ) -> Result<Vec<Skill>> {
        if parent_skill_ids.is_empty() {
            return Ok(Vec::new());
        }

        // 查询 Agent 的所有**有效**技能副本（排除 Expired），按 parent_skill_id 列表过滤。
        // 排除 Expired：软删除的旧副本不算已安装，否则同步/重装的增量检测会误判「已拥有」。
        let page = self
            .query(
                ctx,
                SkillQuery {
                    author_id: Some(agent_id.to_string()),
                    exclude_status: Some(SkillStatus::Expired),
                    ..Default::default()
                },
            )
            .await?;

        let id_set: std::collections::HashSet<&String> = parent_skill_ids.iter().collect();
        let mut skills = page.items;
        skills.retain(|s| id_set.contains(&s.po.parent_skill_id));
        Ok(skills)
    }

    async fn rebuild_vectors(&self, ctx: RequestContext) -> Result<()> {
        // 1. 获取当前启用的 Embedding Provider
        let Some(provider) = self
            .model_provider_dao
            .get_default_embedding_provider(ctx.clone())
            .await?
        else {
            log_debug!(
                &ctx,
                "rebuild_vectors",
                "无可用 Embedding Provider，跳过向量索引"
            );
            return Ok(());
        };
        let current_provider_id = provider.id.clone();

        // 2. 检查集合元数据：model_provider_id 一致则跳过重建
        let collection_name = "skills";
        let stored_id = ctx
            .vector_store()
            .get_collection_model_provider_id(collection_name)
            .await?;
        if stored_id.as_ref() == Some(&current_provider_id) {
            log_info!(
                &ctx,
                "rebuild_vectors",
                collection = %collection_name,
                provider_id = %current_provider_id,
                "向量索引 model_provider_id 一致，跳过重建"
            );
            return Ok(());
        }

        // 3. 清空向量集合并重建
        self.skill_vector_dao.clear_collection(ctx.clone()).await?;

        // 4. 查全量技能并逐条重新索引
        let skills = self.query(ctx.clone(), SkillQuery::default()).await?;
        for skill in &skills.items {
            match self
                .cortex_dao
                .embed_entity(ctx.clone(), &provider, &skill.po)
                .await
            {
                Ok(vec_params) => {
                    if let Err(e) = self
                        .skill_vector_dao
                        .upsert_vector(ctx.clone(), &skill.po.id, &vec_params)
                        .await
                    {
                        log_warn!(
                            &ctx,
                            "rebuild_vectors",
                            skill_id = %skill.po.id,
                            error = ?e,
                            "技能向量索引重建失败"
                        );
                    }
                }
                Err(e) => {
                    log_warn!(
                        &ctx,
                        "rebuild_vectors",
                        skill_id = %skill.po.id,
                        error = ?e,
                        "技能向量化失败，跳过"
                    );
                }
            }
        }

        // 6. 更新元数据
        ctx.vector_store()
            .set_collection_model_provider_id(collection_name, &current_provider_id)
            .await?;

        Ok(())
    }
}

impl SkillDalImpl {
    /// 尝试为技能构建向量索引参数（用于 create/update 索引场景）
    ///
    /// 任何中间步骤失败都会向上抛错；调用方决定是否 warn 降级。
    /// 返回 `Ok(None)` 表示无 Embedding Provider 配置（合法场景）。
    async fn try_build_skill_vector_params(
        &self,
        ctx: RequestContext,
        po: &SkillPo,
    ) -> Result<Option<crate::models::vector::VectorIndexParams>> {
        let Some(provider) = self
            .model_provider_dao
            .get_default_embedding_provider(ctx.clone())
            .await?
        else {
            return Ok(None);
        };

        let params = self.cortex_dao.embed_entity(ctx, &provider, po).await?;
        Ok(Some(params))
    }
}
