//! Memory DAL - 记忆数据访问层（业务逻辑层）
//!
//! 职责：跨 DAO 流程编排
//! - 获取 Embedding Provider
//! - 生成查询向量
//! - 混合搜索（全文 + 向量）
//! - 结果聚合排序

use crate::models::memory::{
    KnowledgeNodeRelationPo, KnowledgeReferencePo, LongTermKnowledgeNodePo, Memory,
    MemoryCreateParams, MemoryPo, MemoryTrace, ShortTermMemoryIndexPo,
};
use crate::models::vector::{MatchType, SearchMatchInfo, VectorIndexParams, Vectorizable};
use crate::pkg::RequestContext;
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::memory::{MemoryDao, MemoryQuery, MemorySearch, MemoryVectorDao};
use crate::service::dao::model_provider::ModelProviderDao;
use async_trait::async_trait;
use common::enums::MemoryStatus;
use common::enums::MemoryType;
use common::error::{Result, bail_err};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraversalStrategy {
    BreadthFirst,
    DepthFirst,
}

/// 遍历过程中的可变状态包：把 3 个 &mut 累积器打包成单个 struct，
/// 用于 `traverse_bfs` / `traverse_dfs` 内部方法的参数瘦身。
struct TraverseState<'a> {
    visited_nodes: &'a mut HashSet<String>,
    visited_relations: &'a mut HashSet<String>,
    result_relations: &'a mut Vec<KnowledgeNodeRelationPo>,
}

// ==================== Factory + Singleton ====================

static MEMORY_DAL_INSTANCE: std::sync::OnceLock<Arc<dyn MemoryDal>> = std::sync::OnceLock::new();

pub fn new(
    memory_dao: Arc<dyn MemoryDao>,
    memory_vector_dao: Arc<dyn MemoryVectorDao>,
    model_provider_dao: Arc<dyn ModelProviderDao>,
    cortex_dao: Arc<dyn CortexDao>,
) -> Arc<dyn MemoryDal> {
    Arc::new(MemoryDalImpl {
        memory_dao,
        memory_vector_dao,
        model_provider_dao,
        cortex_dao,
    })
}

pub fn init() {
    let _ = MEMORY_DAL_INSTANCE.set(new(
        crate::service::dao::memory::dao(),
        crate::service::dao::memory::vector_dao(),
        crate::service::dao::model_provider::dao(),
        crate::service::dao::cortex::dao(),
    ));
}

pub fn dal() -> Arc<dyn MemoryDal> {
    MEMORY_DAL_INSTANCE.get().cloned().unwrap()
}

// ==================== DAL Trait ====================

#[async_trait]
pub trait MemoryDal: Send + Sync {
    /// 🔍 统一混合搜索（关键词 + 向量语义）
    ///
    /// 自动根据参数选择搜索策略：
    /// - keyword 存在 → 走传统全文检索
    /// - query_vector 存在 → 走向量语义搜索
    /// - 两者都有 → 混合搜索，合并结果
    /// - memory_type 过滤 → 只搜索指定类型
    async fn search(&self, ctx: RequestContext, search: MemorySearch) -> Result<Vec<Memory>>;

    /// 📋 通用关系型查询（纯数据库查询，无向量）
    ///
    /// 支持所有组合过滤条件，可单独指定查询哪种记忆类型
    async fn query(&self, ctx: RequestContext, query: MemoryQuery) -> Result<Vec<Memory>>;

    /// 🎯 推荐知识图谱起点节点
    ///
    /// 按节点关联度数（入边 + 出边总数）倒序返回 Top N 节点。
    /// 用于知识图谱页面"推荐起点"功能，帮助用户快速定位核心节点。
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - agent_id: 指定 Agent ID；None 时跨 Agent 全局推荐（仅 published 节点）
    /// - limit: 返回数量上限，默认 5
    async fn recommend_seed_nodes(
        &self,
        ctx: RequestContext,
        agent_id: Option<String>,
        limit: usize,
    ) -> Result<Vec<crate::models::memory::SeedNodeRecommendation>>;

    /// ✍️ 创建记忆（按 MemoryCreateParams 变体分发）
    ///
    /// 聚合流程：
    /// - `Trace` / `BatchTrace` → 写 daily JSONL（不向量化）
    /// - `ShortTerm` → 写库 + 向量化 summary（向量失败仅 warn 降级）
    /// - `KnowledgeNode` → 写库 + 写引用 + 向量化 summary（向量失败仅 warn 降级）
    /// - `Relation` → 写库（不向量化）
    async fn create(&self, ctx: RequestContext, params: MemoryCreateParams) -> Result<Vec<Memory>>;

    /// 🔄 更新记忆（仅支持 ShortTerm / KnowledgeNode）
    ///
    /// 自动重新向量化。Trace / Relation 返回 `common::error::Error::Unsupported`。
    async fn update(&self, ctx: RequestContext, memory: Memory) -> Result<Memory>;

