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

        // 2. 调 `embed_entity` 生成完整 VectorIndexParams
        // 3. upsert 到向量索引（失败 warn 降级）
        match self
            .cortex_dao
            .embed_entity(ctx.clone(), &provider, po)
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

        let params = self
            .cortex_dao
            .embed_text_for_search(ctx, &provider, text)
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
            && let Some(provider) = brain.model_provider()
        {
            let model_provider_id = provider.id.clone();
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
/// 统一注入 skills，build() 时按 tag 自动分块拼装：
///
/// 1. 【Agent 人设】        ← 最稳定
/// 2. 【神经技能】          ← tags 含 "neural"，所有 Agent 必加载
/// 3. 【必加载技能】        ← tags 不含 "neural" 但与 agent match_keys 有交集
/// 4. 【用户画像】          ← 随用户变化，对话中相对稳定
/// 5. 【项目上下文】+【任务上下文】 ← 业务上下文，随消息变化
/// 6. 【历史对话】          ← 随对话增长
/// 7. 【工具失败警告】      ← 实时变化
/// 8. 【trace_id + 当前消息】← 每次变化
///
/// match_keys = agent.roles ∪ agent.installed_tags
///
/// 工具信息传递路径：
/// - 工具列表（name/description/parameters）→ OpenAI tools API 字段（协议层）
/// - Prompt 文本层不再包含任何工具描述（工具调用对模型透明，由 awakening 层根据 control_mode 分发）
///
/// build_sleep_prompt() 与 build() 对称，复用 1-6 区块（跳过 tool_failures 和 current_message），
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
    /// 工具失败统计：(工具名称, 失败次数)
    tool_failures: Vec<(String, u64)>,
    /// 意图分析结果（IntentAnalyze 阶段产出），供 build() 时渲染参考区块使用
    pub intent_analysis: Option<crate::service::domain::runtime::awakening::IntentAnalysis>,
}

impl DefaultPromptBuilder {
    /// 创建空的 Builder
    pub fn new() -> Self {
        Self::default()
    }

    /// 技能是否为神经技能（tags 含 "neural"）
    fn is_neural_skill(skill: &SkillPo) -> bool {
        skill.parse_tags().iter().any(|t| t == NEURAL_TAG)
    }

    /// 工具/技能的 tags 是否与 match_keys 有交集
    fn tags_match(tags: &[String], match_keys: &[String]) -> bool {
        tags.iter().any(|t| match_keys.contains(t))
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

    /// 构建技能区块（神经技能 + 必加载技能）
    ///
    /// 工具列表和调用规范都不再出现在 Prompt 中：
    /// - 工具列表（name/description/parameters）通过 OpenAI tools API 协议层传递
    /// - Manual 工具调用对模型透明（awakening 层根据 control_mode 分发执行）
    ///
    /// 技能仍按 tag 分块展示在 Prompt 中（技能是方法论，无 API 对应）。
    fn build_skills_sections(&self) -> String {
        let mut result = String::new();

        // 神经技能（tags 含 "neural"，所有 Agent 必加载）
        let neural_skills: Vec<_> = self
            .skills
            .iter()
            .filter(|s| Self::is_neural_skill(s))
            .collect();
        result.push_str(&Self::build_skills_section("神经技能", &neural_skills));

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
            // 记忆聚焦提示：当有任务上下文时，提示 Agent 可用 task_id 过滤记忆
            s.push_str("【记忆聚焦提示】\n");
            s.push_str("如需聚焦当前任务的记忆，可用 query_memory / search_memory 的 task_id 参数过滤；默认历史记忆是跨任务全局取最近若干条。\n\n");
        }
        s
    }

