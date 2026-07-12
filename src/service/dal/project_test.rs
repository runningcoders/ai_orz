//! Project DAL 单元测试

use crate::models::brain::CortexTrait;
use crate::models::model_provider::ModelProviderPo;
use crate::models::project::Project;
use crate::models::vector::{MatchType, Vectorizable};
use crate::pkg::request_context::RequestContext;
use crate::service::dal::project::{new, ProjectDal};
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::model_provider::ModelProviderDao;
use crate::service::dao::project::{self, ProjectQuery, ProjectSearch};
use ::rig::tool::ToolDyn;
use common::enums::ProjectStatus;
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

// ==================== Mock 实现 ====================

/// Mock Cortex 实现（不依赖真实 LLM）
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
            result.push(mock_embed(text));
        }
        Ok(result)
    }

    fn support_tools(&self) -> bool {
        false
    }
}

/// Mock 向量生成函数
///
/// 设计意图：通过文本内容控制向量，使得测试能精确触发三态匹配
/// - 包含 "different" → [1.0, 0.0, 0.0]（与默认向量距离 = 1.0 > 0.8，不匹配）
/// - 包含 "similar"   → [0.0, 1.0, 0.9]（与默认向量距离 ≈ 0.001 < 0.8，匹配）
/// - 其他             → [0.0, 1.0, 1.0]（默认向量，与自身距离 = 0）
fn mock_embed(text: &str) -> Vec<f32> {
    if text.contains("different") {
        vec![1.0, 0.0, 0.0]
    } else if text.contains("similar") {
        vec![0.0, 1.0, 0.9]
    } else {
        vec![0.0, 1.0, 1.0]
    }
}

/// Mock CortexDao
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
        Ok(mock_embed(text))
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

/// Mock ModelProviderDao，返回支持 Embedding 的测试 Provider
#[derive(Clone, Debug)]
struct MockModelProviderDao;

#[async_trait::async_trait]
impl ModelProviderDao for MockModelProviderDao {
    async fn insert(&self, _ctx: RequestContext, _provider: &ModelProviderPo) -> common::error::Result<()> {
        Ok(())
    }

    async fn find_by_id(&self, _ctx: RequestContext, _id: &str) -> common::error::Result<Option<ModelProviderPo>> {
        Ok(None)
    }

    async fn query(
        &self,
        _ctx: RequestContext,
        _query: crate::service::dao::model_provider::ModelProviderQuery,
    ) -> common::error::Result<Vec<ModelProviderPo>> {
        Ok(vec![])
    }

    async fn find_all(&self, _ctx: RequestContext) -> common::error::Result<Vec<ModelProviderPo>> {
        Ok(vec![])
    }

    async fn update(&self, _ctx: RequestContext, _provider: &ModelProviderPo) -> common::error::Result<()> {
        Ok(())
    }

    async fn delete(&self, _ctx: RequestContext, _provider: &ModelProviderPo) -> common::error::Result<()> {
        Ok(())
    }

    async fn get_default_embedding_provider(&self, _ctx: RequestContext) -> common::error::Result<Option<ModelProviderPo>> {
        Ok(Some(mock_provider()))
    }
}

/// 构造测试用 ModelProviderPo
fn mock_provider() -> ModelProviderPo {
    ModelProviderPo {
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
    }
}

// ==================== 测试环境初始化 ====================

/// 初始化测试环境（使用全局单例，不含 Mock）
async fn init_test_env(pool: SqlitePool) -> (Arc<dyn ProjectDal + Send + Sync>, RequestContext) {
    // 初始化所有依赖的 DAO 单例
    crate::service::dao::project::init();
    crate::service::dao::cortex::init();
    crate::service::dao::model_provider::init();
    crate::service::dal::project::init();
    let dal = crate::service::dal::project::dal();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);
    (dal, ctx)
}