    /// 🗑️ 删除记忆（仅支持 ShortTerm / KnowledgeNode）
    ///
    /// 入参为业务实体本身，便于 DAL 内做删除前校验/审计而无需重新查询：
    /// - `ShortTerm` → 删库 + 删向量索引
    /// - `KnowledgeNode` → 级联：删入边/出边关系 + 删引用 + 删节点 + 删向量
    /// - `Trace` / `Relation` → 返回 `common::error::Error::Unsupported`
    async fn delete(&self, ctx: RequestContext, memory: Memory) -> Result<()>;

    /// 🌐 知识图谱遍历
    ///
    /// 从种子节点出发，按指定策略遍历知识图谱
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - seed_node_ids: 种子节点 ID 列表
    /// - max_depth: 最大遍历深度（0=不遍历，直接返回种子节点）
    /// - max_breadth: 每层最大展开数（0=不限制）
    /// - strategy: 遍历策略
    ///
    /// # 返回
    /// - 遍历到的所有 Memory（KnowledgeNode 和 Relation）
    async fn traverse_knowledge_graph(
        &self,
        ctx: RequestContext,
        seed_node_ids: &[String],
        max_depth: i32,
        max_breadth: i32,
        strategy: TraversalStrategy,
    ) -> Result<Vec<Memory>>;

    /// 🏛️ 将未沉淀的短期记忆总结并沉淀为长期知识
    ///
    /// 流程：
    /// 1. 查询 Agent 的活跃短期记忆（status = Active）
    /// 2. 按时间/主题分组聚合
    /// 3. 创建知识节点（summary 作为节点描述）
    /// 4. 创建引用关系（关联原始短期记忆）
    /// 5. 标记短期记忆为已沉淀（status = Settled）
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - agent_id: Agent ID
    /// - limit: 每次处理的短期记忆数量上限
    ///
    /// # 返回
    /// - 成功返回创建的知识节点列表
    async fn settle_short_term_to_long_term(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        limit: usize,
    ) -> Result<Vec<Memory>>;

    /// 🔄 重建所有记忆的向量索引
    ///
    /// 清空 short_term 和 knowledge_node 两个向量集合后，
    /// 查询全量短期记忆和知识节点，逐条重新生成 embedding 并 upsert。
    /// 单条失败不影响整体，用 log_warn! 记录。
    async fn rebuild_vectors(&self, ctx: RequestContext) -> Result<()>;
}

// ==================== Implementation ====================

pub struct MemoryDalImpl {
    memory_dao: Arc<dyn MemoryDao>,
    memory_vector_dao: Arc<dyn MemoryVectorDao>,
    model_provider_dao: Arc<dyn ModelProviderDao>,
    cortex_dao: Arc<dyn CortexDao>,
}

