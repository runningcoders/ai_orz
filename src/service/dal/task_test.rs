//! Task DAL 单元测试

use crate::models::brain::CortexTrait;
use crate::models::model_provider::ModelProviderPo;
use crate::models::task::Task;
use crate::models::vector::{MatchType, Vectorizable};
use crate::pkg::request_context::RequestContext;
use crate::pkg::storage::VectorStore;
use crate::service::dal::task::TaskDal;
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::model_provider::ModelProviderDao;
use crate::service::dao::task::{self, TaskQuery, TaskSearch, TaskVectorDao};
use common::enums::{AssigneeType, TaskStatus};
use ::rig::tool::ToolDyn;
use common::error::Result;
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

/// 初始化测试环境
async fn init_test_env(pool: SqlitePool) -> (Arc<dyn TaskDal + Send + Sync>, RequestContext) {
    // 初始化 DAO 单例（cortex 和 model_provider DAO 是 create/update 向量化所需的依赖）
    crate::service::dao::model_provider::init();
    crate::service::dao::cortex::init();
    crate::service::dao::task::init();
    crate::service::dal::task::init();
    let dal = crate::service::dal::task::dal();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);
    (dal, ctx)
}

// ========== Mock Cortex Implementation（搜索测试用） ==========

/// Mock Cortex 实现，用于测试（不依赖真实的 LLM）
#[derive(Clone, Debug)]
struct MockCortex {
    model_name: String,
}

impl MockCortex {
    fn new() -> Self {
        Self {
            model_name: "mock-embedding-v1".to_string(),
        }
    }
}

#[async_trait::async_trait]
impl CortexTrait for MockCortex {
    fn capability(&self) -> common::enums::ModelCapability {
        common::enums::ModelCapability::Embedding
    }

    fn model_provider_id(&self) -> &str {
        "mock-provider"
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    async fn prompt(&self, _prompt: &str) -> anyhow::Result<String> {
        Ok("Mock response".to_string())
    }

    async fn embeddings(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        let mut result = Vec::with_capacity(texts.len());
        for text in texts {
            let vec = if text.contains("nonexistent") {
                vec![1.0, 0.0, 0.0]
            } else {
                vec![0.0, 1.0, 1.0]
            };
            result.push(vec);
        }
        Ok(result)
    }

    fn support_tools(&self) -> bool {
        false
    }
}

/// Mock CortexDao，返回 MockCortex
#[derive(Clone, Debug)]
struct MockCortexDao;

#[async_trait::async_trait]
impl CortexDao for MockCortexDao {
    fn create_cortex_trait(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
        _rig_tools: Vec<Box<dyn ToolDyn>>,
    ) -> anyhow::Result<Box<dyn CortexTrait + Send + Sync>> {
        Ok(Box::new(MockCortex::new()))
    }

    async fn prompt(
        &self,
        _ctx: RequestContext,
        _cortex: &dyn CortexTrait,
        _prompt: &str,
    ) -> anyhow::Result<String> {
        Ok("Mock response".to_string())
    }

    async fn embed_text_raw(
        &self,
        _ctx: RequestContext,
        _cortex: &dyn CortexTrait,
        text: &str,
    ) -> anyhow::Result<Vec<f32>> {
        // 极端化向量差异：让 nonexistent 关键词的向量与其他向量距离 > 0.8
        if text.contains("nonexistent") {
            Ok(vec![1.0, 0.0, 0.0])
        } else {
            Ok(vec![0.0, 1.0, 1.0])
        }
    }

    async fn embed_entity(
        &self,
        ctx: RequestContext,
        cortex: &dyn CortexTrait,
        entity: &dyn Vectorizable,
    ) -> anyhow::Result<crate::models::vector::VectorIndexParams> {
        let content = entity.vectorize_text();
        let embedding = self.embed_text_raw(ctx, cortex, &content).await?;
        Ok(crate::models::vector::VectorIndexParams::new(
            &content,
            embedding,
            "mock-provider".to_string(),
            "mock-embedding-v1".to_string(),
        ))
    }

