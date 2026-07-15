//! Agent DAL 模块

use common::error::{Result, err};
use common::models::{AgentStats, ModelCallStats, StatsFetchOptions, StatsInterval};
use crate::models::agent::{Agent, AgentPo};
use crate::models::brain::Brain;
use crate::models::vector::{MatchType, SearchMatchInfo, Vectorizable};
use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;
use crate::pkg::RequestContext;
use crate::pkg::stats::{ModelCallEvent, AgentAwakeEvent};
use crate::service::dao::agent::{self, AgentDao, AgentQuery, AgentSearch, AgentStatsDao, AgentStatsQuery, AgentVectorDao};
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::model_provider::{ModelProviderDao, ModelProviderStatsDao, ModelProviderStatsQuery};
use common::enums::AgentStatus;
use std::sync::{Arc, OnceLock};

use crate::enrich_ctx;
// ==================== 单例管理 ====================

static AGENT_DAL: OnceLock<Arc<dyn AgentDal>> = OnceLock::new();

/// 获取 Agent DAL 单例
pub fn dal() -> Arc<dyn AgentDal> {
    AGENT_DAL.get().cloned().unwrap()
}

/// 初始化 Agent DAL
pub fn init() {
    agent::stats_init();
    crate::service::dao::model_provider::stats_init();
    let _ = AGENT_DAL.set(new(
        agent::dao(),
        agent::vector_dao(),
        agent::stats_dao(),
        crate::service::dao::model_provider::stats_dao(),
        crate::service::dao::cortex::dao(),
        crate::service::dao::model_provider::dao(),
    ));
}

/// 创建 Agent DAL（返回 trait 对象）
pub fn new(
    agent_dao: Arc<dyn AgentDao + Send + Sync>,
    agent_vector_dao: Arc<dyn AgentVectorDao + Send + Sync>,
    agent_stats_dao: Arc<dyn AgentStatsDao<AwakeEvent = AgentAwakeEvent>>,
    model_provider_stats_dao: Arc<dyn ModelProviderStatsDao<ModelCallEvent = ModelCallEvent>>,
    cortex_dao: Arc<dyn CortexDao + Send + Sync>,
    model_provider_dao: Arc<dyn ModelProviderDao + Send + Sync>,
) -> Arc<dyn AgentDal> {
    Arc::new(AgentDalImpl {
        agent_dao,
        agent_vector_dao,
        agent_stats_dao,
        model_provider_stats_dao,
        cortex_dao,
        model_provider_dao,
    })
}

// ==================== DAL 接口 ====================

/// Agent 附带信息获取选项
#[derive(Debug, Clone, Default)]
pub struct AgentFetchOptions {
    /// 是否加载运行时状态（默认 true）
    pub with_runtime_state: Option<bool>,
    /// 是否加载统计信息（AgentStats: 唤醒次数汇总）
    pub with_stats: Option<bool>,
    /// 是否加载模型调用统计（ModelCallStats: token + 时序）
    pub with_model_call_stats: Option<bool>,
    /// 统计过滤条件（with_stats=true 时生效，按任务 ID 过滤）
    pub stats_task_id: Option<String>,
    /// 统计时间范围（毫秒），None 表示全部历史
    pub stats_time_range: Option<(i64, i64)>,
    /// 时序查询粒度，None 时默认 Daily
    pub stats_interval: Option<StatsInterval>,
}

/// Agent DAL 接口
#[async_trait::async_trait]
pub trait AgentDal: Send + Sync {
    /// 创建 Agent
    async fn create(&self, ctx: RequestContext, agent: &Agent) -> Result<()>;

    /// 根据 ID 查询 Agent
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Agent>>;

    /// 根据 ID 查询 Agent（带附带信息选项）
    async fn get_agent(&self, ctx: RequestContext, id: &str, options: AgentFetchOptions) -> Result<Option<Agent>>;

    /// 通用综合查询
    ///
    /// 支持组合查询条件，所有字段都是 Option
    async fn query(&self, ctx: RequestContext, query: AgentQuery) -> Result<Vec<Agent>>;