#[async_trait]
impl MemoryDal for MemoryDalImpl {
    async fn search(&self, ctx: RequestContext, search: MemorySearch) -> Result<Vec<Memory>> {
        let memory_type = search.filters.memory_type.unwrap_or(MemoryType::All);
        let mut results: Vec<Memory> = Vec::new();

        // 1. 搜索短期记忆
        if memory_type == MemoryType::All || memory_type == MemoryType::ShortTerm {
            let short_term_results = self
                .search_short_term_internal(ctx.clone(), search.clone())
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

    async fn query(&self, ctx: RequestContext, query: MemoryQuery) -> Result<Vec<Memory>> {
        let memory_type = query.memory_type.unwrap_or(MemoryType::All);
        let mut results: Vec<Memory> = Vec::new();

        // 1. 查询短期记忆（用 DAO 的通用 query
        if memory_type == MemoryType::All || memory_type == MemoryType::ShortTerm {
            let pos = self
                .memory_dao
                .query_short_term(ctx.clone(), query.clone())
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
                .query_knowledge_nodes(ctx.clone(), query)
                .await?;
            results.extend(pos.into_iter().map(|po| Memory {
                po: MemoryPo::KnowledgeNode(po),
                search_match: None,
            }));
        }

        // 3. 查询关系（暂不实现，等后续补充）
        if memory_type == MemoryType::All || memory_type == MemoryType::Relation {
            // 目前 Relation 没有 query_relations 方法，后续补充
        }

        Ok(results)
    }

    async fn recommend_seed_nodes(
        &self,
        ctx: RequestContext,
        agent_id: Option<String>,
        limit: usize,
    ) -> Result<Vec<crate::models::memory::SeedNodeRecommendation>> {
        use crate::models::memory::SeedNodeRecommendation;
        use crate::service::dao::memory::MemoryQuery;
        use common::enums::{MemoryStatus, MemoryType};

        // 1. 拉取知识节点（agent_id 为空时走全局 published 路径）
        let query = MemoryQuery {
            memory_type: Some(MemoryType::KnowledgeNode),
            agent_id: agent_id.clone(),
            status: Some(MemoryStatus::Active),
            exclude_status: Some(MemoryStatus::Forgotten),
            limit: Some(500),     // 上限保护，避免节点过多拖慢统计
            include_shared: true, // 全局推荐时包含 published 节点
            ..Default::default()
        };
        let nodes = self
            .memory_dao
            .query_knowledge_nodes(ctx.clone(), query)
            .await?;

        if nodes.is_empty() {
            return Ok(Vec::new());
        }

        // 2. 批量查询这批节点的所有关系
        let node_ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();
        let relations = self.memory_dao.list_relations_batch(ctx, &node_ids).await?;

        // 3. 应用层统计每个节点的度数
        use std::collections::HashMap;
        let mut degree_map: HashMap<String, (usize, usize)> = HashMap::new();
        for rel in &relations {
            // 出边：rel.source_node_id 指向 rel.target_node_id
            degree_map.entry(rel.source_node_id.clone()).or_default().1 += 1;
            // 入边：rel.target_node_id 被 rel.source_node_id 引用
            degree_map.entry(rel.target_node_id.clone()).or_default().0 += 1;
        }

        // 4. 组装推荐列表并按度数倒序
        let mut recommendations: Vec<SeedNodeRecommendation> = nodes
            .into_iter()
            .map(|node| {
                let (incoming, outgoing) = degree_map.get(&node.id).copied().unwrap_or((0, 0));
                SeedNodeRecommendation {
                    degree: incoming + outgoing,
                    incoming_count: incoming,
                    outgoing_count: outgoing,
                    node,
                }
            })
            .collect();
        recommendations.sort_by_key(|r| std::cmp::Reverse(r.degree));

        // 5. 截断到 limit
        recommendations.truncate(limit);
        Ok(recommendations)
    }

    async fn create(&self, ctx: RequestContext, params: MemoryCreateParams) -> Result<Vec<Memory>> {
        match params {
            MemoryCreateParams::AppendTraces(traces) => {
                self.create_append_traces(ctx, traces).await
            }
            MemoryCreateParams::CreateShortTerm(index) => self.create_short_term(ctx, index).await,
            MemoryCreateParams::CreateKnowledgeNode { node, references } => {
                self.create_knowledge_node(ctx, node, references).await
            }
            MemoryCreateParams::CreateRelations(relations) => {
                self.create_relations(ctx, relations).await
            }
        }
    }

    async fn update(&self, ctx: RequestContext, memory: Memory) -> Result<Memory> {
        match memory.po {
            crate::models::memory::MemoryPo::ShortTerm(short_term) => {
                // 更新 SQLite 索引
                self.memory_dao
                    .update_short_term_index(ctx.clone(), short_term.clone())
                    .await?;

                // 重新向量化 summary + tags
                match try_build_vector_params_for_entity(
                    ctx.clone(),
                    &self.cortex_dao,
                    &self.model_provider_dao,
                    &short_term,
                )
                .await
                {
                    Ok(Some(vec_params)) => {
                        if let Err(e) = self
                            .memory_vector_dao
                            .upsert_short_term_vector(ctx.clone(), &short_term.id, &vec_params)
                            .await
                        {
                            log_warn!(ctx, "vector_index", memory_id= %short_term.id, error = ?e, "短期记忆向量索引更新失败，已降级");
                        }
                    }
                    Ok(None) => {
                        log_debug!(ctx, "vector_index", memory_id= %short_term.id, "无可用 Embedding Provider，跳过向量索引更新");
                    }
                    Err(e) => {
                        log_warn!(ctx, "vector_index", memory_id= %short_term.id, error = ?e, "短期记忆向量化失败，跳过向量索引更新");
                    }
                }

                Ok(Memory {
                    po: crate::models::memory::MemoryPo::ShortTerm(short_term),
                    search_match: memory.search_match,
                })
            }
            crate::models::memory::MemoryPo::KnowledgeNode(node) => {
                // 更新 SQLite 节点
                self.memory_dao
                    .update_knowledge_node(ctx.clone(), &node)
                    .await?;

                // 重新向量化（node_description + summary + tags 拼接）
                match try_build_vector_params_for_entity(
                    ctx.clone(),
                    &self.cortex_dao,
                    &self.model_provider_dao,
                    &node,
                )
                .await
                {
                    Ok(Some(vec_params)) => {
                        if let Err(e) = self
                            .memory_vector_dao
                            .upsert_knowledge_node_vector(ctx.clone(), &node.id, &vec_params)
                            .await
                        {
                            log_warn!(ctx, "vector_index", knowledge_id= %node.id, error = ?e, "知识节点向量索引更新失败，已降级");
                        }
                    }
                    Ok(None) => {
                        log_debug!(ctx, "vector_index", knowledge_id= %node.id, "无可用 Embedding Provider，跳过向量索引更新");
                    }
                    Err(e) => {
                        log_warn!(ctx, "vector_index", knowledge_id= %node.id, error = ?e, "知识节点向量化失败，跳过向量索引更新");
                    }
                }

                Ok(Memory {
                    po: crate::models::memory::MemoryPo::KnowledgeNode(node),
                    search_match: memory.search_match,
                })
            }
            crate::models::memory::MemoryPo::Trace(_) => {
                bail_err!(UnsupportedOperation, "原始记忆 Trace 不可修改");
            }
            crate::models::memory::MemoryPo::Relation(_) => {
                bail_err!(UnsupportedOperation, "记忆 Relation 不可修改，需删除后重建");
            }
        }
    }

    async fn delete(&self, ctx: RequestContext, memory: Memory) -> Result<()> {
        match memory.po {
            crate::models::memory::MemoryPo::ShortTerm(short_term) => {
                // 软删除 SQLite 索引
                self.memory_dao
                    .forget_short_term_index(ctx.clone(), &short_term.id)
                    .await?;
                // 删除向量索引（忽略失败，不影响主流程）
                if let Err(e) = self
                    .memory_vector_dao
                    .delete_short_term_vector(ctx.clone(), &short_term.id)
                    .await
                {
                    log_warn!(ctx, "vector_index", memory_id= %short_term.id, error = ?e, "短期记忆向量索引删除失败，已降级");
                }
                Ok(())
            }
            crate::models::memory::MemoryPo::KnowledgeNode(node) => {
                // 级联删除 SQLite 节点（包含关系和引用）
                self.memory_dao
                    .delete_knowledge_node(ctx.clone(), &node.id)
                    .await?;
                // 删除向量索引（忽略失败，不影响主流程）
                if let Err(e) = self
                    .memory_vector_dao
                    .delete_knowledge_node_vector(ctx.clone(), &node.id)
                    .await
                {
                    log_warn!(ctx, "vector_index", knowledge_id= %node.id, error = ?e, "知识节点向量索引删除失败，已降级");
                }
                Ok(())
            }
            crate::models::memory::MemoryPo::Trace(_) => {
                bail_err!(UnsupportedOperation, "原始记忆 Trace 不可删除");
            }
            crate::models::memory::MemoryPo::Relation(_) => {
                bail_err!(UnsupportedOperation, "记忆 Relation 不可删除，需删除后重建");
            }
        }
    }

    async fn traverse_knowledge_graph(
        &self,
        ctx: RequestContext,
        seed_node_ids: &[String],
        max_depth: i32,
        max_breadth: i32,
        strategy: TraversalStrategy,
    ) -> Result<Vec<Memory>> {
        if seed_node_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut visited_nodes: HashSet<String> = HashSet::new();
        let mut visited_relations: HashSet<String> = HashSet::new();
        let mut result_relations: Vec<KnowledgeNodeRelationPo> = Vec::new();

        for id in seed_node_ids {
            visited_nodes.insert(id.clone());
        }

        if max_depth <= 0 {
            let nodes = self.fetch_nodes_by_ids(ctx.clone(), &visited_nodes).await?;
            return Ok(self.build_memories(nodes, result_relations));
        }

        match strategy {
            TraversalStrategy::BreadthFirst => {
                self.traverse_bfs(
                    ctx.clone(),
                    seed_node_ids,
                    max_depth,
                    max_breadth,
                    TraverseState {
                        visited_nodes: &mut visited_nodes,
                        visited_relations: &mut visited_relations,
                        result_relations: &mut result_relations,
                    },
                )
                .await?;
            }
            TraversalStrategy::DepthFirst => {
                self.traverse_dfs(
                    ctx.clone(),
                    seed_node_ids,
                    max_depth,
                    max_breadth,
                    TraverseState {
                        visited_nodes: &mut visited_nodes,
                        visited_relations: &mut visited_relations,
                        result_relations: &mut result_relations,
                    },
                )
                .await?;
            }
        }

        let nodes = self.fetch_nodes_by_ids(ctx.clone(), &visited_nodes).await?;
        Ok(self.build_memories(nodes, result_relations))
    }

    async fn settle_short_term_to_long_term(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        limit: usize,
    ) -> Result<Vec<Memory>> {
        let short_term_indexes = self
            .memory_dao
            .query_short_term(
                ctx.clone(),
                MemoryQuery {
                    agent_id: Some(agent_id.to_string()),
                    status: Some(MemoryStatus::Active),
                    memory_type: Some(MemoryType::ShortTerm),
                    limit: Some(limit),
                    ..Default::default()
                },
            )
            .await?;

        if short_term_indexes.is_empty() {
            log_info!(
                ctx,
                "settle_memory",
                "agent_id={}, 无未沉淀的短期记忆",
                agent_id
            );
            return Ok(Vec::new());
        }

        let mut created_nodes: Vec<Memory> = Vec::new();

        for index in &short_term_indexes {
            let node = LongTermKnowledgeNodePo {
                id: uuid::Uuid::now_v7().to_string(),
                agent_id: agent_id.to_string(),
                node_name: index.summary.clone(),
                node_description: index.summary.clone(),
                node_type: "summary".to_string(),
                summary: index.summary.clone(),
                tags: index.tags.clone(),
                status: MemoryStatus::Active,
                is_published: crate::service::dao::memory::sqlite::tags_has_published(&index.tags),
                created_at: common::constants::utils::current_timestamp_ms(),
                updated_at: common::constants::utils::current_timestamp_ms(),
            };

            let mut results = self
                .create_knowledge_node(ctx.clone(), node, vec![])
                .await?;
            created_nodes.append(&mut results);
        }

        for index in &short_term_indexes {
            let mut index_to_update = index.clone();
            index_to_update.status = MemoryStatus::Settled;
            if let Err(e) = self
                .memory_dao
                .update_short_term_index(ctx.clone(), index_to_update)
                .await
            {
                log_warn!(ctx, "settle_memory", memory_id= %index.id, error = ?e, "标记短期记忆为已沉淀失败");
            }
        }

        log_info!(
            ctx,
            "settle_memory",
            "agent_id={}, 成功沉淀 {} 条短期记忆为 {} 个知识节点",
            agent_id,
            short_term_indexes.len(),
            created_nodes.len()
        );
        Ok(created_nodes)
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

        // 2. 分别检查两个集合的 model_provider_id
        let short_term_stored = ctx
            .vector_store()
            .get_collection_model_provider_id(ShortTermMemoryIndexPo::vector_collection())
            .await?;
        let knowledge_node_stored = ctx
            .vector_store()
            .get_collection_model_provider_id(LongTermKnowledgeNodePo::vector_collection())
            .await?;

        let short_term_need_rebuild = short_term_stored.as_ref() != Some(&current_provider_id);
        let knowledge_node_need_rebuild =
            knowledge_node_stored.as_ref() != Some(&current_provider_id);

        if !short_term_need_rebuild && !knowledge_node_need_rebuild {
            log_info!(
                &ctx,
                "rebuild_vectors",
                provider_id = %current_provider_id,
                "记忆向量索引 model_provider_id 一致，跳过重建"
            );
            return Ok(());
        }

        // 3. 清空需要重建的集合
        if short_term_need_rebuild {
            ctx.vector_store()
                .clear_collection(ShortTermMemoryIndexPo::vector_collection())
                .await?;
        }
        if knowledge_node_need_rebuild {
            ctx.vector_store()
                .clear_collection(LongTermKnowledgeNodePo::vector_collection())
                .await?;
        }

        // 4. 重建短期记忆向量索引（如需要）
        if short_term_need_rebuild {
            let short_terms = self
                .memory_dao
                .query_short_term(ctx.clone(), MemoryQuery::default())
                .await?;
            for index in &short_terms {
                match self
                    .cortex_dao
                    .embed_entity(ctx.clone(), &provider, index)
                    .await
                {
                    Ok(vec_params) => {
                        if let Err(e) = self
                            .memory_vector_dao
                            .upsert_short_term_vector(ctx.clone(), &index.id, &vec_params)
                            .await
                        {
                            log_warn!(
                                &ctx,
                                "rebuild_vectors",
                                memory_id = %index.id,
                                error = ?e,
                                "短期记忆向量索引重建失败"
                            );
                        }
                    }
                    Err(e) => {
                        log_warn!(
                            &ctx,
                            "rebuild_vectors",
                            memory_id = %index.id,
                            error = ?e,
                            "短期记忆向量化失败，跳过"
                        );
                    }
                }
            }
            ctx.vector_store()
                .set_collection_model_provider_id(
                    ShortTermMemoryIndexPo::vector_collection(),
                    &current_provider_id,
                )
                .await?;
        }

        // 6. 重建知识节点向量索引（如需要）
        if knowledge_node_need_rebuild {
            let nodes = self
                .memory_dao
                .query_knowledge_nodes(ctx.clone(), MemoryQuery::default())
                .await?;
            for node in &nodes {
                match self
                    .cortex_dao
                    .embed_entity(ctx.clone(), &provider, node)
                    .await
                {
                    Ok(vec_params) => {
                        if let Err(e) = self
                            .memory_vector_dao
                            .upsert_knowledge_node_vector(ctx.clone(), &node.id, &vec_params)
                            .await
                        {
                            log_warn!(
                                &ctx,
                                "rebuild_vectors",
                                knowledge_id = %node.id,
                                error = ?e,
                                "知识节点向量索引重建失败"
                            );
                        }
                    }
                    Err(e) => {
                        log_warn!(
                            &ctx,
                            "rebuild_vectors",
                            knowledge_id = %node.id,
                            error = ?e,
                            "知识节点向量化失败，跳过"
                        );
                    }
                }
            }
            ctx.vector_store()
                .set_collection_model_provider_id(
                    LongTermKnowledgeNodePo::vector_collection(),
                    &current_provider_id,
                )
                .await?;
        }

        Ok(())
    }
}

// ==================== Internal Helper Methods ====================

impl MemoryDalImpl {
    async fn traverse_bfs(
        &self,
        ctx: RequestContext,
        seed_node_ids: &[String],
        max_depth: i32,
        max_breadth: i32,
        state: TraverseState<'_>,
    ) -> Result<()> {
        let TraverseState {
            visited_nodes,
            visited_relations,
            result_relations,
        } = state;
        let mut queue: VecDeque<(String, i32)> = VecDeque::new();
        for id in seed_node_ids {
            queue.push_back((id.clone(), 0));
        }

        let mut current_depth = 0;
        while current_depth < max_depth && !queue.is_empty() {
            let mut current_level_nodes: Vec<String> = Vec::new();
            while let Some((node_id, depth)) = queue.front() {
                if *depth != current_depth {
                    break;
                }
                current_level_nodes.push(node_id.clone());
                queue.pop_front();
            }

            if current_level_nodes.is_empty() {
                break;
            }

            let all_relations = self
                .memory_dao
                .list_relations_batch(ctx.clone(), &current_level_nodes)
                .await?;

            let mut relations_by_node: HashMap<String, Vec<KnowledgeNodeRelationPo>> =
                HashMap::new();
            for rel in &all_relations {
                relations_by_node
                    .entry(rel.source_node_id.clone())
                    .or_default()
                    .push(rel.clone());
                relations_by_node
                    .entry(rel.target_node_id.clone())
                    .or_default()
                    .push(rel.clone());
            }

            for node_id in &current_level_nodes {
                let node_relations = relations_by_node.get(node_id).cloned().unwrap_or_default();
                let limited_relations = if max_breadth > 0 {
                    node_relations
                        .into_iter()
                        .take(max_breadth as usize)
                        .collect::<Vec<_>>()
                } else {
                    node_relations
                };

                for rel in limited_relations {
                    if !visited_relations.insert(rel.id.clone()) {
                        continue;
                    }
                    result_relations.push(rel.clone());

                    let neighbor_id = if rel.source_node_id == *node_id {
                        rel.target_node_id.clone()
                    } else {
                        rel.source_node_id.clone()
                    };

                    if visited_nodes.insert(neighbor_id.clone()) {
                        queue.push_back((neighbor_id, current_depth + 1));
                    }
                }
            }

            current_depth += 1;
        }

        Ok(())
    }

    async fn traverse_dfs(
        &self,
        ctx: RequestContext,
        seed_node_ids: &[String],
        max_depth: i32,
        max_breadth: i32,
        state: TraverseState<'_>,
    ) -> Result<()> {
        let TraverseState {
            visited_nodes,
            visited_relations,
            result_relations,
        } = state;
        let mut stack: Vec<(String, i32)> = Vec::new();
        for id in seed_node_ids.iter().rev() {
            stack.push((id.clone(), 0));
        }

        // 边预取缓存，避免逐节点查询的 N+1：
        // - fetched：已完整拉取过边的节点；batch 预取后所有 batch_ids 均有缓存条目（含空条目），
        //   故 fetched 命中即代表边已拉全，可跳过查询
        // - edge_cache：节点 → 其边（按双端点分组缓存）；注意仅作为边端点出现的节点也会获得
        //   部分边条目，这类节点不在 fetched 中，pop 时仍需预取其自身全部边
        let mut edge_cache: HashMap<String, Vec<KnowledgeNodeRelationPo>> = HashMap::new();
        let mut fetched: HashSet<String> = HashSet::new();

        while let Some((node_id, depth)) = stack.pop() {
            if depth >= max_depth {
                continue;
            }

            if !fetched.contains(&node_id) {
                // 未完整拉取过：当前节点 + 栈上未拉取的待展开节点一次批量预取
                let mut batch_ids: Vec<String> = vec![node_id.clone()];
                let mut seen: HashSet<&str> = HashSet::new();
                seen.insert(node_id.as_str());
                for (id, d) in &stack {
                    if *d < max_depth && !fetched.contains(id) && seen.insert(id.as_str()) {
                        batch_ids.push(id.clone());
                    }
                }
                let batch_relations = self
                    .memory_dao
                    .list_relations_batch(ctx.clone(), &batch_ids)
                    .await?;
                for id in &batch_ids {
                    fetched.insert(id.clone());
                }
                // 每条边挂到双端点名下的缓存；batch 内未被任何边引用的节点补空条目，
                // 保证 fetched 节点必有缓存条目，pop 时不会误判为「未拉取」重复查询
                for rel in batch_relations {
                    edge_cache
                        .entry(rel.source_node_id.clone())
                        .or_default()
                        .push(rel.clone());
                    edge_cache
                        .entry(rel.target_node_id.clone())
                        .or_default()
                        .push(rel);
                }
                for id in &batch_ids {
                    edge_cache.entry(id.clone()).or_default();
                }
            }

            // batch 内两节点之间的边会挂到双方名下，pop 时可能取到重复边：
            // 按 id 去重，避免重复边占用 take(max_breadth) 名额（旧实现单查无重复）
            let mut node_relations = edge_cache.remove(&node_id).unwrap_or_default();
            node_relations.sort_by_key(|rel| rel.created_at);
            node_relations.dedup_by(|a, b| a.id == b.id);

            let limited_relations: Vec<KnowledgeNodeRelationPo> = if max_breadth > 0 {
                node_relations
                    .into_iter()
                    .take(max_breadth as usize)
                    .collect()
            } else {
                node_relations
            };

            let mut neighbors: Vec<(String, KnowledgeNodeRelationPo)> = Vec::new();
            for rel in limited_relations {
                if !visited_relations.insert(rel.id.clone()) {
                    continue;
                }

                let neighbor_id = if rel.source_node_id == node_id {
                    rel.target_node_id.clone()
                } else {
                    rel.source_node_id.clone()
                };

                neighbors.push((neighbor_id, rel));
            }

            for (neighbor_id, rel) in neighbors.iter().rev() {
                result_relations.push(rel.clone());
                if visited_nodes.insert(neighbor_id.clone()) {
                    stack.push((neighbor_id.clone(), depth + 1));
                }
            }
        }

        Ok(())
    }

    async fn fetch_nodes_by_ids(
        &self,
        ctx: RequestContext,
        node_ids: &HashSet<String>,
    ) -> Result<Vec<LongTermKnowledgeNodePo>> {
        if node_ids.is_empty() {
            return Ok(Vec::new());
        }
        let agent_id = ctx.agent_id().cloned().unwrap_or_default();
        let ids: Vec<String> = node_ids.iter().cloned().collect();
        // 分块：SQLite 绑定参数上限 999，遍历的 visited 集合可能很大
        let mut nodes: Vec<LongTermKnowledgeNodePo> = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(crate::service::dao::memory::sqlite::IN_CLAUSE_CHUNK) {
            let query = MemoryQuery {
                ids: Some(chunk.to_vec()),
                ..Default::default()
            };
            nodes.extend(
                self.memory_dao
                    .query_knowledge_nodes(ctx.clone(), query)
                    .await?,
            );
        }
        // 共享可见性过滤：只保留自己的节点或 published 节点
        // 防止 traverse_graph 通过 id 跨 Agent 遍历私有节点
        // 使用冗余字段 is_published 替代 tags.contains("\"published\"")，避免字符串扫描
        let visible_nodes: Vec<_> = nodes
            .into_iter()
            .filter(|n| n.agent_id == agent_id || n.is_published)
            .collect();
        Ok(visible_nodes)
    }

    fn build_memories(
        &self,
        nodes: Vec<LongTermKnowledgeNodePo>,
        relations: Vec<KnowledgeNodeRelationPo>,
    ) -> Vec<Memory> {
        let mut memories = Vec::with_capacity(nodes.len() + relations.len());
        for node in nodes {
            memories.push(Memory {
                po: MemoryPo::KnowledgeNode(node),
                search_match: None,
            });
        }
        for rel in relations {
            memories.push(Memory {
                po: MemoryPo::Relation(rel),
                search_match: None,
            });
        }
        memories
    }

    /// 搜索短期记忆（内部实现）
    async fn search_short_term_internal(
        &self,
        ctx: RequestContext,
        search: MemorySearch,
    ) -> Result<Vec<Memory>> {
        // 向量距离阈值（可配置，默认 0.8）
        let vector_distance_threshold = search.vector_distance_threshold.unwrap_or(0.8);

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
                    // 向量搜索（前 50 条）
                    match self
                        .memory_vector_dao
                        .search_short_term_vector(ctx.clone(), &vec_params.vector, 50)
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
                    // 向量搜索（前 50 条）
                    match self
                        .memory_vector_dao
                        .search_knowledge_node_vector(ctx.clone(), &vec_params.vector, 50)
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

    // ==================== Create Internal Helpers ====================

    /// AppendTraces：批量追加 trace 到每日 JSONL 文件
    ///
    /// 仅写文件，不向量化（trace 永远不入向量库）。
    /// position 回填到 MemoryTrace.position 字段后包装为 Memory 返回。
    async fn create_append_traces(
        &self,
        ctx: RequestContext,
        mut traces: Vec<MemoryTrace>,
    ) -> Result<Vec<Memory>> {
        if traces.is_empty() {
            return Ok(Vec::new());
        }
        let positions = self.memory_dao.batch_append_traces(ctx, &traces).await?;
        // 回填 position 到 trace
        for (trace, pos) in traces.iter_mut().zip(positions) {
            trace.position = Some(pos);
        }
        Ok(traces
            .into_iter()
            .map(|t| Memory {
                po: MemoryPo::Trace(t),
                search_match: None,
            })
            .collect())
    }

    /// CreateShortTerm：写入短期记忆索引 + 向量化 summary（失败 warn 降级）
    async fn create_short_term(
        &self,
        ctx: RequestContext,
        index: ShortTermMemoryIndexPo,
    ) -> Result<Vec<Memory>> {
        // Step 1: 写 SQLite
        self.memory_dao
            .create_short_term_index(ctx.clone(), index.clone())
            .await?;

        // Step 2: 向量化 summary + tags（失败 warn 降级，不影响主流程）
        match try_build_vector_params_for_entity(
            ctx.clone(),
            &self.cortex_dao,
            &self.model_provider_dao,
            &index,
        )
        .await
        {
            Ok(Some(vec_params)) => {
                if let Err(e) = self
                    .memory_vector_dao
                    .upsert_short_term_vector(ctx.clone(), &index.id, &vec_params)
                    .await
                {
                    log_warn!(ctx, "vector_index", memory_id= %index.id, error = ?e, "短期记忆向量索引写入失败，已降级");
                }
            }
            Ok(None) => {
                log_debug!(ctx, "vector_index", memory_id= %index.id, "无可用 Embedding Provider，跳过向量索引");
            }
            Err(e) => {
                log_warn!(ctx, "vector_index", memory_id= %index.id, error = ?e, "短期记忆向量化失败，已降级");
            }
        }

        Ok(vec![Memory {
            po: MemoryPo::ShortTerm(index),
            search_match: None,
        }])
    }

    /// CreateKnowledgeNode：写知识节点 + 引用 + 向量化（node_description + summary 拼接）
    async fn create_knowledge_node(
        &self,
        ctx: RequestContext,
        node: LongTermKnowledgeNodePo,
        references: Vec<KnowledgeReferencePo>,
    ) -> Result<Vec<Memory>> {
        // Step 1: 写节点
        self.memory_dao
            .save_knowledge_node(ctx.clone(), &node)
            .await?;

        // Step 2: 写引用（如有）
        if !references.is_empty() {
            self.memory_dao
                .batch_add_knowledge_references(ctx.clone(), &references)
                .await?;
        }

        // Step 3: 向量化（node_description + summary + tags 拼接）
        match try_build_vector_params_for_entity(
            ctx.clone(),
            &self.cortex_dao,
            &self.model_provider_dao,
            &node,
        )
        .await
        {
            Ok(Some(vec_params)) => {
                if let Err(e) = self
                    .memory_vector_dao
                    .upsert_knowledge_node_vector(ctx.clone(), &node.id, &vec_params)
                    .await
                {
                    log_warn!(ctx, "vector_index", node_id= %node.id, error = ?e, "知识节点向量索引写入失败，已降级");
                }
            }
            Ok(None) => {
                log_debug!(ctx, "vector_index", node_id= %node.id, "无可用 Embedding Provider，跳过向量索引");
            }
            Err(e) => {
                log_warn!(ctx, "vector_index", node_id= %node.id, error = ?e, "知识节点向量化失败，已降级");
            }
        }

        Ok(vec![Memory {
            po: MemoryPo::KnowledgeNode(node),
            search_match: None,
        }])
    }

    /// CreateRelations：批量添加知识节点关系（无向量化）
    async fn create_relations(
        &self,
        ctx: RequestContext,
        relations: Vec<KnowledgeNodeRelationPo>,
    ) -> Result<Vec<Memory>> {
        if relations.is_empty() {
            return Ok(Vec::new());
        }
        self.memory_dao
            .batch_add_knowledge_relations(ctx, &relations)
            .await?;
        Ok(relations
            .into_iter()
            .map(|r| Memory {
                po: MemoryPo::Relation(r),
                search_match: None,
            })
            .collect())
    }
}

// ==================== Helpers ====================

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

/// 尝试为可向量化实体构建向量索引参数（用于索引场景）
///
/// 流程：
/// 1. 取默认 Embedding ModelProvider；无则返回 None（无可用 provider）
/// 2. 调 `embed_entity` 生成完整 VectorIndexParams（自动调用 entity.vectorize_text()）
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

    let params = cortex_dao
        .embed_entity(ctx.clone(), &provider, entity)
        .await?;
    Ok(Some(params))
}
