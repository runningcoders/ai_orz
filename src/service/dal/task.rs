//! Task DAL 模块
//!
//! 职责：Task 领域的数据访问层，封装 TaskDao 提供统一的查询接口
//! - 混合搜索（FTS5 关键词 + 向量语义）
//! - 向量索引自动维护（create/update/delete）

use common::error::Result;
use common::models::{ModelCallStats, StatsFetchOptions, StatsInterval, TaskStats};
use crate::models::task::{Task, TaskPo};
use crate::models::vector::{MatchType, SearchMatchInfo, VectorIndexParams, Vectorizable};
use crate::pkg::RequestContext;
use crate::pkg::stats::{ModelCallEvent, TaskEvent};
use crate::service::dao::task;
use crate::service::dao::task::{TaskDao, TaskQuery, TaskSearch, TaskStatsDao, TaskStatsQuery, TaskVectorDao};
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::model_provider::{ModelProviderDao, ModelProviderStatsDao, ModelProviderStatsQuery};
use crate::service::dal::model_provider;
use common::enums::{AssigneeType, TaskStatus};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};

use crate::enrich_ctx;

// ==================== 单例管理 ====================

static TASK_DAL: OnceLock<Arc<dyn TaskDal + Send + Sync>> = OnceLock::new();

/// 获取 Task DAL 单例
pub fn dal() -> Arc<dyn TaskDal + Send + Sync> {
    TASK_DAL.get().cloned().unwrap()
}

/// 初始化 Task DAL
pub fn init() {
    task::init_vector();
    task::stats_init();
    model_provider::init();
    crate::service::dao::model_provider::stats_init();
    let _ = TASK_DAL.set(new(
        task::dao(),
        task::vector_dao(),
        task::stats_dao(),
        crate::service::dao::model_provider::stats_dao(),
        crate::service::dao::cortex::dao(),
        crate::service::dao::model_provider::dao(),
    ));
}

/// 创建 Task DAL（返回 trait 对象）
pub fn new(
    task_dao: Arc<dyn TaskDao + Send + Sync>,
    task_vector_dao: Arc<dyn TaskVectorDao>,
    task_stats_dao: Arc<dyn TaskStatsDao<TaskEvent = TaskEvent>>,
    model_provider_stats_dao: Arc<dyn ModelProviderStatsDao<ModelCallEvent = ModelCallEvent>>,
    cortex_dao: Arc<dyn CortexDao>,
    model_provider_dao: Arc<dyn ModelProviderDao>,
) -> Arc<dyn TaskDal + Send + Sync> {
    Arc::new(TaskDalImpl { task_dao, task_vector_dao, task_stats_dao, model_provider_stats_dao, cortex_dao, model_provider_dao })
}

// ==================== DAL 接口 ====================

/// Task 附带信息获取选项
#[derive(Debug, Clone, Default)]
pub struct TaskFetchOptions {
    /// 是否加载统计信息（TaskStats: 事件次数汇总）
    pub with_stats: Option<bool>,
    /// 是否加载模型调用统计（ModelCallStats: token + 时序）
    pub with_model_call_stats: Option<bool>,
    /// 统计时间范围（毫秒），None 表示全部历史
    pub stats_time_range: Option<(i64, i64)>,
    /// 时序查询粒度，None 时默认 Daily
    pub stats_interval: Option<StatsInterval>,
}

/// Task DAL 接口
#[async_trait::async_trait]
pub trait TaskDal: Send + Sync {
    /// 创建任务
    async fn create(&self, ctx: RequestContext, task: &Task) -> Result<()>;

    /// 根据 ID 获取任务
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Task>>;

    /// 根据 ID 获取任务（带附带信息选项）
    async fn get_task(&self, ctx: RequestContext, id: &str, options: TaskFetchOptions) -> Result<Option<Task>>;