    /// 查询所有 Agent
    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<Agent>>;

    /// 🔍 搜索 Agent（关键词 + 向量语义混合搜索）
    ///
    /// 自动根据参数选择搜索策略：
    /// - keyword 存在 → 走 FTS5 全文检索
    /// - query_vector 存在 → 走向量语义搜索
    /// - 两者都有 → 混合搜索，合并结果（三态匹配 + 综合排序）
    async fn search(&self, ctx: RequestContext, search: AgentSearch) -> Result<Vec<Agent>>;

    /// 更新 Agent
    async fn update(&self, ctx: RequestContext, agent: &Agent) -> Result<()>;

    /// 删除 Agent
    async fn delete(&self, ctx: RequestContext, agent: &Agent) -> Result<()>;

    /// 唤醒 Brain
    ///
    /// 直接使用传入的 brain 赋值给 Agent，不负责创建 brain
    /// Brain 已经持有 ModelProvider，可以从中获取 model_provider_id 更新到 Agent po
    ///
    /// 唤醒完成后将 brain 写入 Agent 的 brain 字段
    /// 如果 model_provider_id 发生变化，自动更新数据库
    async fn wake_brain(
        &self,
        ctx: RequestContext,
        agent: &mut Agent,
        brain: Brain,
    ) -> Result<()>;

    // ==================== 统计查询 ====================

    /// 获取 Agent 自身统计数据
    async fn get_stats(&self, ctx: RequestContext, agent_id: &str, options: StatsFetchOptions) -> Result<AgentStats>;

    /// 获取 Agent 维度的模型调用统计
    ///
    /// 由 ModelProviderStatsDao（模型调用领域）负责计算，
    /// 按 agent_id 过滤后返回 ModelCallStats。
    async fn get_model_call_stats(&self, ctx: RequestContext, agent_id: &str, options: StatsFetchOptions) -> Result<ModelCallStats>;
}

/// Agent DAL 实现
struct AgentDalImpl {
    agent_dao: Arc<dyn AgentDao>,
    agent_vector_dao: Arc<dyn AgentVectorDao>,
    agent_stats_dao: Arc<dyn AgentStatsDao<AwakeEvent = AgentAwakeEvent>>,
    model_provider_stats_dao: Arc<dyn ModelProviderStatsDao<ModelCallEvent = ModelCallEvent>>,
    cortex_dao: Arc<dyn CortexDao>,
    model_provider_dao: Arc<dyn ModelProviderDao>,
}

impl AgentDalImpl {
    /// 注入运行时状态到 Agent 实体
    fn inject_runtime_state(agent: Agent) -> Agent {
        let runtime_info = AgentRuntimeStateManager::global()
            .get(&agent.po.id);
        Agent {
            runtime_info,
            ..agent
        }
    }

