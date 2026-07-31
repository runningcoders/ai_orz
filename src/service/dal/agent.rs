//! Agent DAL 模块

use crate::models::agent::{Agent, AgentPo};
use crate::models::brain::Brain;
use crate::models::memory::Memory;
use crate::models::message::Message;
use crate::models::skill::SkillPo;
use crate::models::user::UserPo;
use crate::models::vector::{MatchType, SearchMatchInfo, Vectorizable};
use crate::pkg::RequestContext;
use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;
use crate::pkg::stats::{AgentAwakeEvent, ModelCallEvent, ToolCallEvent};
use crate::service::dao::agent::{
    self, AgentDao, AgentQuery, AgentSearch, AgentStatsDao, AgentStatsQuery, AgentVectorDao,
};
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::model_provider::{
    ModelProviderDao, ModelProviderStatsDao, ModelProviderStatsQuery,
};
use crate::service::dao::tool::ToolStatsDao;
use common::enums::AgentStatus;
use common::enums::ControlMode;
use common::error::Result;
use common::models::{
    AgentStats, ModelCallStats, StatsFetchOptions, StatsInterval, ToolCallSummary,
};
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
    crate::service::dao::tool::stats_init();
    crate::service::dao::model_provider::stats_init();
    let _ = AGENT_DAL.set(new(
        agent::dao(),
        agent::vector_dao(),
        agent::stats_dao(),
        crate::service::dao::tool::stats_dao(),
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
    tool_stats_dao: Arc<dyn ToolStatsDao<ToolCallEvent = ToolCallEvent>>,
    model_provider_stats_dao: Arc<dyn ModelProviderStatsDao<ModelCallEvent = ModelCallEvent>>,
    cortex_dao: Arc<dyn CortexDao + Send + Sync>,
    model_provider_dao: Arc<dyn ModelProviderDao + Send + Sync>,
) -> Arc<dyn AgentDal> {
    Arc::new(AgentDalImpl {
        agent_dao,
        agent_vector_dao,
        agent_stats_dao,
        tool_stats_dao,
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
    /// 是否加载 Agent 绑定的工具（绑定工具 + tag 匹配的内置工具）
    pub with_tools: Option<bool>,
    /// 是否加载 Agent 已安装的技能副本（author_id = agent_id，排除 Expired）
    pub with_skills: Option<bool>,
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
    async fn get_agent(
        &self,
        ctx: RequestContext,
        id: &str,
        options: AgentFetchOptions,
    ) -> Result<Option<Agent>>;

    /// 通用综合查询
    ///
    /// 支持组合查询条件，所有字段都是 Option
    async fn query(
        &self,
        ctx: RequestContext,
        query: AgentQuery,
    ) -> Result<common::api::PagedResult<Agent>>;

    /// 统计符合查询条件的 Agent 数量（透传 DAO count）
    async fn count(&self, ctx: RequestContext, query: AgentQuery) -> Result<u64>;

    /// 查询所有 Agent
    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<Agent>>;

    /// 🔍 搜索 Agent（关键词 + 向量语义混合搜索）
    ///
    /// 自动根据参数选择搜索策略：
    /// - keyword 存在 → 走 FTS5 全文检索
    /// - query_vector 存在 → 走向量语义搜索
    /// - 两者都有 → 混合搜索，合并结果（三态匹配 + 综合排序）
    ///
    /// 返回分页结果，支持 runtime_state 内存过滤。
    async fn search(
        &self,
        ctx: RequestContext,
        search: AgentSearch,
    ) -> Result<common::api::PagedResult<Agent>>;

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
    async fn wake_brain(&self, ctx: RequestContext, agent: &mut Agent, brain: Brain) -> Result<()>;

    // ==================== 统计查询 ====================

    /// 获取 Agent 自身统计数据
    async fn get_stats(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        options: StatsFetchOptions,
    ) -> Result<AgentStats>;

    /// 获取 Agent 维度的模型调用统计
    ///
    /// 由 ModelProviderStatsDao（模型调用领域）负责计算，
    /// 按 agent_id 过滤后返回 ModelCallStats。
    async fn get_model_call_stats(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        options: StatsFetchOptions,
    ) -> Result<ModelCallStats>;

    /// 🔄 重建所有 Agent 的向量索引
    ///
    /// 清空向量集合后，查询全量 Agent，逐条重新生成 embedding 并 upsert。
    /// 单条失败不影响整体，用 log_warn! 记录。
    async fn rebuild_vectors(&self, ctx: RequestContext) -> Result<()>;

    /// 返回该 Dal 对应 Agent 类型的 PromptBuilder
    ///
    /// 基础实现返回 DefaultPromptBuilder（Local Agent 使用）。
    /// 派生 Dal（CodexAgentDal/A2aAgentDal）可重写此方法返回专属 builder。
    fn prompt_builder(&self) -> Box<dyn crate::models::prompt_builder::PromptBuilder> {
        Box::new(DefaultPromptBuilder::new())
    }
}

/// Agent DAL 实现
struct AgentDalImpl {
    agent_dao: Arc<dyn AgentDao>,
    agent_vector_dao: Arc<dyn AgentVectorDao>,
    agent_stats_dao: Arc<dyn AgentStatsDao<AwakeEvent = AgentAwakeEvent>>,
    tool_stats_dao: Arc<dyn ToolStatsDao<ToolCallEvent = ToolCallEvent>>,
    model_provider_stats_dao: Arc<dyn ModelProviderStatsDao<ModelCallEvent = ModelCallEvent>>,
    cortex_dao: Arc<dyn CortexDao>,
    model_provider_dao: Arc<dyn ModelProviderDao>,
}

impl AgentDalImpl {
    /// 注入运行时状态到 Agent 实体
    fn inject_runtime_state(agent: Agent) -> Agent {
        let runtime_info = AgentRuntimeStateManager::global().get(&agent.po.id);
        Agent {
            runtime_info,
            ..agent
        }
    }

    /// 对已加载的 Agent 列表应用 runtime_state 内存过滤 + 分页
    ///
    /// runtime_state 是内存态（AgentRuntimeStateManager），DAO 层无法 SQL 过滤。
    /// 此方法在 DAL 层统一处理：注入 runtime_info → 按目标状态过滤 → 手动分页。
    /// query 和 search 方法复用此逻辑。
    fn apply_runtime_state_filter(
        agents: Vec<Agent>,
        target_state: common::enums::AgentRuntimeState,
        pagination: common::api::PaginationParams,
    ) -> common::api::PagedResult<Agent> {
        let filtered: Vec<Agent> = agents
            .into_iter()
            .filter(|agent| {
                let state = agent
                    .runtime_info
                    .as_ref()
                    .map(|info| info.state)
                    .unwrap_or(common::enums::AgentRuntimeState::Idle);
                state == target_state
            })
            .collect();
        let total = filtered.len();
        let offset = pagination.offset.unwrap_or(0);
        let limit = pagination.limit.unwrap_or(20);
        let items = filtered.into_iter().skip(offset).take(limit).collect();
        common::api::PagedResult { items, total }
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

    async fn get_agent(
        &self,
        ctx: RequestContext,
        id: &str,
        options: AgentFetchOptions,
    ) -> Result<Option<Agent>> {
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
            // stats 查询失败不阻塞 agent 加载
            // 修复：DuckDB 查询失败时整个 get_agent 失败触发 nack 重试，
            // 但重试也无法修复 stats 问题，反而阻塞消息消费
            match self
                .agent_stats_dao
                .get_stats(ctx.clone(), query, stats_options)
                .await
            {
                Ok(stats) => agent.stats = Some(stats),
                Err(e) => {
                    log_warn!(
                        &ctx,
                        "get_agent",
                        "stats query failed, skip depth check: {}",
                        e
                    );
                    // stats 保持 None，consumer 的 thinking_depth 检查会跳过
                }
            }
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
            agent.model_call_stats = Some(model_call_stats);
        }

        Ok(Some(agent))
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: AgentQuery,
    ) -> Result<common::api::PagedResult<Agent>> {
        // runtime_state 是内存态，DAO 层无法过滤。需内存过滤时查全量再手动分页。
        let runtime_state_filter = query.runtime_state;
        if let Some(target_state) = runtime_state_filter {
            let original_pagination = query.pagination.clone();
            let mut full_query = query;
            full_query.runtime_state = None;
            full_query.pagination = common::api::PaginationParams::default();

            let page = self.agent_dao.query(ctx, full_query).await?;
            let all_agents: Vec<Agent> = page
                .items
                .into_iter()
                .map(Agent::from_po)
                .map(Self::inject_runtime_state)
                .collect();

            return Ok(Self::apply_runtime_state_filter(
                all_agents,
                target_state,
                original_pagination,
            ));
        }

        let page = self.agent_dao.query(ctx, query).await?;
        Ok(page.map(Agent::from_po).map(Self::inject_runtime_state))
    }

    async fn count(&self, ctx: RequestContext, query: AgentQuery) -> Result<u64> {
        self.agent_dao.count(ctx, query).await
    }

    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<Agent>> {
        let page = self
            .query(
                ctx,
                AgentQuery {
                    exclude_status: Some(AgentStatus::Deleted),
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    async fn search(
        &self,
        ctx: RequestContext,
        search: AgentSearch,
    ) -> Result<common::api::PagedResult<Agent>> {
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
        if search.keyword.is_some()
            && let Some(keyword) = &search.keyword
        {
            match self
                .try_build_vector_params_for_search(ctx.clone(), keyword)
                .await
            {
                Ok(Some(vec_params)) => {
                    // 向量搜索（前 MAX_SEARCH_RESULTS 条，与 FTS5 限制一致）
                    match self
                        .agent_vector_dao
                        .search_vector(ctx.clone(), &vec_params.vector, 20)
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

        // Step 3: 执行 FTS5 关键词搜索（DAO 返回 Vec<(Po, fts_rank)>）
        let keyword_results = self
            .agent_dao
            .search_agents(ctx.clone(), search.clone())
            .await?;

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
                    all_pos.extend(chunk_pos.items);
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
            // 注入 runtime_info（search 结果也需要 runtime_state 供过滤和展示）
            agent = Self::inject_runtime_state(agent);
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

        // Step 8: 截断到 MAX_SEARCH_RESULTS + runtime_state 内存过滤 + 分页
        // 搜索场景限制总结果数（MAX_SEARCH_RESULTS=20），搜不到应换关键词而非无限分页
        agents.truncate(20);

        let runtime_state_filter = search.filters.runtime_state;
        let pagination = search.filters.pagination.clone();
        let result = if let Some(target_state) = runtime_state_filter {
            Self::apply_runtime_state_filter(agents, target_state, pagination)
        } else {
            // 无 runtime_state 过滤，直接分页（total 最大为 MAX_SEARCH_RESULTS）
            let total = agents.len();
            let offset = pagination.offset.unwrap_or(0);
            let limit = pagination.limit.unwrap_or(20);
            let items = agents.into_iter().skip(offset).take(limit).collect();
            common::api::PagedResult { items, total }
        };

        Ok(result)
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

    async fn wake_brain(&self, ctx: RequestContext, agent: &mut Agent, brain: Brain) -> Result<()> {
        let mut need_update = false;

        if brain.is_local()
            && let Some(cortex) = brain.cortex()
        {
            let model_provider_id = cortex.model_provider.po.id.clone();
            if agent.po.model_provider_id != model_provider_id {
                agent.po.model_provider_id = model_provider_id;
                need_update = true;
            }
        }

        agent.set_brain(brain);

        if need_update {
            let ctx = enrich_ctx!(&ctx, &*agent);
            self.update(ctx, agent).await?;
        }

        Ok(())
    }

    // ==================== 统计查询 ====================

    async fn get_stats(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        options: StatsFetchOptions,
    ) -> Result<AgentStats> {
        // 提前取出 time_range，避免 options 被 move 后无法访问
        let time_range = options.time_range;
        let query = AgentStatsQuery {
            agent_id: agent_id.to_string(),
            time_range,
            ..Default::default()
        };
        let mut stats = self
            .agent_stats_dao
            .get_stats(ctx.clone(), query, options)
            .await?;

        // 同步填充 tool_call_summary（复用 with_call_summary 开关，避免新增 StatsFetchOptions 字段）
        // 失败时 warn 降级，不阻塞主流程
        if stats.call_summary.is_some() {
            match self
                .tool_stats_dao
                .sum_calls_by_tool(ctx.clone(), agent_id, time_range)
                .await
            {
                Ok(by_tool) if !by_tool.is_empty() => {
                    let total_calls: u64 = by_tool.iter().map(|c| c.count).sum();
                    stats.tool_call_summary = Some(ToolCallSummary {
                        total_calls,
                        by_tool,
                    });
                }
                Ok(_) => {
                    // 无工具调用记录，保持 None
                }
                Err(e) => {
                    log_warn!(
                        &ctx,
                        "get_stats",
                        agent_id = %agent_id,
                        error = ?e,
                        "tool_call_summary 查询失败，已降级"
                    );
                }
            }
        }

        Ok(stats)
    }

    async fn get_model_call_stats(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        options: StatsFetchOptions,
    ) -> Result<ModelCallStats> {
        let query = ModelProviderStatsQuery {
            agent_id: Some(agent_id.to_string()),
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
        let current_provider_id = provider.id;

        // 2. 检查集合元数据：model_provider_id 一致则跳过重建
        let collection_name = "agents";
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
        self.agent_vector_dao.clear_collection(ctx.clone()).await?;
        let agents = self.find_all(ctx.clone()).await?;
        for agent in &agents {
            self.upsert_vector_index(ctx.clone(), &agent.po).await;
        }

        // 4. 更新元数据
        ctx.vector_store()
            .set_collection_model_provider_id(collection_name, &current_provider_id)
            .await?;

        Ok(())
    }
}

// ==================== Prompt Builder（Local Agent 默认实现） ====================

/// neural tag 常量：标记为神经级别的工具/技能，所有 Agent 必加载
const NEURAL_TAG: &str = "neural";

/// 默认 Prompt 构建器（Local Agent 使用）
///
/// 统一注入 tools / skills，build() 时按 tag 自动分块拼装：
///
/// 1. 【Agent 人设】        ← 最稳定
/// 2. 【神经工具】          ← tags 含 "neural"，所有 Agent 必加载
/// 3. 【神经技能】          ← tags 含 "neural"，所有 Agent 必加载
/// 4. 【常用工具】          ← tags 不含 "neural" 但与 agent match_keys 有交集
/// 5. 【必加载技能】        ← tags 不含 "neural" 但与 agent match_keys 有交集
/// 6. 【用户画像】          ← 随用户变化，对话中相对稳定
/// 7. 【项目上下文】+【任务上下文】 ← 业务上下文，随消息变化
/// 8. 【历史对话】          ← 随对话增长
/// 9. 【工具失败警告】      ← 实时变化
/// 10. 【trace_id + 当前消息】← 每次变化
///
/// match_keys = agent.roles ∪ agent.installed_tags
///
/// build_sleep_prompt() 与 build() 对称，复用 1-8 区块（跳过 tool_failures 和 current_message），
/// 加上沉淀约束章节 + 待沉淀记忆摘要，用于 sleep_and_settle 场景。
#[derive(Debug, Clone, Default)]
pub struct DefaultPromptBuilder {
    /// 本次思考的 Trace ID
    current_trace_id: Option<String>,
    /// Agent 人设 / System Prompt
    system_prompt: Option<String>,
    /// 匹配键：agent.roles ∪ agent.installed_tags（system_prompt 时缓存）
    match_keys: Vec<String>,
    /// 用户画像信息（仅客服类 Agent 使用）
    user_profile: Option<String>,
    /// 项目上下文摘要（消息关联的项目实体，有值即拼装）
    project_context: Option<String>,
    /// 任务上下文摘要（消息关联的任务实体，有值即拼装）
    task_context: Option<String>,
    /// 历史对话记忆
    history: Vec<String>,
    /// 当前用户消息
    current_message: Option<String>,
    /// 技能（全量，build 时按 tag 分块）
    skills: Vec<SkillPo>,
    /// 工具 PO（全量，build 时按 tag 分块）
    tools: Vec<crate::models::tool::ToolPo>,
    /// 工具失败统计：(工具名称, 失败次数)
    tool_failures: Vec<(String, u64)>,
}

impl DefaultPromptBuilder {
    /// 创建空的 Builder
    pub fn new() -> Self {
        Self::default()
    }

    /// 工具是否为神经工具（tags 含 "neural"）
    fn is_neural_tool(tool: &crate::models::tool::ToolPo) -> bool {
        tool.get_tags().iter().any(|t| t == NEURAL_TAG)
    }

    /// 工具是否适合在 Prompt 中展示（仅 Manual 工具）
    /// Auto 工具通过 Rig 注册由模型直接调用，不需要在 Prompt 中展示
    /// Enabled 状态已在 DB 查询时过滤
    fn is_prompt_visible_tool(tool: &crate::models::tool::ToolPo) -> bool {
        matches!(tool.control_mode, ControlMode::Manual)
    }

    /// 技能是否为神经技能（tags 含 "neural"）
    fn is_neural_skill(skill: &SkillPo) -> bool {
        skill.parse_tags().iter().any(|t| t == NEURAL_TAG)
    }

    /// 工具/技能的 tags 是否与 match_keys 有交集
    fn tags_match(tags: &[String], match_keys: &[String]) -> bool {
        tags.iter().any(|t| match_keys.contains(t))
    }

    /// 构建工具区块字符串
    fn build_tools_section(title: &str, tools: &[&crate::models::tool::ToolPo]) -> String {
        if tools.is_empty() {
            return String::new();
        }
        let mut s = format!("【{}】\n", title);
        s.push_str(
            "以下为 Manual 工具（已注册到 Rig 的 Auto 工具不在此列出，直接通过 function calling 调用）。Manual 工具有两种调用方式，请按场景选择：\n\
             - 同步调用（request_tool_call）：结果在当前轮立即返回，适合轻量、快速的工具\n\
             - 异步调用（send_tool_call_message）：结果在下一轮通过 ToolCallResult 消息送达，适合耗时较长的工具\n",
        );
        for t in tools {
            s.push_str(&format!("- {}\n", t.to_tool_prompt()));
        }
        s.push('\n');
        s
    }

    /// 构建技能区块字符串
    fn build_skills_section(title: &str, skills: &[&SkillPo]) -> String {
        if skills.is_empty() {
            return String::new();
        }
        let mut s = format!("【{}】\n", title);
        for skill in skills {
            s.push_str(&skill.to_prompt_summary());
            s.push('\n');
        }
        s.push('\n');
        s
    }

    /// 构建工具/技能区块（神经工具 + 神经技能 + 常用工具 + 必加载技能）
    ///
    /// 提取自原 build()，供 build() 和 build_sleep_prompt() 复用，避免重复。
    /// 预过滤：仅展示 Manual 工具（Auto 工具走 Rig 不在 Prompt 展示）。
    fn build_tools_and_skills_sections(&self) -> String {
        let mut result = String::new();

        // 神经工具（tags 含 "neural"，所有 Agent 必加载）
        let neural_tools: Vec<_> = self
            .tools
            .iter()
            .filter(|t| Self::is_prompt_visible_tool(t) && Self::is_neural_tool(t))
            .collect();
        result.push_str(&Self::build_tools_section("神经工具", &neural_tools));

        // 神经技能（tags 含 "neural"，所有 Agent 必加载）
        let neural_skills: Vec<_> = self
            .skills
            .iter()
            .filter(|s| Self::is_neural_skill(s))
            .collect();
        result.push_str(&Self::build_skills_section("神经技能", &neural_skills));

        // 常用工具（tags 不含 "neural" 但与 match_keys 有交集）
        let tagged_tools: Vec<_> = self
            .tools
            .iter()
            .filter(|t| {
                Self::is_prompt_visible_tool(t) && {
                    let tags = t.get_tags();
                    !tags.iter().any(|tag| tag == NEURAL_TAG)
                        && Self::tags_match(&tags, &self.match_keys)
                }
            })
            .collect();
        result.push_str(&Self::build_tools_section("常用工具", &tagged_tools));

        // 必加载技能（tags 不含 "neural" 但与 match_keys 有交集）
        let tagged_skills: Vec<_> = self
            .skills
            .iter()
            .filter(|s| {
                let tags = s.parse_tags();
                !tags.iter().any(|tag| tag == NEURAL_TAG)
                    && Self::tags_match(&tags, &self.match_keys)
            })
            .collect();
        result.push_str(&Self::build_skills_section("必加载技能", &tagged_skills));

        result
    }

    /// 构建通用上下文区块：用户画像 + 项目上下文 + 任务上下文
    ///
    /// 这些字段都是"有值即拼装"，唤醒和沉睡场景逻辑一致：
    /// - user_profile：认知是具身的，Agent 需知道"自己是谁"
    /// - project_context / task_context：场景化上下文，沉淀出的经验自带场景标签
    fn build_common_context_sections(&self) -> String {
        let mut s = String::new();
        if let Some(profile) = &self.user_profile {
            s.push_str("【用户画像】\n");
            s.push_str(profile);
            s.push_str("\n\n");
        }
        if let Some(project) = &self.project_context {
            s.push_str(project);
            s.push('\n');
        }
        if let Some(task) = &self.task_context {
            s.push_str(task);
            s.push('\n');
        }
        s
    }
}

/// 实现 PromptBuilder trait
impl crate::models::prompt_builder::PromptBuilder for DefaultPromptBuilder {
    fn current_trace_id(&mut self, trace_id: &str) {
        self.current_trace_id = Some(trace_id.to_string());
    }

    fn system_prompt(&mut self, agent: &Agent) {
        self.system_prompt = Some(agent.to_system_prompt());
        // 缓存匹配键：roles ∪ installed_tags
        let mut keys = agent.po.get_roles();
        keys.extend(agent.po.get_installed_tags());
        keys.sort();
        keys.dedup();
        self.match_keys = keys;
    }

    fn history(&mut self, memories: &[Memory]) {
        for memory in memories {
            if let Some(summary) = memory.to_prompt_summary() {
                self.history.push(summary);
            }
        }
    }

    fn current_message(&mut self, message: &Message) {
        let label = match message.po.message_type {
            common::enums::MessageType::ToolCallResult => "【工具执行结果】",
            common::enums::MessageType::ToolCallRequest => "【工具调用请求】",
            common::enums::MessageType::ConfirmRequest => "【确认请求】",
            common::enums::MessageType::ConfirmResponse => "【确认回复】",
            common::enums::MessageType::TaskAssignment => "【任务分配通知】",
            _ => "【当前消息】",
        };
        self.current_message = Some(format!("{}\n{}", label, message.to_prompt()));
    }

    fn skills(&mut self, skills: &[SkillPo]) {
        self.skills.extend_from_slice(skills);
    }

    fn tools(&mut self, tools: &[crate::models::tool::ToolPo]) {
        self.tools.extend_from_slice(tools);
    }

    fn tool_failures(&mut self, failures: &[(String, u64)]) {
        self.tool_failures.extend_from_slice(failures);
    }

    fn user_profile(&mut self, user: &UserPo) {
        self.user_profile = Some(user.to_basic_info_prompt());
    }

    fn project_context(&mut self, project: &crate::models::project::Project) {
        self.project_context = Some(project.to_prompt_summary());
    }

    fn task_context(&mut self, task: &crate::models::task::Task) {
        self.task_context = Some(task.to_prompt_summary());
    }

    fn build(&self) -> String {
        let mut result = String::new();

        // 1. System Prompt（Agent 人设）
        if let Some(system) = &self.system_prompt {
            result.push_str(system);
            result.push_str("\n\n");
        }

        // 2-5. 工具/技能区块（神经工具/神经技能/常用工具/必加载技能）
        result.push_str(&self.build_tools_and_skills_sections());

        // 6-7. 通用上下文区块（用户画像 + 项目上下文 + 任务上下文，有值即拼装）
        result.push_str(&self.build_common_context_sections());

        // 8. 历史对话记忆
        if !self.history.is_empty() {
            result.push_str("【历史对话】\n");
            for h in &self.history {
                result.push_str(h);
                result.push('\n');
            }
            result.push('\n');
        }

        // 9. 工具失败警告（有失败工具时才显示）
        if !self.tool_failures.is_empty() {
            result.push_str("【工具失败警告】\n");
            result.push_str("以下工具近期失败次数较多，请谨慎使用或考虑替代方案：\n");
            for (tool_name, fail_count) in &self.tool_failures {
                result.push_str(&format!("- {}：失败 {} 次\n", tool_name, fail_count));
            }
            result.push('\n');
        }

        // 10. 本次思考的 Trace ID + 当前用户消息
        if let Some(trace_id) = &self.current_trace_id {
            result.push_str(&format!("【思考 Trace ID】{}\n\n", trace_id));
        }
        if let Some(msg) = &self.current_message {
            result.push_str(msg);
            result.push_str("\n\n请回复：");
        }

        result
    }

    fn build_sleep_prompt(&self, pending_memories_summary: &str) -> String {
        let mut result = String::new();

        // 1. System Prompt（Agent 人设）
        if let Some(system) = &self.system_prompt {
            result.push_str(system);
            result.push_str("\n\n");
        }

        // 2-5. 工具/技能区块（sleep_and_settle 调用前已过滤只保留记忆相关）
        result.push_str(&self.build_tools_and_skills_sections());

        // 6-7. 通用上下文区块（用户画像 + 项目上下文 + 任务上下文）
        // 认知是具身的 → 保留 user_profile
        // 场景化沉淀 → 保留 project/task_context，沉淀出的经验自带场景标签
        result.push_str(&self.build_common_context_sections());

        // 8. 历史对话记忆
        if !self.history.is_empty() {
            result.push_str("【历史对话】\n");
            for h in &self.history {
                result.push_str(h);
                result.push('\n');
            }
            result.push('\n');
        }

        // 9. Trace ID
        if let Some(trace_id) = &self.current_trace_id {
            result.push_str(&format!("【思考 Trace ID】{}\n\n", trace_id));
        }

        // 10. 沉淀约束 + 待沉淀记忆 + 任务步骤（模板内聚在 builder）
        // 跳过 tool_failures（沉淀不调外部工具）和 current_message（沉淀无用户消息）
        result.push_str("【沉淀工作模式触发】\n\n");
        result.push_str("你收到这个消息是因为触发了沉淀流程（类似人脑的睡眠整理记忆）。请进入沉淀工作模式，对以下未沉淀的短期记忆进行归纳整理：\n\n");
        result.push_str(&format!(
            "## 待沉淀的短期记忆\n{}\n\n",
            pending_memories_summary
        ));
        result.push_str("## 沉淀约束（重要）\n\n");
        result.push_str("- **不要发送消息**：睡觉是对自身知识的沉淀积累，不应依赖外部信息\n");
        result.push_str("- **不要调用消息类工具**（send_message / send_task_assignment_message 等），避免触发消息流程导致异步唤醒自己\n");
        result.push_str("- **只使用记忆类工具**：search_memory / save_long_term_memory / update_memory / query_memory\n");
        result.push_str("- 这是一个内循环：你与自己的记忆对话，不是与外部世界交互\n\n");
        result.push_str("## 你的任务\n\n");
        result.push_str("请用已有工具自主完成沉淀：\n\n");
        result.push_str("1. **归纳总结**：对上述短期记忆进行归纳，提炼核心概念、抽象经验、可复用模式（不要记具体细节）\n");
        result.push_str(
            "2. **查询已有图谱**：用 search_memory 检查是否已有相关知识点（避免重复节点）\n",
        );
        result.push_str("3. **创建/更新节点**：\n");
        result.push_str("   - 新知识 → save_long_term_memory 创建节点\n");
        result.push_str("   - 已有相似节点 → update_memory 更新节点内容\n");
        result.push_str("   - 过大且可拆分的旧节点 → 拆分为子节点 + 概述父节点 + contains 关系\n");
        result.push_str("4. **建立关系**：用 save_long_term_memory 的 relations 参数建立节点间关系（related/contains/depends 等）\n");
        result.push_str("5. **评估共享**：判断哪些节点对蜂巢有共享价值，用 update_memory 的 node_tags 字段加 'published' 标签\n");
        result.push_str("6. **标记完成**：每条短期记忆沉淀完成后，用 update_memory 把它的 status 改为 'settled'\n\n");
        result.push_str("## 认知要点\n\n");
        result.push_str("- 图谱是活的，每次沉淀都是迭代优化，不是机械合并\n");
        result.push_str("- 记抽象不记细节，可复用模式才沉淀\n");
        result.push_str("- 新老知识交替不是覆盖是迭代，推翻时用 opposite 关系保留痕迹\n");
        result.push_str(
            "- published 标签让节点全局共享，通过共享节点作为桥梁发现跨 Agent 的知识网络\n",
        );
        result.push_str("- 详见\"记忆认知\"技能的沉淀机制和新老知识交替章节\n\n");
        result.push_str("开始沉淀吧。");

        result
    }
}

/// 便捷函数：快速构建 Agent 对话 Prompt
///
/// 封装了最常用的组合：Trace ID + Agent 人设 + Agent 绑定工具 + 历史记忆 + 当前消息
pub fn build_conversation_prompt(
    trace_id: &str,
    agent: &Agent,
    recent_memories: &[Memory],
    current_message: &Message,
) -> String {
    use crate::models::prompt_builder::PromptBuilder;
    // 提取 ToolPo 列表（Tool 实体不可 Clone，但 PO 可以）
    let tool_pos: Vec<crate::models::tool::ToolPo> =
        agent.tools().iter().map(|t| t.po.clone()).collect();
    let mut builder = DefaultPromptBuilder::new();
    builder.current_trace_id(trace_id);
    builder.system_prompt(agent);
    builder.tools(&tool_pos);
    builder.history(recent_memories);
    builder.current_message(current_message);
    builder.build()
}