/// 初始化带 Mock 的测试环境（用于搜索和向量索引测试）
///
/// 直接创建 DAL 实例，注入 MockCortexDao 和 MockModelProviderDao，
/// 避免依赖真实 LLM 服务。
async fn init_test_with_mocks(pool: SqlitePool) -> (Arc<dyn ProjectDal + Send + Sync>, RequestContext) {
    // 初始化基础 DAO 单例（ProjectDao + ProjectVectorDao）
    crate::service::dao::project::init();

    // 创建 vector_metadata 表（测试环境可能未迁移）
    let _ = sqlx::query(
        "CREATE TABLE IF NOT EXISTS vector_metadata (
            collection TEXT NOT NULL,
            source_id TEXT NOT NULL,
            content_hash TEXT,
            model TEXT,
            dimensions INTEGER,
            indexed_at INTEGER NOT NULL DEFAULT (unixepoch()),
            expire_at INTEGER,
            PRIMARY KEY (collection, source_id)
        );",
    )
    .execute(&pool)
    .await;

    // 使用 Mock 依赖创建 DAL（不用全局单例）
    let dal = new(
        project::dao(),
        project::vector_dao(),
        project::stats_dao(),
        crate::service::dao::model_provider::stats_dao(),
        Arc::new(MockCortexDao),
        Arc::new(MockModelProviderDao),
    );

    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);
    (dal, ctx)
}

/// 创建测试项目
fn create_test_project(name: &str, root_user_id: &str) -> Project {
    Project::new(
        Uuid::now_v7().to_string(),
        name.to_string(),
        format!("Description for {}", name),
        None,
        None,
        1,
        vec![],
        root_user_id.to_string(),
        None,
        None,
        None,
        None,
        "system".to_string(),
    )
}

#[sqlx::test]
async fn test_create_and_find_by_id(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let root_user_id = Uuid::now_v7().to_string();

    let project = create_test_project("Test Project", &root_user_id);
    let project_id = project.po.id.clone();

    dal.create(ctx.clone(), &project).await.unwrap();
    let found = dal.find_by_id(ctx, &project_id).await.unwrap().unwrap();

    assert_eq!(found.po.id, project_id);
    assert_eq!(found.po.name, "Test Project");
    assert_eq!(found.po.root_user_id, root_user_id);
    assert_eq!(found.po.priority, 1);
    assert_eq!(found.po.status, ProjectStatus::Active);
}

#[sqlx::test]
async fn test_list_by_root_user(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let root_user_id = Uuid::now_v7().to_string();
    let other_root_user_id = Uuid::now_v7().to_string();

    // Create 3 projects for user 1
    for i in 0..3 {
        let project = create_test_project(&format!("Project {}", i), &root_user_id);
        dal.create(ctx.clone(), &project).await.unwrap();
    }

    // Create 1 project for user 2
    let project = create_test_project("Other Project", &other_root_user_id);
    dal.create(ctx.clone(), &project).await.unwrap();

    let projects = dal
        .list_by_root_user(ctx, &root_user_id, None)
        .await
        .unwrap();
    assert_eq!(projects.len(), 3);
}

#[sqlx::test]
async fn test_list_by_root_user_and_status(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let root_user_id = Uuid::now_v7().to_string();

    // Create 2 active projects
    for i in 0..2 {
        let project = create_test_project(&format!("Active Project {}", i), &root_user_id);
        dal.create(ctx.clone(), &project).await.unwrap();
    }

    // Create 1 archived project
    let archived_project = create_test_project("Archived Project", &root_user_id);
    let archived_project_id = archived_project.po.id.clone();
    dal.create(ctx.clone(), &archived_project).await.unwrap();
    dal.archive(ctx.clone(), &archived_project_id, "admin")
        .await
        .unwrap();

    // Query only active projects
    let projects = dal
        .list_by_root_user_and_status(
            ctx.clone(),
            &root_user_id,
            vec![ProjectStatus::Active],
            None,
        )
        .await
        .unwrap();
    assert_eq!(projects.len(), 2);

    // Query archived projects
    let projects = dal
        .list_by_root_user_and_status(ctx, &root_user_id, vec![ProjectStatus::Archived], None)
        .await
        .unwrap();
    assert_eq!(projects.len(), 1);
}

#[sqlx::test]
async fn test_query(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let root_user_id = Uuid::now_v7().to_string();

    for i in 0..3 {
        let project = create_test_project(&format!("Project {}", i), &root_user_id);
        dal.create(ctx.clone(), &project).await.unwrap();
    }

    let query = ProjectQuery {
        root_user_id: Some(root_user_id),
        status_in: Some(vec![ProjectStatus::Active]),
        limit: Some(2),
        ..Default::default()
    };

    let projects = dal.query(ctx, query).await.unwrap();
    assert_eq!(projects.len(), 2);
}

