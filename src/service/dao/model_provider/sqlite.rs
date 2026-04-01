//! ModelProvider DAO SQLite 实现

use std::sync::Arc;
use rusqlite::{params, Connection, Result};
use crate::error::AppError;
use crate::models::model_provider::{ModelProviderPo, ProviderType};
use crate::pkg::{RequestContext, constants::ModelProviderStatus};
use super::*;

/// ModelProvider DAO SQLite 实现
pub struct ModelProviderDao {
    conn: Arc<Connection>,
}

impl ModelProviderDao {
    pub fn new(conn: Arc<Connection>) -> Self {
        Self { conn }
    }
}

impl ModelProviderDaoTrait for ModelProviderDao {
    fn insert(&self, _ctx: RequestContext, provider: &ModelProviderPo) -> Result<(), AppError> {
        let sql = r#"
            INSERT INTO model_providers (
                id, name, provider_type, model_name, api_key, base_url, description,
                status, created_by, modified_by, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#;

        let provider_type = serde_json::to_string(&provider.provider_type)?;

        self.conn.execute(sql, params![
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
            provider.created_at,
            provider.updated_at,
        ])?;

        Ok(())
    }

    fn find_by_id(&self, _ctx: RequestContext, id: &str) -> Result<Option<ModelProviderPo>, AppError> {
        let sql = r#"
            SELECT id, name, provider_type, model_name, api_key, base_url, description,
                   status, created_by, modified_by, created_at, updated_at
            FROM model_providers
            WHERE id = ? AND status = 1
        "#;

        let mut stmt = self.conn.prepare(sql)?;
        let mut rows = stmt.query(params![id])?;

        if let Some(row) = rows.next()? {
            let provider_type_str: String = row.get(2)?;
            let provider_type: ProviderType = serde_json::from_str(&provider_type)
                .unwrap_or_default();

            let po = ModelProviderPo {
                id: row.get(0)?,
                name: row.get(1)?,
                provider_type,
                model_name: row.get(3)?,
                api_key: row.get(4)?,
                base_url: row.get(5)?,
                description: row.get(6)?,
                status: ModelProviderStatus::from(row.get::<_, i32>(7)),
                created_by: row.get(8)?,
                modified_by: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            };

            Ok(Some(po))
        } else {
            Ok(None)
        }
    }

    fn find_all(&self, _ctx: RequestContext) -> Result<Vec<ModelProviderPo>, AppError> {
        let sql = r#"
            SELECT id, name, provider_type, model_name, api_key, base_url, description,
                   status, created_by, modified_by, created_at, updated_at
            FROM model_providers
            WHERE status = 1
            ORDER BY created_at DESC
        "#;

        let mut stmt = self.conn.prepare(sql)?;
        let mut rows = stmt.query(params![])?;

        let mut providers = Vec::new();

        while let Some(row) = rows.next()? {
            let provider_type_str: String = row.get(2)?;
            let provider_type: ProviderType = serde_json::from_str(&provider_type)
                .unwrap_or_default();

            let po = ModelProviderPo {
                id: row.get(0)?,
                name: row.get(1)?,
                provider_type,
                model_name: row.get(3)?,
                api_key: row.get(4)?,
                base_url: row.get(5)?,
                description: row.get(6)?,
                status: ModelProviderStatus::from(row.get::<_, i32>(7)),
                created_by: row.get(8)?,
                modified_by: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            };

            providers.push(po);
        }

        Ok(providers)
    }

    fn update(&self, _ctx: RequestContext, provider: &ModelProviderPo) -> Result<(), AppError> {
        let sql = r#"
            UPDATE model_providers
            SET name = ?, provider_type = ?, model_name = ?, api_key = ?, base_url = ?,
                description = ?, status = ?, modified_by = ?, updated_at = ?
            WHERE id = ?
        "#;

        let provider_type = serde_json::to_string(&provider.provider_type)?;

        self.conn.execute(sql, params![
            provider.name,
            provider_type,
            provider.model_name,
            provider.api_key,
            provider.base_url,
            provider.description,
            provider.status.to_i32(),
            provider.modified_by,
            provider.updated_at,
            provider.id,
        ])?;

        Ok(())
    }

    fn delete(&self, _ctx: RequestContext, id: &str) -> Result<(), AppError> {
        // 软删除
        let sql = r#"
            UPDATE model_providers
            SET status = 0
            WHERE id = ?
        "#;

        self.conn.execute(sql, params![id])?;

        Ok(())
    }
}

/// 初始化数据库表
pub fn init(conn: &Connection) -> Result<()> {
    conn.execute(crate::pkg::storage::SQLITE_CREATE_TABLE_MODEL_PROVIDERS, None)?;
    Ok(())
}

/// 创建 DAO
pub fn dao(conn: Arc<Connection>) -> Box<dyn ModelProviderDaoTrait + Send + Sync> {
    Box::new(ModelProviderDao::new(conn))
}
