//! Message DAL 单元测试

use crate::models::brain::CortexTrait;
use crate::models::file::FileMeta;
use crate::models::message::Message;
use crate::models::model_provider::ModelProviderPo;
use crate::models::vector::{MatchType, VectorIndexParams};
use crate::pkg::RequestContext;
use crate::service::dal::message::MessageDal;
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::message;
use crate::service::dao::message::{MessageQuery, MessageSearch};
use crate::service::dao::model_provider::{ModelProviderDao, ModelProviderQuery};
use common::enums::{MessageRole, MessageStatus, MessageType};
use common::error::Result;
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

// ========== Mock 实现（跳过向量依赖）==========

/// Mock ModelProviderDao（返回 None，跳过向量搜索）
struct MockModelProviderDao;

#[async_trait::async_trait]
impl ModelProviderDao for MockModelProviderDao {
    async fn insert(&self, _ctx: RequestContext, _provider: &ModelProviderPo) -> Result<()> {
        Ok(())
    }
    async fn find_by_id(&self, _ctx: RequestContext, _id: &str) -> Result<Option<ModelProviderPo>> {
        Ok(None)
    }
    async fn query(
        &self,
        _ctx: RequestContext,
        _query: ModelProviderQuery,
    ) -> Result<common::api::PagedResult<ModelProviderPo>> {
        Ok(common::api::PagedResult {
            items: Vec::new(),
            total: 0,
        })
    }
    async fn find_all(&self, _ctx: RequestContext) -> Result<Vec<ModelProviderPo>> {
        Ok(Vec::new())
    }
    async fn update(&self, _ctx: RequestContext, _provider: &ModelProviderPo) -> Result<()> {
        Ok(())
    }
    async fn delete(&self, _ctx: RequestContext, _provider: &ModelProviderPo) -> Result<()> {
        Ok(())
    }
    async fn get_default_embedding_provider(
        &self,
        _ctx: RequestContext,
    ) -> Result<Option<ModelProviderPo>> {
        Ok(None)
    }

    async fn find_enabled_embedding_provider(
        &self,
        _ctx: RequestContext,
    ) -> Result<Option<ModelProviderPo>> {
        Ok(None)
    }
}

/// Mock CortexDao（跳过向量搜索）
struct MockCortexDao;

#[async_trait::async_trait]
impl CortexDao for MockCortexDao {
    fn create_cortex_trait(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
        _rig_tools: Vec<rig::tool::DynamicTool>,
    ) -> anyhow::Result<Box<dyn CortexTrait + Send + Sync>> {
        panic!("MockCortexDao::create_cortex_trait not implemented for tests");
    }
    async fn prompt(
        &self,
        _ctx: RequestContext,
        _cortex: &dyn CortexTrait,
        _prompt: &str,
    ) -> anyhow::Result<String> {
        Ok("".to_string())
    }
    async fn embed_text_raw(
        &self,
        _ctx: RequestContext,
        _cortex: &dyn CortexTrait,
        _text: &str,
    ) -> anyhow::Result<Vec<f32>> {
        Ok(Vec::new())
    }
    async fn embed_entity(
        &self,
        _ctx: RequestContext,
        _cortex: &dyn CortexTrait,
        _entity: &dyn crate::models::vector::Vectorizable,
    ) -> anyhow::Result<crate::models::vector::VectorIndexParams> {
        Ok(crate::models::vector::VectorIndexParams {
            vector: Vec::new(),
            content_hash: "".to_string(),
            model_provider_id: "".to_string(),
            embedding_model: "".to_string(),
            expire_at: None,
        })
    }
    async fn embed_text_for_search(
        &self,
        _ctx: RequestContext,
        _cortex: &dyn CortexTrait,
        _text: &str,
    ) -> anyhow::Result<crate::models::vector::VectorIndexParams> {
        Ok(crate::models::vector::VectorIndexParams {
            vector: Vec::new(),
            content_hash: "".to_string(),
            model_provider_id: "".to_string(),
            embedding_model: "".to_string(),
            expire_at: None,
        })
    }
}

// ========== 测试辅助函数 ==========

/// 初始化测试环境（使用 Mock Cortex/ModelProvider，跳过向量索引自动维护）
async fn init_test_env(pool: SqlitePool) -> (Arc<dyn MessageDal + Send + Sync>, RequestContext) {
    let message_dao = message::sqlite::new();
    let message_vector_dao = message::vector::new();
    let cortex_dao: Arc<dyn CortexDao> = Arc::new(MockCortexDao);
    let model_provider_dao: Arc<dyn ModelProviderDao> = Arc::new(MockModelProviderDao);
    let dal = crate::service::dal::message::new(
        message_dao,
        message_vector_dao,
        cortex_dao,
        model_provider_dao,
    );
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);
    (dal, ctx)
}

