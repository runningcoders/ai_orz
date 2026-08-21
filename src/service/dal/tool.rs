//! Tool DAL 模块
//!
//! 基础工具数据访问层，提供工具查询和管理能力
//! 负责组合 DAO 完成业务级数据操作

use crate::models::tool::{Tool, ToolExecutionRequest, ToolPo};
use crate::models::vector::{MatchType, SearchMatchInfo, Vectorizable};
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_tracing::entry::ToolCallEntry;
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::model_provider::ModelProviderDao;
use crate::service::dao::tool::{ToolDao, ToolQuery, ToolStatsDao, ToolStatsQuery, ToolVectorDao};
use crate::service::dao::tool_call::{self, ToolCallDao};
use common::enums::ToolStatus;
use common::error::{Result, bail_err};
use common::models::{StatsFetchOptions, StatsInterval, ToolStats};
use serde_json::Value;
use std::sync::{Arc, OnceLock};

// ==================== 单例管理 ====================

static TOOL_DAL: OnceLock<Arc<dyn ToolDal>> = OnceLock::new();

/// 获取 Tool DAL 单例
pub fn dal() -> Arc<dyn ToolDal> {
    TOOL_DAL.get().cloned().unwrap()
}

/// 初始化 Tool DAL（使用全局单例 DAO）
pub fn init() {
    use crate::service::dao::{cortex, model_provider, tool};
    let _ = TOOL_DAL.set(new(
        tool::dao(),
        tool_call::dao(),
        tool::vector_dao(),
        model_provider::dao(),
        cortex::dao(),
        tool::stats_dao(),
    ));
}

/// 创建 Tool DAL（返回 trait 对象）
pub fn new(
    tool_dao: Arc<dyn ToolDao + Send + Sync>,
    tool_call_dao: Arc<dyn ToolCallDao + Send + Sync>,
    tool_vector_dao: Arc<dyn ToolVectorDao + Send + Sync>,
    model_provider_dao: Arc<dyn ModelProviderDao + Send + Sync>,
    cortex_dao: Arc<dyn CortexDao + Send + Sync>,
    tool_stats_dao: Arc<dyn ToolStatsDao<ToolCallEvent = crate::pkg::stats::ToolCallEvent>>,
) -> Arc<dyn ToolDal> {
    Arc::new(ToolDalImpl {
        tool_dao,
        tool_call_dao,
        tool_vector_dao,
        model_provider_dao,
        cortex_dao,
        tool_stats_dao,
    })
}

// ==================== DAL 接口 ====================

/// Tool 附带信息获取选项
#[derive(Debug, Clone, Default)]
pub struct ToolFetchOptions {
    /// 是否加载统计信息（ToolStats: 调用次数 + 失败次数）
    pub with_stats: Option<bool>,
    /// 统计时间范围（毫秒），None 表示全部历史
    pub stats_time_range: Option<(i64, i64)>,
    /// 时序查询粒度，None 时默认 Daily
    pub stats_interval: Option<StatsInterval>,
}

// ==================== DAL 接口 ====================

/// Tool DAL 接口
#[async_trait::async_trait]
pub trait ToolDal: Send + Sync {
    /// 创建新工具
    async fn create_tool(&self, ctx: RequestContext, po: &ToolPo) -> Result<()>;

    /// 更新现有工具
    async fn update_tool(&self, ctx: RequestContext, tool: &Tool) -> Result<()>;

    /// 删除工具
    async fn delete_tool(&self, ctx: RequestContext, tool_id: &str) -> Result<()>;

    /// 根据 ID 获取完整工具（PO + CoreTool 实例）
    async fn get_by_id(&self, ctx: RequestContext, id: String) -> Result<Option<Tool>>;

    /// 根据 ID 获取工具（带附带信息选项）
    async fn get_tool(
        &self,
        ctx: RequestContext,
        id: &str,
        options: ToolFetchOptions,
    ) -> Result<Option<Tool>>;

    /// 根据名称获取完整工具
    async fn get_by_name(&self, ctx: RequestContext, name: &str) -> Result<Option<Tool>>;

    /// 通用综合查询（返回完整 Tool 实体，包含 PO + CoreTool）
    ///
    /// 支持组合查询条件，所有字段都是 Option
    async fn query(
        &self,
        ctx: RequestContext,
        query: ToolQuery,
    ) -> Result<common::api::PagedResult<Tool>>;

    /// 获取所有启用的工具
    async fn list_enabled(&self, ctx: RequestContext) -> Result<Vec<Tool>>;

