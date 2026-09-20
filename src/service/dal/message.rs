//! Message DAL 模块
//!
//! 基础消息数据访问层，提供消息保存和查询能力
//! 所有保存的消息都会自动发布到 AOP 事件中心

use crate::models::events::MessageCreatedEvent;
use crate::models::message::{Message, MessagePo};
use crate::models::vector::{VectorIndexParams, Vectorizable};
use crate::pkg::RequestContext;
use crate::pkg::aop::{self, Producer, RetryDecision};
use crate::pkg::background_task::TaskProgressCounter;
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::message::{
    self, MessageDao, MessageQuery, MessageSearch, MessageVectorDao,
};
use crate::service::dao::model_provider::ModelProviderDao;
use crate::service::dao::organization::OrganizationDao;
use common::enums::{EventTopic, MessageRole, MessageStatus, MessageType};
use common::error::Result;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

static MESSAGE_DAL: OnceLock<Arc<dyn MessageDal>> = OnceLock::new();

pub fn dal() -> Arc<dyn MessageDal> {
    MESSAGE_DAL.get().cloned().unwrap()
}

/// 装配消息 DAL 单例，并把它**同时**注册为 `message.created` 的生产者
///
/// 同一个对象、两种身份（`Arc<MessageDalImpl>` 各 coerce 一次）：
/// - `Arc<dyn MessageDal>`：业务读写的入口；
/// - `Arc<dyn Producer>`：`message.created` 的**归属** —— 消费完成后由它做业务收尾
///   （`messages.status` 翻转），即原先写在 `consumer/message.rs` 里的 `ack` / `nack`。
///
/// ⚠️ 必须在 `aop::init_all()`（即 `start_all`）**之前**调用：`start_all` 会校验
/// 「声明了 `notify_producer` 的 topic 必须有生产者」，缺了它启动即失败。
/// 实际启动顺序已满足：`service::init()` → `dal::init_all()` 早于 `aop::init_all()`。
pub fn init() {
    let dal = new_impl(
        message::dao(),
        message::vector_dao(),
        crate::service::dao::cortex::dao(),
        crate::service::dao::model_provider::dao(),
        crate::service::dao::organization::dao(),
    );

    // 幂等：OnceLock 抢占失败 = 已初始化过（生产只调一次；测试里
    // `init_service_for_test()` 会被反复调用）→ 直接返回，避免第二次
    // `register_producer` 撞上 topic 占用校验
    // 同一个底层分配，两次 coerce：先取强类型副本，再各自转 trait object
    let concrete = Arc::clone(&dal);
    let as_dal: Arc<dyn MessageDal> = concrete;
    if MESSAGE_DAL.set(as_dal).is_err() {
        return;
    }

    let as_producer: Arc<dyn Producer> = dal;
    aop::registry()
        .register_producer(as_producer)
        .expect("register message.created producer (topic 占用冲突 = 装配错误)");
}

/// 构造具体的 `MessageDalImpl`
///
/// 存在的理由：注册生产者需要**具体类型的** `Arc<MessageDalImpl>`（才能各自 coerce
/// 成 `Arc<dyn MessageDal>` 与 `Arc<dyn Producer>`）；而 [`new`] 的返回类型
/// `Arc<dyn MessageDal>` 保持不动（测试在用）。
pub(crate) fn new_impl(
    message_dao: Arc<dyn MessageDao + Send + Sync>,
    message_vector_dao: Arc<dyn MessageVectorDao + Send + Sync>,
    cortex_dao: Arc<dyn CortexDao + Send + Sync>,
    model_provider_dao: Arc<dyn ModelProviderDao + Send + Sync>,
    organization_dao: Arc<dyn OrganizationDao + Send + Sync>,
) -> Arc<MessageDalImpl> {
    Arc::new(MessageDalImpl {
        message_dao,
        message_vector_dao,
        cortex_dao,
        model_provider_dao,
        organization_dao,
    })
}

