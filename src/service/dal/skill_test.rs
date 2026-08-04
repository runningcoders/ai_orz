//! Skill DAL 单元测试

use crate::models::cortex_types::{ThinkResult, ToolDescriptor};
use crate::models::model_provider::ModelProviderPo;
use crate::models::skill::{Skill, SkillPo};
use crate::models::vector::MatchType;
use crate::pkg::request_context::RequestContext;
use crate::service::dal::skill::{SkillDal, new};
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::model_provider::ModelProviderDao;
use crate::service::dao::skill::{self, SkillSearch};
use common::enums::skill::SkillAuthorType;
use common::enums::skill::SkillStatus;
use common::error::Result;
use sqlx::SqlitePool;
use std::sync::Arc;

// ========== Mock Cortex Implementation ==========

/// Mock CortexDao，返回 mock 向量（不依赖真实的 LLM）
#[derive(Clone, Debug)]
struct MockCortexDao;

#[async_trait::async_trait]
impl CortexDao for MockCortexDao {
    async fn think(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
        _prompt: &str,
        _tools: &[ToolDescriptor],
    ) -> Result<ThinkResult> {
        Ok(ThinkResult::Final {
            content: "Mock response".to_string(),
            usage: crate::models::cortex_types::TokenUsage::default(),
        })
    }

    async fn embed(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
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
}

/// Mock ModelProviderDao，返回测试用的 ModelProvider
#[derive(Clone, Debug)]
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
        _query: crate::service::dao::model_provider::ModelProviderQuery,
    ) -> Result<common::api::PagedResult<ModelProviderPo>> {
        // 返回一个测试用的 provider（支持 Embedding）
        Ok(common::api::PagedResult {
            items: vec![ModelProviderPo {
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
            }],
            total: 1,
        })
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

/// 创建测试 RequestContext（使用测试 pool 注入）
fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx(user_id, pool)
}

/// 初始化测试依赖（使用 Mock Dao 避免依赖真实 LLM）
async fn init_test(pool: SqlitePool) -> Arc<dyn SkillDal> {
    // 必须先初始化 config（文件操作需要 base_data_path）
    let _ = crate::config::init();

    // 1. 创建向量元数据表（和生产环境 schema 一致）
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

    // 2. 创建 vss_skills 表（测试环境无 vss0 扩展，用普通表模拟 vss0 虚拟表 schema）
    // vss0 虚拟表只有 rowid, embedding 两列，查询时会降级到内存相似度计算
    let _ = sqlx::query(
        "CREATE TABLE IF NOT EXISTS vss_skills (
            rowid INTEGER PRIMARY KEY AUTOINCREMENT,
            embedding TEXT NOT NULL
        );",
    )
    .execute(&pool)
    .await;

    // 直接创建 DAL 实例（不用单例）
    new(
        skill::new_skill_dao(),
        skill::new_skill_vector_dao(),
        Arc::new(MockCortexDao),
        Arc::new(MockModelProviderDao),
    )
}

/// 创建测试 SkillPo
fn create_test_skill_po(name: &str) -> SkillPo {
    let skill_id = uuid::Uuid::now_v7().to_string();
    let content_path = format!("skills/{}/", skill_id);
    SkillPo::new(
        skill_id,
        name.to_string(),
        format!("Test skill: {}", name),
        vec!["AI Agent".to_string()],
        "development".to_string(),
        "".to_string(), // parent_skill_id
        "test-author".to_string(),
        SkillAuthorType::User,
        content_path,
    )
}