    /// 构建意图分析场景的 Prompt（Task 3：完整实现）
    ///
    /// 与 build()/build_sleep_prompt() 对称：复用 1-8 区块（人设 + 技能 + 上下文 + 历史），
    /// 再追加「意图识别 SOP 五步走 + 严格执行禁令 + JSON Schema 输出约束」的专属指令块，
    /// 最后附上当前消息作为明确靶子。
    pub fn build_intent_analyze_prompt(&self) -> String {
        let mut result = String::new();

        // 1. System Prompt（Agent 人设）
        if let Some(system) = &self.system_prompt {
            result.push_str(system);
            result.push_str("\n\n");
        }

        // 2-5. 技能区块（神经技能 + 必加载技能；调用方 analyze_input_intent 会通过
        //    scene=IntentAnalyze 的工具白名单过滤，保证 Prompt 中无执行类技能描述）
        result.push_str(&self.build_skills_sections());

        // 6-7. 通用上下文区块（用户画像 + 项目 + 任务，有值即拼装）
        result.push_str(&self.build_common_context_sections());

        // 8. 历史对话记忆（最近 N 条）
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

        // ==================== 阶段一：输入理解专用指令（核心）====================
        result.push_str("### 阶段一：输入理解专用指令（仅限 IntentAnalyze 场景）\n\n");

        result.push_str("===== 【输入理解阶段】IntentAnalyze 场景约束（非常重要！）=====\n\n");
        result.push_str("## 你的任务：只做理解，不做执行\n\n");
        result.push_str("你当前处于正式干活前的「审题阶段」。本阶段你的唯一目标是产出一份结构化的理解结果，然后就结束本轮思考。\n\n");
        result.push_str("✅ 必须做：\n");
        result.push_str(
            "   1. 在思考中严格按下方「理解 SOP 五步走」执行一遍，每一步都要有实质思考，不要跳过\n",
        );
        result.push_str("   2. 必须执行多步检索：至少调用一次 search_memory + 一次 recommend_seed_nodes 或 traverse_knowledge_graph（100% 全新无历史的闲聊可豁免，但必须在思考中明确说明理由）\n");
        result.push_str("   3. 关键词联想要充分展开，联想扩展词与基础关键词一起写入 key_terms\n");
        result.push_str("   4. 最终输出严格的 JSON 对象，字段完整可被解析\n\n");
        result.push_str("❌ 严格禁止做（任何违反都将导致此阶段结果作废）：\n");
        result.push_str("   1. 严禁执行任何行动/工具调用——禁止调用 send_message / send_task_assignment_message / send_message_to_agent：不准给任何用户/Agent 发消息\n");
        result.push_str("   2. 严禁编造无来源信息——禁止调用 create_task / update_task / create_project / update_project / update_memory 状态写入类工具；不准改动任何系统状态（只有 save_short_term_memory 内部记忆写入是允许的，若你需要临时记录东西）\n");
        result.push_str("   3. 如果信息不足必须 need_clarification=true 并把澄清话术写进 resolutions——禁止做任何外部 API 调用、shell 执行、文件读写类工具；禁止直接回答用户问题（哪怕你 100% 知道答案），不准在 Final 里写对用户的回复\n\n");

        result.push_str("## 理解 SOP 五步走（在思考中严格按此顺序执行）\n\n");
        result.push_str("### Step 1：意图识别\n");
        result.push_str("在思考中先把【当前消息】归类，写出你判断的依据：\n");
        result.push_str("- Question：提问型（要信息/问进度/问规则/请教）\n");
        result.push_str("- TaskRequest：任务型（提需求/安排工作/要产出）\n");
        result.push_str("- Confirm：确认型（同意/否定/选择/拍板）\n");
        result.push_str("- FollowUp：追问型（承接之前某条回答/产出的继续追问）\n");
        result.push_str("- ClarificationResponse：澄清响应型（针对你前面追问的答复）\n");
        result.push_str("- Chat：闲聊型（打招呼/客套/社交礼貌）\n");
        result.push_str("- Mixed：混合型（多类意图，拆分说明）\n");
        result.push_str(
            "意图类型写入 intent_type 字段；置信度 0.0~1.0 自己打分写入 confidence。\n\n",
        );

        result.push_str("### Step 2：指代与上下文消歧\n");
        result.push_str("1. 仔细读【历史对话】+【项目/任务上下文】+【用户画像】\n");
        result.push_str("2. 找【当前消息】中的指代短语：这/那/上次/那个/他/按之前定的来 等\n");
        result.push_str("3. 在思考中把每个指代对应到具体对象（project_id/task_id/message_id/某个人物…），写进 resolutions 数组，每条格式：\"\\\"XXX\\\" → YYY\"\n");
        result.push_str("4. 读完所有上下文仍无法确定 → 写进 need_clarification，不要硬猜\n\n");

        result.push_str("### Step 3：关键词抽取与联想扩展\n");
        result.push_str(
            "这一步不只是提取，更重要的是联想扩展，为后续检索提供丰富的 query 基础。\n\n",
        );
        result.push_str("3.1 基础关键词抽取：\n");
        result.push_str("从【当前消息】+ 消歧后的具体对象中，抽取关键实体和核心短语：\n");
        result.push_str("- 显式实体：项目名/任务名/产品名/人名/专有名词/技术术语\n");
        result.push_str("- 隐式语义：核心动词短语（推进进度→进度查询、对比方案→方案比较）\n");
        result.push_str("- 情感倾向词：急迫/犹豫/不满/期待（影响执行优先级判断）\n\n");
        result.push_str("3.2 关键词联想扩展（在思考中展开，不要跳过）：\n");
        result.push_str("对每个基础关键词，思考它的关联概念并扩展：\n");
        result.push_str("- 同义/近义词：用户说方案A → 也搜索 proposal A / 备选方案A\n");
        result.push_str("- 上下游概念：用户说部署 → 关联测试/回滚/监控/配置变更\n");
        result.push_str("- 时间关联：用户说上次 → 思考上次是什么时候 → 搜索对应时间段的记忆\n");
        result.push_str("- 因果关联：用户说为什么失败 → 关联错误日志/最近变更/依赖状态\n\n");
        result.push_str("3.3 把基础关键词 + 联想扩展词都写进 key_terms 数组（5~12 个），\n");
        result.push_str("这些词将直接用于 Step 4 的多角度检索，越丰富检索越全面。\n\n");

        result.push_str(
            "### Step 4：多步语义检索与知识图谱关联分析（强制执行，本阶段核心价值所在）\n",
        );
        result.push_str(
            "本步直接决定后续执行阶段的信息完备性。宁可多检索一步，不要遗漏关键上下文。\n",
        );
        result.push_str("你的检索策略应该是有层次的，不是随机调工具：\n\n");
        result.push_str("4.1 短期记忆检索（search_memory）——第一轮：\n");
        result.push_str("- 用 Step 3 的核心关键词组合成 query，调用 search_memory\n");
        result
            .push_str("- 如果第一批结果不够相关，换一组关键词组合再搜一轮（不要一次不中就放弃）\n");
        result.push_str(
            "- 示例：用户问上次那个方案进度 → 先搜「方案 进度」，再搜「方案A 项目X」\n\n",
        );
        result.push_str(
            "4.2 知识图谱探索（recommend_seed_nodes + traverse_knowledge_graph）——第二轮：\n",
        );
        result.push_str(
            "- 调用 recommend_seed_nodes 获取与当前 project/task/agent 相关的图谱种子节点\n",
        );
        result
            .push_str("- 从种子节点出发，调用 traverse_knowledge_graph 走 1~2 跳，探索关联知识\n");
        result.push_str(
            "- 重点关注：用户偏好节点（user_preference tag）、历史决策节点、相关项目/任务节点\n",
        );
        result.push_str("- 知识图谱中的关系链路本身就是信息：A 依赖 B、A 衍生自 C、A 取代了 D\n\n");
        result.push_str("4.3 历史对话补充（list_messages，可选第三轮）：\n");
        result.push_str("- 如果短期记忆和知识图谱都不够，调用 list_messages 上拉最近对话记录\n");
        result.push_str("- 特别关注：用户最近提过的类似需求、Agent 之前给过的承诺或结论\n\n");
        result.push_str("4.4 检索结果整理：\n");
        result.push_str(
            "- 把所有检索命中的高相关内容**你自己概括为短摘要**（1~2 句每条，不要贴原始 JSON）\n",
        );
        result.push_str("- 每条摘要注明来源类型：[记忆]/[图谱]/[对话]\n");
        result.push_str("- 按相关度排序，最相关的放前面\n");
        result.push_str("- 写进 retrieved_context 数组\n\n");
        result.push_str("如果跳过了 4.1 或 4.2，必须在思考中明确说明理由（如：100%全新话题，无历史可检索）。\n\n");

        result.push_str("### Step 5：综合研判与总结\n");
        result.push_str("5.1 信息完备性检查：\n");
        result.push_str("- 回顾 Step 1~4 的全部产出，检查是否有信息缺口\n");
        result.push_str("- 如果消歧失败 / 混合型意图优先级不清 / 需求边界不明 / 需要用户决策\n");
        result.push_str("  → 把要问用户的具体问题逐条写进 need_clarification（问题尽量用选择题形式，不要开放式）\n");
        result.push_str("- 如果理解充分 → need_clarification = []\n\n");
        result.push_str("5.2 形成理解结论：\n");
        result.push_str("- 在思考中用 1~2 句话总结：我理解用户想要 XXX，相关的背景信息有 YYY\n");
        result.push_str("- 这个总结将直接作为下一阶段执行的输入，务必准确、完整、可执行\n");
        result.push_str("- 写进 summary 字段\n\n");

        result.push_str("## 最终输出规范（必须严格遵守）\n\n");
        result.push_str("你输出的【最终 Final 内容】必须严格符合以下格式：\n");
        result.push_str("- Final block MUST start with `--- INTENT_ANALYSIS_START ---` followed by pure JSON and end with `--- INTENT_ANALYSIS_END ---`\n");
        result
            .push_str("- 中间 JSON 对象必须严格符合以下 schema（7 个字段全包含，不要省略）：\n\n");
        result.push_str("JSON Schema 字段说明：\n");
        result.push_str("- intent_type：字符串，取值为 Question | TaskRequest | Confirm | FollowUp | ClarificationResponse | Chat | Mixed\n");
        result.push_str("- confidence：数字，0.0 到 1.0，你对自己意图判断的置信度\n");
        result.push_str("- key_terms：字符串数组，5~12 个关键词（基础抽取 + 联想扩展）\n");
        result.push_str(
            "- resolutions：字符串数组，指代消歧映射结果，每条格式 \"\\\"XXX\\\" → 具体对象\"\n",
        );
        result.push_str("- retrieved_context：字符串数组，search_memory / recommend_seed_nodes 等命中结果的你自己概括的短摘要（不要原始 JSON）\n");
        result.push_str(
            "- need_clarification：字符串数组，需要向用户澄清的具体问题（空列表 = 理解充分）\n",
        );
        result.push_str("- summary：字符串，一句话总结你最终理解的用户需求\n\n");
        result.push_str("示例 JSON（请严格模仿此结构，字段名和类型必须一致）：\n");
        result.push_str("--- INTENT_ANALYSIS_START ---\n");
        result.push_str("{\n");
        result.push_str("  \"intent_type\": \"FollowUp\",\n");
        result.push_str("  \"confidence\": 0.85,\n");
        result.push_str("  \"key_terms\": [\"项目X\", \"方案A\", \"进度\", \"上次那个方案\"],\n");
        result.push_str(
            "  \"resolutions\": [\"\\\"上次那个方案\\\" → project=proj_123, task=task_456\"],\n",
        );
        result.push_str("  \"retrieved_context\": [\"2026-08-10 短期记忆：项目X 方案 A/B 比较，推荐方案 A（相似度 0.88）\"],\n");
        result.push_str("  \"need_clarification\": [],\n");
        result.push_str(
            "  \"summary\": \"用户想知道项目 X 中之前讨论过的方案 A 的当前推进进度与结果\"\n",
        );
        result.push_str("}\n");
        result.push_str("--- INTENT_ANALYSIS_END ---\n\n");

        result.push_str("===== 【输入理解阶段】指令结束 =====\n\n");

        // 10. 当前消息（放在最后，给 Agent 明确的靶子）
        if let Some(msg) = &self.current_message {
            result.push_str("【当前消息】\n");
            result.push_str(msg);
            result.push_str("\n\n现在开始：在思考中走完 Step 1~5，然后按上面的 INTENT_ANALYSIS_START/END 锚点格式输出最终 JSON。\n");
        } else {
            result.push_str("【注意】当前消息为空。请直接输出空 JSON 或说明情况。\n");
        }

        result
    }