pub fn new(
    message_dao: Arc<dyn MessageDao + Send + Sync>,
    message_vector_dao: Arc<dyn MessageVectorDao + Send + Sync>,
    cortex_dao: Arc<dyn CortexDao + Send + Sync>,
    model_provider_dao: Arc<dyn ModelProviderDao + Send + Sync>,
    organization_dao: Arc<dyn OrganizationDao + Send + Sync>,
) -> Arc<dyn MessageDal> {
    new_impl(
        message_dao,
        message_vector_dao,
        cortex_dao,
        model_provider_dao,
        organization_dao,
    )
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

    /// 按外部渠道键反查内部消息 ID（未留痕返回 None）
    ///
    /// 入站幂等吸收用：external_key 已存在 → 该外部消息已落库（游标回退 / 服务端
    /// 重推 / 事件重投后的重复拉取），跳过落库返回既有消息，避免重复投递给 Agent。
    async fn find_id_by_external_key(
        &self,
        ctx: RequestContext,
        external_key: &str,
    ) -> Result<Option<String>>;

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

    /// 🔄 重建所有消息的向量索引
    ///
    /// 无条件清空向量集合后，**分页**查询全量消息，逐条重新生成 embedding 并 upsert。
    /// 单条失败不影响整体，用 log_warn! 记录；每页处理完通过 `progress` 上报条数。
    async fn rebuild_vectors(
        &self,
        ctx: RequestContext,
        progress: &crate::pkg::background_task::TaskProgressCounter,
    ) -> Result<()>;
}

pub(crate) struct MessageDalImpl {
    message_dao: Arc<dyn MessageDao>,
    message_vector_dao: Arc<dyn MessageVectorDao>,
    cortex_dao: Arc<dyn CortexDao>,
    model_provider_dao: Arc<dyn ModelProviderDao>,
    organization_dao: Arc<dyn OrganizationDao>,
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
        aop::publish(&ctx, event).await;

        // 组织级开关：默认不构建消息向量索引（enable_message_vector 默认 false）。
        // 配置经组织 DAO 的缓存读取；关闭时连 Embedding Provider 都不查，
        // 关键词 FTS 搜索不受影响。
        let vector_enabled = match message.po.organization_id.as_deref() {
            Some(org_id) => self
                .organization_dao
                .get_org_config(ctx.clone(), org_id)
                .await
                .map(|config| config.enable_message_vector)
                .unwrap_or(false),
            None => false,
        };
        if !vector_enabled {
            log_debug!(
                &ctx,
                "vector_index",
                message_id = %message.po.id,
                "组织未开启消息向量索引，跳过"
            );
            return Ok(());
        }

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