    /// 列出所有启用工具的 distinct tags
    async fn list_tags(&self, ctx: RequestContext) -> Result<Vec<String>>;

    /// 获取 Agent 的所有完整工具（每个都是 PO + CoreTool）
    async fn list_tools_for_agent_full(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<Tool>>;

    /// 添加工具到 Agent
    async fn add_tool_to_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        tool_id: &str,
        created_by: Option<String>,
    ) -> Result<()>;

    /// 从 Agent 移除工具
    async fn remove_tool_from_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        tool_id: &str,
    ) -> Result<()>;

    /// 同步所有注册的内置工具到数据库
    /// 已存在的工具（按 ID）跳过，避免重复
    /// 返回新增的工具数量
    async fn sync_builtin_tools_to_db(&self, ctx: RequestContext) -> Result<usize>;

    /// 执行已获取的工具并返回调用追踪 entry
    ///
    /// 统一执行入口：Auto/Manual/普通工具都走此路径（domain → DAL 统一传参，D26）。
    /// 内部 per-call 经 registry 重组装新实例（D22 单次实例，不复用调用方
    /// 预装配实例）→ `CoreTool::check` 注入凭据 → 转发到 `ToolCallDao::execute`
    /// （幂等防重/trace/AOP 全保留），由 DAO 层通过 AOP ToolExecEvent 捕获 trace。
    async fn call_tool(
        &self,
        ctx: RequestContext,
        request: ToolExecutionRequest,
    ) -> Result<(Value, ToolCallEntry)>;

    /// 执行 Auto 工具调用
    ///
    /// 统一入口：awakening 循环调此方法执行 Auto 工具。
    /// 内部调用 call_tool（直接执行层，含装饰器）。
    async fn execute_auto(
        &self,
        ctx: RequestContext,
        tool: &Tool,
        args: Value,
    ) -> Result<(Value, ToolCallEntry)> {
        self.call_tool(
            ctx,
            ToolExecutionRequest {
                tool: tool.po.clone(),
                args,
                resolved: Vec::new(),
            },
        )
        .await
    }

    /// 执行 Manual 工具调用（通过特殊 tool 转发）
    ///
    /// 统一入口：awakening 循环调此方法执行 Manual 工具。
    /// ToolDalImpl 重写此方法，根据 dispatch_mode 分发：
    /// - sync（默认）：通过 request_tool_call 特殊 tool 转发（同步执行）
    /// - async：通过 send_tool_call_message 特殊 tool 转发（异步派发）
    ///
    /// 默认实现直接调 call_tool（直接执行层），适用于不需要特殊转发的场景（如测试 mock）。
    /// 特殊 tool 不加装饰器（它只是转发器），真实工具的 trace 在 call_tool 层记录。
    async fn execute_manual(
        &self,
        ctx: RequestContext,
        tool: &Tool,
        args: Value,
    ) -> Result<(Value, ToolCallEntry)> {
        self.call_tool(
            ctx,
            ToolExecutionRequest {
                tool: tool.po.clone(),
                args,
                resolved: Vec::new(),
            },
        )
        .await
    }

    /// 搜索工具（向量 + 关键词混合搜索）
    async fn search(
        &self,
        ctx: RequestContext,
        params: crate::service::dao::tool::ToolSearch,
    ) -> Result<common::api::PagedResult<Tool>>;

    // ==================== 统计查询 ====================

    /// 获取工具统计数据
    async fn get_stats(
        &self,
        ctx: RequestContext,
        tool_id: &str,
        options: StatsFetchOptions,
    ) -> Result<ToolStats>;

    /// 🔄 重建所有工具的向量索引
    ///
    /// 清空向量集合后，查询全量工具，逐条重新生成 embedding 并 upsert。
    /// 单条失败不影响整体，用 log_warn! 记录。
    async fn rebuild_vectors(&self, ctx: RequestContext) -> Result<()>;
}

// ==================== DAL 实现 ====================

/// Tool DAL 基础实现
pub struct ToolDalImpl {
    tool_dao: Arc<dyn ToolDao + Send + Sync>,
    tool_call_dao: Arc<dyn ToolCallDao + Send + Sync>,
    tool_vector_dao: Arc<dyn ToolVectorDao + Send + Sync>,
    model_provider_dao: Arc<dyn ModelProviderDao + Send + Sync>,
    cortex_dao: Arc<dyn CortexDao + Send + Sync>,
    tool_stats_dao: Arc<dyn ToolStatsDao<ToolCallEvent = crate::pkg::stats::ToolCallEvent>>,
}

