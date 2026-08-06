//! Project DAL 模块
//!
//! 职责：Project 领域的数据访问层，封装 ProjectDao 提供统一的查询接口
//! - 基础 CRUD（委托 ProjectDao）
//! - 向量索引自动维护（create/update 时调用 ProjectVectorDao）
//! - 混合搜索（FTS5 关键词 + 向量语义）

use crate::models::project::Project;
use crate::models::vector::{MatchType, SearchMatchInfo, Vectorizable};
use crate::pkg::RequestContext;
use crate::pkg::stats::{ModelCallEvent, ProjectEvent};
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::model_provider::ModelProviderDao;
use crate::service::dao::model_provider::{ModelProviderStatsDao, ModelProviderStatsQuery};
use crate::service::dao::project;
use crate::service::dao::project::{
    ProjectDao, ProjectQuery, ProjectSearch, ProjectStatsDao, ProjectStatsQuery, ProjectVectorDao,
};
use common::enums::ProjectStatus;
use common::error::Result;
use common::models::{ModelCallStats, ProjectStats, StatsFetchOptions, StatsInterval};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};

use crate::enrich_ctx;

// ==================== 单例管理 ====================

static PROJECT_DAL: OnceLock<Arc<dyn ProjectDal + Send + Sync>> = OnceLock::new();

/// 获取 Project DAL 单例
pub fn dal() -> Arc<dyn ProjectDal + Send + Sync> {
    PROJECT_DAL.get().cloned().unwrap()
}

/// 初始化 Project DAL
pub fn init() {
    project::stats_init();
    crate::service::dao::model_provider::stats_init();
    let _ = PROJECT_DAL.set(new(
        project::dao(),
        project::vector_dao(),
        project::stats_dao(),
        crate::service::dao::model_provider::stats_dao(),
        crate::service::dao::cortex::dao(),
        crate::service::dao::model_provider::dao(),
    ));
}

/// 创建 Project DAL（返回 trait 对象）
pub fn new(
    project_dao: Arc<dyn ProjectDao + Send + Sync>,
    project_vector_dao: Arc<dyn ProjectVectorDao + Send + Sync>,
    project_stats_dao: Arc<dyn ProjectStatsDao<ProjectEvent = ProjectEvent>>,
    model_provider_stats_dao: Arc<dyn ModelProviderStatsDao<ModelCallEvent = ModelCallEvent>>,
    cortex_dao: Arc<dyn CortexDao + Send + Sync>,
    model_provider_dao: Arc<dyn ModelProviderDao + Send + Sync>,
) -> Arc<dyn ProjectDal + Send + Sync> {
    Arc::new(ProjectDalImpl {
        project_dao,
        project_vector_dao,
        project_stats_dao,
        model_provider_stats_dao,
        cortex_dao,
        model_provider_dao,
    })
}

// ==================== DAL 接口 ====================

/// Project 附带信息获取选项
#[derive(Debug, Clone, Default)]
pub struct ProjectFetchOptions {
    /// 是否加载统计信息（ProjectStats: 事件次数汇总）
    pub with_stats: Option<bool>,
    /// 是否加载模型调用统计（ModelCallStats: token + 时序）
    pub with_model_call_stats: Option<bool>,
    /// 统计时间范围（毫秒），None 表示全部历史
    pub stats_time_range: Option<(i64, i64)>,
    /// 时序查询粒度，None 时默认 Daily
    pub stats_interval: Option<StatsInterval>,
    /// 是否加载任务依赖图（Mermaid 字符串）
    pub with_task_graph: Option<bool>,
    /// 是否加载产物列表（ArtifactDetail）
    pub with_artifacts: Option<bool>,
    /// 是否加载项目进度汇总（ProjectProgressSummary: 按任务状态实时聚合）
    pub with_progress_summary: Option<bool>,
}

/// Project DAL 接口
#[async_trait::async_trait]
pub trait ProjectDal: Send + Sync {
    /// 创建项目
    async fn create(&self, ctx: RequestContext, project: &Project) -> Result<()>;