    async fn find_id_by_external_key(
        &self,
        ctx: RequestContext,
        external_key: &str,
    ) -> Result<Option<String>> {
        self.message_dao
            .find_id_by_external_key(ctx, external_key)
            .await
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

    async fn rebuild_vectors(
        &self,
        ctx: RequestContext,
        progress: &TaskProgressCounter,
    ) -> Result<()> {
        // 清空全部 message 向量：天然让“关闭开关的 org”无残留（已被清、不重建）
        self.message_vector_dao
            .clear_collection(ctx.clone())
            .await?;

        // 分页重建：默认排序 `created_at ASC`（不可变）翻页，排序键稳定 ⇒ 不会漏行。
        // 期间新写入的消息会由常规写路径自行 upsert 向量，故偏移漂移无副作用。
        // 组织级开关逐条读取（DAO 读穿缓存，同组织仅首次落库）。
        let mut enabled = 0usize;
        let mut skipped = 0usize;
        let mut offset = 0usize;
        loop {
            let page = self
                .query(
                    ctx.clone(),
                    MessageQuery {
                        limit: Some(crate::service::dal::VECTOR_REBUILD_PAGE_SIZE),
                        offset: Some(offset),
                        ..Default::default()
                    },
                )
                .await?;
            let fetched = page.len();
            if fetched == 0 {
                break;
            }

            for message in page {
                let org_id = message.po.organization_id.clone().unwrap_or_default();
                // 无组织归属的消息不建向量
                if org_id.is_empty() {
                    skipped += 1;
                    continue;
                }
                // DAL→DAO 注入字段读取组织级开关，关闭则整组跳过
                let cfg = self
                    .organization_dao
                    .get_org_config(ctx.clone(), &org_id)
                    .await?;
                if !cfg.enable_message_vector {
                    skipped += 1;
                    continue;
                }
                enabled += 1;
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

            offset += fetched;
            progress.advance(fetched);
            if fetched < crate::service::dal::VECTOR_REBUILD_PAGE_SIZE {
                break;
            }
        }

        sys_info!(
            &ctx,
            "vector_index",
            "rebuild message vectors: enabled_msgs={} skipped_msgs={}",
            enabled,
            skipped
        );
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
        // pre-filter：白名单字段（organization_id / task_id / project_id / from_id /
        // to_id / status_in）在 DAO 内转译为向量谓词下推，Top-K 在满足谓词的候选集内选取
        let results = message_vector_dao
            .search_vector(
                ctx.clone(),
                query_vector,
                search.top_k.unwrap_or(20),
                &search.filters,
            )
            .await?;

        // ⚠️ 非白名单字段（reply_to_id / root_id / to_role / message_type 等）仍需
        // 回业务表过滤兜底；白名单字段在此重复校验，双保险不损害正确性。
        let hit_ids: Vec<String> = results.iter().map(|h| h.row.id.clone()).collect();
        let mut hits_by_id: HashMap<String, MessagePo> = HashMap::new();
        if !hit_ids.is_empty() {
            let lookup = MessageQuery {
                ids: Some(hit_ids),
                // 按 id 精确回填，不能被调用方 limit / offset 截断（最终条数由合并阶段收口）
                limit: None,
                offset: None,
                order_by: None,
                ..search.filters.clone()
            };
            for po in message_dao.query(ctx.clone(), lookup).await? {
                hits_by_id.insert(po.id.clone(), po);
            }
        }

        // 按向量距离顺序保留命中集：merge_search_results 依赖该顺序打分
        let mut matches = Vec::new();
        for hit in results {
            if let Some(po) = hits_by_id.remove(&hit.row.id) {
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

impl MessageDalImpl {
    /// 事件封套里的 `event_id` 就是 messages 表主键
    ///
    /// `MessageCreatedEvent::id()` 返回的正是 message_id，而 `Registry::publish` 会把
    /// `event.id()` 注入封套顶层的 `event_id` —— 框架不感知业务主键，只搬运 `Event::id()`。
    pub(crate) fn message_id_of(event: &serde_json::Value) -> Option<&str> {
        event.get("event_id").and_then(|v| v.as_str())
    }
}

/// `message.created` 的归属：业务收尾由**拥有这份状态的那个对象自身**完成
///
/// 这就是「生产者对象 = 拥有该 topic 业务收尾能力的对象本身」的一个实例 ——
/// 不新建 `MessageCreatedProducer`，因为那只会把 DAL 已有的能力再包一层。
///
/// 搬进来的两个方法体原本是 `consumer/message.rs` 的 `Consumer::ack` / `nack`：
/// 搬走后消费者只剩「把消息送到 Agent / 让 Agent 沉淀」，投递生命周期完全归
/// 框架（判定 ack/nack）+ 生产者（业务持久化）。消费者的 `Arc<dyn MessageDal>`
/// 依赖也从「写状态」缩回到「读消息」。
#[async_trait::async_trait]
impl Producer for MessageDalImpl {
    fn name(&self) -> &str {
        "message_dal"
    }

    fn topic(&self) -> EventTopic {
        EventTopic::MessageCreated
    }

    /// 事件生命周期**终结**（成功消费 **或** 被放弃）→ 消息置为 `Processed`
    ///
    /// ⚠️ 框架在 `queue.ack` **之前**调用，且 `Discard` 后还会再回调一次
    /// → 必须幂等（写同一个状态值，幂等成立）。
    async fn on_consumed(&self, ctx: &RequestContext, event: &serde_json::Value) -> Result<()> {
        let Some(message_id) = Self::message_id_of(event) else {
            // 理论不可达（publish 必注入 event_id）。缺了便无从收尾，但「收尾失败」
            // 不该影响投递结论（框架只记日志）→ 直接返回，让事件正常 ack。
            return Ok(());
        };

        self.update_status(ctx.clone(), message_id, MessageStatus::Processed)
            .await
    }

    /// 本次尝试失败：**按消费者给出的结论**决定底层数据怎么改
    ///
    /// 这里不做任何终局判定 —— 重试还是放弃由 `MessageConsumer::decide_retry`
    /// 回答（默认策略：永久性错误码 / 累计 8 次），本方法只收结论：
    /// - `Retry` → 消息置回 `Pending`：它是**启动恢复的依据** —— `message_dal` 按
    ///   `status = Pending` 扫出未处理消息重投（等价于改造前 `Consumer::nack` 的副作用）；
    ///   漏这一步，失败的消息会停在 `Processing` 上再也没人捞。
    /// - `Discard` → **不**置回 `Pending`：否则启动恢复会把这条已放弃的消息再扫出来重投。
    ///   框架随后回调 `on_consumed` → 消息置为 `Processed`，并留下 error 日志 + 独立埋点。
    async fn on_failed(
        &self,
        ctx: &RequestContext,
        event: &serde_json::Value,
        _err: &str,
        decision: RetryDecision,
        _attempt: u32,
    ) -> Result<()> {
        if decision == RetryDecision::Discard {
            return Ok(());
        }

        if let Some(message_id) = Self::message_id_of(event) {
            self.update_status(ctx.clone(), message_id, MessageStatus::Pending)
                .await?;
        }
        Ok(())
    }
}