    async fn embed_text_for_search(
        &self,
        ctx: RequestContext,
        cortex: &dyn CortexTrait,
        text: &str,
    ) -> anyhow::Result<crate::models::vector::VectorIndexParams> {
        let embedding = self.embed_text_raw(ctx, cortex, text).await?;
        Ok(crate::models::vector::VectorIndexParams::new(
            text,
            embedding,
            "mock-provider".to_string(),
            "mock-embedding-v1".to_string(),
        ))
    }
}

/// Mock ModelProviderDao，返回测试用的 ModelProvider
#[derive(Clone, Debug)]
struct MockModelProviderDao;

#[async_trait::async_trait]
impl ModelProviderDao for MockModelProviderDao {
    async fn insert(&self, _ctx: RequestContext, _provider: &ModelProviderPo) -> Result<()> {
        Ok(())
    }

    async fn find_by_id(
        &self,
        _ctx: RequestContext,
        _id: &str,
    ) -> Result<Option<ModelProviderPo>> {
        Ok(None)
    }

    async fn query(
        &self,
        _ctx: RequestContext,
        _query: crate::service::dao::model_provider::ModelProviderQuery,
    ) -> Result<Vec<ModelProviderPo>> {
        Ok(vec![])
    }

    async fn find_all(&self, _ctx: RequestContext) -> Result<Vec<ModelProviderPo>> {
        Ok(vec![])
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
        Ok(Some(ModelProviderPo {
            id: "mock-provider".to_string(),
            name: "Mock Provider".to_string(),
            provider_type: common::enums::ProviderType::Ollama,
            model_name: "mock-embedding".to_string(),
            capability: common::enums::ModelCapability::Embedding,
            api_key: "".to_string(),
            base_url: Some("http://localhost:11434".to_string()),
            description: None,
            config: "{}".to_string(),
            status: common::enums::ModelProviderStatus::Normal,
            created_by: "system".to_string(),
            modified_by: "system".to_string(),
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
        }))
    }

    async fn find_enabled_embedding_provider(
        &self,
        _ctx: RequestContext,
    ) -> Result<Option<ModelProviderPo>> {
        Ok(None)
    }
}

/// 初始化搜索测试环境（注入 MockCortexDao + MockModelProviderDao）
async fn init_search_test_env(
    pool: SqlitePool,
) -> (Arc<dyn TaskDal + Send + Sync>, RequestContext, Arc<dyn TaskVectorDao>) {
    let _ = crate::config::init();

    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);

    let task_dao = task::new();
    let task_vector_dao = task::new_task_vector_dao();
    let task_stats_dao = task::stats_new();
    let cortex_dao: Arc<dyn CortexDao> = Arc::new(MockCortexDao);
    let model_provider_dao: Arc<dyn ModelProviderDao + Send + Sync> = Arc::new(MockModelProviderDao);

    let dal = crate::service::dal::task::new(
        task_dao,
        task_vector_dao.clone(),
        task_stats_dao,
        crate::service::dao::model_provider::stats_new(),
        cortex_dao,
        model_provider_dao,
    );

    (dal, ctx, task_vector_dao)
}

/// 创建测试任务
fn create_test_task(title: &str, assignee_id: &str) -> Task {
    Task::new(
        Uuid::now_v7().to_string(),
        title.to_string(),
        format!("Description for {}", title),
        1,
        vec![],
        None,
        None,
        None,
        vec![],
        Uuid::now_v7().to_string(), // root_user_id
        AssigneeType::User,
        assignee_id.to_string(),
        None,
        "admin".to_string(),
    )
}

#[sqlx::test]
async fn test_create_and_find_by_id(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let assignee_id = Uuid::now_v7().to_string();

    let task = create_test_task("Test Task", &assignee_id);
    let task_id = task.po.id.clone();
    let root_user_id = task.po.root_user_id.clone();

    dal.create(ctx.clone(), &task).await.unwrap();
    let found = dal.find_by_id(ctx, &task_id).await.unwrap().unwrap();

    assert_eq!(found.po.id, task_id);
    assert_eq!(found.po.title, "Test Task");
    assert_eq!(found.po.root_user_id, root_user_id);
    assert_eq!(found.po.assignee_type, AssigneeType::User);
    assert_eq!(found.po.assignee_id, assignee_id);
    assert_eq!(found.po.status, TaskStatus::Pending);
}