    /// 根据 ID 获取项目
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Project>>;

    /// 根据 ID 获取项目（带附带信息选项）
    async fn get_project(
        &self,
        ctx: RequestContext,
        id: &str,
        options: ProjectFetchOptions,
    ) -> Result<Option<Project>>;

    /// 获取根用户下的所有项目
    async fn list_by_root_user(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Project>>;

    /// 获取根用户下指定状态的项目
    async fn list_by_root_user_and_status(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        status: Vec<ProjectStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<Project>>;

    /// 查询所有指定状态的项目（不限 root_user_id，用于系统级查询）
    async fn list_all_by_status(
        &self,
        ctx: RequestContext,
        status: ProjectStatus,
        limit: Option<usize>,
    ) -> Result<Vec<Project>>;

    /// 通用综合查询
    async fn query(
        &self,
        ctx: RequestContext,
        query: ProjectQuery,
    ) -> Result<common::api::PagedResult<Project>>;

    /// 统计符合查询条件的项目数量（透传 DAO count）
    async fn count(&self, ctx: RequestContext, query: ProjectQuery) -> Result<u64>;

    /// 更新项目信息
    async fn update(&self, ctx: RequestContext, project: &Project) -> Result<()>;

    /// 更新项目状态
    async fn update_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: ProjectStatus,
        modified_by: &str,
    ) -> Result<()>;

    /// 归档项目（软删除）
    async fn archive(&self, ctx: RequestContext, id: &str, modified_by: &str) -> Result<()>;

    /// 统计根用户的项目总数
    async fn count_by_root_user(&self, ctx: RequestContext, root_user_id: &str) -> Result<u64>;

    /// 统计根用户指定状态的项目数
    async fn count_by_root_user_and_status(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        status: ProjectStatus,
    ) -> Result<u64>;

    // ==================== 搜索 ====================

    /// 🔍 统一混合搜索（关键词 + 向量语义）
    ///
    /// 自动根据参数选择搜索策略：
    /// - keyword 存在 → 走 FTS5 全文检索
    /// - keyword 存在且 Embedding Provider 可用 → 同时走向量语义搜索，合并结果
    /// - 仅 query_vector 存在 → 走纯向量搜索
    /// - filters 透传业务过滤条件（root_user_id / status_in / limit）
    async fn search(
        &self,
        ctx: RequestContext,
        search: ProjectSearch,
    ) -> Result<common::api::PagedResult<Project>>;

    // ==================== 统计查询 ====================

    /// 获取 Project 统计数据（按 options 控制返回哪些维度）
    async fn get_stats(
        &self,
        ctx: RequestContext,
        project_id: &str,
        options: StatsFetchOptions,
    ) -> Result<ProjectStats>;

    /// 获取 Project 维度的模型调用统计
    ///
    /// 由 ModelProviderStatsDao（模型调用领域）负责计算，
    /// 按 project_id 过滤后返回 ModelCallStats。
    async fn get_model_call_stats(
        &self,
        ctx: RequestContext,
        project_id: &str,
        options: StatsFetchOptions,
    ) -> Result<ModelCallStats>;

    /// 🔄 重建所有项目的向量索引
    ///
    /// 清空向量集合后，查询全量项目，逐条重新生成 embedding 并 upsert。
    /// 单条失败不影响整体，用 log_warn! 记录。
    async fn rebuild_vectors(&self, ctx: RequestContext) -> Result<()>;
}

// ==================== DAL 实现 ====================

/// Project DAL 实现
struct ProjectDalImpl {
    project_dao: Arc<dyn ProjectDao + Send + Sync>,
    project_vector_dao: Arc<dyn ProjectVectorDao + Send + Sync>,
    project_stats_dao: Arc<dyn ProjectStatsDao<ProjectEvent = ProjectEvent>>,
    model_provider_stats_dao: Arc<dyn ModelProviderStatsDao<ModelCallEvent = ModelCallEvent>>,
    cortex_dao: Arc<dyn CortexDao + Send + Sync>,
    model_provider_dao: Arc<dyn ModelProviderDao + Send + Sync>,
}

#[async_trait::async_trait]
impl ProjectDal for ProjectDalImpl {
    async fn create(&self, ctx: RequestContext, project: &Project) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, project);
        // 1. 写入数据库
        self.project_dao.insert(ctx.clone(), &project.po).await?;

