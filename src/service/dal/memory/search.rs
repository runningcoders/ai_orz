//! 混合搜索与关系型查询（关键词 + 向量语义 + 结果聚合排序）

use super::{MemoryDalImpl, drop_dangling_relations};
use crate::models::memory::{LongTermKnowledgeNodePo, Memory, MemoryPo, ShortTermMemoryIndexPo};
use crate::models::vector::{MatchType, SearchMatchInfo, VectorIndexParams};
use crate::pkg::RequestContext;
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::memory::{MemoryQuery, MemorySearch};
use crate::service::dao::model_provider::ModelProviderDao;
use common::enums::MemoryType;
use common::error::Result;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

impl MemoryDalImpl {
    pub(super) async fn search(
        &self,
        ctx: RequestContext,
        search: MemorySearch,
    ) -> Result<Vec<Memory>> {
        let memory_type = search.filters.memory_type.unwrap_or(MemoryType::All);
        let mut results: Vec<Memory> = Vec::new();

        // 1. 搜索短期记忆
        if memory_type == MemoryType::All || memory_type == MemoryType::ShortTerm {
            // 短期记忆按**归属**隔离：调用方没给作用域时回退到「请求上下文里自己的 Agent」，
            // 这样 Agent 的检索天然只看到自己的便签。
            // ⚠️ 知识节点**不做**这个回退：它是蜂巢共享资产，回退会让 Agent 只搜到自己的节点。
            // ⚠️ 上下文里也没有归属时（人类在前端浏览），这里保持**不过滤**的既有行为，
            // 不在本次改动里收紧 —— 见 `private_agent_scope` 的说明。
            let mut short_term_search = search.clone();
            short_term_search.filters = private_agent_scope(&ctx, search.filters.clone());
            let short_term_results = self
                .search_short_term_internal(ctx.clone(), short_term_search)
                .await?;
            results.extend(short_term_results);
        }

        // 2. 搜索知识节点
        if memory_type == MemoryType::All || memory_type == MemoryType::KnowledgeNode {
            let knowledge_results = self
                .search_knowledge_nodes_internal(ctx.clone(), search.clone())
                .await?;
            results.extend(knowledge_results);
        }

        // 3. Relation 类型不支持向量搜索，但支持关键词查询（如果有关键词）
        if (memory_type == MemoryType::All || memory_type == MemoryType::Relation)
            && search.keyword.is_some()
        {
            let relation_results = self
                .search_relations_internal(ctx.clone(), search.clone())
                .await?;
            results.extend(relation_results);
        }

        // 3.5 图批次不变式：边必须两端节点同在批内。
        // `search_relations_internal` 会把命中节点的全部入/出边一并带出，
        // 其中「远端节点没命中」的边必须在这里丢掉，否则前端只能把远端画成
        // 「未命名节点」、LLM 拿到「A → ?」的半条信息。
        drop_dangling_relations(&mut results);

        // 4. 统一排序：Hybrid 优先 → Vector 次之 → Keyword/None 最后
        //    组内排序：Hybrid/Vector 按向量距离升序，Keyword 按 fts_rank 升序（BM25 越小越相关）
        results.sort_by(|a, b| {
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

        // 5. 应用 limit
        if let Some(limit) = search.filters.limit {
            results.truncate(limit);
        }

        Ok(results)
    }

    pub(super) async fn query(
        &self,
        ctx: RequestContext,
        query: MemoryQuery,
    ) -> Result<Vec<Memory>> {
        let memory_type = query.memory_type.unwrap_or(MemoryType::All);
        let mut results: Vec<Memory> = Vec::new();

        // 1. 查询短期记忆（用 DAO 的通用 query
        //    短期记忆按归属隔离：作用域缺省时回退 ctx 自己的 Agent（Agent 调用天然只看自己）；
        //    ctx 里也没有归属时（人类浏览全局页面）保持**不过滤**的既有行为。
        //    知识节点的蜂巢共享**不**延伸到短期记忆。
        if memory_type == MemoryType::All || memory_type == MemoryType::ShortTerm {
            let pos = self
                .memory_dao
                .query_short_term(ctx.clone(), private_agent_scope(&ctx, query.clone()))
                .await?;
            results.extend(pos.into_iter().map(|po| Memory {
                po: MemoryPo::ShortTerm(po),
                search_match: None,
            }));
        }

        // 2. 查询知识节点
        if memory_type == MemoryType::All || memory_type == MemoryType::KnowledgeNode {
            let pos = self
                .memory_dao
                .query_knowledge_nodes(ctx.clone(), query.clone())
                .await?;
            results.extend(pos.into_iter().map(|po| Memory {
                po: MemoryPo::KnowledgeNode(po),
                search_match: None,
            }));
        }

        // 3. 查询关系（按 ids 精确取边；关系是依附节点的派生视图，
        //    无 ids 时 DAO 返回空 —— All 的列表语义天然不包含独立边）
        if memory_type == MemoryType::All || memory_type == MemoryType::Relation {
            let pos = self
                .memory_dao
                .query_knowledge_relations(ctx.clone(), query)
                .await?;
            results.extend(pos.into_iter().map(|po| Memory {
                po: MemoryPo::Relation(po),
                search_match: None,
            }));
        }

        Ok(results)
    }

    /// 搜索短期记忆（内部实现）
    async fn search_short_term_internal(
        &self,
        ctx: RequestContext,
        search: MemorySearch,
    ) -> Result<Vec<Memory>> {
        // 向量距离阈值（可配置，默认 0.8）
        let vector_distance_threshold = search.vector_distance_threshold.unwrap_or(0.8);
        // 向量召回条数（可配置，默认 50）
        let vector_top_k = search.top_k.unwrap_or(50);

        // Step 1: 准备向量搜索结果容器
        let mut vector_scores: HashMap<String, f32> = HashMap::new();
        let mut vector_ids: HashSet<String> = HashSet::new();

        // Step 2: 如果有关键词，执行向量搜索（用 try_build_vector_params 统一方式）
        if search.keyword.is_some()
            && let Some(keyword) = &search.keyword
        {
            match try_build_vector_params_for_search(
                ctx.clone(),
                &self.cortex_dao,
                &self.model_provider_dao,
                keyword,
            )
            .await
            {
                Ok(Some(vec_params)) => {
                    // 向量搜索（前 top_k 条；业务过滤在 DAO 内转译为向量谓词下推）
                    match self
                        .memory_vector_dao
                        .search_short_term_vector(
                            ctx.clone(),
                            &vec_params.vector,
                            vector_top_k,
                            &search.filters,
                        )
                        .await
                    {
                        Ok(vector_results) => {
                            // 过滤距离小于阈值的结果
                            let filtered_results: Vec<(String, f32)> = vector_results
                                .into_iter()
                                .filter(|hit| hit.distance < vector_distance_threshold)
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
                                "短期记忆向量搜索失败，降级到关键词搜索: {}",
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
                    log_warn!(ctx, "vector_search", error = ?e, "短期记忆向量化失败，跳过向量搜索");
                }
            }
        }

        // Step 3: 执行关键词搜索（DAO 返回 Vec<(Po, fts_rank)>）
        let keyword_results = self
            .memory_dao
            .search_short_term(ctx.clone(), search.clone())
            .await?;

        // 提取 fts_rank 并转换为 Vec<Po> 便于聚合
        let mut fts_ranks: HashMap<String, f32> = HashMap::new();
        let keyword_pos: Vec<ShortTermMemoryIndexPo> = keyword_results
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

        if !vector_ids.is_empty() {
            let ids_to_fetch: Vec<String> = vector_ids
                .into_iter()
                .filter(|id| !keyword_pos.iter().any(|po| po.id == *id))
                .collect();

            if !ids_to_fetch.is_empty() {
                // 用通用 query 批量获取 ids_to_fetch 的结果
                let mut query_for_ids = search.filters.clone();
                query_for_ids.ids = Some(ids_to_fetch);
                let vector_pos = self
                    .memory_dao
                    .query_short_term(ctx.clone(), query_for_ids)
                    .await?;
                all_pos.extend(vector_pos);
            }
        }

        // Step 5: 去重
        all_pos.sort_by(|a, b| a.id.cmp(&b.id));
        all_pos.dedup_by(|a, b| a.id == b.id);

        // Step 6: 构建业务对象
        let mut memories = Vec::with_capacity(all_pos.len());
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
            memories.push(Memory {
                po: MemoryPo::ShortTerm(po),
                search_match: match_info,
            });
        }

        Ok(memories)
    }

    /// 搜索知识节点（内部实现）
    async fn search_knowledge_nodes_internal(
        &self,
        ctx: RequestContext,
        search: MemorySearch,
    ) -> Result<Vec<Memory>> {
        // 向量距离阈值（可配置，默认 0.8）
        let vector_distance_threshold = search.vector_distance_threshold.unwrap_or(0.8);
        // 向量召回条数（可配置，默认 50）
        let vector_top_k = search.top_k.unwrap_or(50);

        // Step 1: 准备向量搜索结果容器
        let mut vector_scores: HashMap<String, f32> = HashMap::new();
        let mut vector_ids: HashSet<String> = HashSet::new();

        // Step 2: 如果有关键词，执行向量搜索（用 try_build_vector_params 统一方式）
        if search.keyword.is_some()
            && let Some(keyword) = &search.keyword
        {
            match try_build_vector_params_for_search(
                ctx.clone(),
                &self.cortex_dao,
                &self.model_provider_dao,
                keyword,
            )
            .await
            {
                Ok(Some(vec_params)) => {
                    // 向量搜索（前 top_k 条；业务过滤在 DAO 内转译为向量谓词下推）
                    match self
                        .memory_vector_dao
                        .search_knowledge_node_vector(
                            ctx.clone(),
                            &vec_params.vector,
                            vector_top_k,
                            &search.filters,
                        )
                        .await
                    {
                        Ok(vector_results) => {
                            // 过滤距离小于阈值的结果
                            let filtered_results: Vec<(String, f32)> = vector_results
                                .into_iter()
                                .filter(|hit| hit.distance < vector_distance_threshold)
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
                                "知识节点向量搜索失败，降级到关键词搜索: {}",
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
                    log_warn!(ctx, "vector_search", error = ?e, "知识节点向量化失败，跳过向量搜索");
                }
            }
        }

        // Step 3: 执行关键词搜索（DAO 返回 Vec<(Po, fts_rank)>）
        let keyword_results = self
            .memory_dao
            .search_knowledge_nodes(ctx.clone(), search.clone())
            .await?;

        // 提取 fts_rank 并转换为 Vec<Po> 便于聚合
        let mut fts_ranks: HashMap<String, f32> = HashMap::new();
        let keyword_pos: Vec<LongTermKnowledgeNodePo> = keyword_results
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

        if !vector_ids.is_empty() {
            let ids_to_fetch: Vec<String> = vector_ids
                .into_iter()
                .filter(|id| !keyword_pos.iter().any(|po| po.id == *id))
                .collect();

            if !ids_to_fetch.is_empty() {
                // 用通用 query 批量获取 ids_to_fetch 的结果
                let mut query_for_ids = search.filters.clone();
                query_for_ids.ids = Some(ids_to_fetch);
                let vector_pos = self
                    .memory_dao
                    .query_knowledge_nodes(ctx.clone(), query_for_ids)
                    .await?;
                all_pos.extend(vector_pos);
            }
        }

        // Step 5: 去重
        all_pos.sort_by(|a, b| a.id.cmp(&b.id));
        all_pos.dedup_by(|a, b| a.id == b.id);

        // Step 6: 构建业务对象
        let mut nodes = Vec::with_capacity(all_pos.len());
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
            nodes.push(Memory {
                po: MemoryPo::KnowledgeNode(po),
                search_match: match_info,
            });
        }

        Ok(nodes)
    }

    /// 搜索关系（通过知识节点 FTS5 间接搜索关系）
    ///
    /// 关系表无独立 FTS 索引，因此先搜索匹配的知识节点，
    /// 再查询这些节点关联的所有关系（出入边），一并返回。
    ///
    /// ⚠️ 这里必然会带出「远端节点没命中」的边，**不在这里过滤**：
    /// 由调用方 `search` 统一过 `drop_dangling_relations`（图批次不变式）。
    async fn search_relations_internal(
        &self,
        ctx: RequestContext,
        search: MemorySearch,
    ) -> Result<Vec<Memory>> {
        let keyword = search.keyword.as_deref().unwrap_or("");
        if keyword.trim().is_empty() {
            return Ok(Vec::new());
        }

        // 1. 用 FTS5 搜索匹配的知识节点
        let nodes = self
            .memory_dao
            .search_knowledge_nodes(ctx.clone(), search.clone())
            .await?;
        let node_ids: Vec<String> = nodes.iter().map(|(n, _)| n.id.clone()).collect();

        if node_ids.is_empty() {
            return Ok(Vec::new());
        }

        // 2. 查询这些节点的所有关系（出入边）
        let relations = self
            .memory_dao
            .list_relations_batch(ctx.clone(), &node_ids)
            .await?;

        // 3. 构建 Memory 列表（KnowledgeNode + Relation）
        let mut memories = Vec::with_capacity(nodes.len() + relations.len());
        for (node, fts_rank) in nodes {
            memories.push(Memory {
                po: MemoryPo::KnowledgeNode(node),
                search_match: Some(SearchMatchInfo {
                    match_type: MatchType::Keyword,
                    fts_rank,
                    ..Default::default()
                }),
            });
        }
        for rel in relations {
            memories.push(Memory {
                po: MemoryPo::Relation(rel),
                search_match: None,
            });
        }

        Ok(memories)
    }
}

/// 私有资产（短期记忆）的作用域补全：调用方没显式给归属时，回退到「本次请求自己的 Agent」。
///
/// 为什么只有私有资产能回退、知识节点不能：
/// - 短期记忆是 Agent 的工作便签，天然属于请求发起者。Agent 调 `search_memory` 时 ctx 里有
///   自己的 Agent 身份 → 自动收窄成「只看自己的便签」，与改动前行为一致。
/// - 知识节点是**蜂巢共享**资产。若也回退 ctx，Agent 调 `search_memory` 时会静默收窄成
///   「只看自己沉淀的节点」，与「知识不重复不遗漏」的蜂巢语义正好相反。
///
/// ⚠️ 回退后仍为空（人类在前端浏览，ctx 里没有 Agent 身份）时**保持不过滤**——这是本次
/// 改动之前就有的行为（`Some("")` 被 DAO 当作「不筛选」），刻意不在这次一起收紧：
/// 收紧会让「记忆搜索」页在不选 Agent 时查不到任何短期记忆，属于另一条独立的产品决策。
fn private_agent_scope(ctx: &RequestContext, mut query: MemoryQuery) -> MemoryQuery {
    if query.agent_id.is_none() {
        query.agent_id = ctx.agent_id().cloned().filter(|s| !s.is_empty());
    }
    query
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

    let params = cortex_dao
        .embed_text_for_search(ctx.clone(), &provider, text)
        .await?;
    Ok(Some(params))
}