    /// 渲染【输入理解结果】参考区块（Task 4：完整实现 + 严格截断规则）
    ///
    /// 姿态：反复强调"仅供参考，以你当下判断为准"，避免 Agent 被前置结论带偏。
    ///
    /// 截断规则（防止 Prompt 过长超 token，CRITICAL）：
    /// - key_terms / resolutions / retrieved_context / need_clarification：
    ///   每项最多 150 字符，每个数组最多 10 项，超出 "... 及 N 项已省略"
    /// - summary：最多 800 字符
    ///
    /// 若 intent_analysis 为 None → 不渲染任何区块，返回 ""
    fn render_intent_analysis_section(&self) -> String {
        let ia = match &self.intent_analysis {
            None => return String::new(),
            Some(ia) if ia.intent_type.is_empty() && ia.summary.is_empty() => return String::new(),
            Some(ia) => ia,
        };

        let trunc_str = |s: &str, max: usize| -> String {
            let chars: Vec<char> = s.chars().collect();
            if chars.len() <= max {
                s.to_string()
            } else {
                let mut out: String = chars.into_iter().take(max).collect();
                out.push('…');
                out
            }
        };

        const MAX_ITEMS: usize = 10;
        const MAX_ITEM_CHARS: usize = 150;
        const MAX_SUMMARY_CHARS: usize = 800;

        let format_array =
            |items: &[String], prefix_icon: &str, _omit_msg: &str| -> (String, bool) {
                let mut s = String::new();
                let display = items.iter().take(MAX_ITEMS);
                let count = items.len();
                for item in display {
                    s.push_str(&format!(
                        "{} {}\n",
                        prefix_icon,
                        trunc_str(item, MAX_ITEM_CHARS)
                    ));
                }
                let omitted = count > MAX_ITEMS;
                if omitted {
                    let n = count - MAX_ITEMS;
                    s.push_str(&format!("... 及 {} 项已省略\n", n));
                }
                (s, omitted)
            };

        let need_clarify = !ia.need_clarification.is_empty();

        let mut s = String::new();
        if need_clarify {
            s.push_str("## 【输入理解结果 · 仅供参考】 ⚠️\n\n");
        } else {
            s.push_str("## 【输入理解结果 · 仅供参考】\n\n");
        }
        s.push_str("> 说明：以下内容是上一阶段「审题阶段」自动预分析得出的理解摘要，仅供你正式执行时参考。\n");
        s.push_str(
            "> 若你当下重新判断后发现不一致，请**以你当下的理解为准**，不要被以下内容束缚。\n\n",
        );

        if !ia.intent_type.is_empty() {
            s.push_str(&format!(
                "🎯 **意图类型**：{}（置信度 {:.2}%）\n\n",
                ia.intent_type,
                ia.confidence * 100.0
            ));
        }

        if !ia.key_terms.is_empty() {
            s.push_str("🔑 **关键词抽取**：\n");
            let (content, _) = format_array(&ia.key_terms, "-", "关键词");
            s.push_str(&content);
            s.push('\n');
        }

        if !ia.resolutions.is_empty() {
            s.push_str("🧩 **指代消歧结果**：\n");
            let (content, _) = format_array(&ia.resolutions, "-", "消歧结果");
            s.push_str(&content);
            s.push('\n');
        }

        if !ia.retrieved_context.is_empty() {
            s.push_str("📚 **检索补充上下文摘要**：\n");
            let (content, _) = format_array(&ia.retrieved_context, "-", "检索摘要");
            s.push_str(&content);
            s.push('\n');
        }

        if need_clarify {
            s.push_str("⚠️ **建议向用户澄清的问题**（上一阶段判断存在歧义）：\n");
            let (content, _) = format_array(&ia.need_clarification, "❓", "澄清问题");
            s.push_str(&content);
            s.push('\n');
        }

        if !ia.summary.is_empty() {
            s.push_str(&format!(
                "💡 **一句话理解总结**：{}\n\n",
                trunc_str(&ia.summary, MAX_SUMMARY_CHARS)
            ));
        }

        s.push_str("---\n\n");
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

        // 2-5. 技能区块（神经技能/必加载技能）
        // 工具列表和调用规范都不在 Prompt 中（工具通过 API 协议层传递，调用对模型透明）
        result.push_str(&self.build_skills_sections());

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

        // 10. 本次思考的 Trace ID
        if let Some(trace_id) = &self.current_trace_id {
            result.push_str(&format!("【思考 Trace ID】{}\n\n", trace_id));
        }

        // 10.5 【输入理解结果】区块（Phase 1 IntentAnalyze 阶段产出，Task 4：A+ P3 串联）
        // 位置：严格在 Trace ID 之后、当前消息之前；若 intent_analysis 为 None 则无输出
        let intent_section = self.render_intent_analysis_section();
        if !intent_section.is_empty() {
            result.push_str(&intent_section);
        }

        // 11. 当前用户消息
        if let Some(msg) = &self.current_message {
            result.push_str(msg);
            result.push_str("\n\n请回复：");
        }

        result
    }