/// 创建测试消息
fn create_test_message(
    task_id: &str,
    from_id: &str,
    to_id: &str,
    from_role: MessageRole,
    to_role: MessageRole,
    content: String,
) -> Message {
    let id = Uuid::now_v7().to_string();
    let file_meta = FileMeta::default();
    Message::new_with_context(
        id,
        None,
        Some(task_id.to_string()),
        from_id.to_string(),
        to_id.to_string(),
        from_role,
        to_role,
        MessageType::Text,
        content,
        None,
        file_meta,
        None,
        None, // root_id
        None, // organization_id
        from_id.to_string(),
    )
}

/// 创建测试向量参数
fn create_test_vector_params(message_id: &str, dimension: usize) -> VectorIndexParams {
    VectorIndexParams {
        vector: (0..dimension)
            .map(|i| i as f32 / dimension as f32)
            .collect(),
        content_hash: format!("hash_{}", message_id),
        model_provider_id: "test_provider".to_string(),
        embedding_model: "test-embedding-v1".to_string(),
        expire_at: None,
    }
}

#[sqlx::test]
async fn test_save_and_find_by_id(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let msg = create_test_message(
        "task-1",
        "user-1",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "Hello world".to_string(),
    );

    dal.save_message(ctx.clone(), &msg).await.unwrap();
    let found = dal.find_by_id(ctx, msg.id()).await.unwrap();

    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.po.content, "Hello world");
    assert_eq!(found.task_id(), Some("task-1"));
    assert_eq!(found.po.from_role, MessageRole::User);
    assert_eq!(found.po.to_role, MessageRole::Agent);
    assert_eq!(found.po.status, MessageStatus::Pending);
}

#[sqlx::test]
async fn test_list_by_task_id(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    // Add messages to two different tasks
    for i in 0..5 {
        let msg = create_test_message(
            "task-1",
            "user-1",
            "agent-1",
            MessageRole::User,
            MessageRole::Agent,
            format!("Message {}", i),
        );
        dal.save_message(ctx.clone(), &msg).await.unwrap();
    }

    for i in 0..3 {
        let msg = create_test_message(
            "task-2",
            "user-1",
            "agent-2",
            MessageRole::User,
            MessageRole::Agent,
            format!("Other {}", i),
        );
        dal.save_message(ctx.clone(), &msg).await.unwrap();
    }

    let list = dal
        .list_by_task_id(ctx.clone(), "task-1", None)
        .await
        .unwrap();
    assert_eq!(list.len(), 5);
    // Check order: created_at ASC
    for (i, msg) in list.iter().enumerate() {
        assert_eq!(msg.po.content, format!("Message {}", i));
    }

    let list2 = dal.list_by_task_id(ctx, "task-2", None).await.unwrap();
    assert_eq!(list2.len(), 3);
}

#[sqlx::test]
async fn test_list_by_task_id_with_limit(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    for i in 0..10 {
        let msg = create_test_message(
            "task-1",
            "user-1",
            "agent-1",
            MessageRole::User,
            MessageRole::Agent,
            format!("Message {}", i),
        );
        dal.save_message(ctx.clone(), &msg).await.unwrap();
    }

    let list = dal.list_by_task_id(ctx, "task-1", Some(5)).await.unwrap();
    assert_eq!(list.len(), 5);
    // First 5 messages in order
    for (i, msg) in list.iter().enumerate() {
        assert_eq!(msg.po.content, format!("Message {}", i));
    }
}

#[sqlx::test]
async fn test_list_by_from_id(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    for i in 0..3 {
        let msg = create_test_message(
            "task-1",
            "user-alice",
            "agent-1",
            MessageRole::User,
            MessageRole::Agent,
            format!("From Alice {}", i),
        );
        dal.save_message(ctx.clone(), &msg).await.unwrap();
    }

    let msg2 = create_test_message(
        "task-1",
        "user-bob",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "From Bob".to_string(),
    );
    dal.save_message(ctx.clone(), &msg2).await.unwrap();

    let list = dal.list_by_from_id(ctx, "user-alice", None).await.unwrap();
    assert_eq!(list.len(), 3);
}

