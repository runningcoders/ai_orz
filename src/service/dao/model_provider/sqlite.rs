//! ModelProviderDao SQLite 实现

use common::error::{Error, Result};
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::RequestContext;
use crate::service::dao::model_provider::{ModelProviderDao, ModelProviderQuery};
use chrono::Utc;
use common::enums::{ModelCapability, ModelProviderStatus, ProviderType};
use sqlx::QueryBuilder;
use std::sync::{Arc, OnceLock};
// ==================== 单例 ====================

static MODEL_PROVIDER_DAO: OnceLock<Arc<dyn ModelProviderDao>> = OnceLock::new();

/// 获取 ModelProviderDao 单例
pub fn dao() -> Arc<dyn ModelProviderDao> {
    MODEL_PROVIDER_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = MODEL_PROVIDER_DAO.set(Arc::new(ModelProviderDaoSqliteImpl::new()));
}

// ==================== 实现 ====================

struct ModelProviderDaoSqliteImpl;

impl ModelProviderDaoSqliteImpl {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl ModelProviderDao for ModelProviderDaoSqliteImpl {
    async fn insert(
        &self,
        ctx: RequestContext,
        provider: &ModelProviderPo,
    ) -> Result<()> {
        let provider_type = provider.provider_type as i32;
        let capability = provider.capability as i32;
        let status = provider.status as i32;
        let pool = ctx.db_pool();
        sqlx::query!(
            "INSERT INTO model_providers (id, name, provider_type, model_name, capability, api_key, base_url, description, config, status, created_by, modified_by, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            provider.id,
            provider.name,
            provider_type,
            provider.model_name,
            capability,
            provider.api_key,
            provider.base_url,
            provider.description,
            provider.config,
            status,
            provider.created_by,
            provider.modified_by,
            provider.created_at,
            provider.updated_at
        )
            .execute(pool)
            .await?;

        Ok(())
    }

    async fn find_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<ModelProviderPo>> {
        let pool = ctx.db_pool();
        let provider = QueryBuilder::new(
            r#"
SELECT id, name, provider_type, model_name, capability, api_key, base_url, description, config,
       status, created_by, modified_by, created_at, updated_at
FROM model_providers WHERE id = 
        "#,
        )
        .push_bind(id)
        .push(" AND status != 0")
        .build_query_as()
        .fetch_optional(pool)
        .await?;

        Ok(provider)
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: ModelProviderQuery,
    ) -> Result<Vec<ModelProviderPo>> {
        let pool = ctx.db_pool();
        let mut builder = QueryBuilder::new(
            r#"
SELECT id, name, provider_type, model_name, capability, api_key, base_url, description, config,
       status, created_by, modified_by, created_at, updated_at
FROM model_providers WHERE 1=1
        "#,
        );

        // 枚举查询：直接转 i32 绑定
        if let Some(provider_type) = query.provider_type {
            builder.push(" AND provider_type = ");
            builder.push_bind(provider_type as i32);
        }

        if let Some(capability) = query.capability {
            builder.push(" AND capability = ");
            builder.push_bind(capability as i32);
        }

        if let Some(status) = query.status {
            builder.push(" AND status = ");
            builder.push_bind(status as i32);
        }

        if let Some(exclude_status) = query.exclude_status {
            builder.push(" AND status != ");
            builder.push_bind(exclude_status as i32);
        }

        if let Some(limit) = query.limit {
            builder.push(" LIMIT ");
            builder.push_bind(limit as i64);
        }

        let providers: Vec<ModelProviderPo> = builder.build_query_as().fetch_all(pool).await?;

        Ok(providers)
    }

    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<ModelProviderPo>> {
        self.query(
            ctx,
            ModelProviderQuery {
                exclude_status: Some(ModelProviderStatus::Deleted),
                ..Default::default()
            },
        )
        .await
    }

    async fn update(
        &self,
        ctx: RequestContext,
        provider: &ModelProviderPo,
    ) -> Result<()> {
        let current_timestamp = Utc::now().timestamp();
        let provider_type = provider.provider_type as i32;
        let capability = provider.capability as i32;
        let status = provider.status as i32;
        let pool = ctx.db_pool();
        sqlx::query!(
            r#"
UPDATE model_providers
SET name = ?, provider_type = ?, model_name = ?, capability = ?, api_key = ?, base_url = ?, description = ?, config = ?,
    status = ?, modified_by = ?, updated_at = ?
WHERE id = ?
            "#,
            provider.name,
            provider_type,
            provider.model_name,
            capability,
            provider.api_key,
            provider.base_url,
            provider.description,
            provider.config,
            status,
            provider.modified_by,
            current_timestamp,
            provider.id
        )
            .execute(pool)
            .await?;
        Ok(())
    }

    async fn delete(
        &self,
        ctx: RequestContext,
        provider: &ModelProviderPo,
    ) -> Result<()> {
        let current_timestamp = Utc::now().timestamp();
        let uid = ctx.uid().to_string();
        let pool = ctx.db_pool();
        sqlx::query!(
            r#"
UPDATE model_providers SET status = 0, modified_by = ?, updated_at = ? WHERE id = ?
            "#,
            uid,
            current_timestamp,
            provider.id
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    async fn get_default_embedding_provider(
        &self,
        ctx: RequestContext,
    ) -> Result<Option<ModelProviderPo>> {
        let providers = self
            .query(
                ctx,
                ModelProviderQuery {
                    capability: Some(ModelCapability::Embedding),
                    status: Some(ModelProviderStatus::Normal),
                    limit: Some(1),
                    ..Default::default()
                },
            )
            .await?;
        Ok(providers.into_iter().next())
    }
}

fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
use common::error::Result;
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