    /// 自动向量化 Agent（失败 warn 降级，不影响主流程）
    ///
    /// 流程：
    /// 1. 取默认 Embedding ModelProvider；无则跳过（合法场景）
    /// 2. 创建 Cortex（trait 对象）
    /// 3. 调 `embed_entity` 生成完整 VectorIndexParams
    /// 4. upsert 到向量索引（失败 warn 降级）
    async fn upsert_vector_index(&self, ctx: RequestContext, po: &AgentPo) {
        // 1. 取默认 Embedding ModelProvider；无则跳过（合法场景）
        let provider = match self
            .model_provider_dao
            .get_default_embedding_provider(ctx.clone())
            .await
        {
            Ok(Some(p)) => p,
            Ok(None) => {
                log_debug!(
                    &ctx,
                    "vector_index",
                    agent_id = %po.id,
                    "无可用 Embedding Provider，跳过向量索引"
                );
                return;
            }
            Err(e) => {
                log_warn!(
                    &ctx,
                    "vector_index",
                    agent_id = %po.id,
                    error = ?e,
                    "Agent 查询 Embedding Provider 失败，跳过向量化"
                );
                return;
            }
        };

        // 2. 创建 Cortex（trait 对象）
        let cortex = match self
            .cortex_dao
            .create_cortex_trait(ctx.clone(), &provider, vec![])
        {
            Ok(c) => c,
            Err(e) => {
                log_warn!(
                    &ctx,
                    "vector_index",
                    agent_id = %po.id,
                    error = ?e,
                    "Agent 创建 Cortex 失败，跳过向量化"
                );
                return;
            }
        };

        // 3. 调 `embed_entity` 生成完整 VectorIndexParams
        // 4. upsert 到向量索引（失败 warn 降级）
        match self
            .cortex_dao
            .embed_entity(ctx.clone(), cortex.as_ref(), po)
            .await
        {
            Ok(vec_params) => {
                if let Err(e) = self
                    .agent_vector_dao
                    .upsert_vector(ctx.clone(), &po.id, &vec_params)
                    .await
                {
                    log_warn!(
                        &ctx,
                        "vector_index",
                        agent_id = %po.id,
                        error = ?e,
                        "Agent 向量索引写入失败，已降级（可能 vss0 扩展未安装）"
                    );
                }
            }
            Err(e) => {
                log_warn!(
                    &ctx,
                    "vector_index",
                    agent_id = %po.id,
                    error = ?e,
                    "Agent 向量化失败，已降级"
                );
            }
        }
    }

    /// 尝试为查询文本构建向量索引参数（用于搜索场景）
    ///
    /// 任何中间步骤失败都会向上抛错；调用方决定是否 warn 降级。
    /// 返回 `Ok(None)` 表示无 Embedding Provider 配置（合法场景）。
    async fn try_build_vector_params_for_search(
        &self,
        ctx: RequestContext,
        text: &str,
    ) -> Result<Option<crate::models::vector::VectorIndexParams>> {
        let Some(provider) = self
            .model_provider_dao
            .get_default_embedding_provider(ctx.clone())
            .await?
        else {
            return Ok(None);
        };

        let cortex = self
            .cortex_dao
            .create_cortex_trait(ctx.clone(), &provider, vec![])?;
        let params = self
            .cortex_dao
            .embed_text_for_search(ctx, cortex.as_ref(), text)
            .await?;
        Ok(Some(params))
    }
}

