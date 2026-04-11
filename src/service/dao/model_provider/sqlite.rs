//! ModelProviderDao SQLite 实现

use crate::error::AppError;
use crate::models::model_provider::ModelProviderPo;
use common::enums::{ModelProviderStatus, ProviderType};
use crate::pkg::RequestContext;
use crate::service::dao::model_provider::ModelProviderDaoTrait;
use async_trait::async_trait;
use std::sync::{Arc, OnceLock};
use chrono::Utc;
// ==================== 单例 ====================

static MODEL_PROVIDER_DAO: OnceLock<Arc<dyn ModelProviderDaoTrait>> = OnceLock::new();

/// 获取 ModelProviderDao 单例
pub fn dao() -> Arc<dyn ModelProviderDaoTrait> {
    MODEL_PROVIDER_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = MODEL_PROVIDER_DAO.set(Arc::new(ModelProviderDaoImpl::new()));
}

// ==================== 实现 ====================

pub struct ModelProviderDaoImpl;

impl ModelProviderDaoImpl {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl ModelProviderDaoTrait for ModelProviderDaoImpl {
    async fn insert(&self, ctx: RequestContext, provider: &ModelProviderPo) -> Result<(), AppError> {
        let provider_type = provider.provider_type as i32;
        let status = provider.status as i32;
        let pool = ctx.db_pool();
        sqlx::query!(
            "INSERT INTO model_providers (id, name, provider_type, model_name, api_key, base_url, description, status, created_by, modified_by, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            provider.id,
            provider.name,
            provider_type,
            provider.model_name,
            provider.api_key,
            provider.base_url,
            provider.description,
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

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<ModelProviderPo>, AppError> {
        let pool = ctx.db_pool();
        let provider = sqlx::query_as!(
            ModelProviderPo,
            r#"
SELECT id, name, provider_type as 'provider_type: ProviderType', model_name, api_key, base_url, description,
       status as 'status: ModelProviderStatus', created_by, modified_by, created_at, updated_at
FROM model_providers WHERE id = ?
            "#,
            id
        )
            .fetch_optional(pool)
            .await?;

        Ok(provider)
    }

    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<ModelProviderPo>, AppError> {
        let pool = ctx.db_pool();
        let providers = sqlx::query_as!(
            ModelProviderPo,
            r#"
SELECT id, name, provider_type as 'provider_type: ProviderType', model_name, api_key, base_url, description,
       status as 'status: ModelProviderStatus', created_by, modified_by, created_at, updated_at
FROM model_providers WHERE status != 0
            "#
        )
            .fetch_all(pool)
            .await?;

        Ok(providers)
    }

    async fn update(&self, ctx: RequestContext, provider: &ModelProviderPo) -> Result<(), AppError> {
        let current_timestamp = Utc::now().timestamp();
        let provider_type = provider.provider_type as i32;
        let status = provider.status as i32;
        let pool = ctx.db_pool();
        sqlx::query!(
            r#"
UPDATE model_providers
SET name = ?, provider_type = ?, model_name = ?, api_key = ?, base_url = ?, description = ?,
    status = ?, modified_by = ?, updated_at = ?
WHERE id = ?
            "#,
            provider.name,
            provider_type,
            provider.model_name,
            provider.api_key,
            provider.base_url,
            provider.description,
            status,
            provider.modified_by,
            current_timestamp,
            provider.id
        )
            .execute(pool)
            .await?;

        Ok(())
    }

    async fn delete(&self, ctx: RequestContext, provider: &ModelProviderPo) -> Result<(), AppError> {
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
}

fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
