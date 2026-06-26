//! Memory DAL - 记忆数据访问层（业务逻辑层）
//!
//! 职责：跨 DAO 流程编排
//! - 获取 Embedding Provider
//! - 生成查询向量
//! - 混合搜索（全文 + 向量）
//! - 结果聚合排序

use common::error::{err, bail_err, Result};
use crate::models::memory::{
    KnowledgeNodeRelationPo, KnowledgeReferencePo, LongTermKnowledgeNodePo, Memory,
    MemoryCreateParams, MemoryPo, MemoryTrace, ShortTermMemoryIndexPo,
};
use crate::models::vector::{SearchMatchInfo, VectorIndexParams, Vectorizable};
use crate::pkg::RequestContext;
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::memory::{MemoryDao, MemoryQuery, MemorySearch, MemoryVectorDao};
use crate::service::dao::model_provider::ModelProviderDao;
use async_trait::async_trait;
use common::enums::MemoryType;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

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
    async fn search(
        &self,
        ctx: RequestContext,
        search: MemorySearch,
    ) -> Result<Vec<Memory>>;

    /// 📋 通用关系型查询（纯数据库查询，无向量）
    ///
    /// 支持所有组合过滤条件，可单独指定查询哪种记忆类型
    async fn query(&self, ctx: RequestContext, query: MemoryQuery)
    -> Result<Vec<Memory>>;

    /// ✍️ 创建记忆（按 MemoryCreateParams 变体分发）
    ///
    /// 聚合流程：
    /// - `Trace` / `BatchTrace` → 写 daily JSONL（不向量化）
    /// - `ShortTerm` → 写库 + 向量化 summary（向量失败仅 warn 降级）
    /// - `KnowledgeNode` → 写库 + 写引用 + 向量化 summary（向量失败仅 warn 降级）
    /// - `Relation` → 写库（不向量化）
    async fn create(
        &self,
        ctx: RequestContext,
        params: MemoryCreateParams,
    ) -> Result<Vec<Memory>>;

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
    async fn search(
        &self,
        ctx: RequestContext,
        search: MemorySearch,
    ) -> Result<Vec<Memory>> {
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

        // 4. 统一排序：按向量距离排序（有距离的在前），然后按创建时间倒序
        results.sort_by(|a, b| {
            let dist_a = a
                .search_match
                .as_ref()
                .and_then(|m| m.vector_distance)
                .unwrap_or(f32::MAX);
            let dist_b = b
                .search_match
                .as_ref()
                .and_then(|m| m.vector_distance)
                .unwrap_or(f32::MAX);
            dist_a
                .partial_cmp(&dist_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // 5. 应用 limit
        if let Some(limit) = search.filters.limit {
            results.truncate(limit);
        }

        Ok(results)
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: MemoryQuery,
    ) -> Result<Vec<Memory>> {
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

    async fn create(
        &self,
        ctx: RequestContext,
        params: MemoryCreateParams,
    ) -> Result<Vec<Memory>> {
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

                // 重新向量化 summary
                match try_build_vector_params_for_search(
                    ctx.clone(),
                    &self.cortex_dao,
                    &self.model_provider_dao,
                    &short_term.summary,
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

                // 重新向量化（node_description + summary 拼接）
                let text_for_embedding = format!("{}\n{}", node.node_description, node.summary);
                match try_build_vector_params_for_search(
                    ctx.clone(),
                    &self.cortex_dao,
                    &self.model_provider_dao,
                    &text_for_embedding,
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
}

// ==================== Internal Helper Methods ====================

impl MemoryDalImpl {
    /// 搜索短期记忆（内部实现）
    async fn search_short_term_internal(
        &self,
        ctx: RequestContext,
        search: MemorySearch,
    ) -> Result<Vec<Memory>> {
        // Step 1: 准备向量搜索结果容器
        let mut vector_scores: HashMap<String, f32> = HashMap::new();
        let mut vector_ids: HashSet<String> = HashSet::new();

        // Step 2: 如果有关键词，执行向量搜索（用 try_build_vector_params 统一方式）
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
                        const VECTOR_DISTANCE_THRESHOLD: f32 = 0.8;
                        match self
                            .memory_vector_dao
                            .search_short_term_vector(ctx.clone(), &vec_params.vector, 50)
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
        }

        // Step 3: 执行关键词搜索
        let keyword_pos = self
            .memory_dao
            .search_short_term(ctx.clone(), search.clone())
            .await?;

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
            let match_info = if let Some(distance) = vector_scores.get(&po.id) {
                Some(SearchMatchInfo {
                    match_type: crate::models::vector::MatchType::Hybrid,
                    vector_distance: Some(*distance),
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
        // Step 1: 准备向量搜索结果容器
        let mut vector_scores: HashMap<String, f32> = HashMap::new();
        let mut vector_ids: HashSet<String> = HashSet::new();

        // Step 2: 如果有关键词，执行向量搜索（用 try_build_vector_params 统一方式）
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
                        const VECTOR_DISTANCE_THRESHOLD: f32 = 0.8;
                        match self
                            .memory_vector_dao
                            .search_knowledge_node_vector(ctx.clone(), &vec_params.vector, 50)
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
        }

        // Step 3: 执行关键词搜索
        let keyword_pos = self
            .memory_dao
            .search_knowledge_nodes(ctx.clone(), search.clone())
            .await?;

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
            let match_info = if let Some(distance) = vector_scores.get(&po.id) {
                Some(SearchMatchInfo {
                    match_type: crate::models::vector::MatchType::Hybrid,
                    vector_distance: Some(*distance),
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

    /// 搜索关系（仅关键词搜索）
    async fn search_relations_internal(
        &self,
        _ctx: RequestContext,
        _search: MemorySearch,
    ) -> Result<Vec<Memory>> {
        // TODO: DAO 层目前没有关系搜索方法，后续补充
        Ok(Vec::new())
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
        for (trace, pos) in traces.iter_mut().zip(positions.into_iter()) {
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

        // Step 2: 向量化 summary（失败 warn 降级，不影响主流程）
        match try_build_vector_params_for_search(
            ctx.clone(),
            &self.cortex_dao,
            &self.model_provider_dao,
            &index.summary,
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

        // Step 3: 向量化（node_description + summary 拼接）
        let vector_text = format!("{}\n{}", node.node_description, node.summary);
        match try_build_vector_params_for_search(
            ctx.clone(),
            &self.cortex_dao,
            &self.model_provider_dao,
            &vector_text,
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

/// 尝试为指定文本构建向量索引参数
///
/// 流程：
/// 1. 取默认 Embedding ModelProvider；无则返回 None（无可用 provider）
/// 2. 创建 Cortex（trait 对象）
/// 3. 调 `embeddings(&[text])` 生成向量
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
        .embed_text_for_search(ctx.clone(), cortex.as_ref(), text)
        .await?;
    Ok(Some(params))
}

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