#[async_trait::async_trait]
impl ToolDal for ToolDalImpl {
    async fn create_tool(&self, ctx: RequestContext, po: &ToolPo) -> Result<()> {
        self.tool_dao.create_tool(ctx.clone(), po).await?;

        // 向量索引自动维护（失败仅 warn 降级，不影响主流程）
        match try_build_vector_params_for_entity(
            ctx.clone(),
            &self.cortex_dao,
            &self.model_provider_dao,
            po as &dyn Vectorizable,
        )
        .await
        {
            Ok(Some(vec_params)) => {
                if let Err(e) = self
                    .tool_vector_dao
                    .upsert_vector(ctx.clone(), &po.id, &vec_params)
                    .await
                {
                    log_warn!(
                        &ctx,
                        "vector_index",
                        tool_id = %po.id,
                        error = ?e,
                        "工具向量索引写入失败，已降级"
                    );
                }
            }
            Ok(None) => {
                log_debug!(
                    &ctx,
                    "vector_index",
                    tool_id = %po.id,
                    "无可用 Embedding Provider，跳过工具向量索引"
                );
            }
            Err(e) => {
                log_warn!(
                    &ctx,
                    "vector_index",
                    tool_id = %po.id,
                    error = ?e,
                    "工具向量化失败，已降级"
                );
            }
        }

        Ok(())
    }

    async fn update_tool(&self, ctx: RequestContext, tool: &Tool) -> Result<()> {
        self.tool_dao.update_tool(ctx.clone(), &tool.po).await?;

        // 向量索引自动维护：内容变化时重新索引
        let old_hash = self
            .tool_vector_dao
            .get_vector_row(ctx.clone(), &tool.po.id)
            .await?
            .map(|r| r.meta.content_hash);
        let new_hash = tool.po.vector_content_hash();

        if old_hash.as_deref() != Some(&new_hash) {
            match try_build_vector_params_for_entity(
                ctx.clone(),
                &self.cortex_dao,
                &self.model_provider_dao,
                &tool.po as &dyn Vectorizable,
            )
            .await
            {
                Ok(Some(vec_params)) => {
                    if let Err(e) = self
                        .tool_vector_dao
                        .upsert_vector(ctx.clone(), &tool.po.id, &vec_params)
                        .await
                    {
                        log_warn!(
                            &ctx,
                            "vector_index",
                            tool_id = %tool.po.id,
                            error = ?e,
                            "工具向量索引更新失败，已降级"
                        );
                    }
                }
                Ok(None) => {
                    log_debug!(
                        &ctx,
                        "vector_index",
                        tool_id = %tool.po.id,
                        "无可用 Embedding Provider，跳过工具向量索引更新"
                    );
                }
                Err(e) => {
                    log_warn!(
                        &ctx,
                        "vector_index",
                        tool_id = %tool.po.id,
                        error = ?e,
                        "工具向量化失败，跳过向量索引更新"
                    );
                }
            }
        }

        Ok(())
    }

    async fn delete_tool(&self, ctx: RequestContext, tool_id: &str) -> Result<()> {
        self.tool_dao.delete_tool(ctx.clone(), tool_id).await?;

        // 删除时清理向量索引
        let _ = self.tool_vector_dao.delete_vector(ctx, tool_id).await;

        Ok(())
    }

    async fn get_by_id(&self, ctx: RequestContext, id: String) -> Result<Option<Tool>> {
        let Some(po) = self.tool_dao.get_by_id(ctx, id).await? else {
            return Ok(None);
        };
        let Some(our_tool) = self.tool_call_dao.assemble_core_tool(&po)? else {
            if matches!(po.protocol, common::enums::ToolProtocol::Builtin) {
                return Ok(None);
            }
            return Ok(Some(Tool::from_po_for_management(po)));
        };
        Ok(Some(Tool {
            po,
            our_tool,
            search_match: None,
            stats: None,
        }))
    }

