//! 记忆写入 / 更新 / 删除 / 状态流转 / 向量重建

use super::MemoryDalImpl;
use crate::models::memory::{
    KnowledgeNodeRelationPo, KnowledgeReferencePo, LongTermKnowledgeNodePo, Memory,
    MemoryCreateParams, MemoryPo, MemoryTrace, ShortTermMemoryIndexPo,
};
use crate::models::vector::{ReindexDecision, VectorIndexParams, Vectorizable};
use crate::pkg::RequestContext;
use crate::pkg::background_task::TaskProgressCounter;
use crate::service::dal::VECTOR_REBUILD_PAGE_SIZE;
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::memory::MemoryQuery;
use crate::service::dao::model_provider::ModelProviderDao;
use common::enums::{MemoryStatus, MemoryType};
use common::error::{Result, bail_err};
use std::sync::Arc;

impl MemoryDalImpl {
    pub(super) async fn create(
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

    pub(super) async fn update(&self, ctx: RequestContext, memory: Memory) -> Result<Memory> {
        match memory.po {
            crate::models::memory::MemoryPo::ShortTerm(short_term) => {
                // 更新 SQLite 索引
                self.memory_dao
                    .update_short_term_index(ctx.clone(), short_term.clone())
                    .await?;

                // 重新向量化 summary + tags（三态：Skip / PayloadOnly / FullReindex）
                match try_build_vector_params_for_entity(
                    ctx.clone(),
                    &self.cortex_dao,
                    &self.model_provider_dao,
                    &short_term,
                    ShortTermMemoryIndexPo::vector_collection(),
                    &short_term.id,
                )
                .await
                {
                    Ok(VectorIndexAction::Reindex(vec_params)) => {
                        if let Err(e) = self
                            .memory_vector_dao
                            .upsert_short_term_vector(ctx.clone(), &short_term.id, &vec_params)
                            .await
                        {
                            log_warn!(ctx, "vector_index", memory_id= %short_term.id, error = ?e, "短期记忆向量索引更新失败，已降级");
                        }
                    }
                    Ok(VectorIndexAction::Skipped) => {
                        log_debug!(ctx, "vector_index", memory_id= %short_term.id, "向量索引未变化，跳过");
                    }
                    Ok(VectorIndexAction::PayloadRefreshed) => {}
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

                // 重新向量化（node_description + summary + tags 拼接；三态：Skip / PayloadOnly / FullReindex）
                match try_build_vector_params_for_entity(
                    ctx.clone(),
                    &self.cortex_dao,
                    &self.model_provider_dao,
                    &node,
                    LongTermKnowledgeNodePo::vector_collection(),
                    &node.id,
                )
                .await
                {
                    Ok(VectorIndexAction::Reindex(vec_params)) => {
                        if let Err(e) = self
                            .memory_vector_dao
                            .upsert_knowledge_node_vector(ctx.clone(), &node.id, &vec_params)
                            .await
                        {
                            log_warn!(ctx, "vector_index", knowledge_id= %node.id, error = ?e, "知识节点向量索引更新失败，已降级");
                        }
                    }
                    Ok(VectorIndexAction::Skipped) => {
                        log_debug!(ctx, "vector_index", knowledge_id= %node.id, "向量索引未变化，跳过");
                    }
                    Ok(VectorIndexAction::PayloadRefreshed) => {}
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

    pub(super) async fn delete(&self, ctx: RequestContext, memory: Memory) -> Result<()> {
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
            crate::models::memory::MemoryPo::Relation(rel) => {
                // 软删除：标记边为 Deleted(2)，行保留支持恢复；仅降级生效边的保护在 DAO 层
                self.memory_dao
                    .delete_knowledge_relation(ctx, &rel.id)
                    .await
            }
        }
    }

    pub(super) async fn mark_short_term_settled(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        memory_ids: &[String],
    ) -> Result<usize> {
        if memory_ids.is_empty() {
            return Ok(0);
        }

        // 先按 id + agent 捞一遍，只翻转仍为 Active 的
        // （Agent 可能已自行处理掉一部分，不覆盖它的判断）
        let still_active = self
            .memory_dao
            .query_short_term(
                ctx.clone(),
                MemoryQuery {
                    ids: Some(memory_ids.to_vec()),
                    agent_id: Some(agent_id.to_string()),
                    status: Some(MemoryStatus::Active),
                    memory_type: Some(MemoryType::ShortTerm),
                    ..Default::default()
                },
            )
            .await?;

        let mut marked = 0usize;
        let now = common::constants::utils::current_timestamp_ms();
        for index in still_active {
            let mut updated = index.clone();
            updated.status = MemoryStatus::Settled;
            // DAO 的 update_short_term_index 是整行 UPDATE，updated_at 取传入值，
            // 这里必须显式刷新，否则状态改了但 updated_at 仍停在原值。
            updated.updated_at = now;
            match self
                .memory_dao
                .update_short_term_index(ctx.clone(), updated)
                .await
            {
                Ok(_) => marked += 1,
                Err(e) => log_warn!(
                    ctx,
                    "settle_memory",
                    memory_id = %index.id,
                    error = ?e,
                    "标记短期记忆为已沉淀失败"
                ),
            }
        }

        Ok(marked)
    }

    pub(super) async fn rebuild_vectors(
        &self,
        ctx: RequestContext,
        progress: &TaskProgressCounter,
    ) -> Result<()> {
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

        // 4. 重建短期记忆向量索引（如需要；清空后旧行不存在，必然 FullReindex）
        // 分页重建：排序键 `updated_at DESC, id DESC` 确定页序；期间被更新的记忆
        // 会由常规写路径自行 upsert 向量，故偏移漂移无副作用。
        if short_term_need_rebuild {
            let mut offset = 0usize;
            loop {
                let page = self
                    .memory_dao
                    .query_short_term(
                        ctx.clone(),
                        MemoryQuery {
                            limit: Some(VECTOR_REBUILD_PAGE_SIZE),
                            offset: Some(offset),
                            ..Default::default()
                        },
                    )
                    .await?;
                let fetched = page.len();
                if fetched == 0 {
                    break;
                }
                for index in &page {
                    match try_build_vector_params_for_entity(
                        ctx.clone(),
                        &self.cortex_dao,
                        &self.model_provider_dao,
                        index,
                        ShortTermMemoryIndexPo::vector_collection(),
                        &index.id,
                    )
                    .await
                    {
                        Ok(VectorIndexAction::Reindex(vec_params)) => {
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
                        Ok(VectorIndexAction::Skipped) => {
                            log_debug!(
                                &ctx,
                                "rebuild_vectors",
                                memory_id = %index.id,
                                "向量索引未变化，跳过"
                            );
                        }
                        Ok(VectorIndexAction::PayloadRefreshed) => {}
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
                offset += fetched;
                progress.advance(fetched);
                if fetched < VECTOR_REBUILD_PAGE_SIZE {
                    break;
                }
            }
            ctx.vector_store()
                .set_collection_model_provider_id(
                    ShortTermMemoryIndexPo::vector_collection(),
                    &current_provider_id,
                )
                .await?;
        }

        // 6. 重建知识节点向量索引（如需要；清空后旧行不存在，必然 FullReindex）
        if knowledge_node_need_rebuild {
            let mut offset = 0usize;
            loop {
                let page = self
                    .memory_dao
                    .query_knowledge_nodes(
                        ctx.clone(),
                        MemoryQuery {
                            limit: Some(VECTOR_REBUILD_PAGE_SIZE),
                            offset: Some(offset),
                            ..Default::default()
                        },
                    )
                    .await?;
                let fetched = page.len();
                if fetched == 0 {
                    break;
                }
                for node in &page {
                    match try_build_vector_params_for_entity(
                        ctx.clone(),
                        &self.cortex_dao,
                        &self.model_provider_dao,
                        node,
                        LongTermKnowledgeNodePo::vector_collection(),
                        &node.id,
                    )
                    .await
                    {
                        Ok(VectorIndexAction::Reindex(vec_params)) => {
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
                        Ok(VectorIndexAction::Skipped) => {
                            log_debug!(
                                &ctx,
                                "rebuild_vectors",
                                knowledge_id = %node.id,
                                "向量索引未变化，跳过"
                            );
                        }
                        Ok(VectorIndexAction::PayloadRefreshed) => {}
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
                offset += fetched;
                progress.advance(fetched);
                if fetched < VECTOR_REBUILD_PAGE_SIZE {
                    break;
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

// ==================== Create Internal Helpers ====================

impl MemoryDalImpl {
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

        // Step 2: 向量化 summary + tags（三态：Skip / PayloadOnly / FullReindex；失败 warn 降级，不影响主流程）
        match try_build_vector_params_for_entity(
            ctx.clone(),
            &self.cortex_dao,
            &self.model_provider_dao,
            &index,
            ShortTermMemoryIndexPo::vector_collection(),
            &index.id,
        )
        .await
        {
            Ok(VectorIndexAction::Reindex(vec_params)) => {
                if let Err(e) = self
                    .memory_vector_dao
                    .upsert_short_term_vector(ctx.clone(), &index.id, &vec_params)
                    .await
                {
                    log_warn!(ctx, "vector_index", memory_id= %index.id, error = ?e, "短期记忆向量索引写入失败，已降级");
                }
            }
            Ok(VectorIndexAction::Skipped) => {
                log_debug!(ctx, "vector_index", memory_id= %index.id, "向量索引未变化，跳过");
            }
            Ok(VectorIndexAction::PayloadRefreshed) => {}
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

        // Step 3: 向量化（node_description + summary + tags 拼接；三态：Skip / PayloadOnly / FullReindex）
        match try_build_vector_params_for_entity(
            ctx.clone(),
            &self.cortex_dao,
            &self.model_provider_dao,
            &node,
            LongTermKnowledgeNodePo::vector_collection(),
            &node.id,
        )
        .await
        {
            Ok(VectorIndexAction::Reindex(vec_params)) => {
                if let Err(e) = self
                    .memory_vector_dao
                    .upsert_knowledge_node_vector(ctx.clone(), &node.id, &vec_params)
                    .await
                {
                    log_warn!(ctx, "vector_index", node_id= %node.id, error = ?e, "知识节点向量索引写入失败，已降级");
                }
            }
            Ok(VectorIndexAction::Skipped) => {
                log_debug!(ctx, "vector_index", node_id= %node.id, "向量索引未变化，跳过");
            }
            Ok(VectorIndexAction::PayloadRefreshed) => {}
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

// ==================== Vector Helper ====================

/// 统一向量索引三态决策结果
enum VectorIndexAction {
    /// 无可用 provider 或内容未变化：无需任何操作
    Skipped,
    /// 仅 payload 变化：已在内部完成刷新（未调 embedding）
    PayloadRefreshed,
    /// 需要完整重索引：调用方执行 upsert（Box 避免 enum 体积被最大变体撑大）
    Reindex(Box<VectorIndexParams>),
}

/// 尝试为可向量化实体构建向量索引参数（三态：Skip / PayloadOnly / FullReindex）
///
/// 三态判断取旧行比对双 hash（文本 hash + payload hash）；
/// PayloadOnly 直接刷新 payload 列，不重新调 embedding。
async fn try_build_vector_params_for_entity(
    ctx: RequestContext,
    cortex_dao: &Arc<dyn CortexDao>,
    model_provider_dao: &Arc<dyn ModelProviderDao>,
    entity: &dyn Vectorizable,
    collection: &str,
    id: &str,
) -> Result<VectorIndexAction> {
    let Some(provider) = model_provider_dao
        .get_default_embedding_provider(ctx.clone())
        .await?
    else {
        return Ok(VectorIndexAction::Skipped);
    };

    // 三态：取旧行比对双 hash
    if let Some(row) = ctx.vector_store().get(collection, id).await? {
        match entity.reindex_decision(&row.meta.content_hash, &row.meta.payload_hash) {
            ReindexDecision::Skip => return Ok(VectorIndexAction::Skipped),
            ReindexDecision::PayloadOnly => {
                ctx.vector_store()
                    .update_payload(collection, id, &entity.vector_payload())
                    .await?;
                return Ok(VectorIndexAction::PayloadRefreshed);
            }
            ReindexDecision::FullReindex => {}
        }
    }

    let params = cortex_dao
        .embed_entity(ctx.clone(), &provider, entity)
        .await?;
    Ok(VectorIndexAction::Reindex(Box::new(params)))
}