#[sqlx::test]
async fn test_update_project(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let root_user_id = Uuid::now_v7().to_string();

    let mut project = create_test_project("Original Name", &root_user_id);
    let project_id = project.po.id.clone();
    dal.create(ctx.clone(), &project).await.unwrap();

    // Update project
    project.po.name = "Updated Name".to_string();
    project.po.description = "Updated description".to_string();
    project.po.priority = 2;
    dal.update(ctx.clone(), &project).await.unwrap();

    let found = dal.find_by_id(ctx, &project_id).await.unwrap().unwrap();
    assert_eq!(found.po.name, "Updated Name");
    assert_eq!(found.po.description, "Updated description");
    assert_eq!(found.po.priority, 2);
}

#[sqlx::test]
async fn test_update_status_and_archive(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let root_user_id = Uuid::now_v7().to_string();

    let project = create_test_project("Test Project", &root_user_id);
    let project_id = project.po.id.clone();
    dal.create(ctx.clone(), &project).await.unwrap();

    // Update status to InProgress
    dal.update_status(ctx.clone(), &project_id, ProjectStatus::InProgress, "admin")
        .await
        .unwrap();
    let found = dal
        .find_by_id(ctx.clone(), &project_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.po.status, ProjectStatus::InProgress);

    // Archive project
    dal.archive(ctx.clone(), &project_id, "admin")
        .await
        .unwrap();
    let found = dal.find_by_id(ctx, &project_id).await.unwrap().unwrap();
    assert_eq!(found.po.status, ProjectStatus::Archived);
}

#[sqlx::test]
async fn test_count_by_root_user(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let root_user_id = Uuid::now_v7().to_string();

    for i in 0..5 {
        let project = create_test_project(&format!("Project {}", i), &root_user_id);
        dal.create(ctx.clone(), &project).await.unwrap();
    }

    let count = dal.count_by_root_user(ctx, &root_user_id).await.unwrap();
    assert_eq!(count, 5);
}

#[sqlx::test]
async fn test_count_by_root_user_and_status(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let root_user_id = Uuid::now_v7().to_string();

    // Create 3 active projects
    for i in 0..3 {
        let project = create_test_project(&format!("Project {}", i), &root_user_id);
        dal.create(ctx.clone(), &project).await.unwrap();
    }

    // Create 2 archived projects
    for i in 0..2 {
        let mut project = create_test_project(&format!("Archived Project {}", i), &root_user_id);
        let project_id = project.po.id.clone();
        dal.create(ctx.clone(), &project).await.unwrap();
        dal.archive(ctx.clone(), &project_id, "admin")
            .await
            .unwrap();
    }

    let active_count = dal
        .count_by_root_user_and_status(ctx.clone(), &root_user_id, ProjectStatus::Active)
        .await
        .unwrap();
    assert_eq!(active_count, 3);

    let archived_count = dal
        .count_by_root_user_and_status(ctx, &root_user_id, ProjectStatus::Archived)
        .await
        .unwrap();
    assert_eq!(archived_count, 2);
}

// ==================== 搜索测试（使用 Mock） ====================

/// 创建带描述的测试项目（用于搜索测试）
fn create_searchable_project(name: &str, description: &str, root_user_id: &str) -> Project {
    let mut project = create_test_project(name, root_user_id);
    project.po.description = description.to_string();
    project
}