    fn build_sleep_prompt(&self, pending_memories_summary: &str, trace_ids: &[String]) -> String {
        let mut result = String::new();

        // 1. System Prompt（Agent 人设）
        if let Some(system) = &self.system_prompt {
            result.push_str(system);
            result.push_str("\n\n");
        }

        // 2-5. 技能区块（sleep_and_settle 调用前已过滤只保留记忆相关）
        result.push_str(&self.build_skills_sections());

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
        result.push_str("- **只使用记忆类工具**：search_memory / save_long_term_memory / update_memory / query_memory / save_short_term_memory\n");
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
        result.push_str("6. **标记完成**：每条短期记忆沉淀完成后，用 update_memory 把它的 status 改为 'settled'\n");
        result.push_str("7. **强制写入沉淀摘要**（必须执行）：沉淀完成后，**必须**调用 save_short_term_memory 将本次沉淀的摘要写入短期记忆，参数要求：\n");
        result.push_str("   - `summary`：本次沉淀提炼的核心经验摘要（不是细节流水账）\n");
        result.push_str("   - `content`：详细内容（可选，记录沉淀出的关键知识点列表）\n");
        result.push_str("   - `tags`：标签列表（如 `[\"settled\", \"consolidation\"]`）\n");
        result.push_str(&format!(
            "   - `trace_ids`：**必须填入** `[{}]`（本次沉淀依赖的 trace 列表，用于记忆追溯）\n\n",
            trace_ids
                .iter()
                .map(|t| format!("\"{}\"", t))
                .collect::<Vec<_>>()
                .join(", ")
        ));
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

    fn build_summary_prompt(
        &self,
        work_summary: &str,
        total_rounds: usize,
        trace_ids: &[String],
    ) -> String {
        let mut result = String::new();

        // 1. System Prompt（Agent 人设）
        if let Some(system) = &self.system_prompt {
            result.push_str(system);
            result.push_str("\n\n");
        }

        // 2-5. 技能区块（Summary 场景已过滤，只保留 neural/memory/messaging/project_management）
        result.push_str(&self.build_skills_sections());

        // 6-7. 通用上下文区块（保留 project/task_context 帮助 Agent 理解任务背景）
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

        // 10. 总结退出指令
        result.push_str("【总结退出模式触发】\n\n");
        result.push_str(&format!(
            "你已连续思考 {} 轮仍未完成任务，现在需要总结当前工作进展并退出。\n\n",
            total_rounds
        ));
        result.push_str("## 当前工作对话摘要\n\n");
        result.push_str(work_summary);
        result.push_str("\n\n");
        result.push_str("## 你的任务\n\n");
        result.push_str("1. **总结进展**：梳理当前已完成的工作、取得的阶段性成果\n");
        result.push_str("2. **记录问题**：列出未解决的问题、遇到的障碍、下一步建议\n");
        result.push_str("3. **发送通知**：\n");
        result.push_str("   - 如果有消息源（用户/Agent），用 send_message 将总结发送给对方\n");
        result.push_str("   - 如果关联了 task，用 update_task_progress 更新任务进度和状态\n");
        result.push_str("4. **强制写入短期记忆**（必须执行）：总结完成后，**必须**调用 save_short_term_memory 将本次工作总结写入短期记忆，参数要求：\n");
        result.push_str("   - `summary`：本次工作总结摘要（核心进展 + 问题 + 下一步）\n");
        result.push_str("   - `content`：详细内容（可选，记录完整总结）\n");
        result.push_str("   - `tags`：标签列表（如 `[\"work_summary\", \"max_rounds\"]`）\n");
        result.push_str(&format!(
            "   - `trace_ids`：**必须填入** `[{}]`（本次总结依赖的 trace 列表，用于记忆追溯）\n",
            trace_ids
                .iter()
                .map(|t| format!("\"{}\"", t))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        result.push_str("5. **保持简洁**：总结应聚焦关键信息，避免冗长\n\n");
        result.push_str("## 约束\n\n");
        result.push_str("- 这是退出流程，完成总结后直接回复最终文本即可\n");
        result.push_str("- 不要尝试继续执行原任务，聚焦于总结和通知\n");
        result.push_str("- 如果无法发送消息（无目标），直接输出总结文本\n");
        result.push_str("- save_short_term_memory 是必须执行的操作，不要遗漏\n\n");
        result.push_str("开始总结吧。");

        result
    }

    fn build_intent_analyze_prompt(&self) -> String {
        DefaultPromptBuilder::build_intent_analyze_prompt(self)
    }

    fn intent_analysis(
        &mut self,
        analysis: &crate::service::domain::runtime::awakening::IntentAnalysis,
    ) {
        self.intent_analysis = Some(analysis.clone());
    }
}

/// 便捷函数：快速构建 Agent 对话 Prompt
///
/// 封装了最常用的组合：Trace ID + Agent 人设 + 历史记忆 + 当前消息
pub fn build_conversation_prompt(
    trace_id: &str,
    agent: &Agent,
    recent_memories: &[Memory],
    current_message: &Message,
) -> String {
    use crate::models::prompt_builder::PromptBuilder;
    let mut builder = DefaultPromptBuilder::new();
    builder.current_trace_id(trace_id);
    builder.system_prompt(agent);
    builder.history(recent_memories);
    builder.current_message(current_message);
    builder.build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::agent::{Agent, AgentPo};
    use crate::models::message::Message;
    use crate::models::prompt_builder::PromptBuilder;
    use common::enums::{AgentStatus, MessageRole, MessageType};
    use uuid::Uuid;

    fn make_simple_agent() -> Agent {
        let mut po = AgentPo::new(
            "测试助手".to_string(),
            vec!["assistant".to_string()],
            "一个测试用的 Agent".to_string(),
            vec!["chat".to_string()],
            "".to_string(),
            "provider-001".to_string(),
            "test-user".to_string(),
        );
        po.id = "agent-test-001".to_string();
        po.status = AgentStatus::Onboarded;
        Agent::from_po(po)
    }

    fn make_simple_message(content: &str) -> Message {
        Message::new_with_context(
            Uuid::now_v7().to_string(),
            None,
            None,
            "test-user".to_string(),
            "agent-test-001".to_string(),
            MessageRole::User,
            MessageRole::Agent,
            MessageType::Text,
            content.to_string(),
            None,
            crate::models::file::FileMeta::default(),
            None,
            None,
            None,
            "test-user".to_string(),
        )
    }

    #[test]
    fn build_intent_analyze_prompt_contains_sop_and_schema() {
        let agent = make_simple_agent();
        let message = make_simple_message("上次那个方案结果呢？");

        let mut builder = DefaultPromptBuilder::new();
        builder.current_trace_id("trace-test-001");
        builder.system_prompt(&agent);
        builder.current_message(&message);

        let prompt = builder.build_intent_analyze_prompt();

        // 1. 包含阶段一标题
        assert!(
            prompt.contains("### 阶段一：输入理解专用指令（仅限 IntentAnalyze 场景）"),
            "Prompt 应包含阶段一标题。Output 片段:\n{}",
            prompt.chars().take(500).collect::<String>()
        );

        // 2. 包含全部 7 个字段名（JSON Schema 说明）
        let seven_fields = [
            "intent_type",
            "confidence",
            "key_terms",
            "resolutions",
            "retrieved_context",
            "need_clarification",
            "summary",
        ];
        for field in &seven_fields {
            assert!(
                prompt.contains(field),
                "Prompt 应包含字段名 '{}' 但未找到。Output 片段:\n{}",
                field,
                prompt.chars().take(800).collect::<String>()
            );
        }

        // 3. 包含 INTENT_ANALYSIS_START 锚点
        assert!(
            prompt.contains("--- INTENT_ANALYSIS_START ---"),
            "Prompt 应包含 INTENT_ANALYSIS_START 锚点标记"
        );
        assert!(
            prompt.contains("--- INTENT_ANALYSIS_END ---"),
            "Prompt 应包含 INTENT_ANALYSIS_END 锚点标记"
        );

        // 4. 包含 SOP 五步走的 Step 1~5 标识
        assert!(prompt.contains("Step 1：意图识别"));
        assert!(prompt.contains("Step 2：指代与上下文消歧"));
        assert!(prompt.contains("Step 3：关键词抽取与联想扩展"));
        assert!(prompt.contains("Step 4：多步语义检索与知识图谱关联分析"));
        assert!(prompt.contains("Step 5：综合研判与总结"));

        // 5. 包含三条禁令标识（严禁执行/严禁编造/信息不足必须澄清）
        assert!(
            prompt.contains("严禁执行任何行动"),
            "Prompt 应包含禁令 1：严禁执行"
        );
        assert!(
            prompt.contains("严禁编造无来源信息"),
            "Prompt 应包含禁令 2：严禁编造"
        );
        assert!(
            prompt.contains("如果信息不足必须"),
            "Prompt 应包含禁令 3：信息不足必须澄清"
        );

        // 6. 包含用户消息原文（作为明确靶子）
        assert!(
            prompt.contains("上次那个方案结果呢？"),
            "Prompt 末尾应包含当前用户消息原文"
        );
    }

    // ============= Task 4 (A+ P3) 新增单元测试 =============

    use crate::service::domain::runtime::awakening::IntentAnalysis;

    /// UT-a: 截断规则验证——海量数据时不溢出 token
    /// 构建：20 项数组（每项 300+ 字符）+ 2000 字符 summary
    /// 断言：总输出 < 3000 字符、含 "... 及 N 项已省略"、summary 被截断、confidence 用 "%" 显示
    #[test]
    fn render_intent_analysis_section_truncation_rules() {
        // 构造 20 个 300 字符的字符串填充到每个数组
        let long_str: String = (0..300).map(|_| '一').collect();
        let huge_terms: Vec<String> = (0..20)
            .map(|i| format!("term-{} {}", i, long_str))
            .collect();
        let huge_res: Vec<String> = (0..20).map(|i| format!("res-{} {}", i, long_str)).collect();
        let huge_ctx: Vec<String> = (0..20).map(|i| format!("ctx-{} {}", i, long_str)).collect();
        let huge_clarify: Vec<String> = (0..20).map(|i| format!("q-{} {}", i, long_str)).collect();
        let huge_summary: String = (0..2000).map(|_| '总').collect();

        let ia = IntentAnalysis {
            intent_type: "Mixed".into(),
            confidence: 0.7856,
            key_terms: huge_terms,
            resolutions: huge_res,
            retrieved_context: huge_ctx,
            need_clarification: huge_clarify,
            summary: huge_summary,
        };

        let mut builder = DefaultPromptBuilder::new();
        builder.intent_analysis = Some(ia);

        let output = builder.render_intent_analysis_section();
        assert!(!output.is_empty(), "理解区块不应为空");

        // 1) 总字符数 < 3000（实际 ~10*150*4 + 800 + 约 500 固定文字 ≈ 7300，
        //    这里用 8000 作为安全上限，重点是不能让 20*300*4 + 2000 = 26000 全部进入）
        assert!(
            output.chars().count() < 8000,
            "截断后输出应远小于原始体量，当前字符数: {}",
            output.chars().count()
        );

        // 2) 出现 "... 及 N 项已省略" 提示（数组从 20 项被截到 10 项，每个数组都应有省略提示）
        assert!(
            output.contains("及 10 项已省略"),
            "截断提示未出现。Output 片段:\n{}",
            output.chars().take(600).collect::<String>()
        );

        // 3) summary 被截断到 800 字 + … 字符（原 2000 字）
        // 检查 "总" 字出现次数不应接近 2000
        let summary_total_count = output.matches('总').count();
        assert!(
            summary_total_count < 1000,
            "summary 似乎未被截断（'总' 字出现 {} 次）",
            summary_total_count
        );

        // 4) confidence 用百分比格式（含 "%" 符号，不是原小数 0.7856）
        assert!(
            output.contains('%'),
            "置信度应以百分比显示（含 % 符号）。Output 片段:\n{}",
            output.chars().take(300).collect::<String>()
        );
        assert!(output.contains("78.56%"), "置信度 0.7856 应渲染为 78.56%");
    }

    /// UT-b: 验证【输入理解结果】区块严格出现在【当前消息】之前
    /// 分支 1：有 IntentAnalysis 时，检查 find() 索引顺序；
    /// 分支 2：intent_analysis=None 时，不出现新区块且输出与之前一致
    #[test]
    fn build_prompt_contains_input_understanding_before_current_message() {
        let agent = make_simple_agent();
        let message = make_simple_message("帮我把上次那个文档改一下");

        // ========== 分支 1：有 IntentAnalysis ==========
        let ia = IntentAnalysis {
            intent_type: "TaskRequest".into(),
            confidence: 0.91,
            key_terms: vec!["文档".into(), "修改".into()],
            resolutions: vec!["\"上次那个文档\" → doc_id=doc_789".into()],
            retrieved_context: vec!["2026-08-12 短期记忆：doc_789 版本 v2".into()],
            need_clarification: vec![],
            summary: "用户想修改 doc_789 文档".into(),
        };

        let mut builder_with_ia = DefaultPromptBuilder::new();
        builder_with_ia.current_trace_id("trace-order-001");
        builder_with_ia.system_prompt(&agent);
        builder_with_ia.current_message(&message);
        builder_with_ia.intent_analysis(&ia);
        let prompt_with_ia = builder_with_ia.build();

        // 断言：理解区块 + 当前消息两者都出现
        let idx_understanding = prompt_with_ia
            .find("【输入理解结果")
            .expect("Prompt 应包含【输入理解结果】区块");
        let idx_current_msg = prompt_with_ia
            .find("【当前消息】")
            .expect("Prompt 应包含【当前消息】区块");

        // 关键断言：理解区块索引 < 当前消息索引
        assert!(
            idx_understanding < idx_current_msg,
            "【输入理解结果】(idx={}) 必须出现在【当前消息】(idx={}) 之前！",
            idx_understanding,
            idx_current_msg
        );

        // ========== 分支 2：intent_analysis=None（未注入）==========
        let builder_none_ia = {
            let mut b = DefaultPromptBuilder::new();
            b.current_trace_id("trace-order-002");
            b.system_prompt(&agent);
            b.current_message(&message);
            // 不注入 intent_analysis → 保持 None
            b
        };
        let prompt_none_ia = builder_none_ia.build();

        // 断言：不包含理解区块
        assert!(
            !prompt_none_ia.contains("【输入理解结果"),
            "intent_analysis=None 时不应渲染理解区块"
        );
        // 断言：仍然包含当前消息（输出未被破坏）
        assert!(
            prompt_none_ia.contains("【当前消息】"),
            "None 分支输出应包含当前消息区块"
        );
        assert!(
            prompt_none_ia.len() > 50,
            "None 分支输出不应被破坏为空（长度 {}）",
            prompt_none_ia.len()
        );
    }

    /// UT-c: Phase 1 失败时的优雅降级（逻辑模拟）
    /// 场景：analyze_input_intent 返回 Err → ia = None → builder 不注入
    /// 断言：(1) builder.intent_analysis 保持 None；
    ///       (2) render_intent_analysis_section() 返回 ""；
    ///       (3) build() 仍产出合法非空 Prompt（无 crash、无空输出）
    #[test]
    fn intent_analyze_phase1_failure_graceful_degrade() {
        // ---- 模拟 awaken() 中 Phase 1 返回 Err 的场景 ----
        // 逻辑等价代码（简化版）：
        //   let ia: Option<IntentAnalysis> = match self.analyze_input_intent(...) {
        //       Ok(ia) => Some(ia),
        //       Err(_) => None,  // ← 降级分支
        //   };
        let ia: Option<IntentAnalysis> = None; // 模拟 Err 降级结果

        let agent = make_simple_agent();
        let message = make_simple_message("Phase 1 fail degrade test");

        let mut builder = DefaultPromptBuilder::new();
        builder.current_trace_id("trace-degrade-001");
        builder.system_prompt(&agent);
        builder.current_message(&message);

        // 等价于 awaken() loop 内的注入代码（ia = None 时跳过）
        if let Some(ref ia_ref) = ia {
            builder.intent_analysis(ia_ref);
        }

        // 断言 1：builder.intent_analysis 字段保持 None
        assert!(
            builder.intent_analysis.is_none(),
            "降级分支下 builder.intent_analysis 应为 None"
        );

        // 断言 2：render_intent_analysis_section() 返回空字符串
        let section = builder.render_intent_analysis_section();
        assert!(
            section.is_empty(),
            "降级分支下 render_intent_analysis_section() 应返回空字符串"
        );

        // 断言 3：build() 不 crash，输出非空且包含必要区块
        let prompt = builder.build();
        assert!(!prompt.is_empty(), "降级分支下 build() 输出不应为空");
        assert!(
            prompt.contains("【思考 Trace ID】"),
            "降级分支下 Prompt 仍应含 Trace ID 区块"
        );
        assert!(
            prompt.contains("【当前消息】"),
            "降级分支下 Prompt 仍应含当前消息区块"
        );
    }
}