#[async_trait::async_trait]
impl AgentDal for AgentDalImpl {
    async fn create(&self, ctx: RequestContext, agent: &Agent) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, agent);
        // 1. 写入基础数据
        self.agent_dao.insert(ctx.clone(), &agent.po).await?;

        // 2. 自动向量化（失败 warn 降级，不影响主流程）
        self.upsert_vector_index(ctx, &agent.po).await;

        Ok(())
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Agent>> {
        let opt = self.agent_dao.find_by_id(ctx, id).await?;
        Ok(opt.map(Agent::from_po).map(Self::inject_runtime_state))
    }

    async fn get_agent(&self, ctx: RequestContext, id: &str, options: AgentFetchOptions) -> Result<Option<Agent>> {
        let opt = self.agent_dao.find_by_id(ctx.clone(), id).await?;
        let Some(mut agent) = opt.map(Agent::from_po) else {
            return Ok(None);
        };

        let with_runtime = options.with_runtime_state.unwrap_or(true);
        if with_runtime {
            agent = Self::inject_runtime_state(agent);
        }

        if options.with_stats.unwrap_or(false) {
            let stats_options = StatsFetchOptions {
                with_call_summary: true,
                with_token_summary: false,
                with_time_series: false,
                time_range: options.stats_time_range,
                interval: options.stats_interval,
            };
            let query = AgentStatsQuery {
                agent_id: id.to_string(),
                task_id: options.stats_task_id.clone(),
                time_range: stats_options.time_range,
                ..Default::default()
            };
            let stats = self.agent_stats_dao.get_stats(ctx.clone(), query, stats_options).await?;
            agent.stats = Some(stats);
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
            agent.model_call_stats = Some(model_call_stats);
        }

        Ok(Some(agent))
    }

    async fn query(&self, ctx: RequestContext, query: AgentQuery) -> Result<Vec<Agent>> {
        let agents = self.agent_dao.query(ctx, query).await?;
        Ok(agents.into_iter().map(Agent::from_po).map(Self::inject_runtime_state).collect())
    }

    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<Agent>> {
        self.query(
            ctx,
            AgentQuery {
                exclude_status: Some(AgentStatus::Deleted),
                ..Default::default()
            },
        )
        .await
    }

    async fn search(&self, ctx: RequestContext, search: AgentSearch) -> Result<Vec<Agent>> {
        // 默认排除软删除的 Agent（遵循项目软删除约定：status=0 视为已删除）
        let mut search = search;
        if search.filters.exclude_status.is_none() {
            search.filters.exclude_status = Some(AgentStatus::Deleted);
        }

        // 向量距离阈值（可配置，默认 0.8）
        let vector_distance_threshold = search.vector_distance_threshold.unwrap_or(0.8);

        // Step 1: 准备向量搜索结果容器
        let mut vector_scores: std::collections::HashMap<String, f32> =
            std::collections::HashMap::new();
        let mut vector_agent_ids: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        // Step 2: 如果有关键词，尝试向量搜索
        if search.keyword.is_some() {
            if let Some(keyword) = &search.keyword {
                match self.try_build_vector_params_for_search(ctx.clone(), keyword).await {
                    Ok(Some(vec_params)) => {
                        // 向量搜索（前 50 条）
                        match self
                            .agent_vector_dao
                            .search_vector(ctx.clone(), &vec_params.vector, 50)
                            .await
                        {
                            Ok(vector_results) => {
                                // 过滤距离小于阈值的结果
                                let filtered_results: Vec<(String, f32)> = vector_results
                                    .into_iter()
                                    .filter(|hit| hit.distance < vector_distance_threshold)
                                    .map(|hit| (hit.row.id, hit.distance))
                                    .collect();

                                vector_agent_ids =
                                    filtered_results.iter().map(|(id, _)| id.clone()).collect();
                                vector_scores = filtered_results.into_iter().collect();
                            }
                            Err(e) => {
                                log_warn!(
                                    &ctx,
                                    "vector_search",
                                    "Agent 向量搜索失败，降级到关键词搜索: {}",
                                    e
                                );
                            }
                        }
                    }
                    Ok(None) => {
                        log_debug!(
                            &ctx,
                            "vector_search",
                            "无可用 Embedding Provider，跳过向量搜索"
                        );
                    }
                    Err(e) => {
                        log_warn!(&ctx, "vector_search", error = ?e, "Agent 向量化失败，跳过向量搜索");
                    }
                }
            }
        }

        // Step 3: 执行 FTS5 关键词搜索（DAO 返回 Vec<(Po, fts_rank)>）
        let keyword_results = self.agent_dao.search_agents(ctx.clone(), search.clone()).await?;

        // 提取 fts_rank 并转换为 Vec<Po> 便于聚合
        let mut fts_ranks: std::collections::HashMap<String, f32> =
            std::collections::HashMap::new();
        let keyword_pos: Vec<_> = keyword_results
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

        if !vector_agent_ids.is_empty() {
            let ids_to_fetch: Vec<String> = vector_agent_ids
                .into_iter()
                .filter(|id| !keyword_pos.iter().any(|po| po.id == *id))
                .collect();

            if !ids_to_fetch.is_empty() {
                let mut ids_to_fetch = ids_to_fetch;
                ids_to_fetch.sort();
                ids_to_fetch.dedup();

                for chunk in ids_to_fetch.chunks(20) {
                    let chunk_ids: Vec<String> = chunk.to_vec();
                    let chunk_query = AgentQuery {
                        ids: Some(chunk_ids),
                        exclude_status: Some(AgentStatus::Deleted),
                        ..Default::default()
                    };
                    let chunk_pos = self.agent_dao.query(ctx.clone(), chunk_query).await?;
                    all_pos.extend(chunk_pos);
                }
            }
        }

        // Step 5: 去重
        all_pos.sort_by(|a, b| a.id.cmp(&b.id));
        all_pos.dedup_by(|a, b| a.id == b.id);

        // Step 6: 构建 Agent 对象，附加 SearchMatchInfo（三态匹配）
        let mut agents = Vec::with_capacity(all_pos.len());
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
            let mut agent = Agent::from_po(po);
            agent.search_match = match_info;
            agents.push(agent);
        }

        // Step 7: 综合排序（Hybrid 优先 → Vector 次之 → Keyword/None 最后）
        //    组内排序：Hybrid/Vector 按向量距离升序，Keyword 按 fts_rank 升序（BM25 越小越相关）
        agents.sort_by(|a, b| {
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
        if let Some(limit) = search.filters.limit {
            agents.truncate(limit);
        }

        Ok(agents)
    }

    async fn update(&self, ctx: RequestContext, agent: &Agent) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, agent);
        // 1. 更新基础数据
        self.agent_dao.update(ctx.clone(), &agent.po).await?;

        // 2. 检查内容是否变化，变化则重新向量化
        let old_hash = self
            .agent_vector_dao
            .get_vector_row(ctx.clone(), &agent.po.id)
            .await?
            .map(|r| r.meta.content_hash);

        let content = agent.po.vectorize_text();
        let new_hash = sha256::digest(&content);

        if old_hash.as_deref() != Some(&new_hash) {
            self.upsert_vector_index(ctx, &agent.po).await;
        }

        Ok(())
    }

    async fn delete(&self, ctx: RequestContext, agent: &Agent) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, agent);
        // 1. 软删除基础数据
        self.agent_dao.delete(ctx.clone(), &agent.po).await?;

        // 2. 删除向量索引（忽略失败，不影响主流程）
        if let Err(e) = self
            .agent_vector_dao
            .delete_vector(ctx.clone(), &agent.po.id)
            .await
        {
            log_warn!(ctx, "vector_index", agent_id= %agent.po.id, error = ?e, "Agent 向量索引删除失败，已降级");
        }

        Ok(())
    }

    async fn wake_brain(
        &self,
        ctx: RequestContext,
        agent: &mut Agent,
        brain: Brain,
    ) -> Result<()> {
        // 1. 从 Brain 中获取 Cortex，Cortex 持有 ModelProvider，从中获取 model_provider_id
        let model_provider_id = brain.cortex().model_provider.po.id.clone();

        // 2. 如果 model_provider_id 发生变化，更新 Agent po 中的 model_provider_id
        let need_update = agent.po.model_provider_id != model_provider_id;

        if need_update {
            agent.po.model_provider_id = model_provider_id;
        }
        // 3. 直接使用传入的 brain 赋值给 Agent
        agent.set_brain(brain);

        // 4. 如果我们更新了 model_provider_id，需要更新数据库
        if need_update {
            let ctx = enrich_ctx!(&ctx, &*agent);
            self.update(ctx, agent).await?;
        }

        Ok(())
    }

    // ==================== 统计查询 ====================

    async fn get_stats(&self, ctx: RequestContext, agent_id: &str, options: StatsFetchOptions) -> Result<AgentStats> {
        let query = AgentStatsQuery {
            agent_id: agent_id.to_string(),
            time_range: options.time_range,
            ..Default::default()
        };
        self.agent_stats_dao.get_stats(ctx, query, options).await
    }

    async fn get_model_call_stats(&self, ctx: RequestContext, agent_id: &str, options: StatsFetchOptions) -> Result<ModelCallStats> {
        let query = ModelProviderStatsQuery {
            agent_id: Some(agent_id.to_string()),
            time_range: options.time_range,
            interval: options.interval,
            ..Default::default()
        };
        self.model_provider_stats_dao.get_stats(ctx, query, options).await
    }
}