#[sqlx::test]
async fn test_list_by_assignee(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let assignee_id = Uuid::now_v7().to_string();
    let other_assignee_id = Uuid::now_v7().to_string();

    // Create 3 tasks for assignee 1
    for i in 0..3 {
        let task = create_test_task(&format!("Task {}", i), &assignee_id);
        dal.create(ctx.clone(), &task).await.unwrap();
    }

    // Create 1 task for assignee 2
    let task = create_test_task("Other Task", &other_assignee_id);
    dal.create(ctx.clone(), &task).await.unwrap();

    let tasks = dal
        .list_by_assignee(ctx, Some(AssigneeType::User), &assignee_id, None)
        .await
        .unwrap();
    assert_eq!(tasks.len(), 3);
}

#[sqlx::test]
async fn test_list_by_status(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let assignee_id = Uuid::now_v7().to_string();

    // Create 2 pending tasks
    for i in 0..2 {
        let task = create_test_task(&format!("Pending Task {}", i), &assignee_id);
        dal.create(ctx.clone(), &task).await.unwrap();
    }

    // Create 1 completed task
    let completed_task = create_test_task("Completed Task", &assignee_id);
    let completed_task_id = completed_task.po.id.clone();
    dal.create(ctx.clone(), &completed_task).await.unwrap();
    dal.update_status(
        ctx.clone(),
        &completed_task_id,
        TaskStatus::Completed,
        "admin",
    )
    .await
    .unwrap();

    // Query only pending tasks
    let tasks = dal
        .list_by_status(
            ctx.clone(),
            Some(AssigneeType::User),
            &assignee_id,
            vec![TaskStatus::Pending],
            None,
        )
        .await
        .unwrap();
    assert_eq!(tasks.len(), 2);

    // Query completed tasks
    let tasks = dal
        .list_by_status(
            ctx,
            Some(AssigneeType::User),
            &assignee_id,
            vec![TaskStatus::Completed],
            None,
        )
        .await
        .unwrap();
    assert_eq!(tasks.len(), 1);
}

#[sqlx::test]
async fn test_query(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let assignee_id = Uuid::now_v7().to_string();

    for i in 0..3 {
        let task = create_test_task(&format!("Task {}", i), &assignee_id);
        dal.create(ctx.clone(), &task).await.unwrap();
    }

    let query = TaskQuery {
        assignee_type: Some(AssigneeType::User),
        assignee_id: Some(assignee_id),
        project_id: None,
        status_in: Some(vec![TaskStatus::Pending]),
        limit: Some(2),
        ..Default::default()
    };

    let tasks = dal.query(ctx, query).await.unwrap();
    assert_eq!(tasks.len(), 2);
}

#[sqlx::test]
async fn test_update_task(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let assignee_id = Uuid::now_v7().to_string();

    let mut task = create_test_task("Original Title", &assignee_id);
    let task_id = task.po.id.clone();
    dal.create(ctx.clone(), &task).await.unwrap();

    // Update task
    task.po.title = "Updated Title".to_string();
    task.po.description = "Updated description".to_string();
    task.po.priority = 2;
    dal.update(ctx.clone(), &task).await.unwrap();

    let found = dal.find_by_id(ctx, &task_id).await.unwrap().unwrap();
    assert_eq!(found.po.title, "Updated Title");
    assert_eq!(found.po.description, "Updated description");
    assert_eq!(found.po.priority, 2);
}

#[sqlx::test]
async fn test_update_status_and_cancel(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let assignee_id = Uuid::now_v7().to_string();

    let task = create_test_task("Test Task", &assignee_id);
    let task_id = task.po.id.clone();
    dal.create(ctx.clone(), &task).await.unwrap();

    // Update status to InProgress
    dal.update_status(ctx.clone(), &task_id, TaskStatus::InProgress, "admin")
        .await
        .unwrap();
    let found = dal
        .find_by_id(ctx.clone(), &task_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.po.status, TaskStatus::InProgress);

    // Cancel task
    dal.cancel(ctx.clone(), &task_id, "admin").await.unwrap();
    // Cancelled tasks are soft-deleted, so find_by_id returns None
    let found = dal.find_by_id(ctx, &task_id).await.unwrap();
    assert!(found.is_none());
}

