//! Message DAL 模块
//!
//! 基础消息数据访问层，提供消息保存和查询能力
//! 所有保存的消息都会自动入队事件队列，并自动维护向量索引

use common::error::Result;
use crate::models::event::Event;
use crate::models::message::{Message, MessagePo};
use crate::models::vector::{MatchType, SearchMatchInfo, VectorIndexParams, Vectorizable};
use crate::pkg::RequestContext;
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::event_queue::{self, EventQueueDao};
use crate::service::dao::message::{self, MessageDao, MessageQuery, MessageSearch, MessageVectorDao};
use crate::service::dao::model_provider::ModelProviderDao;
use common::enums::MessageStatus;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};

// ==================== 单例管理 ====================

static MESSAGE_DAL: OnceLock<Arc<dyn MessageDal>> = OnceLock::new();

/// 获取 Message DAL 单例
pub fn dal() -> Arc<dyn MessageDal> {
    MESSAGE_DAL.get().cloned().unwrap()
}

/// 初始化 Message DAL（使用全局单例 DAO）
pub fn init() {
    let _ = MESSAGE_DAL.set(new(
        message::dao(),
        message::vector_dao(),
        event_queue::message_dao(),
        crate::service::dao::cortex::dao(),
        crate::service::dao::model_provider::dao(),
    ));
}

/// 创建 Message DAL（返回 trait 对象）
pub fn new(
    message_dao: Arc<dyn MessageDao + Send + Sync>,
    message_vector_dao: Arc<dyn MessageVectorDao + Send + Sync>,
    event_queue_dao: Arc<dyn EventQueueDao<Message> + Send + Sync>,
    cortex_dao: Arc<dyn CortexDao + Send + Sync>,
    model_provider_dao: Arc<dyn ModelProviderDao + Send + Sync>,
) -> Arc<dyn MessageDal> {
    Arc::new(MessageDalImpl {
        message_dao,
        message_vector_dao,
        event_queue_dao,
        cortex_dao,
        model_provider_dao,
    })
}

// ==================== DAL 接口 ====================

/// Message DAL 接口
#[async_trait::async_trait]
pub trait MessageDal: Send + Sync {
    /// 保存消息
    ///
    /// 保存到数据库后自动入队事件队列，所有消息都入队不做过滤
    async fn save_message(&self, ctx: RequestContext, message: &Message) -> Result<()>;

    /// 通用综合查询
    ///
    /// 支持组合查询条件，所有字段都是 Option
    /// 示例：
    /// ```ignore
    /// let messages = dal.query(ctx, MessageQuery {
    ///     task_id: Some("task-123".to_string()),
    ///     status_in: Some(vec![MessageStatus::Pending, MessageStatus::Processing]),
    ///     limit: Some(10),
    ///     ..Default::default()
    /// }).await?;
    /// ```
    async fn query(
        &self,
        ctx: RequestContext,
        query: MessageQuery,
    ) -> Result<Vec<Message>>;