    async fn get_tool(
        &self,
        ctx: RequestContext,
        id: &str,
        options: ToolFetchOptions,
    ) -> Result<Option<Tool>> {
        // Step 1: 获取基础 Tool 实体
        let mut tool = self.get_by_id(ctx.clone(), id.to_string()).await?;

        if let Some(ref mut tool) = tool {
            // Step 2: 按 options 注入 stats
            if options.with_stats.unwrap_or(false) {
                let stats_options = StatsFetchOptions {
                    with_call_summary: true,
                    time_range: options.stats_time_range,
                    ..Default::default()
                };

                match self.get_stats(ctx.clone(), id, stats_options).await {
                    Ok(stats) => {
                        tool.stats = Some(stats);
                    }
                    Err(e) => {
                        log_warn!(&ctx, "get_tool", tool_id = %id, error = ?e, "工具统计注入失败，已降级");
                    }
                }
            }
        }

        Ok(tool)
    }

    async fn get_by_name(&self, ctx: RequestContext, name: &str) -> Result<Option<Tool>> {
        let Some(po) = self.tool_dao.get_by_name(ctx, name).await? else {
            return Ok(None);
        };
        let Some(our_tool) = self.tool_call_dao.assemble_core_tool(&po)? else {
            if matches!(po.protocol, common::enums::ToolProtocol::Builtin) {
                return Ok(None);
            }
            return Ok(Some(Tool::from_po_for_management(po)));
        };
        Ok(Some(Tool {
            po,
            our_tool,
            search_match: None,
            stats: None,
        }))
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: ToolQuery,
    ) -> Result<common::api::PagedResult<Tool>> {
        let query = exclude_stale_by_default(query);
        let page = self.tool_dao.query(ctx, query).await?;
        let total = page.total;
        let mut tools = Vec::new();
        for po in page.items {
            if let Some(our_tool) = self.tool_call_dao.assemble_core_tool(&po)? {
                tools.push(Tool {
                    po,
                    our_tool,
                    search_match: None,
                    stats: None,
                });
                continue;
            }
            if !matches!(po.protocol, common::enums::ToolProtocol::Builtin) {
                tools.push(Tool::from_po_for_management(po));
            }
        }
        Ok(common::api::PagedResult {
            items: tools,
            total,
        })
    }