#[sqlx::test]
async fn test_count_by_assignee(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let assignee_id = Uuid::now_v7().to_string();

    for i in 0..5 {
        let task = create_test_task(&format!("Task {}", i), &assignee_id);
        dal.create(ctx.clone(), &task).await.unwrap();
    }

    let count = dal.count_by_assignee(ctx, &assignee_id).await.unwrap();
    assert_eq!(count, 5);
}

#[sqlx::test]
async fn test_count_by_assignee_and_status(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let assignee_id = Uuid::now_v7().to_string();

    // Create 3 pending tasks
    for i in 0..3 {
        let task = create_test_task(&format!("Task {}", i), &assignee_id);
        dal.create(ctx.clone(), &task).await.unwrap();
    }

    // Create 2 completed tasks
    for i in 0..2 {
        let task = create_test_task(&format!("Completed Task {}", i), &assignee_id);
        let task_id = task.po.id.clone();
        dal.create(ctx.clone(), &task).await.unwrap();
        dal.update_status(ctx.clone(), &task_id, TaskStatus::Completed, "admin")
            .await
            .unwrap();
    }

    let pending_count = dal
        .count_by_assignee_and_status(ctx.clone(), &assignee_id, TaskStatus::Pending)
        .await
        .unwrap();
    assert_eq!(pending_count, 3);

    let completed_count = dal
        .count_by_assignee_and_status(ctx, &assignee_id, TaskStatus::Completed)
        .await
        .unwrap();
    assert_eq!(completed_count, 2);
}

// ==================== 搜索能力测试 ====================

