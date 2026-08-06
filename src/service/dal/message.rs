//! Message DAL 模块
//!
//! 基础消息数据访问层，提供消息保存和查询能力
//! 所有保存的消息都会自动发布到 AOP 事件中心

use crate::models::events::MessageCreatedEvent;
use crate::models::message::{Message, MessagePo};
use crate::models::vector::{VectorIndexParams, Vectorizable};
use crate::pkg::RequestContext;
use crate::pkg::aop;
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::message::{
    self, MessageDao, MessageQuery, MessageSearch, MessageVectorDao,
};
use crate::service::dao::model_provider::ModelProviderDao;
use common::enums::{MessageRole, MessageStatus, MessageType};
use common::error::Result;
use std::sync::{Arc, OnceLock};

static MESSAGE_DAL: OnceLock<Arc<dyn MessageDal>> = OnceLock::new();

pub fn dal() -> Arc<dyn MessageDal> {
    MESSAGE_DAL.get().cloned().unwrap()
}

pub fn init() {
    let _ = MESSAGE_DAL.set(new(
        message::dao(),
        message::vector_dao(),
        crate::service::dao::cortex::dao(),
        crate::service::dao::model_provider::dao(),
    ));
}

pub fn new(
    message_dao: Arc<dyn MessageDao + Send + Sync>,
    message_vector_dao: Arc<dyn MessageVectorDao + Send + Sync>,
    cortex_dao: Arc<dyn CortexDao + Send + Sync>,
    model_provider_dao: Arc<dyn ModelProviderDao + Send + Sync>,
) -> Arc<dyn MessageDal> {
    Arc::new(MessageDalImpl {
        message_dao,
        message_vector_dao,
        cortex_dao,
        model_provider_dao,
    })
}

#[async_trait::async_trait]
pub trait MessageDal: Send + Sync {
    async fn save_message(&self, ctx: RequestContext, message: &Message) -> Result<()>;

    async fn query(&self, ctx: RequestContext, query: MessageQuery) -> Result<Vec<Message>>;