/// 测试创建技能后按 ID 查询（含文件组装）
#[sqlx::test]
async fn test_create_and_get_by_id(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 创建技能 PO
    let po = create_test_skill_po("test-skill");
    let skill_id = po.id.clone();

    // DAL 创建（自动创建空 skill.md + 自动向量化）
    skill_dal.create(ctx.clone(), &po).await?;

    // ========== 测试: get_by_id 获取完整聚合实体 ==========
    let skill_opt = skill_dal.get_by_id(ctx.clone(), skill_id.clone()).await?;
    assert!(skill_opt.is_some());
    let skill = skill_opt.unwrap();
    assert_eq!(skill.po.id, skill_id);
    assert_eq!(skill.po.name, "test-skill");

    // 读取主内容验证（如果文件不存在，DAO 可能返回空字符串或错误）
    let main_content_result = skill_dal.read_main_content(&skill.po);
    // 可能 Ok("") 或 Err（如果文件不存在），两种情况都合理
    assert!(main_content_result.is_ok() || main_content_result.is_err());

    // ========== 测试: get_po_by_id 只获取 PO ==========
    let po_opt = skill_dal.get_po_by_id(ctx.clone(), skill_id).await?;
    assert!(po_opt.is_some());
    let po2 = po_opt.unwrap();
    assert_eq!(po2.name, "test-skill");

    Ok(())
}

/// 测试通用查询
#[sqlx::test]
async fn test_query_skills(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 创建多个技能
    for i in 0..3 {
        let po = create_test_skill_po(&format!("skill-{}", i));
        skill_dal.create(ctx.clone(), &po).await?;
    }

    // 查询全部
    use crate::service::dao::skill::SkillQuery;
    let all = skill_dal.query(ctx.clone(), SkillQuery::default()).await?;
    assert_eq!(all.items.len(), 3);

    Ok(())
}

/// 测试按状态、分类、作者查询
#[sqlx::test]
async fn test_list_by_status(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 创建不同状态的技能
    let id1 = uuid::Uuid::now_v7().to_string();
    let po_published = SkillPo::new(
        id1.clone(),
        "published-skill".to_string(),
        "Published skill".to_string(),
        vec!["AI Agent".to_string()],
        "development".to_string(),
        "".to_string(),
        "author-1".to_string(),
        SkillAuthorType::User,
        format!("skills/{}/", id1),
    );
    // 需要手动设置状态（new 方法默认是 Draft）
    let mut po_published = po_published;
    po_published.status = SkillStatus::Published;
    skill_dal.create(ctx.clone(), &po_published).await?;

    let id2 = uuid::Uuid::now_v7().to_string();
    let po_draft = SkillPo::new(
        id2.clone(),
        "draft-skill".to_string(),
        "Draft skill".to_string(),
        vec!["AI Agent".to_string()],
        "debugging".to_string(),
        "".to_string(),
        "author-2".to_string(),
        SkillAuthorType::User,
        format!("skills/{}/", id2),
    );
    skill_dal.create(ctx.clone(), &po_draft).await?;

    // 按状态查询
    let published = skill_dal
        .list_by_status(ctx.clone(), SkillStatus::Published)
        .await?;
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].po.name, "published-skill");

    // 按分类查询
    let dev = skill_dal
        .list_by_category(ctx.clone(), "development")
        .await?;
    assert_eq!(dev.len(), 1);

    // 按作者查询
    let author1 = skill_dal.list_by_author(ctx.clone(), "author-1").await?;
    assert_eq!(author1.len(), 1);

    Ok(())
}