/// 测试 FTS5 关键词搜索（含中文）
///
/// 注意：DAL search() 是混合搜索，keyword 会同时触发 FTS5 和向量搜索。
/// 为隔离 FTS5 行为，所有任务 title 都含 "nonexistent" → 向量 [1.0, 0.0, 0.0]，
/// 搜索关键词不含 "nonexistent" → 查询向量 [0.0, 1.0, 1.0]，
/// 向量距离 = 1.0 > 0.8 阈值 → 向量不命中，只返回 FTS5 结果。
#[sqlx::test]
async fn test_search_fts5(pool: SqlitePool) {
    let (dal, ctx, _) = init_search_test_env(pool).await;
    let assignee_id = Uuid::now_v7().to_string();

    // 创建 3 个任务（title 含 "nonexistent" 使向量与查询向量距离 > 0.8）
    let task1 = create_test_task("nonexistent-debug-helper", &assignee_id);
    let task2 = create_test_task("nonexistent-python-tool", &assignee_id);
    let task3 = create_test_task("nonexistent-测试任务", &assignee_id);
    dal.create(ctx.clone(), &task1).await.unwrap();
    dal.create(ctx.clone(), &task2).await.unwrap();
    dal.create(ctx.clone(), &task3).await.unwrap();

    // 搜索 "debug"：应只匹配 task1（FTS5 命中，向量不命中）
    let results = dal
        .search(
            ctx.clone(),
            TaskSearch {
                keyword: Some("debug".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].po.title, "nonexistent-debug-helper");

    // 搜索 "python"：应只匹配 task2
    let results = dal
        .search(
            ctx.clone(),
            TaskSearch {
                keyword: Some("python".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].po.title, "nonexistent-python-tool");

    // 搜索中文 "测试任务"：应只匹配 task3（trigram 分词器需要 ≥3 字符）
    let results = dal
        .search(
            ctx.clone(),
            TaskSearch {
                keyword: Some("测试任务".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].po.title, "nonexistent-测试任务");

    // 搜索无匹配关键词（不含 "nonexistent" 以避免向量命中）
    let results = dal
        .search(
            ctx,
            TaskSearch {
                keyword: Some("zzz-no-match-xyz".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(results.len(), 0);
}

/// 测试 DAL search 方法的三态匹配（Hybrid / Vector / Keyword）
///
/// MockCortexDao 向量生成策略：
/// - 文本含 "nonexistent" → 向量 [1.0, 0.0, 0.0]
/// - 其他文本 → 向量 [0.0, 1.0, 1.0]
///
/// 场景设计：
/// - task_matching：title 含 "debug"，向量 [0.0, 1.0, 1.0]
/// - task_vector_only：title 不含 "debug"，向量 [0.0, 1.0, 1.0]
/// - 搜索关键词 "debug"：查询向量 [0.0, 1.0, 1.0]（不含 "nonexistent"）
/// - task_matching：FTS5 命中 + 向量距离 0.0 → Hybrid
/// - task_vector_only：FTS5 未命中 + 向量距离 0.0 → Vector
#[sqlx::test]
async fn test_search_three_state_matching(pool: SqlitePool) {
    let (dal, ctx, _) = init_search_test_env(pool).await;
    let assignee_id = Uuid::now_v7().to_string();

    // 1. 创建 title 含 "debug" 的任务（会同时被 FTS5 和向量命中 → Hybrid）
    let task_matching = create_test_task("debug-helper", &assignee_id);
    let matching_id = task_matching.po.id.clone();
    dal.create(ctx.clone(), &task_matching).await.unwrap();

    // 2. 创建 title 不含 "debug" 的任务（只被向量命中 → Vector）
    let task_vector_only = create_test_task("python-utility", &assignee_id);
    let vector_only_id = task_vector_only.po.id.clone();
    dal.create(ctx.clone(), &task_vector_only).await.unwrap();

    // 3. 搜索 "debug"：查询向量 [0.0, 1.0, 1.0]（不含 "nonexistent"）
    let results = dal
        .search(
            ctx.clone(),
            TaskSearch {
                keyword: Some("debug".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // 应返回 2 条结果（Hybrid + Vector）
    assert_eq!(results.len(), 2, "应返回 Hybrid + Vector 共 2 条结果");

    // 第一条应是 Hybrid（优先级最高）
    assert_eq!(results[0].po.id, matching_id);
    assert_eq!(
        results[0].search_match.as_ref().unwrap().match_type,
        MatchType::Hybrid,
        "task_matching 应是 Hybrid 匹配"
    );
    assert!(results[0]
        .search_match
        .as_ref()
        .unwrap()
        .vector_distance
        .is_some());
    assert!(results[0]
        .search_match
        .as_ref()
        .unwrap()
        .fts_rank
        .is_some());

    // 第二条应是 Vector（仅向量命中）
    assert_eq!(results[1].po.id, vector_only_id);
    assert_eq!(
        results[1].search_match.as_ref().unwrap().match_type,
        MatchType::Vector,
        "task_vector_only 应是 Vector 匹配"
    );
    assert!(results[1]
        .search_match
        .as_ref()
        .unwrap()
        .vector_distance
        .is_some());
    assert!(results[1]
        .search_match
        .as_ref()
        .unwrap()
        .fts_rank
        .is_none());
}

/// 测试 DAL search 方法的 Keyword-only 匹配
///
/// 当搜索关键词不含 "nonexistent" 但任务内容含 "nonexistent" 时：
/// - 查询向量 [0.0, 1.0, 1.0]
/// - 任务向量 [1.0, 0.0, 0.0]（含 "nonexistent"）
/// - 向量距离 = 1.0 > 0.8 阈值 → 向量不命中
/// - FTS5 命中 → Keyword-only
#[sqlx::test]
async fn test_search_keyword_only_match(pool: SqlitePool) {
    let (dal, ctx, _) = init_search_test_env(pool).await;
    let assignee_id = Uuid::now_v7().to_string();

    // 创建一个 title 含 "nonexistent" + "debug" 的任务
    let task = create_test_task("nonexistent-debug-tool", &assignee_id);
    let task_id = task.po.id.clone();
    dal.create(ctx.clone(), &task).await.unwrap();

    // 搜索 "debug"：查询向量 [0.0, 1.0, 1.0]（不含 "nonexistent"）
    // 任务向量 [1.0, 0.0, 0.0]（含 "nonexistent"）
    // 向量距离 = 1.0 > 0.8 → 向量不命中
    // FTS5 命中（title 和 description 含 "debug"）→ Keyword-only
    let results = dal
        .search(
            ctx.clone(),
            TaskSearch {
                keyword: Some("debug".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1, "应返回 1 条 Keyword 匹配结果");
    assert_eq!(results[0].po.id, task_id);
    assert_eq!(
        results[0].search_match.as_ref().unwrap().match_type,
        MatchType::Keyword,
        "应是 Keyword 匹配"
    );
    assert!(results[0]
        .search_match
        .as_ref()
        .unwrap()
        .fts_rank
        .is_some());
    assert!(results[0]
        .search_match
        .as_ref()
        .unwrap()
        .vector_distance
        .is_none());
}

/// 测试向量索引自动维护（create / update / cancel）
#[sqlx::test]
async fn test_vector_auto_maintenance(pool: SqlitePool) {
    let (dal, ctx, vector_dao) = init_search_test_env(pool).await;
    let assignee_id = Uuid::now_v7().to_string();

    // 1. Create：创建任务后应自动写入向量索引
    let mut task = create_test_task("vector-test-task", &assignee_id);
    let task_id = task.po.id.clone();
    dal.create(ctx.clone(), &task).await.unwrap();

    let row = vector_dao
        .get_vector_row(ctx.clone(), &task_id)
        .await
        .unwrap();
    assert!(row.is_some(), "创建任务后向量索引应存在");
    let original_hash = row.unwrap().meta.content_hash.clone();

    // 2. Update：更新任务后向量索引应被更新（content_hash 变化）
    task.po.title = "updated-vector-test".to_string();
    task.po.description = "Updated description for vector test".to_string();
    dal.update(ctx.clone(), &task).await.unwrap();

    let row = vector_dao
        .get_vector_row(ctx.clone(), &task_id)
        .await
        .unwrap();
    assert!(row.is_some(), "更新任务后向量索引应仍存在");
    let updated_hash = row.unwrap().meta.content_hash.clone();
    assert_ne!(
        original_hash, updated_hash,
        "更新后 content_hash 应变化（因为 title+description 变了）"
    );

    // 3. Cancel：取消任务后应清理向量索引
    dal.cancel(ctx.clone(), &task_id, "admin").await.unwrap();

    let row = vector_dao
        .get_vector_row(ctx, &task_id)
        .await
        .unwrap();
    assert!(row.is_none(), "取消任务后向量索引应被清理");
}

/// 测试搜索结果的综合排序（Hybrid 优先 → Vector → Keyword）
#[sqlx::test]
async fn test_search_comprehensive_sorting(pool: SqlitePool) {
    let (dal, ctx, _) = init_search_test_env(pool).await;
    let assignee_id = Uuid::now_v7().to_string();

    // 1. Hybrid 任务：title 含 "debug"，向量 [0.0, 1.0, 1.0]
    let hybrid_task = create_test_task("debug-hybrid", &assignee_id);
    let hybrid_id = hybrid_task.po.id.clone();
    dal.create(ctx.clone(), &hybrid_task).await.unwrap();

    // 2. Vector-only 任务：title 不含 "debug"，向量 [0.0, 1.0, 1.0]
    let vector_task = create_test_task("vector-only-item", &assignee_id);
    let vector_id = vector_task.po.id.clone();
    dal.create(ctx.clone(), &vector_task).await.unwrap();

    // 3. Keyword-only 任务：title 含 "debug" + "nonexistent"，向量 [1.0, 0.0, 0.0]
    let keyword_task = create_test_task("nonexistent-debug-keyword", &assignee_id);
    let keyword_id = keyword_task.po.id.clone();
    dal.create(ctx.clone(), &keyword_task).await.unwrap();

    // 搜索 "debug"
    let results = dal
        .search(
            ctx.clone(),
            TaskSearch {
                keyword: Some("debug".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // 应返回 3 条结果，排序为 Hybrid → Vector → Keyword
    assert_eq!(results.len(), 3, "应返回 3 条结果");

    assert_eq!(results[0].po.id, hybrid_id, "第一条应是 Hybrid");
    assert_eq!(
        results[0].search_match.as_ref().unwrap().match_type,
        MatchType::Hybrid
    );

    assert_eq!(results[1].po.id, vector_id, "第二条应是 Vector");
    assert_eq!(
        results[1].search_match.as_ref().unwrap().match_type,
        MatchType::Vector
    );

    assert_eq!(results[2].po.id, keyword_id, "第三条应是 Keyword");
    assert_eq!(
        results[2].search_match.as_ref().unwrap().match_type,
        MatchType::Keyword
    );
}
