//! ModelProviderDao SQLite 实现

use crate::error::AppError;
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::storage;
use common::constants::{RequestContext, ModelProviderPoStatus, ProviderType};
use crate::service::dao::model_provider::ModelProviderDaoTrait;
use std::sync::{Arc, OnceLock};

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

struct ModelProviderDaoImpl;

impl ModelProviderDaoImpl {
    fn new() -> Self {
        Self
    }
}

impl ModelProviderDaoTrait for ModelProviderDaoImpl {
    fn insert(&self, _ctx: RequestContext, provider: &ModelProviderPo) -> Result<(), AppError> {
        let conn = storage::get().conn();
        let now = current_timestamp();

        let provider_type = serde_json::to_string(&provider.provider_type)
            .map_err(|e| AppError::Internal(e.to_string()))?;

        conn.execute(
            "INSERT INTO model_providers (id, name, provider_type, model_name, api_key, base_url, description, status, created_by, modified_by, created_at, updated_at) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                provider.id,
                provider.name,
                provider_type,
                provider.model_name,
                provider.api_key,
                provider.base_url,
                provider.description,
                provider.status.to_i32(),
                provider.created_by,
                provider.modified_by,
                now,
                now,
            ],
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    fn find_by_id(&self, _ctx: RequestContext, id: &str) -> Result<Option<ModelProviderPo>, AppError> {
        let conn = storage::get().conn();

        let mut stmt = conn
            .prepare(
                "SELECT id, name, provider_type, model_name, api_key, base_url, description, status, created_by, modified_by, created_at, updated_at 
                 FROM model_providers WHERE id = ?1 AND status != 0",
            )
            .map_err(|e| AppError::Internal(e.to_string()))?;

        match stmt.query_row([id], |row| {
            let provider_type_str: String = row.get(2)?;
            let provider_type: ProviderType = serde_json::from_str(&provider_type_str)
                .unwrap_or_default();

            Ok(ModelProviderPo {
                id: row.get(0)?,
                name: row.get(1)?,
                provider_type,
                model_name: row.get(3)?,
                api_key: row.get(4)?,
                base_url: row.get(5)?,
                description: row.get(6)?,
                status: ModelProviderPoStatus::from_i32(row.get::<_, i32>(7)?),
                created_by: row.get(8)?,
                modified_by: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        }) {
            Ok(p) => Ok(Some(p)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Internal(e.to_string())),
        }
    }

    fn find_all(&self, _ctx: RequestContext) -> Result<Vec<ModelProviderPo>, AppError> {
        let conn = storage::get().conn();

        let mut stmt = conn
            .prepare(
                "SELECT id, name, provider_type, model_name, api_key, base_url, description, status, created_by, modified_by, created_at, updated_at 
                 FROM model_providers WHERE status != 0 ORDER BY created_at DESC",
            )
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let providers = stmt
            .query_map([], |row| {
                let provider_type_str: String = row.get(2)?;
                let provider_type: ProviderType = serde_json::from_str(&provider_type_str)
                    .unwrap_or_default();

                Ok(ModelProviderPo {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    provider_type,
                    model_name: row.get(3)?,
                    api_key: row.get(4)?,
                    base_url: row.get(5)?,
                    description: row.get(6)?,
                    status: ModelProviderPoStatus::from_i32(row.get::<_, i32>(7)?),
                    created_by: row.get(8)?,
                    modified_by: row.get(9)?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                })
            })
            .map_err(|e| AppError::Internal(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(providers)
    }

    fn update(&self, ctx: RequestContext, provider: &ModelProviderPo) -> Result<(), AppError> {
        let conn = storage::get().conn();

        let provider_type = serde_json::to_string(&provider.provider_type)
            .map_err(|e| AppError::Internal(e.to_string()))?;

        conn.execute(
            "UPDATE model_providers SET name = ?1, provider_type = ?2, model_name = ?3, api_key = ?4, base_url = ?5, description = ?6, status = ?7, modified_by = ?8, updated_at = ?9 WHERE id = ?10",
            rusqlite::params![
                provider.name,
                provider_type,
                provider.model_name,
                provider.api_key,
                provider.base_url,
                provider.description,
                provider.status.to_i32(),
                ctx.uid(),
                current_timestamp(),
                provider.id,
            ],
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    fn delete(&self, ctx: RequestContext, provider: &ModelProviderPo) -> Result<(), AppError> {
        let conn = storage::get().conn();

        conn.execute(
            "UPDATE model_providers SET status = 0, modified_by = ?1, updated_at = ?2 WHERE id = ?3 AND status != 0",
            rusqlite::params![ctx.uid(), current_timestamp(), provider.id],
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;
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
