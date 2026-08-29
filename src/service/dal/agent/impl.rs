//! AgentDalImpl 实现——CRUD / 搜索 / 统计 / 唤醒 / 向量索引
//!
//! 拆分自原 agent.rs（本次文件重构）：本文件承载 [`super::AgentDal`] trait 的
//! 全部实现，以及 [`super::AgentDalImpl`] 的私有辅助方法：
//! - runtime_state 注入与内存过滤（`inject_runtime_state` / `apply_runtime_state_filter`）
//! - 自动向量化（`upsert_vector_index` / `try_build_vector_params_for_search`）
//!
//! 单例管理 / trait 定义 / [`super::AgentFetchOptions`] 见 mod.rs。

use super::{AgentDal, AgentDalImpl, AgentFetchOptions};
use crate::enrich_ctx;
use crate::models::agent::{Agent, AgentPo};
use crate::models::brain::Brain;
use crate::models::vector::{MatchType, SearchMatchInfo, Vectorizable};
use crate::pkg::RequestContext;
use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;
use crate::service::dao::agent::{AgentQuery, AgentSearch, AgentStatsQuery};
use crate::service::dao::model_provider::ModelProviderStatsQuery;
use common::enums::AgentStatus;
use common::error::Result;
use common::models::{AgentStats, ModelCallStats, StatsFetchOptions, ToolCallSummary};

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