#[sqlx::test]
async fn test_list_by_to_id(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    for i in 0..4 {
        let msg = create_test_message(
            "task-1",
            "user-1",
            "agent-alice",
            MessageRole::User,
            MessageRole::Agent,
            format!("To Alice {}", i),
        );
        dal.save_message(ctx.clone(), &msg).await.unwrap();
    }

    let list = dal.list_by_to_id(ctx, "agent-alice", None).await.unwrap();
    assert_eq!(list.len(), 4);
}

#[sqlx::test]
async fn test_list_by_status(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let mut msg1 = create_test_message(
        "task-1",
        "user-1",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "Pending".to_string(),
    );
    let mut msg2 = create_test_message(
        "task-1",
        "agent-1",
        "user-1",
        MessageRole::Agent,
        MessageRole::User,
        "Processed".to_string(),
    );
    msg1.po.status = MessageStatus::Pending;
    msg2.po.status = MessageStatus::Processed;

    dal.save_message(ctx.clone(), &msg1).await.unwrap();
    dal.save_message(ctx.clone(), &msg2).await.unwrap();

    let pending = dal
        .list_by_status(ctx.clone(), vec![MessageStatus::Pending], None)
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);

    let processed = dal
        .list_by_status(ctx.clone(), vec![MessageStatus::Processed], None)
        .await
        .unwrap();
    assert_eq!(processed.len(), 1);

    let both = dal
        .list_by_status(
            ctx,
            vec![MessageStatus::Pending, MessageStatus::Processed],
            None,
        )
        .await
        .unwrap();
    assert_eq!(both.len(), 2);
}

#[sqlx::test]
async fn test_update_status(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let mut msg = create_test_message(
        "task-1",
        "user-1",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "Hello".to_string(),
    );
    msg.po.status = MessageStatus::Pending;
    dal.save_message(ctx.clone(), &msg).await.unwrap();

    let found = dal
        .find_by_id(ctx.clone(), msg.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.po.status, MessageStatus::Pending);

    dal.update_status(ctx.clone(), msg.id(), MessageStatus::Processed)
        .await
        .unwrap();

    let found = dal.find_by_id(ctx, msg.id()).await.unwrap().unwrap();
    assert_eq!(found.po.status, MessageStatus::Processed);
}

#[sqlx::test]
async fn test_count_by_task_id(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    for i in 0..7 {
        let msg = create_test_message(
            "task-counter",
            "user-1",
            "agent-1",
            MessageRole::User,
            MessageRole::Agent,
            format!("Msg {}", i),
        );
        dal.save_message(ctx.clone(), &msg).await.unwrap();
    }

    let count = dal
        .count_by_task_id(ctx.clone(), "task-counter")
        .await
        .unwrap();
    assert_eq!(count, 7);

    let count2 = dal.count_by_task_id(ctx, "empty-task").await.unwrap();
    assert_eq!(count2, 0);
}

#[sqlx::test]
async fn test_delete_message(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let msg = create_test_message(
        "task-1",
        "user-1",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "To be deleted".to_string(),
    );
    dal.save_message(ctx.clone(), &msg).await.unwrap();

    let found = dal.find_by_id(ctx.clone(), msg.id()).await.unwrap();
    assert!(found.is_some());

    dal.delete_message(ctx.clone(), msg.id()).await.unwrap();

    let found = dal.find_by_id(ctx, msg.id()).await.unwrap();
    assert!(found.is_none());
}

#[sqlx::test]
async fn test_delete_by_task_id(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    for i in 0..5 {
        let msg = create_test_message(
            "task-to-delete",
            "user-1",
            "agent-1",
            MessageRole::User,
            MessageRole::Agent,
            format!("Msg {}", i),
        );
        dal.save_message(ctx.clone(), &msg).await.unwrap();
    }

    let count = dal
        .count_by_task_id(ctx.clone(), "task-to-delete")
        .await
        .unwrap();
    assert_eq!(count, 5);

    dal.delete_by_task_id(ctx.clone(), "task-to-delete")
        .await
        .unwrap();

    let count = dal.count_by_task_id(ctx, "task-to-delete").await.unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test]
async fn test_find_not_exists(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let found = dal.find_by_id(ctx, "not-existent-id").await.unwrap();
    assert!(found.is_none());
}

// ========== 搜索测试 ==========