    async fn list_enabled(&self, ctx: RequestContext) -> Result<Vec<Tool>> {
        let page = self
            .query(
                ctx,
                ToolQuery {
                    enabled_only: Some(true),
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    async fn list_tags(&self, ctx: RequestContext) -> Result<Vec<String>> {
        self.tool_dao.list_distinct_tags(ctx).await
    }

    async fn list_tools_for_agent_full(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<Tool>> {
        let page = self
            .query(
                ctx,
                ToolQuery {
                    agent_id: Some(agent_id.to_string()),
                    enabled_only: Some(true),
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    async fn add_tool_to_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        tool_id: &str,
        created_by: Option<String>,
    ) -> Result<()> {
        let ctx = ctx.to_builder().agent_id(agent_id).build();
        Ok(self
            .tool_dao
            .add_tool_to_agent(ctx, agent_id, tool_id, created_by)
            .await?)
    }

    async fn remove_tool_from_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        tool_id: &str,
    ) -> Result<()> {
        let ctx = ctx.to_builder().agent_id(agent_id).build();
        Ok(self
            .tool_dao
            .remove_tool_from_agent(ctx, agent_id, tool_id)
            .await?)
    }

    async fn sync_builtin_tools_to_db(&self, ctx: RequestContext) -> Result<usize> {
        Ok(self.tool_dao.sync_builtin_tools_to_db(ctx).await?)
    }

    async fn search(
        &self,
        ctx: RequestContext,
        params: crate::service::dao::tool::ToolSearch,
    ) -> Result<common::api::PagedResult<Tool>> {
        // 向量距离阈值（默认 0.8，可由调用方覆盖）
        const DEFAULT_VECTOR_DISTANCE_THRESHOLD: f32 = 0.8;
        let vector_distance_threshold = params
            .vector_distance_threshold
            .unwrap_or(DEFAULT_VECTOR_DISTANCE_THRESHOLD);
        // 向量搜索 Top K（默认 20，与 FTS5 限制一致）
        let top_k = if params.top_k > 0 {
            params.top_k as i32
        } else {
            20
        };
        // 提前捕获分页参数（params 后续会被 move 给 search_tools）
        let pagination = params.filters.pagination.clone();

        // Step 1: 准备向量搜索结果容器
        let mut vector_scores: std::collections::HashMap<String, f32> =
            std::collections::HashMap::new();
        let mut vector_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Step 2: 如果有关键词，尝试向量搜索（用关键词生成 query vector）
        if let Some(keyword) = &params.keyword
            && let Some(provider) = self
                .model_provider_dao
                .get_default_embedding_provider(ctx.clone())
                .await?
        {
            let query_vector_params = self
                .cortex_dao
                .embed_text_for_search(ctx.clone(), &provider, keyword)
                .await?;
            let query_vector = query_vector_params.vector;

            match self
                .tool_vector_dao
                .search_vector(ctx.clone(), &query_vector, top_k)
                .await
            {
                Ok(vector_results) => {
                    let filtered_results: Vec<(String, f32)> = vector_results
                        .into_iter()
                        .filter(|hit| hit.distance < vector_distance_threshold)
                        .map(|hit| (hit.row.id, hit.distance))
                        .collect();

                    vector_ids = filtered_results.iter().map(|(id, _)| id.clone()).collect();
                    vector_scores = filtered_results.into_iter().collect();
                }
                Err(e) => {
                    log_warn!(
                        ctx.clone(),
                        "vector_search",
                        "Tool vector search failed: {}, fallback to keyword only",
                        e
                    );
                }
            }
        }

        // Step 3: FTS5 关键词搜索（DAO 返回 Vec<(ToolPo, fts_rank)>）
        let fts_results = self.tool_dao.search_tools(ctx.clone(), params).await?;

        // 提取 fts_rank 并转换为 Vec<ToolPo> 便于聚合
        let mut fts_ranks: std::collections::HashMap<String, f32> =
            std::collections::HashMap::new();
        let keyword_pos: Vec<ToolPo> = fts_results
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
                let query = crate::service::dao::tool::ToolQuery {
                    ids: Some(ids_to_fetch),
                    exclude_status: Some(ToolStatus::Stale),
                    ..Default::default()
                };
                let vector_pos = self.tool_dao.query(ctx.clone(), query).await?;
                all_pos.extend(vector_pos.items);
            }
        }

        // Step 5: 去重
        all_pos.sort_by(|a, b| a.id.cmp(&b.id));
        all_pos.dedup_by(|a, b| a.id == b.id);

        // Step 6: 构建业务对象（三态匹配：Hybrid / Vector / Keyword）
        let mut tools = Vec::with_capacity(all_pos.len());
        for po in all_pos {
            let Some(our_tool) = self.tool_call_dao.assemble_core_tool(&po)? else {
                continue;
            };

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

            tools.push(Tool {
                po,
                our_tool,
                search_match: match_info,
                stats: None,
            });
        }

        // Step 7: 统一排序：Hybrid 优先 → Vector 次之 → Keyword/None 最后
        //    组内排序：Hybrid/Vector 按向量距离升序，Keyword 按 fts_rank 升序（BM25 越小越相关）
        tools.sort_by(|a, b| {
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
        tools.truncate(20);

        let total = tools.len();
        let offset = pagination.offset.unwrap_or(0);
        let limit = pagination.limit.unwrap_or(20);
        let items = tools.into_iter().skip(offset).take(limit).collect();
        Ok(common::api::PagedResult { items, total })
    }

    async fn call_tool(
        &self,
        ctx: RequestContext,
        request: ToolExecutionRequest,
    ) -> Result<(Value, ToolCallEntry)> {
        // per-call 经 registry 重组装新实例（D22 单次实例：不复用调用方
        // 预装配的 request.tool 来源实例）；check 在实例装入 Tool 前完成
        // （局部 mut），Tool 结构无需 mut。
        let Some(mut our_tool) = self.tool_call_dao.assemble_core_tool(&request.tool)? else {
            bail_err!(
                ToolExecutionFailed,
                "Tool {} is not executable",
                request.tool.id
            );
        };
        our_tool.check(&request.resolved)?;
        let tool = Tool {
            po: request.tool,
            our_tool,
            search_match: None,
            stats: None,
        };
        self.tool_call_dao
            .execute(ctx, &tool, request.args)
            .await
            .map_err(Into::into)
    }

    async fn execute_auto(
        &self,
        ctx: RequestContext,
        tool: &Tool,
        args: Value,
    ) -> Result<(Value, ToolCallEntry)> {
        // Auto 工具直接执行（call_tool 内部含装饰器）；
        // 直连入口不经 domain 凭据编排，注入空集（check 按空集放行）
        self.call_tool(
            ctx,
            ToolExecutionRequest {
                tool: tool.po.clone(),
                args,
                resolved: Vec::new(),
            },
        )
        .await
    }

    async fn execute_manual(
        &self,
        ctx: RequestContext,
        tool: &Tool,
        args: Value,
    ) -> Result<(Value, ToolCallEntry)> {
        let mode = parse_dispatch_mode(tool);
        let special_tool_id = match mode {
            "async" => "send_tool_call_message",
            _ => "request_tool_call",
        };

        // 通过 registry 创建特殊 tool 实例（不加装饰器，特殊 tool 只是转发器）
        let special_po = ToolPo::new(
            special_tool_id.to_string(),
            special_tool_id.to_string(),
            "manual tool dispatcher".to_string(),
            common::enums::ToolProtocol::Builtin,
            serde_json::Value::Null,
            None,
            vec![],
            None,
        );
        let special_tool = crate::pkg::tool_registry::get_registry()
            .create_tool(special_po)
            .ok_or_else(|| {
                common::error::err!(
                    Internal,
                    "special tool {} not found in registry",
                    special_tool_id
                )
            })?;

        // 组织参数：把真实工具调用包装成特殊 tool 的参数格式
        // RequestToolCallParams / SendToolCallMessageParams 字段一致：
        // tool_id, tool_name, params, project_id, task_id（agent_id 由 ctx 携带）
        let special_args = serde_json::json!({
            "tool_id": tool.po.id,
            "tool_name": tool.po.name,
            "params": args,
            "project_id": ctx.project_id().cloned(),
            "task_id": ctx.task_id().cloned(),
        });

        // 调用特殊 tool（不加装饰器）
        // 特殊 tool handler 内部会调 call_manual_tool_for_agent → call_tool（含装饰器）
        let result_value = special_tool.call(ctx, special_args).await?;

        // 构造占位 entry（真实 trace 在 call_tool 层记录）
        let entry = ToolCallEntry {
            call_id: format!(
                "manual_dispatch_{}_{}",
                special_tool_id,
                uuid::Uuid::now_v7()
            ),
            tool_id: tool.po.id.clone(),
            ..Default::default()
        };

        Ok((result_value, entry))
    }

    async fn get_stats(
        &self,
        ctx: RequestContext,
        tool_id: &str,
        options: StatsFetchOptions,
    ) -> Result<ToolStats> {
        let query = ToolStatsQuery {
            tool_id: tool_id.to_string(),
            ..Default::default()
        };
        Ok(self.tool_stats_dao.get_stats(ctx, query, options).await?)
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
        let collection_name = "tools";
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
        self.tool_vector_dao.clear_collection(ctx.clone()).await?;

        // 4. 查全量工具 PO（排除 Stale 状态）并逐条重新索引
        let pos = self
            .tool_dao
            .query(
                ctx.clone(),
                ToolQuery {
                    exclude_status: Some(ToolStatus::Stale),
                    ..Default::default()
                },
            )
            .await?;

        for po in &pos.items {
            match try_build_vector_params_for_entity(
                ctx.clone(),
                &self.cortex_dao,
                &self.model_provider_dao,
                po as &dyn Vectorizable,
            )
            .await
            {
                Ok(Some(vec_params)) => {
                    if let Err(e) = self
                        .tool_vector_dao
                        .upsert_vector(ctx.clone(), &po.id, &vec_params)
                        .await
                    {
                        log_warn!(
                            &ctx,
                            "rebuild_vectors",
                            tool_id = %po.id,
                            error = ?e,
                            "工具向量索引重建失败"
                        );
                    }
                }
                Ok(None) => {
                    log_debug!(
                        &ctx,
                        "rebuild_vectors",
                        tool_id = %po.id,
                        "无可用 Embedding Provider，跳过向量索引"
                    );
                }
                Err(e) => {
                    log_warn!(
                        &ctx,
                        "rebuild_vectors",
                        tool_id = %po.id,
                        error = ?e,
                        "工具向量化失败，跳过"
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

fn exclude_stale_by_default(mut query: ToolQuery) -> ToolQuery {
    if query.status.is_none() && query.exclude_status.is_none() {
        query.exclude_status = Some(ToolStatus::Stale);
    }
    query
}

/// 从 ToolPo.config 解析 dispatch_mode，默认 "sync"
///
/// 复用现有 config JSON 字段表达同步/异步属性，不改 schema。
/// config 示例：{ "dispatch_mode": "async" }
fn parse_dispatch_mode(tool: &Tool) -> &'static str {
    if let Some(mode) = tool.po.config.get("dispatch_mode").and_then(|v| v.as_str())
        && mode == "async"
    {
        return "async";
    }
    "sync"
}

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