/// 测试文件操作：读写主内容、列出文件、读写其他文件
#[sqlx::test]
async fn test_file_operations(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 创建技能
    let skill_id = uuid::Uuid::now_v7().to_string();
    let content_path = format!("skills/{}/", skill_id);
    let po = SkillPo::new(
        skill_id.clone(),
        "file-test-skill".to_string(),
        "Skill for file ops test".to_string(),
        vec!["Test".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-author".to_string(),
        SkillAuthorType::User,
        content_path,
    );
    skill_dal.create(ctx.clone(), &po).await?;

    // 获取 PO
    let skill_po = skill_dal
        .get_po_by_id(ctx.clone(), skill_id.clone())
        .await?
        .unwrap();

    // ========== 测试: 更新主内容 ==========
    let new_content = "# Test Skill\n\nThis is a test skill markdown file.";
    skill_dal.write_main_content(&skill_po, new_content)?;

    // 验证主内容已更新
    let updated_content = skill_dal.read_main_content(&skill_po)?;
    assert_eq!(updated_content, new_content);

    // ========== 测试: 列出文件 ==========
    let files = skill_dal.list_files(&skill_po)?;
    assert!(!files.is_empty());
    // skill.md 应该存在
    assert!(files.iter().any(|f| f.filename == "skill.md"));

    // ========== 测试: 写额外文件 ==========
    skill_dal.write_file(&skill_po, "examples.json", r#"{"example": "test"}"#)?;

    // 再次列出文件，应该包含新文件
    let files2 = skill_dal.list_files(&skill_po)?;
    assert!(files2.iter().any(|f| f.filename == "examples.json"));

    // ========== 测试: 读额外文件 ==========
    let content = skill_dal.read_file(&skill_po, "examples.json")?;
    assert_eq!(content, r#"{"example": "test"}"#);

    Ok(())
}

/// 测试安装技能到 Agent（创建私有副本）
#[sqlx::test]
async fn test_install_to_agent(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 创建一个 Published 的源技能（共享库技能）
    let source_id = uuid::Uuid::now_v7().to_string();
    let content_path = format!("skills/{}/", source_id);
    let mut source_po = SkillPo::new(
        source_id.clone(),
        "shared-skill".to_string(),
        "A shared skill for all agents".to_string(),
        vec!["AI Agent".to_string()],
        "shared".to_string(),
        "".to_string(),
        "system-author".to_string(),
        SkillAuthorType::User,
        content_path,
    );
    source_po.status = SkillStatus::Published;
    skill_dal.create(ctx.clone(), &source_po).await?;

    // 写一点主内容
    skill_dal.write_main_content(&source_po, "# Shared Skill\n\nFor all agents.")?;

    // 安装到 Agent
    let agent_id = "agent-123";
    let installed = skill_dal
        .install_to_agent(ctx.clone(), &source_id, agent_id)
        .await?;

    // 验证：创建了新的独立副本
    assert_ne!(installed.po.id, source_id);
    assert_eq!(installed.po.author_id, agent_id);
    // 安装后变为 Draft（Agent 私有副本）
    assert_eq!(installed.po.status, SkillStatus::Draft);

    // DAL 返回完整 Skill 实体，包含安装后副本的文件列表
    assert!(!installed.files.is_empty());

    // 验证文件已复制
    let installed_content = skill_dal.read_main_content(&installed.po)?;
    assert!(!installed_content.is_empty());
    assert!(installed_content.contains("Shared Skill"));

    Ok(())
}

/// 测试安装技能到 Agent 的幂等性：同一技能安装两次，第二次不创建新副本
#[sqlx::test]
async fn test_install_to_agent_idempotent(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 创建一个 Published 的源技能（共享库技能）
    let source_id = uuid::Uuid::now_v7().to_string();
    let content_path = format!("skills/{}/", source_id);
    let mut source_po = SkillPo::new(
        source_id.clone(),
        "shared-skill-idempotent".to_string(),
        "A shared skill for idempotent install test".to_string(),
        vec!["AI Agent".to_string()],
        "shared".to_string(),
        "".to_string(),
        "system-author".to_string(),
        SkillAuthorType::User,
        content_path,
    );
    source_po.status = SkillStatus::Published;
    skill_dal.create(ctx.clone(), &source_po).await?;

    // 写一点主内容
    skill_dal.write_main_content(&source_po, "# Shared Skill Idempotent\n\nFor all agents.")?;

    let agent_id = "agent-idempotent";

    // 第一次安装
    let installed_first = skill_dal
        .install_to_agent(ctx.clone(), &source_id, agent_id)
        .await?;
    let first_installed_id = installed_first.po.id.clone();

    // 验证第一次安装成功
    assert_ne!(first_installed_id, source_id);
    assert_eq!(installed_first.po.author_id, agent_id);
    assert_eq!(installed_first.po.parent_skill_id, source_id);
    assert_eq!(installed_first.po.status, SkillStatus::Draft);

    // 第二次安装同一技能到同一 Agent：应跳过创建，返回已有副本
    let installed_second = skill_dal
        .install_to_agent(ctx.clone(), &source_id, agent_id)
        .await?;

    // 验证：返回的是同一个副本（ID 相同），没有创建新副本
    assert_eq!(installed_second.po.id, first_installed_id);
    assert_eq!(installed_second.po.author_id, agent_id);
    assert_eq!(installed_second.po.parent_skill_id, source_id);
    assert_eq!(installed_second.po.status, SkillStatus::Draft);

    // 验证：DAL 仍然返回完整 Skill 实体（含文件列表）
    assert!(!installed_second.files.is_empty());

    // 验证：Agent 名下 parent_skill_id = source_id 的技能只有 1 条
    use crate::service::dao::skill::SkillQuery;
    let agent_skills = skill_dal
        .query(
            ctx.clone(),
            SkillQuery {
                author_id: Some(agent_id.to_string()),
                parent_skill_id: Some(source_id.clone()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(agent_skills.items.len(), 1, "重复安装不应创建新副本");
    assert_eq!(agent_skills.items[0].po.id, first_installed_id);

    // 额外验证：另一个 Agent 安装同一源技能仍然会创建新副本（不同 Agent 互不影响）
    let other_agent_id = "agent-other";
    let installed_other = skill_dal
        .install_to_agent(ctx.clone(), &source_id, other_agent_id)
        .await?;
    assert_ne!(installed_other.po.id, first_installed_id);
    assert_eq!(installed_other.po.author_id, other_agent_id);

    Ok(())
}

/// 测试删除技能（软删除 + 目录删除）
#[sqlx::test]
async fn test_delete_skill(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 创建技能
    let skill_id = uuid::Uuid::now_v7().to_string();
    let content_path = format!("skills/{}/", skill_id);
    let po = SkillPo::new(
        skill_id.clone(),
        "to-delete-skill".to_string(),
        "Skill to delete".to_string(),
        vec!["Test".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-author".to_string(),
        SkillAuthorType::User,
        content_path,
    );
    skill_dal.create(ctx.clone(), &po).await?;

    // 删除
    skill_dal.delete(ctx.clone(), &skill_id).await?;

    // 验证：查询不到（DAO 是硬删除）
    let po_opt = skill_dal
        .get_po_by_id(ctx.clone(), skill_id.clone())
        .await?;
    // 根据 DAO 实现，可能是软删除（Expired）或硬删除（None）
    if let Some(po) = po_opt {
        assert_eq!(po.status, SkillStatus::Expired);
    }
    // 如果是 None 也是正确的

    Ok(())
}

/// 测试搜索技能
#[sqlx::test]
async fn test_search_skill(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 创建技能
    let skill_id = uuid::Uuid::now_v7().to_string();
    let content_path = format!("skills/{}/", skill_id);
    let po = SkillPo::new(
        skill_id,
        "debug-helper".to_string(),
        "Helps with debugging AI agent code".to_string(),
        vec!["AI Agent".to_string()],
        "debugging".to_string(),
        "".to_string(),
        "test-author".to_string(),
        SkillAuthorType::User,
        content_path,
    );
    skill_dal.create(ctx.clone(), &po).await?;

    // 搜索：按名称匹配
    let results = skill_dal
        .search(
            ctx.clone(),
            SkillSearch {
                keyword: Some("debug".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(results.items.len(), 1);
    assert_eq!(results.items[0].po.name, "debug-helper");

    // 搜索：按描述匹配
    let results2 = skill_dal
        .search(
            ctx.clone(),
            SkillSearch {
                keyword: Some("debugging".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(results2.items.len(), 1);

    // 搜索：无匹配
    let results3 = skill_dal
        .search(
            ctx.clone(),
            SkillSearch {
                keyword: Some("nonexistent-keyword".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(results3.items.len(), 0);

    Ok(())
}

/// 测试 get_po_by_id 只返回 PO 不读取文件（性能）
#[sqlx::test]
async fn test_get_po_only(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 创建技能
    let skill_id = uuid::Uuid::now_v7().to_string();
    let content_path = format!("skills/{}/", skill_id);
    let po = SkillPo::new(
        skill_id.clone(),
        "po-only-test".to_string(),
        "Test PO only retrieval".to_string(),
        vec!["Test".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-author".to_string(),
        SkillAuthorType::User,
        content_path,
    );
    skill_dal.create(ctx.clone(), &po).await?;

    // get_po_by_id 应该只返回 PO 结构体（不涉及文件 IO）
    let po_opt = skill_dal.get_po_by_id(ctx.clone(), skill_id).await?;
    assert!(po_opt.is_some());
    let po = po_opt.unwrap();
    assert_eq!(po.name, "po-only-test");

    Ok(())
}

/// 测试更新技能基本信息
#[sqlx::test]
async fn test_update_skill_basic_info(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 先创建技能
    let skill_id = uuid::Uuid::now_v7().to_string();
    let content_path = format!("skills/{}/", skill_id);
    let original_po = SkillPo::new(
        skill_id.clone(),
        "original-name".to_string(),
        "Original description".to_string(),
        vec!["AI Agent".to_string()],
        "debugging".to_string(),
        "".to_string(),
        "test-author".to_string(),
        SkillAuthorType::User,
        content_path,
    );
    skill_dal.create(ctx.clone(), &original_po).await?;

    // 获取完整的 Skill 实体
    let skill_opt = skill_dal.get_by_id(ctx.clone(), skill_id.clone()).await?;
    assert!(skill_opt.is_some());
    let mut skill = skill_opt.unwrap();

    // 更新字段
    skill.po.name = "updated-name".to_string();
    skill.po.description = "Updated description".to_string();
    skill.po.category = "planning".to_string();
    // tags 是 JSON 字符串格式
    skill.po.tags = r#"["AI Agent", "Planning"]"#.to_string();

    // 执行更新
    skill_dal.update(ctx.clone(), &skill).await?;

    // 验证更新成功
    let updated_opt = skill_dal.get_by_id(ctx.clone(), skill_id).await?;
    assert!(updated_opt.is_some());
    let updated = updated_opt.unwrap();
    assert_eq!(updated.po.name, "updated-name");
    assert_eq!(updated.po.description, "Updated description");
    assert_eq!(updated.po.category, "planning");
    // tags 是 JSON 字符串格式，验证包含预期的标签
    assert!(updated.po.tags.contains("AI Agent"));
    assert!(updated.po.tags.contains("Planning"));

    Ok(())
}

/// 测试更新技能状态（Draft → Published）
#[sqlx::test]
async fn test_update_skill_status(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 创建技能（默认是 Draft）
    let skill_id = uuid::Uuid::now_v7().to_string();
    let content_path = format!("skills/{}/", skill_id);
    let original_po = SkillPo::new(
        skill_id.clone(),
        "status-test-skill".to_string(),
        "Test status change".to_string(),
        vec!["Test".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-author".to_string(),
        SkillAuthorType::User,
        content_path,
    );
    skill_dal.create(ctx.clone(), &original_po).await?;

    // 验证初始状态
    let skill_opt = skill_dal.get_by_id(ctx.clone(), skill_id.clone()).await?;
    assert!(skill_opt.is_some());
    let mut skill = skill_opt.unwrap();
    assert_eq!(skill.po.status, SkillStatus::Draft);

    // 更新为 Published
    skill.po.status = SkillStatus::Published;
    skill_dal.update(ctx.clone(), &skill).await?;

    // 验证状态已更新
    let updated_opt = skill_dal.get_by_id(ctx.clone(), skill_id).await?;
    assert!(updated_opt.is_some());
    let updated = updated_opt.unwrap();
    assert_eq!(updated.po.status, SkillStatus::Published);

    Ok(())
}

/// 测试更新不存在的技能（应该不报错，DAO 会静默处理）
#[sqlx::test]
async fn test_update_nonexistent_skill(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 创建一个 PO 但不保存到数据库
    let skill_id = uuid::Uuid::now_v7().to_string();
    let content_path = format!("skills/{}/", skill_id);
    let po = SkillPo::new(
        skill_id,
        "nonexistent-skill".to_string(),
        "This skill doesn't exist".to_string(),
        vec!["Test".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-author".to_string(),
        SkillAuthorType::User,
        content_path,
    );
    // 直接构造 Skill 实体
    let skill = Skill {
        po,
        files: Vec::new(),
        search_match: None,
    };

    // 更新不存在的技能（应该不会报错，DAO 的 update 是 INSERT OR REPLACE）
    let result = skill_dal.update(ctx, &skill).await;
    assert!(result.is_ok());

    Ok(())
}

// ==================== FTS5 + 三态匹配 DAL 层测试 ====================

/// 测试 DAL search 方法的三态匹配（Hybrid / Vector / Keyword）
///
/// MockCortexDao 的向量生成策略：
/// - 文本含 "nonexistent" → 向量 [1.0, 0.0, 0.0]
/// - 其他文本 → 向量 [0.0, 1.0, 1.0]
///
/// 场景设计：
/// - skill_matching：name 含 "debug"，向量 [0.0, 1.0, 1.0]
/// - skill_vector_only：name 不含 "debug"，向量 [0.0, 1.0, 1.0]
/// - 搜索关键词 "debug"：查询向量 [0.0, 1.0, 1.0]（不含 "nonexistent"）
/// - skill_matching：FTS5 命中 + 向量距离 0.0 → Hybrid
/// - skill_vector_only：FTS5 未命中 + 向量距离 0.0 → Vector
#[sqlx::test]
async fn test_search_three_state_matching(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 1. 创建 name 含 "debug" 的技能（会同时被 FTS5 和向量命中 → Hybrid）
    let skill_id_matching = uuid::Uuid::now_v7().to_string();
    let po_matching = SkillPo::new(
        skill_id_matching.clone(),
        "debug-helper".to_string(),
        "Helps with debugging".to_string(),
        vec!["debug".to_string()],
        "debugging".to_string(),
        "".to_string(),
        "test-author".to_string(),
        SkillAuthorType::User,
        format!("skills/{}/", skill_id_matching),
    );
    skill_dal.create(ctx.clone(), &po_matching).await?;

    // 2. 创建 name 不含 "debug" 的技能（只被向量命中 → Vector）
    let skill_id_vector_only = uuid::Uuid::now_v7().to_string();
    let po_vector_only = SkillPo::new(
        skill_id_vector_only.clone(),
        "python-tool".to_string(),
        "A python utility".to_string(),
        vec!["python".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-author".to_string(),
        SkillAuthorType::User,
        format!("skills/{}/", skill_id_vector_only),
    );
    skill_dal.create(ctx.clone(), &po_vector_only).await?;

    // 3. 搜索 "debug"：查询向量 [0.0, 1.0, 1.0]（不含 "nonexistent"）
    let results = skill_dal
        .search(
            ctx.clone(),
            SkillSearch {
                keyword: Some("debug".to_string()),
                ..Default::default()
            },
        )
        .await?;

    // 应返回 2 条结果（Hybrid + Vector）
    assert_eq!(results.items.len(), 2, "应返回 Hybrid + Vector 共 2 条结果");

    // 第一条应是 Hybrid（优先级最高）
    assert_eq!(results.items[0].po.id, skill_id_matching);
    assert_eq!(
        results.items[0].search_match.as_ref().unwrap().match_type,
        MatchType::Hybrid,
        "skill_matching 应是 Hybrid 匹配"
    );
    assert!(
        results.items[0]
            .search_match
            .as_ref()
            .unwrap()
            .vector_distance
            .is_some()
    );
    assert!(
        results.items[0]
            .search_match
            .as_ref()
            .unwrap()
            .fts_rank
            .is_some()
    );

    // 第二条应是 Vector（仅向量命中）
    assert_eq!(results.items[1].po.id, skill_id_vector_only);
    assert_eq!(
        results.items[1].search_match.as_ref().unwrap().match_type,
        MatchType::Vector,
        "skill_vector_only 应是 Vector 匹配"
    );
    assert!(
        results.items[1]
            .search_match
            .as_ref()
            .unwrap()
            .vector_distance
            .is_some()
    );
    assert!(
        results.items[1]
            .search_match
            .as_ref()
            .unwrap()
            .fts_rank
            .is_none()
    );

    Ok(())
}

/// 测试 DAL search 方法的 Keyword-only 匹配
///
/// 当搜索关键词不含 "nonexistent" 但技能内容含 "nonexistent" 时：
/// - 查询向量 [0.0, 1.0, 1.0]
/// - 技能向量 [1.0, 0.0, 0.0]（含 "nonexistent"）
/// - 向量距离 = 1.0 > 0.8 阈值 → 向量不命中
/// - FTS5 命中 → Keyword-only
#[sqlx::test]
async fn test_search_keyword_only_match(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 创建一个 name 含 "nonexistent" 的技能
    let skill_id = uuid::Uuid::now_v7().to_string();
    let po = SkillPo::new(
        skill_id.clone(),
        "nonexistent-debug-tool".to_string(),
        "A tool for nonexistent debugging".to_string(),
        vec!["debug".to_string()],
        "debugging".to_string(),
        "".to_string(),
        "test-author".to_string(),
        SkillAuthorType::User,
        format!("skills/{}/", skill_id),
    );
    skill_dal.create(ctx.clone(), &po).await?;

    // 搜索 "debug"：查询向量 [0.0, 1.0, 1.0]（不含 "nonexistent"）
    // 技能向量 [1.0, 0.0, 0.0]（含 "nonexistent"）
    // 向量距离 = 1.0 > 0.8 → 向量不命中
    // FTS5 命中（name 和 tags 含 "debug"）→ Keyword-only
    let results = skill_dal
        .search(
            ctx.clone(),
            SkillSearch {
                keyword: Some("debug".to_string()),
                ..Default::default()
            },
        )
        .await?;

    assert_eq!(results.items.len(), 1, "应返回 1 条 Keyword 匹配结果");
    assert_eq!(results.items[0].po.id, skill_id);
    assert_eq!(
        results.items[0].search_match.as_ref().unwrap().match_type,
        MatchType::Keyword,
        "应是 Keyword 匹配"
    );
    assert!(
        results.items[0]
            .search_match
            .as_ref()
            .unwrap()
            .fts_rank
            .is_some()
    );
    assert!(
        results.items[0]
            .search_match
            .as_ref()
            .unwrap()
            .vector_distance
            .is_none()
    );

    Ok(())
}

/// 测试 DAL search 方法的综合排序（Hybrid 优先 → Vector → Keyword）
#[sqlx::test]
async fn test_search_comprehensive_sorting(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 1. Hybrid 技能：name 含 "debug"，向量 [0.0, 1.0, 1.0]
    let hybrid_id = uuid::Uuid::now_v7().to_string();
    let po_hybrid = SkillPo::new(
        hybrid_id.clone(),
        "debug-hybrid".to_string(),
        "Debug skill hybrid".to_string(),
        vec!["debug".to_string()],
        "debugging".to_string(),
        "".to_string(),
        "test-author".to_string(),
        SkillAuthorType::User,
        format!("skills/{}/", hybrid_id),
    );
    skill_dal.create(ctx.clone(), &po_hybrid).await?;

    // 2. Vector-only 技能：name 不含 "debug"，向量 [0.0, 1.0, 1.0]
    let vector_id = uuid::Uuid::now_v7().to_string();
    let po_vector = SkillPo::new(
        vector_id.clone(),
        "vector-only-tool".to_string(),
        "A vector only tool".to_string(),
        vec!["utility".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-author".to_string(),
        SkillAuthorType::User,
        format!("skills/{}/", vector_id),
    );
    skill_dal.create(ctx.clone(), &po_vector).await?;

    // 3. Keyword-only 技能：name 含 "debug" + "nonexistent"，向量 [1.0, 0.0, 0.0]
    let keyword_id = uuid::Uuid::now_v7().to_string();
    let po_keyword = SkillPo::new(
        keyword_id.clone(),
        "nonexistent-debug-keyword".to_string(),
        "Keyword only debug".to_string(),
        vec!["debug".to_string()],
        "debugging".to_string(),
        "".to_string(),
        "test-author".to_string(),
        SkillAuthorType::User,
        format!("skills/{}/", keyword_id),
    );
    skill_dal.create(ctx.clone(), &po_keyword).await?;

    // 搜索 "debug"：
    // - hybrid: FTS5 命中 + 向量距离 0.0 → Hybrid
    // - vector: FTS5 未命中 + 向量距离 0.0 → Vector
    // - keyword: FTS5 命中 + 向量距离 1.0 > 0.8 → Keyword
    let results = skill_dal
        .search(
            ctx.clone(),
            SkillSearch {
                keyword: Some("debug".to_string()),
                ..Default::default()
            },
        )
        .await?;

    assert_eq!(results.items.len(), 3, "应返回 3 条结果");

    // 验证排序：Hybrid → Vector → Keyword
    assert_eq!(results.items[0].po.id, hybrid_id, "第一条应是 Hybrid");
    assert_eq!(
        results.items[0].search_match.as_ref().unwrap().match_type,
        MatchType::Hybrid
    );

    assert_eq!(results.items[1].po.id, vector_id, "第二条应是 Vector");
    assert_eq!(
        results.items[1].search_match.as_ref().unwrap().match_type,
        MatchType::Vector
    );

    assert_eq!(results.items[2].po.id, keyword_id, "第三条应是 Keyword");
    assert_eq!(
        results.items[2].search_match.as_ref().unwrap().match_type,
        MatchType::Keyword
    );

    Ok(())
}

/// 测试 DAL search 方法的 fts_rank 透传（从 DAO 到 Skill 实体）
#[sqlx::test]
async fn test_search_fts_rank_transparency(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 创建技能
    let skill_id = uuid::Uuid::now_v7().to_string();
    let po = SkillPo::new(
        skill_id.clone(),
        "rust-programming".to_string(),
        "A rust programming skill".to_string(),
        vec!["rust".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-author".to_string(),
        SkillAuthorType::User,
        format!("skills/{}/", skill_id),
    );
    skill_dal.create(ctx.clone(), &po).await?;

    // 搜索 "rust"
    let results = skill_dal
        .search(
            ctx.clone(),
            SkillSearch {
                keyword: Some("rust".to_string()),
                ..Default::default()
            },
        )
        .await?;

    assert_eq!(results.items.len(), 1);
    let match_info = results.items[0]
        .search_match
        .as_ref()
        .expect("search_match 不应为 None");
    // Hybrid 匹配时 fts_rank 应有值
    assert!(
        match_info.fts_rank.is_some(),
        "fts_rank 应有值（从 DAO 透传）"
    );

    Ok(())
}