        // 2. 向量索引自动维护（失败仅 warn 降级，不影响主流程）
        match try_build_vector_params_for_entity(
            ctx.clone(),
            &self.cortex_dao,
            &self.model_provider_dao,
            &project.po,
        )
        .await
        {
            Ok(Some(vec_params)) => {
                if let Err(e) = self
                    .project_vector_dao
                    .upsert_vector(ctx.clone(), &project.po.id, &vec_params)
                    .await
                {
                    log_warn!(
                        &ctx,
                        "vector_index",
                        project_id = %project.po.id,
                        error = ?e,
                        "项目向量索引写入失败，已降级"
                    );
                }
            }
            Ok(None) => {
                log_debug!(
                    &ctx,
                    "vector_index",
                    project_id = %project.po.id,
                    "无可用 Embedding Provider，跳过项目向量索引"
                );
            }
            Err(e) => {
                log_warn!(
                    &ctx,
                    "vector_index",
                    project_id = %project.po.id,
                    error = ?e,
                    "项目向量化失败，已降级"
                );
            }
        }

        Ok(())
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Project>> {
        let opt = self.project_dao.find_by_id(ctx, id).await?;
        Ok(opt.map(Project::from_po))
    }

    async fn get_project(
        &self,
        ctx: RequestContext,
        id: &str,
        options: ProjectFetchOptions,
    ) -> Result<Option<Project>> {
        let opt = self.project_dao.find_by_id(ctx.clone(), id).await?;
        let Some(mut project) = opt.map(Project::from_po) else {
            return Ok(None);
        };

        if options.with_stats.unwrap_or(false) {
            let stats_options = StatsFetchOptions {
                with_call_summary: true,
                with_token_summary: false,
                with_time_series: false,
                time_range: options.stats_time_range,
                interval: options.stats_interval,
            };
            let stats = self.get_stats(ctx.clone(), id, stats_options).await?;
            project.stats = Some(stats);
        }

        if options.with_model_call_stats.unwrap_or(false) {
            let stats_options = StatsFetchOptions {
                with_call_summary: true,
                with_token_summary: true,
                with_time_series: true,
                time_range: options.stats_time_range,
                interval: options.stats_interval,
            };
            let model_call_stats = self
                .get_model_call_stats(ctx.clone(), id, stats_options)
                .await?;
            project.model_call_stats = Some(model_call_stats);
        }

        Ok(Some(project))
    }