    /// 获取分配对象下的所有任务
    async fn list_by_assignee(
        &self,
        ctx: RequestContext,
        assignee_type: Option<AssigneeType>,
        assignee_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<TaskPo>>;

    /// 获取分配对象下指定状态的任务
    async fn list_by_status(
        &self,
        ctx: RequestContext,
        assignee_type: Option<AssigneeType>,
        assignee_id: &str,
        status: Vec<TaskStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<TaskPo>>;

    /// 获取项目下的所有任务
    async fn list_by_project(
        &self,
        ctx: RequestContext,
        project_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Task>>;

    /// 通用综合查询
    async fn query(&self, ctx: RequestContext, query: TaskQuery) -> Result<common::api::PagedResult<Task>>;

    /// 🔍 统一混合搜索（FTS5 关键词 + 向量语义）
    ///
    /// 自动根据参数选择搜索策略：
    /// - keyword 存在 → FTS5 全文检索
    /// - query_vector 存在 → 向量语义搜索
    /// - 两者都有 → 混合搜索，合并结果
    async fn search(&self, ctx: RequestContext, search: TaskSearch) -> Result<Vec<Task>>;

    /// 更新任务信息
    async fn update(&self, ctx: RequestContext, task: &Task) -> Result<()>;

    /// 更新任务状态
    async fn update_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: TaskStatus,
        modified_by: &str,
    ) -> Result<()>;

    /// 取消任务
    async fn cancel(
        &self,
        ctx: RequestContext,
        id: &str,
        modified_by: &str,
    ) -> Result<()>;

    /// 统计分配对象的任务总数
    async fn count_by_assignee(
        &self,
        ctx: RequestContext,
        assignee_id: &str,
    ) -> Result<u64>;

    /// 统计分配对象指定状态的任务数
    async fn count_by_assignee_and_status(
        &self,
        ctx: RequestContext,
        assignee_id: &str,
        status: TaskStatus,
    ) -> Result<u64>;

    // ==================== 统计查询 ====================

    /// 获取 Task 统计数据（按 options 控制返回哪些维度）
    async fn get_stats(&self, ctx: RequestContext, task_id: &str, options: StatsFetchOptions) -> Result<TaskStats>;

    /// 获取 Task 维度的模型调用统计
    ///
    /// 由 ModelProviderStatsDao（模型调用领域）负责计算，
    /// 按 task_id 过滤后返回 ModelCallStats。
    async fn get_model_call_stats(&self, ctx: RequestContext, task_id: &str, options: StatsFetchOptions) -> Result<ModelCallStats>;

    /// 🔄 重建所有任务的向量索引
    ///
    /// 清空向量集合后，查询全量任务，逐条重新生成 embedding 并 upsert。
    /// 单条失败不影响整体，用 log_warn! 记录。
    async fn rebuild_vectors(&self, ctx: RequestContext) -> Result<()>;
}

// ==================== DAL 实现 ====================

/// Task DAL 实现
struct TaskDalImpl {
    task_dao: Arc<dyn TaskDao + Send + Sync>,
    task_vector_dao: Arc<dyn TaskVectorDao>,
    task_stats_dao: Arc<dyn TaskStatsDao<TaskEvent = TaskEvent>>,
    model_provider_stats_dao: Arc<dyn ModelProviderStatsDao<ModelCallEvent = ModelCallEvent>>,
    cortex_dao: Arc<dyn CortexDao>,
    model_provider_dao: Arc<dyn ModelProviderDao>,
}

#[async_trait::async_trait]
impl TaskDal for TaskDalImpl {
    async fn create(&self, ctx: RequestContext, task: &Task) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, task);
        // 1. 写入 SQLite
        self.task_dao.insert(ctx.clone(), &task.po).await?;

        // 2. 向量化（title + description 拼接，失败 warn 降级，不影响主流程）
        match try_build_vector_params_for_entity(
            ctx.clone(),
            &self.cortex_dao,
            &self.model_provider_dao,
            &task.po,
        )
        .await
        {
            Ok(Some(vec_params)) => {
                if let Err(e) = self
                    .task_vector_dao
                    .upsert_vector(ctx.clone(), &task.po.id, &vec_params)
                    .await
                {
                    log_warn!(ctx, "vector_index", task_id = %task.po.id, error = ?e, "任务向量索引写入失败，已降级");
                }
            }
            Ok(None) => {
                log_debug!(ctx, "vector_index", task_id = %task.po.id, "无可用 Embedding Provider，跳过向量索引");
            }
            Err(e) => {
                log_warn!(ctx, "vector_index", task_id = %task.po.id, error = ?e, "任务向量化失败，已降级");
            }
        }

        Ok(())
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Task>> {
        let opt = self.task_dao.find_by_id(ctx, id).await?;
        Ok(opt.map(Task::from_po))
    }

    async fn get_task(&self, ctx: RequestContext, id: &str, options: TaskFetchOptions) -> Result<Option<Task>> {
        let opt = self.task_dao.find_by_id(ctx.clone(), id).await?;
        let Some(mut task) = opt.map(Task::from_po) else {
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
            task.stats = Some(stats);
        }

        if options.with_model_call_stats.unwrap_or(false) {
            let stats_options = StatsFetchOptions {
                with_call_summary: true,
                with_token_summary: true,
                with_time_series: true,
                time_range: options.stats_time_range,
                interval: options.stats_interval,
            };
            let model_call_stats = self.get_model_call_stats(ctx.clone(), id, stats_options).await?;
            task.model_call_stats = Some(model_call_stats);
        }

        Ok(Some(task))
    }

    async fn list_by_assignee(
        &self,
        ctx: RequestContext,
        assignee_type: Option<AssigneeType>,
        assignee_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<TaskPo>> {
        self.task_dao
            .list_by_assignee(ctx, assignee_type, assignee_id, limit)
            .await
    }

    async fn list_by_status(
        &self,
        ctx: RequestContext,
        assignee_type: Option<AssigneeType>,
        assignee_id: &str,
        status: Vec<TaskStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<TaskPo>> {
        self.task_dao
            .list_by_status(ctx, assignee_type, assignee_id, status, limit)
            .await
    }

    async fn list_by_project(
        &self,
        ctx: RequestContext,
        project_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Task>> {
        let page = self
            .task_dao
            .query(
                ctx,
                TaskQuery {
                    assignee_type: None,
                    assignee_id: None,
                    project_id: Some(project_id.to_string()),
                    status_in: None,
                    pagination: common::api::PaginationParams { limit, offset: None },
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items.into_iter().map(Task::from_po).collect())
    }

    async fn query(&self, ctx: RequestContext, query: TaskQuery) -> Result<common::api::PagedResult<Task>> {
        let page = self.task_dao.query(ctx, query).await?;
        Ok(page.map(Task::from_po))
    }

    async fn search(&self, ctx: RequestContext, search: TaskSearch) -> Result<Vec<Task>> {
        // 向量距离阈值（默认 0.8）
        const VECTOR_DISTANCE_THRESHOLD: f32 = 0.8;

        // Step 1: 准备向量搜索结果容器
        let mut vector_scores: HashMap<String, f32> = HashMap::new();
        let mut vector_ids: HashSet<String> = HashSet::new();

        // Step 2: 如果有关键词，执行向量搜索（用关键词生成查询向量）
        if search.keyword.is_some() {
            if let Some(keyword) = &search.keyword {
                match try_build_vector_params_for_search(
                    ctx.clone(),
                    &self.cortex_dao,
                    &self.model_provider_dao,
                    keyword,
                )
                .await
                {
                    Ok(Some(vec_params)) => {
                        // 向量搜索（前 50 条）
                        match self
                            .task_vector_dao
                            .search_vector(ctx.clone(), &vec_params.vector, 50)
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
                                    ctx,
                                    "vector_search",
                                    "任务向量搜索失败，降级到关键词搜索: {}",
                                    e
                                );
                            }
                        }
                    }
                    Ok(None) => {
                        log_debug!(
                            ctx,
                            "vector_search",
                            "无可用 Embedding Provider，跳过向量搜索"
                        );
                    }
                    Err(e) => {
                        log_warn!(ctx, "vector_search", error = ?e, "任务向量化失败，跳过向量搜索");
                    }
                }
            }
        }

        // Step 3: 执行 FTS5 关键词搜索（DAO 返回 Vec<(Po, fts_rank)>）
        let keyword_results = self
            .task_dao
            .search_tasks(ctx.clone(), search.clone())
            .await?;

        // 提取 fts_rank 并转换为 Vec<Po> 便于聚合
        let mut fts_ranks: HashMap<String, f32> = HashMap::new();
        let keyword_pos: Vec<TaskPo> = keyword_results
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
                let query_for_ids = TaskQuery {
                    ids: Some(ids_to_fetch),
                    ..search.filters.clone()
                };
                let vector_pos = self.task_dao.query(ctx.clone(), query_for_ids).await?;
                all_pos.extend(vector_pos.items);
            }
        }

        // Step 5: 去重
        all_pos.sort_by(|a, b| a.id.cmp(&b.id));
        all_pos.dedup_by(|a, b| a.id == b.id);

        // Step 6: 构建业务对象 + 匹配信息
        let mut tasks = Vec::with_capacity(all_pos.len());
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
            tasks.push(Task {
                po,
                search_match: match_info,
                stats: None,
                model_call_stats: None,
            });
        }