    async fn list_by_task_id(
        &self,
        ctx: RequestContext,
        task_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Message>>;

    async fn list_by_project_id(
        &self,
        ctx: RequestContext,
        project_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Message>>;

    async fn list_by_from_id(
        &self,
        ctx: RequestContext,
        from_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Message>>;

    async fn list_by_to_id(
        &self,
        ctx: RequestContext,
        to_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Message>>;

    async fn list_by_status(
        &self,
        ctx: RequestContext,
        status: Vec<MessageStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<Message>>;

    async fn update_status(
        &self,
        ctx: RequestContext,
        message_id: &str,
        status: MessageStatus,
    ) -> Result<()>;

    async fn count_by_task_id(&self, ctx: RequestContext, task_id: &str) -> Result<u64>;

    /// 统计符合查询条件的消息数量（透传 DAO count）
    async fn count(&self, ctx: RequestContext, query: MessageQuery) -> Result<u64>;

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Message>>;

    /// 检查指定 Agent 是否有 Pending 状态的指定类型消息
    ///
    /// 用于 TaskEventConsumer 发送通知前去重，避免对同一 Agent 重复投递
    /// TaskDispatchNotification 等系统通知。
    async fn has_pending_message_for_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        message_type: MessageType,
    ) -> Result<bool>;

    async fn delete_message(&self, ctx: RequestContext, id: &str) -> Result<()>;

    async fn delete_by_task_id(&self, ctx: RequestContext, task_id: &str) -> Result<()>;

    async fn search(&self, ctx: RequestContext, search: MessageSearch) -> Result<Vec<Message>>;

    async fn rebuild_vectors(&self, ctx: RequestContext) -> Result<()>;
}

struct MessageDalImpl {
    message_dao: Arc<dyn MessageDao>,
    message_vector_dao: Arc<dyn MessageVectorDao>,
    cortex_dao: Arc<dyn CortexDao>,
    model_provider_dao: Arc<dyn ModelProviderDao>,
}

#[async_trait::async_trait]
impl MessageDal for MessageDalImpl {
    async fn save_message(&self, ctx: RequestContext, message: &Message) -> Result<()> {
        self.message_dao.insert(ctx.clone(), &message.po).await?;

        let event = MessageCreatedEvent {
            message_id: message.id().to_string(),
            project_id: message.project_id().map(|s| s.to_string()),
            task_id: message.task_id().map(|s| s.to_string()),
            from_id: message.from_id().to_string(),
            from_role: message.from_role() as i32,
            to_id: message.to_id().to_string(),
            to_role: message.to_role() as i32,
            message_type: message.message_type() as i32,
            content: message.content().to_string(),
            created_at: message.created_at(),
        };
        aop::publish(event).await;

        match try_build_vector_params_for_entity(
            ctx.clone(),
            &*self.cortex_dao,
            &*self.model_provider_dao,
            &message.po,
        )
        .await
        {
            Ok(Some(vec_params)) => {
                if let Err(e) = self
                    .message_vector_dao
                    .upsert_vector(ctx.clone(), &message.po.id, &vec_params)
                    .await
                {
                    log_warn!(
                        &ctx,
                        "vector_index",
                        message_id = %message.po.id,
                        error = ?e,
                        "消息向量索引写入失败，已降级"
                    );
                }
            }
            Ok(None) => {
                log_debug!(
                    &ctx,
                    "vector_index",
                    message_id = %message.po.id,
                    "无可用 Embedding Provider，跳过向量索引"
                );
            }
            Err(e) => {
                log_warn!(
                    &ctx,
                    "vector_index",
                    message_id = %message.po.id,
                    error = ?e,
                    "消息向量化失败，已降级"
                );
            }
        }

        Ok(())
    }

    async fn query(&self, ctx: RequestContext, query: MessageQuery) -> Result<Vec<Message>> {
        let pos = self.message_dao.query(ctx, query).await?;
        Ok(pos.into_iter().map(Message::from_po).collect())
    }

    async fn list_by_task_id(
        &self,
        ctx: RequestContext,
        task_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Message>> {
        self.query(
            ctx,
            MessageQuery {
                task_id: Some(task_id.to_string()),
                limit,
                ..Default::default()
            },
        )
        .await
    }

    async fn list_by_project_id(
        &self,
        ctx: RequestContext,
        project_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Message>> {
        self.query(
            ctx,
            MessageQuery {
                project_id: Some(project_id.to_string()),
                limit,
                ..Default::default()
            },
        )
        .await
    }

    async fn list_by_from_id(
        &self,
        ctx: RequestContext,
        from_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Message>> {
        self.query(
            ctx,
            MessageQuery {
                from_id: Some(from_id.to_string()),
                limit,
                ..Default::default()
            },
        )
        .await
    }

    async fn list_by_to_id(
        &self,
        ctx: RequestContext,
        to_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Message>> {
        self.query(
            ctx,
            MessageQuery {
                to_id: Some(to_id.to_string()),
                limit,
                ..Default::default()
            },
        )
        .await
    }

    async fn list_by_status(
        &self,
        ctx: RequestContext,
        status: Vec<MessageStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<Message>> {
        self.query(
            ctx,
            MessageQuery {
                status_in: Some(status),
                limit,
                ..Default::default()
            },
        )
        .await
    }

    async fn update_status(
        &self,
        ctx: RequestContext,
        message_id: &str,
        status: MessageStatus,
    ) -> Result<()> {
        self.message_dao
            .update_status(ctx, message_id, status)
            .await
    }

    async fn count_by_task_id(&self, ctx: RequestContext, task_id: &str) -> Result<u64> {
        // 语法糖：调用通用 count
        self.count(
            ctx,
            MessageQuery {
                task_id: Some(task_id.to_string()),
                ..Default::default()
            },
        )
        .await
    }

    async fn count(&self, ctx: RequestContext, query: MessageQuery) -> Result<u64> {
        self.message_dao.count(ctx, query).await
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Message>> {
        let opt = self.message_dao.find_by_id(ctx, id).await?;
        Ok(opt.map(Message::from_po))
    }

    async fn has_pending_message_for_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        message_type: MessageType,
    ) -> Result<bool> {
        let count = self
            .count(
                ctx,
                MessageQuery {
                    to_id: Some(agent_id.to_string()),
                    to_role: Some(MessageRole::Agent),
                    message_type: Some(message_type),
                    status_in: Some(vec![MessageStatus::Pending]),
                    ..Default::default()
                },
            )
            .await?;
        Ok(count > 0)
    }

    async fn delete_message(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.message_dao.delete(ctx.clone(), id).await?;
        if let Err(e) = self.message_vector_dao.delete_vector(ctx.clone(), id).await {
            log_warn!(
                &ctx,
                "vector_index",
                message_id = %id,
                error = ?e,
                "消息向量索引删除失败，已降级"
            );
        }
        Ok(())
    }

    async fn delete_by_task_id(&self, ctx: RequestContext, task_id: &str) -> Result<()> {
        let ctx = ctx.to_builder().task_id(task_id).build();
        self.message_dao.delete_by_task_id(ctx, task_id).await
    }

    async fn search(&self, ctx: RequestContext, mut search: MessageSearch) -> Result<Vec<Message>> {
        // 如果有关键词但还没有 query_vector，尝试嵌入关键词生成查询向量
        if let Some(keyword) = &search.keyword
            && search.query_vector.is_none()
            && let Some(provider) = self
                .model_provider_dao
                .get_default_embedding_provider(ctx.clone())
                .await?
        {
            match self
                .cortex_dao
                .embed_text_for_search(ctx.clone(), &provider, keyword)
                .await
            {
                Ok(params) => {
                    search.query_vector = Some(params.vector);
                }
                Err(e) => {
                    log_warn!(
                        ctx.clone(),
                        "vector_search",
                        "Message keyword embedding failed: {}, fallback to keyword only",
                        e
                    );
                }
            }
        }

        let (keyword_matches, vector_matches) = do_search(
            &*self.message_dao,
            &*self.message_vector_dao,
            ctx.clone(),
            &search,
        )
        .await?;

        let merged = merge_search_results(keyword_matches, vector_matches, &search);

        Ok(merged)
    }

    async fn rebuild_vectors(&self, ctx: RequestContext) -> Result<()> {
        self.message_vector_dao
            .clear_collection(ctx.clone())
            .await?;

        let messages = self.query(ctx.clone(), MessageQuery::default()).await?;
        sys_info!("rebuilding vector index for {} messages", messages.len());

        for message in messages {
            let ctx = enrich_ctx(&ctx, &message.po);
            match try_build_vector_params_for_entity(
                ctx.clone(),
                &*self.cortex_dao,
                &*self.model_provider_dao,
                &message.po,
            )
            .await
            {
                Ok(Some(vec_params)) => {
                    if let Err(e) = self
                        .message_vector_dao
                        .upsert_vector(ctx.clone(), message.id(), &vec_params)
                        .await
                    {
                        log_warn!(
                            &ctx,
                            "vector_index",
                            message_id = %message.id(),
                            error = ?e,
                            "消息向量索引重建失败，已降级"
                        );
                    }
                }
                Err(e) => {
                    log_warn!(
                        &ctx,
                        "vector_index",
                        message_id = %message.id(),
                        error = ?e,
                        "消息向量化失败，已降级"
                    );
                }
                _ => {}
            }
        }

        Ok(())
    }
}

async fn try_build_vector_params_for_entity(
    ctx: RequestContext,
    cortex_dao: &dyn CortexDao,
    model_provider_dao: &dyn ModelProviderDao,
    entity: &dyn Vectorizable,
) -> Result<Option<VectorIndexParams>> {
    let Some(provider) = model_provider_dao
        .get_default_embedding_provider(ctx.clone())
        .await?
    else {
        return Ok(None);
    };

    let params = cortex_dao.embed_entity(ctx, &provider, entity).await?;
    Ok(Some(params))
}

async fn do_search(
    message_dao: &dyn MessageDao,
    message_vector_dao: &dyn MessageVectorDao,
    ctx: RequestContext,
    search: &MessageSearch,
) -> Result<(Vec<Message>, Vec<Message>)> {
    use crate::models::vector::{MatchType, SearchMatchInfo};

    let keyword_matches = if search.keyword.is_some() {
        let results = message_dao
            .search_messages(ctx.clone(), search.clone())
            .await?;
        results
            .into_iter()
            .map(|(po, fts_rank)| {
                let mut msg = Message::from_po(po);
                msg.search_match = Some(SearchMatchInfo {
                    match_type: MatchType::Keyword,
                    fts_rank,
                    ..Default::default()
                });
                msg
            })
            .collect()
    } else {
        Vec::new()
    };

    let vector_matches = if let Some(query_vector) = &search.query_vector {
        let results = message_vector_dao
            .search_vector(ctx.clone(), query_vector, search.top_k.unwrap_or(20))
            .await?;
        let mut matches = Vec::new();
        for hit in results {
            if let Ok(Some(po)) = message_dao.find_by_id(ctx.clone(), &hit.row.id).await {
                let mut msg = Message::from_po(po);
                msg.search_match = Some(SearchMatchInfo {
                    match_type: MatchType::Vector,
                    vector_distance: Some(hit.distance),
                    embedding_model: Some(hit.row.meta.embedding_model.clone()),
                    content_hash: Some(hit.row.meta.content_hash.clone()),
                    indexed_at: Some(hit.row.meta.indexed_at),
                    ..Default::default()
                });
                matches.push(msg);
            }
        }
        matches
    } else {
        Vec::new()
    };

    Ok((keyword_matches, vector_matches))
}

fn merge_search_results(
    keyword_matches: Vec<Message>,
    vector_matches: Vec<Message>,
    search: &MessageSearch,
) -> Vec<Message> {
    use crate::models::vector::MatchType;
    use std::collections::HashMap;

    let mut seen: HashMap<String, usize> = HashMap::new();

    let keyword_weight = 0.7;
    let vector_weight = 0.3;

    let mut scored_results: Vec<(f64, Message)> = Vec::new();

    let keyword_len = keyword_matches.len() as f64;
    for (i, msg) in keyword_matches.into_iter().enumerate() {
        let id = msg.id().to_string();
        if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(id) {
            let score = keyword_weight * (1.0 - (i as f64 / keyword_len));
            e.insert(scored_results.len());
            scored_results.push((score, msg));
        }
    }

    let vector_len = vector_matches.len() as f64;
    for (i, msg) in vector_matches.into_iter().enumerate() {
        let id = msg.id().to_string();
        let score = vector_weight * (1.0 - (i as f64 / vector_len));
        if let Some(&idx) = seen.get(&id) {
            let (ref mut total_score, ref mut existing_msg) = scored_results[idx];
            *total_score += score;
            if let Some(ref mut match_info) = existing_msg.search_match {
                match_info.match_type = MatchType::Hybrid;
                if let Some(distance) = msg.search_match.as_ref().and_then(|m| m.vector_distance) {
                    match_info.vector_distance = Some(distance);
                }
            }
        } else {
            seen.insert(id, scored_results.len());
            scored_results.push((score, msg));
        }
    }

    scored_results.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let limit = search.filters.limit.unwrap_or(20);
    scored_results
        .into_iter()
        .take(limit)
        .map(|(_, msg)| msg)
        .collect()
}

fn enrich_ctx(ctx: &RequestContext, po: &MessagePo) -> RequestContext {
    ctx.to_builder()
        .try_project_id(po.project_id.as_deref())
        .try_task_id(po.task_id.as_deref())
        .build()
}