    /// 按任务 ID 查询消息列表
    ///
    /// 默认按 created_at 升序排序，保持对话顺序
    /// 支持限制返回条数，用于分页加载
    async fn list_by_task_id(
        &self,
        ctx: RequestContext,
        task_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Message>>;

    /// 按项目 ID 查询消息列表
    ///
    /// 默认按 created_at 升序排序，保持对话顺序
    /// 支持限制返回条数，用于分页加载
    async fn list_by_project_id(
        &self,
        ctx: RequestContext,
        project_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Message>>;

    /// 按发送方 ID 查询消息列表
    async fn list_by_from_id(
        &self,
        ctx: RequestContext,
        from_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Message>>;

    /// 按接收方 ID 查询消息列表
    async fn list_by_to_id(
        &self,
        ctx: RequestContext,
        to_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Message>>;

    /// 按状态查询消息列表
    async fn list_by_status(
        &self,
        ctx: RequestContext,
        status: Vec<MessageStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<Message>>;

    /// 更新消息状态
    async fn update_status(
        &self,
        ctx: RequestContext,
        message_id: &str,
        status: MessageStatus,
    ) -> Result<()>;

    /// 统计任务消息数量
    async fn count_by_task_id(&self, ctx: RequestContext, task_id: &str) -> Result<u64>;

    /// 根据 ID 查询消息
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Message>>;

    /// 删除消息
    async fn delete_message(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 删除任务下所有消息
    async fn delete_by_task_id(&self, ctx: RequestContext, task_id: &str) -> Result<()>;

    /// 从事件队列中取出下一个待处理的消息事件
    ///
    /// 返回 None 表示队列为空
    /// 获取后消息事件进入 "处理中" 状态，需要调用 ack_message 确认完成
    async fn dequeue_next_message(&self, ctx: RequestContext) -> Result<Option<Message>>;

    /// 确认消息处理完成，从事件队列中移除
    async fn ack_message(&self, ctx: RequestContext, message_id: &str) -> Result<()>;

    /// 标记消息处理失败，重新放回队列等待重试
    async fn nack_message(&self, ctx: RequestContext, message_id: &str) -> Result<()>;

    /// 🔍 统一混合搜索（关键词 + 向量语义）
    ///
    /// 自动根据参数选择搜索策略：
    /// - keyword 存在 → 走 FTS5 全文检索
    /// - query_vector 存在 → 走向量语义搜索
    /// - 两者都有 → 混合搜索，合并结果
    async fn search(
        &self,
        ctx: RequestContext,
        search: MessageSearch,
    ) -> Result<Vec<Message>>;

    /// 🔄 重建所有消息的向量索引
    ///
    /// 清空向量集合后，查询全量消息，逐条重新生成 embedding 并 upsert。
    /// 单条失败不影响整体，用 log_warn! 记录。
    async fn rebuild_vectors(&self, ctx: RequestContext) -> Result<()>;
}

// ==================== DAL 实现 ====================

/// Message DAL 实现
struct MessageDalImpl {
    message_dao: Arc<dyn MessageDao>,
    message_vector_dao: Arc<dyn MessageVectorDao>,
    event_queue_dao: Arc<dyn EventQueueDao<Message>>,
    cortex_dao: Arc<dyn CortexDao>,
    model_provider_dao: Arc<dyn ModelProviderDao>,
}

#[async_trait::async_trait]
impl MessageDal for MessageDalImpl {
    async fn save_message(&self, ctx: RequestContext, message: &Message) -> Result<()> {
        // 1. 保存消息到数据库
        self.message_dao.insert(ctx.clone(), &message.po).await?;

        // 2. 消息本身就是事件，直接入队，所有消息都入队不做过滤
        // Message 已经实现了 Event trait
        let event: Box<Message> = Box::new(message.clone());
        self.event_queue_dao.enqueue(ctx.clone(), event)?;

        // 3. 自动维护向量索引（失败仅 warn 降级，不影响主流程）
        match try_build_vector_params_for_entity(
            ctx.clone(),
            &self.cortex_dao,
            &self.model_provider_dao,
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

    async fn query(
        &self,
        ctx: RequestContext,
        query: MessageQuery,
    ) -> Result<Vec<Message>> {
        // 调用 DAO 层查询，得到 PO 列表
        let pos = self.message_dao.query(ctx, query).await?;
        // 转换为业务实体
        Ok(pos.into_iter().map(Message::from_po).collect())
    }

    async fn list_by_task_id(
        &self,
        ctx: RequestContext,
        task_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Message>> {
        // 语法糖：调用通用查询
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
        // 语法糖：调用通用查询
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
        // 语法糖：调用通用查询
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
        // 语法糖：调用通用查询
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
        // 语法糖：调用通用查询
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
        self.message_dao.count_by_task_id(ctx, task_id).await
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Message>> {
        let opt = self.message_dao.find_by_id(ctx, id).await?;
        Ok(opt.map(Message::from_po))
    }

    async fn delete_message(&self, ctx: RequestContext, id: &str) -> Result<()> {
        // 1. 软删除消息（更新状态为 Recalled）
        self.message_dao.delete(ctx.clone(), id).await?;
        // 2. 删除向量索引（失败仅 warn 降级，不影响主流程）
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

    async fn dequeue_next_message(&self, ctx: RequestContext) -> Result<Option<Message>> {
        // 1. 优先从内存队列取出
        let opt_msg = self.event_queue_dao.dequeue_next(ctx.clone())?;
        match opt_msg {
            Some(msg) => {
                // 出队成功后更新状态为 Processing，避免回源重复入队
                let msg = *msg;
                self.update_status(ctx.clone(), msg.id(), MessageStatus::Processing)
                    .await?;
                Ok(Some(msg))
            }
            None => {
                // 2. 队列为空，回源 DB 查询 pending 状态的消息
                // 查询 Pending 状态，最多取 5 条，按创建时间升序
                let pending_messages = self
                    .list_by_status(ctx.clone(), vec![MessageStatus::Pending], Some(5))
                    .await?;

                if pending_messages.is_empty() {
                    // DB 也没有，真的空了
                    return Ok(None);
                }

                // 3. 将 DB 查到的消息全部入队到内存队列
                for msg in pending_messages {
                    let event: Box<Message> = Box::new(msg.clone());
                    self.event_queue_dao.enqueue(ctx.clone(), event)?;
                }

                // 4. 再次尝试出队（肯定能取到了）
                Ok(self
                    .event_queue_dao
                    .dequeue_next(ctx.clone())?
                    .map(|msg| *msg))
            }
        }
    }

    async fn ack_message(&self, _ctx: RequestContext, message_id: &str) -> Result<()> {
        self.event_queue_dao.ack(_ctx.clone(), message_id)
    }

    async fn nack_message(&self, _ctx: RequestContext, message_id: &str) -> Result<()> {
        self.event_queue_dao.nack(_ctx.clone(), message_id)
    }

    async fn search(
        &self,
        ctx: RequestContext,
        search: MessageSearch,
    ) -> Result<Vec<Message>> {
        // 向量距离阈值（默认 0.8，越小越相似）
        let vector_distance_threshold = 0.8_f32;
        let top_k = search.top_k.unwrap_or(50);

        // Step 1: 准备向量搜索结果容器
        let mut vector_scores: HashMap<String, f32> = HashMap::new();
        let mut vector_ids: HashSet<String> = HashSet::new();

        // Step 2: 如果有关键词或 query_vector，执行向量搜索
        // 参照 memory 模式：有关键词时用关键词生成 query_vector（如果未显式提供）
        let has_keyword = search
            .keyword
            .as_deref()
            .map(|k| !k.trim().is_empty())
            .unwrap_or(false);
        let has_query_vector = search
            .query_vector
            .as_ref()
            .map(|v| !v.is_empty())
            .unwrap_or(false);

        if has_keyword || has_query_vector {
            // 优先使用显式传入的 query_vector，否则用关键词生成
            let vec_params_opt = if has_query_vector {
                // 直接使用传入的 query_vector
                Some(crate::models::vector::VectorIndexParams {
                    vector: search.query_vector.clone().unwrap(),
                    content_hash: String::new(),
                    model_provider_id: String::new(),
                    embedding_model: String::new(),
                    expire_at: None,
                })
            } else {
                // 用关键词生成 query_vector
                match try_build_vector_params_for_search(
                    ctx.clone(),
                    &self.cortex_dao,
                    &self.model_provider_dao,
                    search.keyword.as_deref().unwrap_or(""),
                )
                .await
                {
                    Ok(params) => params,
                    Err(e) => {
                        log_warn!(
                            &ctx,
                            "vector_search",
                            error = ?e,
                            "消息向量化失败，降级到关键词搜索"
                        );
                        None
                    }
                }
            };

            if let Some(vec_params) = vec_params_opt {
                match self
                    .message_vector_dao
                    .search_vector(ctx.clone(), &vec_params.vector, top_k)
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
                        log_warn!(
                            &ctx,
                            "vector_search",
                            "消息向量搜索失败，降级到关键词搜索: {}",
                            e
                        );
                    }
                }
            }
        }

        // Step 3: 执行关键词搜索（DAO 返回 Vec<(Po, fts_rank)>）
        let keyword_results = if has_keyword {
            self.message_dao
                .search_messages(ctx.clone(), search.clone())
                .await?
        } else {
            Vec::new()
        };

        // 提取 fts_rank 并转换为 Vec<Po> 便于聚合
        let mut fts_ranks: HashMap<String, f32> = HashMap::new();
        let keyword_pos: Vec<MessagePo> = keyword_results
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
                let vector_pos = self.message_dao.query(ctx.clone(), query_for_ids).await?;
                all_pos.extend(vector_pos);
            }
        }

        // Step 5: 去重
        all_pos.sort_by(|a, b| a.id.cmp(&b.id));
        all_pos.dedup_by(|a, b| a.id == b.id);

        // Step 6: 构建业务对象，附加三态匹配信息
        let mut messages = Vec::with_capacity(all_pos.len());
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
            messages.push(Message {
                po,
                search_match: match_info,
            });
        }

        // Step 7: 综合排序：Hybrid 优先 → Vector 次之 → Keyword 最后
        //    组内排序：Hybrid/Vector 按向量距离升序，Keyword 按 fts_rank 升序（BM25 越小越相关）
        messages.sort_by(|a, b| {
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
            messages.truncate(limit);
        }

        Ok(messages)
    }

    async fn rebuild_vectors(&self, ctx: RequestContext) -> Result<()> {
        // 1. 清空向量集合
        self.message_vector_dao.clear_collection(ctx.clone()).await?;

        // 2. 查全量消息
        let messages = self.query(ctx.clone(), MessageQuery::default()).await?;

        // 3. 逐条重新索引
        for message in &messages {
            match try_build_vector_params_for_entity(
                ctx.clone(),
                &self.cortex_dao,
                &self.model_provider_dao,
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
                            "rebuild_vectors",
                            message_id = %message.po.id,
                            error = ?e,
                            "消息向量索引重建失败"
                        );
                    }
                }
                Ok(None) => {
                    log_debug!(
                        &ctx,
                        "rebuild_vectors",
                        message_id = %message.po.id,
                        "无可用 Embedding Provider，跳过向量索引"
                    );
                }
                Err(e) => {
                    log_warn!(
                        &ctx,
                        "rebuild_vectors",
                        message_id = %message.po.id,
                        error = ?e,
                        "消息向量化失败，跳过"
                    );
                }
            }
        }

        Ok(())
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

    let cortex = cortex_dao.create_cortex_trait(ctx.clone(), &provider, vec![])?;
    let params = cortex_dao
        .embed_text_for_search(ctx, cortex.as_ref(), text)
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