        // Step 7: 排序：Hybrid 优先 → Vector 次之 → Keyword 最后
        //    组内排序：Hybrid/Vector 按向量距离升序，Keyword 按 fts_rank 升序（BM25 越小越相关）
        tasks.sort_by(|a, b| {
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
            order_a.cmp(&order_b).then_with(|| {
                match (a_type, b_type) {
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
                        a_dist.partial_cmp(&b_dist).unwrap_or(std::cmp::Ordering::Equal)
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
                        a_rank.partial_cmp(&b_rank).unwrap_or(std::cmp::Ordering::Equal)
                    }
                }
            })
        });

        // Step 8: 应用 limit
        if let Some(limit) = search.filters.pagination.limit {
            tasks.truncate(limit);
        }

        Ok(tasks)
    }

    async fn update(&self, ctx: RequestContext, task: &Task) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, task);
        // 1. 更新 SQLite
        self.task_dao.update(ctx.clone(), &task.po).await?;

        // 2. 重新向量化（title + description 拼接，失败 warn 降级）
        match try_build_vector_params_for_entity(
            ctx.clone(),
            &self.cortex_dao,
            &self.model_provider_dao,
            &task.po,
        )
        .await
        {
            Ok(Some(vec_params)) => {
                if let Err(e) = self
                    .task_vector_dao
                    .upsert_vector(ctx.clone(), &task.po.id, &vec_params)
                    .await
                {
                    log_warn!(ctx, "vector_index", task_id = %task.po.id, error = ?e, "任务向量索引更新失败，已降级");
                }
            }
            Ok(None) => {
                log_debug!(ctx, "vector_index", task_id = %task.po.id, "无可用 Embedding Provider，跳过向量索引更新");
            }
            Err(e) => {
                log_warn!(ctx, "vector_index", task_id = %task.po.id, error = ?e, "任务向量化失败，跳过向量索引更新");
            }
        }

        Ok(())
    }

    async fn update_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: TaskStatus,
        modified_by: &str,
    ) -> Result<()> {
        let ctx = ctx.to_builder().task_id(id).build();
        self.task_dao
            .update_status(ctx, id, status, modified_by)
            .await
    }

    async fn cancel(
        &self,
        ctx: RequestContext,
        id: &str,
        modified_by: &str,
    ) -> Result<()> {
        let ctx = ctx.to_builder().task_id(id).build();
        // 1. 软删除（status = Cancelled = 0）
        self.task_dao
            .update_status(ctx.clone(), id, TaskStatus::Cancelled, modified_by)
            .await?;

        // 2. 清理向量索引（忽略失败，不影响主流程）
        if let Err(e) = self.task_vector_dao.delete_vector(ctx.clone(), id).await {
            log_warn!(ctx, "vector_index", task_id = %id, error = ?e, "任务向量索引删除失败，已降级");
        }

        Ok(())
    }

    async fn count_by_assignee(
        &self,
        ctx: RequestContext,
        assignee_id: &str,
    ) -> Result<u64> {
        self.task_dao.count_by_assignee(ctx, assignee_id).await
    }

    async fn count_by_assignee_and_status(
        &self,
        ctx: RequestContext,
        assignee_id: &str,
        status: TaskStatus,
    ) -> Result<u64> {
        self.task_dao
            .count_by_assignee_and_status(ctx, assignee_id, status)
            .await
    }

    // ==================== 统计查询 ====================

    async fn get_stats(&self, ctx: RequestContext, task_id: &str, options: StatsFetchOptions) -> Result<TaskStats> {
        let query = TaskStatsQuery {
            task_id: task_id.to_string(),
            time_range: options.time_range,
            ..Default::default()
        };
        self.task_stats_dao.get_stats(ctx, query, options).await
    }

    async fn get_model_call_stats(&self, ctx: RequestContext, task_id: &str, options: StatsFetchOptions) -> Result<ModelCallStats> {
        let query = ModelProviderStatsQuery {
            task_id: Some(task_id.to_string()),
            time_range: options.time_range,
            interval: options.interval,
            ..Default::default()
        };
        self.model_provider_stats_dao.get_stats(ctx, query, options).await
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
        let collection_name = "tasks";
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
        self.task_vector_dao.clear_collection(ctx.clone()).await?;

        // 4. 查全量任务并逐条重新索引
        let tasks = self.query(ctx.clone(), TaskQuery::default()).await?.items;
        for task in &tasks {
            match try_build_vector_params_for_entity(
                ctx.clone(),
                &self.cortex_dao,
                &self.model_provider_dao,
                &task.po,
            )
            .await
            {
                Ok(Some(vec_params)) => {
                    if let Err(e) = self
                        .task_vector_dao
                        .upsert_vector(ctx.clone(), &task.po.id, &vec_params)
                        .await
                    {
                        log_warn!(
                            &ctx,
                            "rebuild_vectors",
                            task_id = %task.po.id,
                            error = ?e,
                            "任务向量索引重建失败"
                        );
                    }
                }
                Ok(None) => {
                    log_debug!(
                        &ctx,
                        "rebuild_vectors",
                        task_id = %task.po.id,
                        "无可用 Embedding Provider，跳过向量索引"
                    );
                }
                Err(e) => {
                    log_warn!(
                        &ctx,
                        "rebuild_vectors",
                        task_id = %task.po.id,
                        error = ?e,
                        "任务向量化失败，跳过"
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
    cortex_dao: &Arc<dyn CortexDao>,
    model_provider_dao: &Arc<dyn ModelProviderDao>,
    entity: &dyn Vectorizable,
) -> Result<Option<VectorIndexParams>> {
    let Some(provider) = model_provider_dao
        .get_default_embedding_provider(ctx.clone())
        .await?
    else {
        return Ok(None);
    };

    let cortex = cortex_dao.create_cortex_trait(ctx.clone(), &provider, vec![])?;
    let params = cortex_dao
        .embed_entity(ctx, cortex.as_ref(), entity)
        .await?;
    Ok(Some(params))
}

/// 尝试为查询文本构建向量索引参数（用于搜索场景）
///
/// 任何中间步骤失败都会向上抛错；调用方决定是否 warn 降级。
/// 返回 `Ok(None)` 表示无 Embedding Provider 配置（合法场景）。
async fn try_build_vector_params_for_search(
    ctx: RequestContext,
    cortex_dao: &Arc<dyn CortexDao>,
    model_provider_dao: &Arc<dyn ModelProviderDao>,
    text: &str,
) -> Result<Option<VectorIndexParams>> {
    let Some(provider) = model_provider_dao
        .get_default_embedding_provider(ctx.clone())
        .await?
    else {
        return Ok(None);
    };

    let cortex = cortex_dao.create_cortex_trait(ctx.clone(), &provider, vec![])?;
    let params = cortex_dao
        .embed_text_for_search(ctx, cortex.as_ref(), text)
        .await?;
    Ok(Some(params))
}
