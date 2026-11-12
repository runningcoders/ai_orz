//! 技能 DAL 测试公共设施：Mock 依赖 + 初始化 + PO 构造。
//!
//! Mock 的意义：技能写路径会顺带打 embedding，单测不能连真实模型服务。
//! `MockCortexDao::embed` 同时充当降级护栏的开关（词里带 `nonexistent` / `embed-fail`）。

use super::{SkillDal, new};
use crate::models::cortex_types::{ThinkResult, ToolDescriptor};
use crate::models::model_provider::ModelProviderPo;
use crate::models::skill::SkillPo;
use crate::pkg::request_context::RequestContext;
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::model_provider::ModelProviderDao;
use crate::service::dao::skill;
use common::enums::skill::{SkillAuthorType, SkillStatus};
use common::error::Result;
use sqlx::SqlitePool;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

// ========== Mock Cortex Implementation ==========

/// 统计 embedding 调用次数的句柄（克隆共享同一计数）
///
/// 「查漏补缺」是否真的省下钱，唯一可观测的判据就是**实际发起了几次 embedding** ——
/// 断言向量 hash / indexed_at 都不行：同文本重算得到同样的 hash，而 `indexed_at`
/// 只有秒级精度，测试跑得比时钟快。
#[derive(Clone, Debug, Default)]
pub(super) struct EmbedCalls(Arc<AtomicUsize>);

impl EmbedCalls {
    fn bump(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }

    /// 已发起的 embedding 次数
    pub(super) fn count(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }

    /// 归零（进入「要观察的动作」前调用）
    pub(super) fn reset(&self) {
        self.0.store(0, Ordering::Relaxed);
    }
}

/// Mock CortexDao，返回 mock 向量（不依赖真实的 LLM）
#[derive(Clone, Debug)]
struct MockCortexDao {
    embed_calls: EmbedCalls,
}

#[async_trait::async_trait]
impl CortexDao for MockCortexDao {
    async fn think(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
        _messages: &[crate::models::cortex_types::ChatMessage],
        _tools: &[ToolDescriptor],
        _options: &crate::models::cortex_types::ModelCallOptions,
    ) -> Result<ThinkResult> {
        Ok(ThinkResult::Final {
            content: "Mock response".to_string(),
            usage: crate::models::cortex_types::TokenUsage::default(),
            reasoning_items: Vec::new(),
        })
    }

    async fn embed(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        self.embed_calls.bump();
        let mut result = Vec::with_capacity(texts.len());
        for text in texts {
            // 专供「查询向量生成失败」降级护栏：关键词里带这个标记就让 embed 报错
            if text.contains("embed-fail") {
                return Err(common::error::Error::internal("mock embedding failure"));
            }
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

    async fn get_default_cerebellum_provider(
        &self,
        _ctx: RequestContext,
    ) -> Result<Option<ModelProviderPo>> {
        Ok(None)
    }

    async fn find_enabled_decision_provider(
        &self,
        _ctx: RequestContext,
    ) -> Result<Option<ModelProviderPo>> {
        Ok(None)
    }
}

/// 创建测试 RequestContext（使用测试 pool 注入）
pub(super) fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx(user_id, pool)
}

/// 初始化测试依赖（使用 Mock Dao 避免依赖真实 LLM）
pub(super) async fn init_test(pool: SqlitePool) -> Arc<dyn SkillDal> {
    init_test_with_embed_calls(pool).await.0
}

/// 同上，额外返回 embedding 调用计数器（用于断言「重算了几条」）
pub(super) async fn init_test_with_embed_calls(
    pool: SqlitePool,
) -> (Arc<dyn SkillDal>, EmbedCalls) {
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
    let embed_calls = EmbedCalls::default();
    let dal = new(
        skill::new_skill_dao(),
        skill::new_skill_vector_dao(),
        Arc::new(MockCortexDao {
            embed_calls: embed_calls.clone(),
        }),
        Arc::new(MockModelProviderDao),
    );
    (dal, embed_calls)
}

/// 创建测试 SkillPo
pub(super) fn create_test_skill_po(name: &str) -> SkillPo {
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

/// 构造测试技能 PO（可指定 author/parent/status/tags）
pub(super) fn make_m2_skill_po(
    id: &str,
    name: &str,
    author_id: &str,
    author_type: SkillAuthorType,
    parent_skill_id: &str,
    tags: Vec<String>,
    status: SkillStatus,
) -> SkillPo {
    let mut po = SkillPo::new(
        id.to_string(),
        name.to_string(),
        format!("M2 test skill: {}", name),
        tags,
        "testing".to_string(),
        parent_skill_id.to_string(),
        author_id.to_string(),
        author_type,
        format!("skills/pending/{}", id),
    );
    po.status = status;
    po
}