#[sqlx::test]
async fn test_search_fts5_keyword(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    // 创建几条消息
    let msg1 = create_test_message(
        "task-search",
        "user-1",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "Hello world from Rust".to_string(),
    );
    let msg2 = create_test_message(
        "task-search",
        "user-1",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "Rust programming language".to_string(),
    );
    let msg3 = create_test_message(
        "task-search",
        "user-1",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "Python is also great".to_string(),
    );
    dal.save_message(ctx.clone(), &msg1).await.unwrap();
    dal.save_message(ctx.clone(), &msg2).await.unwrap();
    dal.save_message(ctx.clone(), &msg3).await.unwrap();

    // 搜索关键词 "Rust"
    let results = dal
        .search(
            ctx.clone(),
            MessageSearch {
                keyword: Some("Rust".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // 应该匹配到 msg1 和 msg2
    assert_eq!(results.len(), 2);
    for msg in &results {
        assert!(msg.po.content.contains("Rust"));
        // 关键词命中，match_type 应为 Keyword
        assert_eq!(
            msg.search_match.as_ref().unwrap().match_type,
            MatchType::Keyword
        );
    }
}

#[sqlx::test]
async fn test_search_fts5_chinese(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    // 创建包含中文的消息
    let msg1 = create_test_message(
        "task-cn",
        "user-1",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "你好世界，这是一个测试消息".to_string(),
    );
    let msg2 = create_test_message(
        "task-cn",
        "user-1",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "Rust 是一门系统级编程语言".to_string(),
    );
    let msg3 = create_test_message(
        "task-cn",
        "user-1",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "今天天气不错".to_string(),
    );
    dal.save_message(ctx.clone(), &msg1).await.unwrap();
    dal.save_message(ctx.clone(), &msg2).await.unwrap();
    dal.save_message(ctx.clone(), &msg3).await.unwrap();

    // 搜索中文关键词 "测试消息"（trigram 分词器需要 ≥3 字符才能形成 trigram）
    let results = dal
        .search(
            ctx.clone(),
            MessageSearch {
                keyword: Some("测试消息".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // trigram 分词器应该能匹配到包含 "测试消息" 的消息
    assert!(!results.is_empty());
    for msg in &results {
        assert!(msg.po.content.contains("测试"));
    }

    // 搜索 "编程语言"
    let results2 = dal
        .search(
            ctx,
            MessageSearch {
                keyword: Some("编程语言".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert!(!results2.is_empty());
    for msg in &results2 {
        assert!(msg.po.content.contains("编程语言"));
    }
}

#[sqlx::test]
async fn test_search_vector_with_explicit_query(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    // 直接用 MessageVectorDao 插入向量索引（绕过 save_message 的向量自动维护）
    let vector_dao = message::vector::new();

    // 创建 3 条消息并保存
    let msg1 = create_test_message(
        "task-vec",
        "user-1",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "Vector search test 1".to_string(),
    );
    let msg2 = create_test_message(
        "task-vec",
        "user-1",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "Vector search test 2".to_string(),
    );
    let msg3 = create_test_message(
        "task-vec",
        "user-1",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "Vector search test 3".to_string(),
    );
    dal.save_message(ctx.clone(), &msg1).await.unwrap();
    dal.save_message(ctx.clone(), &msg2).await.unwrap();
    dal.save_message(ctx.clone(), &msg3).await.unwrap();

    // 手动插入向量索引
    let params1 = create_test_vector_params(msg1.id(), 3);
    let mut params2 = create_test_vector_params(msg2.id(), 3);
    params2.vector = vec![0.9, 0.1, 0.0];
    let mut params3 = create_test_vector_params(msg3.id(), 3);
    params3.vector = vec![0.0, 0.9, 0.1];

    vector_dao
        .upsert_vector(ctx.clone(), msg1.id(), &params1)
        .await
        .unwrap();
    vector_dao
        .upsert_vector(ctx.clone(), msg2.id(), &params2)
        .await
        .unwrap();
    vector_dao
        .upsert_vector(ctx.clone(), msg3.id(), &params3)
        .await
        .unwrap();

    // 用接近 msg1 的 query_vector 搜索（无关键词，纯向量搜索）
    // params1 = [0.0, 1/3, 2/3]，query 用 [0.0, 0.3, 0.6]（同方向，cosine 距离=0）
    let results = dal
        .search(
            ctx,
            MessageSearch {
                query_vector: Some(vec![0.0, 0.3, 0.6]),
                top_k: Some(2),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // 应该返回结果，且 match_type 为 Vector
    assert!(!results.is_empty());
    for msg in &results {
        assert_eq!(
            msg.search_match.as_ref().unwrap().match_type,
            MatchType::Vector
        );
        assert!(msg.search_match.as_ref().unwrap().vector_distance.is_some());
    }
}

#[sqlx::test]
async fn test_search_hybrid_matching(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let vector_dao = message::vector::new();

    // 创建消息
    let msg1 = create_test_message(
        "task-hybrid",
        "user-1",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "Hybrid search Rust test".to_string(),
    );
    let msg2 = create_test_message(
        "task-hybrid",
        "user-1",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "Another Rust message".to_string(),
    );
    let msg3 = create_test_message(
        "task-hybrid",
        "user-1",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "Python is different".to_string(),
    );
    dal.save_message(ctx.clone(), &msg1).await.unwrap();
    dal.save_message(ctx.clone(), &msg2).await.unwrap();
    dal.save_message(ctx.clone(), &msg3).await.unwrap();

    // 为 msg1 和 msg2 插入向量索引
    let params1 = create_test_vector_params(msg1.id(), 3);
    let mut params2 = create_test_vector_params(msg2.id(), 3);
    params2.vector = vec![0.9, 0.1, 0.0];

    vector_dao
        .upsert_vector(ctx.clone(), msg1.id(), &params1)
        .await
        .unwrap();
    vector_dao
        .upsert_vector(ctx.clone(), msg2.id(), &params2)
        .await
        .unwrap();

    // 搜索 "Rust" 关键词 + query_vector（接近 msg1，cosine 距离=0）
    // msg1 应该同时命中关键词和向量 → Hybrid
    // msg2 应该命中关键词但向量距离过大 → Keyword
    let results = dal
        .search(
            ctx,
            MessageSearch {
                keyword: Some("Rust".to_string()),
                query_vector: Some(vec![0.0, 0.3, 0.6]),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // 应该有结果
    assert!(!results.is_empty());

    // 检查是否有 Hybrid 匹配
    let has_hybrid = results.iter().any(|m| {
        m.search_match
            .as_ref()
            .map(|sm| sm.match_type == MatchType::Hybrid)
            .unwrap_or(false)
    });
    assert!(has_hybrid, "应该有 Hybrid 匹配的结果");
}

#[sqlx::test]
async fn test_vector_index_lifecycle(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let vector_dao = message::vector::new();

    // 创建消息并保存
    let msg = create_test_message(
        "task-lifecycle",
        "user-1",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "Vector lifecycle test".to_string(),
    );
    dal.save_message(ctx.clone(), &msg).await.unwrap();

    // 手动插入向量索引
    let params = create_test_vector_params(msg.id(), 3);
    vector_dao
        .upsert_vector(ctx.clone(), msg.id(), &params)
        .await
        .unwrap();

    // 验证向量索引已创建 → 可以搜索到
    // params = [0.0, 1/3, 2/3]，query 用 [0.0, 0.3, 0.6]（同方向，cosine 距离=0）
    let results = dal
        .search(
            ctx.clone(),
            MessageSearch {
                query_vector: Some(vec![0.0, 0.3, 0.6]),
                top_k: Some(10),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let found = results.iter().find(|m| m.id() == msg.id());
    assert!(found.is_some(), "向量索引创建后应该能搜索到消息");

    // 删除消息 → DAL 应自动删除向量索引
    dal.delete_message(ctx.clone(), msg.id()).await.unwrap();

    // 验证向量索引已被删除 → 搜索不到
    let results_after_delete = dal
        .search(
            ctx,
            MessageSearch {
                query_vector: Some(vec![0.0, 0.3, 0.6]),
                top_k: Some(10),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let found_after = results_after_delete.iter().find(|m| m.id() == msg.id());
    assert!(
        found_after.is_none(),
        "消息删除后向量索引也应被删除，搜索不到"
    );
}

#[sqlx::test]
async fn test_search_empty_keyword(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let msg = create_test_message(
        "task-empty",
        "user-1",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "Some content".to_string(),
    );
    dal.save_message(ctx.clone(), &msg).await.unwrap();

    // 空关键词搜索应返回空结果
    let results = dal
        .search(
            ctx,
            MessageSearch {
                keyword: Some("".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert!(results.is_empty());
}

#[sqlx::test]
async fn test_search_with_filters(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    // 创建不同 task 的消息
    let msg1 = create_test_message(
        "task-A",
        "user-1",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "Search filter test Rust".to_string(),
    );
    let msg2 = create_test_message(
        "task-B",
        "user-1",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "Search filter test Rust".to_string(),
    );
    dal.save_message(ctx.clone(), &msg1).await.unwrap();
    dal.save_message(ctx.clone(), &msg2).await.unwrap();

    // 搜索 "Rust" 并过滤 task_id
    let results = dal
        .search(
            ctx,
            MessageSearch {
                keyword: Some("Rust".to_string()),
                filters: MessageQuery {
                    task_id: Some("task-A".to_string()),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].task_id(), Some("task-A"));
}