/// 测试关键词搜索：名称匹配的项目应返回 Hybrid（关键词 + 向量双命中）
#[sqlx::test]
async fn test_search_keyword_only(pool: SqlitePool) {
    let (dal, ctx) = init_test_with_mocks(pool).await;
    let root_user_id = Uuid::now_v7().to_string();

    // p1: 名称含 "Alpha" → 关键词匹配 + 默认向量 → Hybrid
    let p1 = create_searchable_project("Alpha Project", "machine learning", &root_user_id);
    // p2: 名称不含 "Alpha"，描述含 "different" → 向量远离查询 → 不匹配
    let p2 = create_searchable_project("Beta Task", "different category", &root_user_id);
    dal.create(ctx.clone(), &p1).await.unwrap();
    dal.create(ctx.clone(), &p2).await.unwrap();

    // 关键词搜索 "Alpha"
    let results = dal
        .search(
            ctx,
            ProjectSearch {
                keyword: Some("Alpha".to_string()),
                filters: ProjectQuery {
                    root_user_id: Some(root_user_id),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // 只应返回 p1（p2 向量距离 > 0.8，不匹配）
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].po.name, "Alpha Project");
    // p1 同时匹配关键词和向量 → Hybrid
    let match_info = results[0].search_match.as_ref().expect("应有匹配信息");
    assert_eq!(match_info.match_type, MatchType::Hybrid);
    assert!(match_info.fts_rank.is_some());
    assert!(match_info.vector_distance.is_some());
}

/// 测试向量搜索：通过 Mock 生成向量，验证向量匹配能找到语义相似的项目
#[sqlx::test]
async fn test_search_vector_match(pool: SqlitePool) {
    let (dal, ctx) = init_test_with_mocks(pool).await;
    let root_user_id = Uuid::now_v7().to_string();

    // 创建一个项目，描述包含 "similar" → 向量为 [0.0, 1.0, 0.9]
    // 搜索关键词 "alpha" → 查询向量为 [0.0, 1.0, 1.0]（默认）
    // 两者距离 ≈ 0.001 < 0.8 → 向量匹配
    // 但项目名不含 "alpha" → 关键词不匹配 → 纯 Vector 命中
    let p1 = create_searchable_project(
        "Beta Similar",
        "a similar project with semantic overlap",
        &root_user_id,
    );
    dal.create(ctx.clone(), &p1).await.unwrap();

    let results = dal
        .search(
            ctx,
            ProjectSearch {
                keyword: Some("alpha".to_string()),
                filters: ProjectQuery {
                    root_user_id: Some(root_user_id),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // 应该通过向量匹配找到 "Beta Similar"
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].po.name, "Beta Similar");
    let match_info = results[0].search_match.as_ref().expect("应有匹配信息");
    assert_eq!(match_info.match_type, MatchType::Vector);
    assert!(match_info.vector_distance.is_some());
    assert!(match_info.vector_distance.unwrap() < 0.8);
}

/// 测试混合搜索三态匹配：Hybrid / Vector / Keyword
#[sqlx::test]
async fn test_search_hybrid_three_states(pool: SqlitePool) {
    let (dal, ctx) = init_test_with_mocks(pool).await;
    let root_user_id = Uuid::now_v7().to_string();

    // 1. Hybrid 命中：名称含 "alpha"（关键词匹配），向量默认 [0.0, 1.0, 1.0]（与查询向量距离=0）
    let p_hybrid = create_searchable_project(
        "Alpha Near",
        "direct keyword and vector match",
        &root_user_id,
    );
    dal.create(ctx.clone(), &p_hybrid).await.unwrap();

    // 2. Vector 仅命中：名称不含 "alpha"（关键词不匹配），但描述含 "similar" → 向量接近
    let p_vector = create_searchable_project(
        "Beta Similar",
        "a similar project for semantic match",
        &root_user_id,
    );
    dal.create(ctx.clone(), &p_vector).await.unwrap();

    // 3. Keyword 仅命中：名称含 "alpha"（关键词匹配），但描述含 "different" → 向量远离
    let p_keyword = create_searchable_project(
        "Alpha Different",
        "keyword match but different vector",
        &root_user_id,
    );
    dal.create(ctx.clone(), &p_keyword).await.unwrap();

    let results = dal
        .search(
            ctx,
            ProjectSearch {
                keyword: Some("alpha".to_string()),
                filters: ProjectQuery {
                    root_user_id: Some(root_user_id),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // 应该返回 3 条结果（Hybrid + Vector + Keyword）
    assert_eq!(results.len(), 3, "应返回 3 条匹配结果");

    // 验证排序：Hybrid → Vector → Keyword
    let types: Vec<MatchType> = results
        .iter()
        .map(|p| p.search_match.as_ref().map(|m| m.match_type).unwrap_or(MatchType::Vector))
        .collect();
    assert_eq!(types[0], MatchType::Hybrid, "首条应为 Hybrid 命中");
    assert_eq!(types[1], MatchType::Vector, "次条应为 Vector 命中");
    assert_eq!(types[2], MatchType::Keyword, "末条应为 Keyword 命中");

    // 验证 Hybrid 命中的项目名
    assert_eq!(results[0].po.name, "Alpha Near");
    // 验证 Vector 命中的项目名
    assert_eq!(results[1].po.name, "Beta Similar");
    // 验证 Keyword 命中的项目名
    assert_eq!(results[2].po.name, "Alpha Different");

    // 验证 Hybrid 同时有 vector_distance 和 fts_rank
    let hybrid_info = results[0].search_match.as_ref().unwrap();
    assert!(hybrid_info.vector_distance.is_some(), "Hybrid 应有 vector_distance");
    assert!(hybrid_info.fts_rank.is_some(), "Hybrid 应有 fts_rank");

    // 验证 Vector 只有 vector_distance，无 fts_rank
    let vector_info = results[1].search_match.as_ref().unwrap();
    assert!(vector_info.vector_distance.is_some(), "Vector 应有 vector_distance");
    assert!(vector_info.fts_rank.is_none(), "Vector 不应有 fts_rank");

    // 验证 Keyword 只有 fts_rank，无 vector_distance
    let keyword_info = results[2].search_match.as_ref().unwrap();
    assert!(keyword_info.fts_rank.is_some(), "Keyword 应有 fts_rank");
    assert!(keyword_info.vector_distance.is_none(), "Keyword 不应有 vector_distance");
}

// ==================== 向量索引自动维护测试 ====================

/// 测试创建项目时自动维护向量索引
#[sqlx::test]
async fn test_vector_index_auto_maintain_on_create(pool: SqlitePool) {
    let (dal, ctx) = init_test_with_mocks(pool).await;
    let root_user_id = Uuid::now_v7().to_string();

    let project = create_searchable_project(
        "Auto Index Project",
        "project for testing auto vector indexing",
        &root_user_id,
    );
    let project_id = project.po.id.clone();

    // 创建项目（应自动生成向量索引）
    dal.create(ctx.clone(), &project).await.unwrap();

    // 通过向量搜索验证索引已建立
    let results = dal
        .search(
            ctx,
            ProjectSearch {
                keyword: Some("index".to_string()),
                filters: ProjectQuery {
                    root_user_id: Some(root_user_id),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // 应能通过向量搜索找到该项目
    let found = results.iter().find(|p| p.po.id == project_id);
    assert!(found.is_some(), "创建后应能通过向量搜索找到项目");
}

/// 测试更新项目时自动维护向量索引（内容变化时重索引）
#[sqlx::test]
async fn test_vector_index_auto_maintain_on_update(pool: SqlitePool) {
    let (dal, ctx) = init_test_with_mocks(pool).await;
    let root_user_id = Uuid::now_v7().to_string();

    // 创建项目（描述不含 "similar"）
    let mut project = create_searchable_project(
        "Update Test Project",
        "initial description without special keywords",
        &root_user_id,
    );
    let project_id = project.po.id.clone();
    dal.create(ctx.clone(), &project).await.unwrap();

    // 更新描述，加入 "similar" 关键词（改变向量内容）
    project.po.description = "updated with similar keyword for vector change".to_string();
    dal.update(ctx.clone(), &project).await.unwrap();

    // 搜索 "alpha"（查询向量为默认 [0.0, 1.0, 1.0]）
    // 更新后项目向量为 [0.0, 1.0, 0.9]（因描述含 "similar"），距离 ≈ 0.001 < 0.8
    let results = dal
        .search(
            ctx,
            ProjectSearch {
                keyword: Some("alpha".to_string()),
                filters: ProjectQuery {
                    root_user_id: Some(root_user_id),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // 更新后应能通过向量匹配找到该项目
    let found = results.iter().find(|p| p.po.id == project_id);
    assert!(
        found.is_some(),
        "更新内容后应通过向量搜索找到项目（索引已重建）"
    );

    // 验证是向量匹配（因为项目名不含 "alpha"）
    if let Some(p) = found {
        let match_info = p.search_match.as_ref().expect("应有匹配信息");
        assert_eq!(
            match_info.match_type,
            MatchType::Vector,
            "应为向量匹配（项目名不含搜索关键词）"
        );
    }
}

/// 测试更新项目时内容未变化则跳过重索引
#[sqlx::test]
async fn test_vector_index_skip_when_unchanged(pool: SqlitePool) {
    let (dal, ctx) = init_test_with_mocks(pool).await;
    let root_user_id = Uuid::now_v7().to_string();

    let mut project = create_searchable_project(
        "Unchanged Project",
        "description stays the same",
        &root_user_id,
    );
    let project_id = project.po.id.clone();
    dal.create(ctx.clone(), &project).await.unwrap();

    // 只更新 priority（不影响向量化内容：name + description + workflow + guidance）
    project.po.priority = 99;
    dal.update(ctx.clone(), &project).await.unwrap();

    // 验证项目 priority 已更新
    let found = dal.find_by_id(ctx.clone(), &project_id).await.unwrap().unwrap();
    assert_eq!(found.po.priority, 99);

    // 向量搜索仍然能找到项目（索引未被破坏）
    let results = dal
        .search(
            ctx,
            ProjectSearch {
                keyword: Some("unchanged".to_string()),
                filters: ProjectQuery {
                    root_user_id: Some(root_user_id),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let found = results.iter().find(|p| p.po.id == project_id);
    assert!(found.is_some(), "内容未变化时索引应保留，仍可搜索到");
}