    async fn list_by_root_user(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Project>> {
        let list = self
            .project_dao
            .list_by_root_user(ctx, root_user_id, limit)
            .await?;
        Ok(list.into_iter().map(Project::from_po).collect())
    }

    async fn list_by_root_user_and_status(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        status: Vec<ProjectStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<Project>> {
        let list = self
            .project_dao
            .list_by_root_user_and_status(ctx, root_user_id, status, limit)
            .await?;
        Ok(list.into_iter().map(Project::from_po).collect())
    }

    async fn list_all_by_status(
        &self,
        ctx: RequestContext,
        status: ProjectStatus,
        limit: Option<usize>,
    ) -> Result<Vec<Project>> {
        let list = self
            .project_dao
            .list_all_by_status(ctx, status, limit)
            .await?;
        Ok(list.into_iter().map(Project::from_po).collect())
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: ProjectQuery,
    ) -> Result<common::api::PagedResult<Project>> {
        let page = self.project_dao.query(ctx, query).await?;
        Ok(page.map(Project::from_po))
    }

    async fn count(&self, ctx: RequestContext, query: ProjectQuery) -> Result<u64> {
        self.project_dao.count(ctx, query).await
    }

    async fn update(&self, ctx: RequestContext, project: &Project) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, project);
        // 1. 更新数据库
        self.project_dao.update(ctx.clone(), &project.po).await?;

        // 2. 向量索引自动维护：内容变化时重新索引（失败仅 warn 降级）
        // 先检查内容哈希是否变化，避免不必要的重索引
        let old_hash = self
            .project_vector_dao
            .get_vector_row(ctx.clone(), &project.po.id)
            .await?
            .map(|r| r.meta.content_hash);
        let new_hash = project.po.vector_content_hash();

        if old_hash.as_deref() != Some(&new_hash) {
            match try_build_vector_params_for_entity(
                ctx.clone(),
                &self.cortex_dao,
                &self.model_provider_dao,
                &project.po,
            )
            .await
            {
                Ok(Some(vec_params)) => {
                    if let Err(e) = self
                        .project_vector_dao
                        .upsert_vector(ctx.clone(), &project.po.id, &vec_params)
                        .await
                    {
                        log_warn!(
                            &ctx,
                            "vector_index",
                            project_id = %project.po.id,
                            error = ?e,
                            "项目向量索引更新失败，已降级"
                        );
                    }
                }
                Ok(None) => {
                    log_debug!(
                        &ctx,
                        "vector_index",
                        project_id = %project.po.id,
                        "无可用 Embedding Provider，跳过项目向量索引更新"
                    );
                }
                Err(e) => {
                    log_warn!(
                        &ctx,
                        "vector_index",
                        project_id = %project.po.id,
                        error = ?e,
                        "项目向量化失败，跳过向量索引更新"
                    );
                }
            }
        }

        Ok(())
    }

