//! ModelProviderDao SQLite 实现

use crate::models::model_provider::ModelProviderPo;
use crate::pkg::RequestContext;
use crate::service::dao::model_provider::{ModelProviderDao, ModelProviderQuery};
use chrono::Utc;
use common::api::PagedResult;
use common::enums::{ModelCapability, ModelProviderStatus};
use common::error::Result;
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
    async fn insert(&self, ctx: RequestContext, provider: &ModelProviderPo) -> Result<()> {
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

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<ModelProviderPo>> {
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
    ) -> Result<PagedResult<ModelProviderPo>> {
        let pool = ctx.db_pool();

        let mut count_builder =
            QueryBuilder::new(r#"SELECT COUNT(*) FROM model_providers WHERE 1=1"#);
        push_query_filters(&mut count_builder, &query);
        let total: i64 = count_builder.build_query_scalar().fetch_one(pool).await?;

        let mut list_builder = QueryBuilder::new(
            r#"
SELECT id, name, provider_type, model_name, capability, api_key, base_url, description, config,
       status, created_by, modified_by, created_at, updated_at
FROM model_providers WHERE 1=1
            "#,
        );
        push_query_filters(&mut list_builder, &query);

        // 排序
        list_builder.push(" ORDER BY created_at DESC");

        // 分页
        if let Some(limit) = query.pagination.limit {
            list_builder.push(" LIMIT ").push_bind(limit as i64);
        } else if query.pagination.offset.is_some() {
            list_builder.push(" LIMIT -1");
        }
        if let Some(offset) = query.pagination.offset {
            list_builder.push(" OFFSET ").push_bind(offset as i64);
        }

        let items = list_builder.build_query_as().fetch_all(pool).await?;

        Ok(PagedResult {
            items,
            total: total as usize,
        })
    }

    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<ModelProviderPo>> {
        let page = self
            .query(
                ctx,
                ModelProviderQuery {
                    exclude_status: Some(ModelProviderStatus::Deleted),
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    async fn update(&self, ctx: RequestContext, provider: &ModelProviderPo) -> Result<()> {
        let current_timestamp = Utc::now().timestamp_millis();
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

    async fn delete(&self, ctx: RequestContext, provider: &ModelProviderPo) -> Result<()> {
        let current_timestamp = Utc::now().timestamp_millis();
        let uid = ctx.caller_id_or_system();
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
        let page = self
            .query(
                ctx,
                ModelProviderQuery {
                    capability: Some(ModelCapability::Embedding),
                    status: Some(ModelProviderStatus::Normal),
                    pagination: common::api::PaginationParams {
                        limit: Some(1),
                        offset: None,
                    },
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items.into_iter().next())
    }

    async fn get_default_agent_provider(
        &self,
        ctx: RequestContext,
    ) -> Result<Option<ModelProviderPo>> {
        let page = self
            .query(
                ctx,
                ModelProviderQuery {
                    capability: Some(ModelCapability::Agent),
                    status: Some(ModelProviderStatus::Normal),
                    pagination: common::api::PaginationParams {
                        limit: Some(1),
                        offset: None,
                    },
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items.into_iter().next())
    }

    async fn find_enabled_embedding_provider(
        &self,
        ctx: RequestContext,
    ) -> Result<Option<ModelProviderPo>> {
        let page = self
            .query(
                ctx,
                ModelProviderQuery {
                    capability: Some(ModelCapability::Embedding),
                    status: Some(ModelProviderStatus::Normal),
                    pagination: common::api::PaginationParams {
                        limit: Some(1),
                        offset: None,
                    },
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items.into_iter().next())
    }
}

/// 推送查询过滤条件到 QueryBuilder（COUNT 和 LIST 查询复用）
fn push_query_filters<'args>(
    builder: &mut QueryBuilder<'args, sqlx::Sqlite>,
    query: &ModelProviderQuery,
) {
    if let Some(provider_type) = query.provider_type {
        builder
            .push(" AND provider_type = ")
            .push_bind(provider_type as i32);
    }
    if let Some(capability) = query.capability {
        builder
            .push(" AND capability = ")
            .push_bind(capability as i32);
    }
    if let Some(status) = query.status {
        builder.push(" AND status = ").push_bind(status as i32);
    }
    if let Some(exclude_status) = query.exclude_status {
        builder
            .push(" AND status != ")
            .push_bind(exclude_status as i32);
    }
}