    async fn update_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: ProjectStatus,
        modified_by: &str,
    ) -> Result<()> {
        let ctx = ctx.to_builder().project_id(id).build();
        self.project_dao
            .update_status(ctx, id, status, modified_by)
            .await
    }

    async fn archive(&self, ctx: RequestContext, id: &str, modified_by: &str) -> Result<()> {
        let ctx = ctx.to_builder().project_id(id).build();
        self.project_dao
            .update_status(ctx.clone(), id, ProjectStatus::Archived, modified_by)
            .await?;
        // 归档时清理向量索引
        let _ = self.project_vector_dao.delete_vector(ctx, id).await;
        Ok(())
    }

    async fn count_by_root_user(&self, ctx: RequestContext, root_user_id: &str) -> Result<u64> {
        // 语法糖：调用通用 count
        self.count(
            ctx,
            ProjectQuery {
                root_user_id: Some(root_user_id.to_string()),
                ..Default::default()
            },
        )
        .await
    }

    async fn count_by_root_user_and_status(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        status: ProjectStatus,
    ) -> Result<u64> {
        // 语法糖：调用通用 count
        self.count(
            ctx,
            ProjectQuery {
                root_user_id: Some(root_user_id.to_string()),
                status_in: Some(vec![status]),
                ..Default::default()
            },
        )
        .await
    }

    // ==================== 搜索 ====================

    async fn search(
        &self,
        ctx: RequestContext,
        search: ProjectSearch,
    ) -> Result<common::api::PagedResult<Project>> {
        // 向量距离阈值（默认 0.8，余弦距离 0-2，0 完全相同）
        const VECTOR_DISTANCE_THRESHOLD: f32 = 0.8;

        // Step 1: 准备向量搜索结果容器
        let mut vector_scores: HashMap<String, f32> = HashMap::new();
        let mut vector_ids: HashSet<String> = HashSet::new();

        // Step 2: 如果有关键词，尝试向量搜索（用关键词生成查询向量）
        if search.keyword.is_some() {
            match try_build_vector_params_for_search(
                ctx.clone(),
                &self.cortex_dao,
                &self.model_provider_dao,
                search.keyword.as_deref().unwrap_or(""),
            )
            .await
            {
                Ok(Some(vec_params)) => {
                    // 向量搜索（前 MAX_SEARCH_RESULTS 条，与 FTS5 限制一致）
                    match self
                        .project_vector_dao
                        .search_vector(ctx.clone(), &vec_params.vector, 20)
                        .await
                    {
                        Ok(vector_results) => {
                            // 过滤距离小于阈值的结果
                            let filtered_results: Vec<(String, f32)> = vector_results
                                .into_iter()
                                .filter(|hit| hit.distance < VECTOR_DISTANCE_THRESHOLD)
                                .map(|hit| (hit.row.id, hit.distance))
                                .collect();

                            vector_ids =
                                filtered_results.iter().map(|(id, _)| id.clone()).collect();
                            vector_scores = filtered_results.into_iter().collect();
                        }
                        Err(e) => {
                            // 向量搜索失败，降级到纯关键词搜索
                            log_warn!(
                                &ctx,
                                "vector_search",
                                "项目向量搜索失败，降级到关键词搜索: {}",
                                e
                            );
                        }
                    }
                }
                Ok(None) => {
                    log_debug!(
                        &ctx,
                        "vector_search",
                        "无可用 Embedding Provider，跳过项目向量搜索"
                    );
                }
                Err(e) => {
                    log_warn!(
                        &ctx,
                        "vector_search",
                        error = ?e,
                        "项目向量化失败，跳过向量搜索"
                    );
                }
            }
        }

        // Step 3: 执行关键词搜索（DAO 返回 Vec<(Po, fts_rank)>）
        let keyword_results = self
            .project_dao
            .search_projects(ctx.clone(), search.clone())
            .await?;

        // 提取 fts_rank 并转换为 Vec<Po> 便于聚合
        let mut fts_ranks: HashMap<String, f32> = HashMap::new();
        let keyword_pos: Vec<crate::models::project::ProjectPo> = keyword_results
            .into_iter()
            .map(|(po, rank)| {
                if let Some(r) = rank {
                    fts_ranks.insert(po.id.clone(), r);
                }
                po
            })
            .collect();

        // Step 4: 聚合结果（如果有向量结果但不在关键词结果中，用通用 query 批量获取）
        let mut all_pos = keyword_pos.clone();

        if !vector_ids.is_empty() {
            let ids_to_fetch: Vec<String> = vector_ids
                .into_iter()
                .filter(|id| !keyword_pos.iter().any(|po| po.id == *id))
                .collect();

            if !ids_to_fetch.is_empty() {
                // 用通用 query 批量获取 ids_to_fetch 的结果
                let query_for_ids = ProjectQuery {
                    ids: Some(ids_to_fetch),
                    ..search.filters.clone()
                };
                let vector_pos = self.project_dao.query(ctx.clone(), query_for_ids).await?;
                all_pos.extend(vector_pos.items);
            }
        }

        // Step 5: 去重
        all_pos.sort_by(|a, b| a.id.cmp(&b.id));
        all_pos.dedup_by(|a, b| a.id == b.id);

        // Step 6: 构建业务对象（三态匹配：Hybrid / Vector / Keyword）
        let mut projects = Vec::with_capacity(all_pos.len());
        for po in all_pos {
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
            projects.push(Project {
                po,
                search_match: match_info,
                stats: None,
                model_call_stats: None,
                task_graph: None,
                artifacts: None,
                progress_summary: None,
            });
        }

        // Step 7: 统一排序：Hybrid 优先 → Vector 次之 → Keyword 最后
        //    组内排序：Hybrid/Vector 按向量距离升序，Keyword 按 fts_rank 升序（BM25 越小越相关）
        projects.sort_by(|a, b| {
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

        // Step 8: 截断到 MAX_SEARCH_RESULTS + 分页
        // 搜索场景限制总结果数（MAX_SEARCH_RESULTS=20），搜不到应换关键词而非无限分页
        projects.truncate(20);

        let pagination = search.filters.pagination.clone();
        let total = projects.len();
        let offset = pagination.offset.unwrap_or(0);
        let limit = pagination.limit.unwrap_or(20);
        let items = projects.into_iter().skip(offset).take(limit).collect();
        Ok(common::api::PagedResult { items, total })
    }

    // ==================== 统计查询 ====================

    async fn get_stats(
        &self,
        ctx: RequestContext,
        project_id: &str,
        options: StatsFetchOptions,
    ) -> Result<ProjectStats> {
        let query = ProjectStatsQuery {
            project_id: project_id.to_string(),
            time_range: options.time_range,
            ..Default::default()
        };
        self.project_stats_dao.get_stats(ctx, query, options).await
    }

    async fn get_model_call_stats(
        &self,
        ctx: RequestContext,
        project_id: &str,
        options: StatsFetchOptions,
    ) -> Result<ModelCallStats> {
        let query = ModelProviderStatsQuery {
            project_id: Some(project_id.to_string()),
            time_range: options.time_range,
            interval: options.interval,
            ..Default::default()
        };
        self.model_provider_stats_dao
            .get_stats(ctx, query, options)
            .await
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
        let collection_name = "projects";
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
        self.project_vector_dao
            .clear_collection(ctx.clone())
            .await?;

        // 4. 查全量项目并逐条重新索引
        let projects = self
            .query(ctx.clone(), ProjectQuery::default())
            .await?
            .items;
        for project in &projects {
            match try_build_vector_params_for_entity(
                ctx.clone(),
                &self.cortex_dao,
                &self.model_provider_dao,
                &project.po,
            )
            .await
            {
                Ok(Some(vec_params)) => {
                    if let Err(e) = self
                        .project_vector_dao
                        .upsert_vector(ctx.clone(), &project.po.id, &vec_params)
                        .await
                    {
                        log_warn!(
                            &ctx,
                            "rebuild_vectors",
                            project_id = %project.po.id,
                            error = ?e,
                            "项目向量索引重建失败"
                        );
                    }
                }
                Ok(None) => {
                    log_debug!(
                        &ctx,
                        "rebuild_vectors",
                        project_id = %project.po.id,
                        "无可用 Embedding Provider，跳过向量索引"
                    );
                }
                Err(e) => {
                    log_warn!(
                        &ctx,
                        "rebuild_vectors",
                        project_id = %project.po.id,
                        error = ?e,
                        "项目向量化失败，跳过"
                    );
                }
            }
        }

        // 5. 更新元数据
        ctx.vector_store()
            .set_collection_model_provider_id(collection_name, &current_provider_id)
            .await?;

        Ok(())
    }
}

// ==================== Helpers ====================

/// 尝试为实体构建向量索引参数（用于索引场景）
///
/// 任何中间步骤失败都会向上抛错；调用方决定是否 warn 降级。
/// 返回 `Ok(None)` 表示无 Embedding Provider 配置（合法场景）。
async fn try_build_vector_params_for_entity(
    ctx: RequestContext,
    cortex_dao: &Arc<dyn CortexDao + Send + Sync>,
    model_provider_dao: &Arc<dyn ModelProviderDao + Send + Sync>,
    entity: &dyn Vectorizable,
) -> Result<Option<crate::models::vector::VectorIndexParams>> {
    let Some(provider) = model_provider_dao
        .get_default_embedding_provider(ctx.clone())
        .await?
    else {
        return Ok(None);
    };

    let params = cortex_dao.embed_entity(ctx, &provider, entity).await?;
    Ok(Some(params))
}

/// 尝试为查询文本构建向量索引参数（用于搜索场景）
///
/// 任何中间步骤失败都会向上抛错；调用方决定是否 warn 降级。
/// 返回 `Ok(None)` 表示无 Embedding Provider 配置（合法场景）。
async fn try_build_vector_params_for_search(
    ctx: RequestContext,
    cortex_dao: &Arc<dyn CortexDao + Send + Sync>,
    model_provider_dao: &Arc<dyn ModelProviderDao + Send + Sync>,
    text: &str,
) -> Result<Option<crate::models::vector::VectorIndexParams>> {
    let Some(provider) = model_provider_dao
        .get_default_embedding_provider(ctx.clone())
        .await?
    else {
        return Ok(None);
    };

    let params = cortex_dao
        .embed_text_for_search(ctx.clone(), &provider, text)
        .await?;
    Ok(Some(params))
}
