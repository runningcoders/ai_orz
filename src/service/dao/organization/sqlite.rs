//! Organization DAO SQLite 实现

use crate::error::AppError;
use crate::models::organization::OrganizationPo;
use crate::pkg::storage;
use common::constants::RequestContext;
use crate::service::dao::organization::OrganizationDaoTrait;
use std::sync::{Arc, OnceLock};

// ==================== 单例管理 ====================

static ORGANIZATION_DAO: OnceLock<Arc<dyn OrganizationDaoTrait>> = OnceLock::new();

/// 获取 Organization DAO 单例
pub fn dao() -> Arc<dyn OrganizationDaoTrait> {
    ORGANIZATION_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = ORGANIZATION_DAO.set(Arc::new(OrganizationDaoImpl::new()));
}

// ==================== 实现 ====================

pub struct OrganizationDaoImpl;

impl OrganizationDaoImpl {
    pub fn new() -> Self {
        Self
    }
}

impl OrganizationDaoTrait for OrganizationDaoImpl {
    fn insert(&self, _ctx: RequestContext, org: &OrganizationPo) -> Result<(), AppError> {
        let conn = storage::get().conn();

        conn.execute(
            "INSERT INTO organizations (id, name, description, base_url, status, created_by, modified_by, created_at, updated_at) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                org.id,
                org.name,
                org.description,
                org.base_url,
                org.status,
                org.created_by,
                org.modified_by,
                org.created_at,
                org.updated_at,
            ],
        )
            .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    fn find_by_id(&self, _ctx: RequestContext, id: &str) -> Result<Option<OrganizationPo>, AppError> {
        let conn = storage::get().conn();

        let mut stmt = conn
            .prepare(
                "SELECT id, name, description, base_url, status, created_by, modified_by, created_at, updated_at 
                 FROM organizations WHERE id = ?1 AND status != 0",
            )
            .map_err(|e| AppError::Internal(e.to_string()))?;

        match stmt.query_row([id], |row| {
            Ok(OrganizationPo {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                base_url: row.get(3)?,
                status: row.get(4)?,
                created_by: row.get(5)?,
                modified_by: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        }) {
            Ok(org) => Ok(Some(org)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Internal(e.to_string())),
        }
    }

    fn find_all(&self, _ctx: RequestContext) -> Result<Vec<OrganizationPo>, AppError> {
        let conn = storage::get().conn();

        let mut stmt = conn
            .prepare(
                "SELECT id, name, description, base_url, status, created_by, modified_by, created_at, updated_at 
                 FROM organizations WHERE status != 0 ORDER BY created_at DESC",
            )
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let orgs = stmt
            .query_map([], |row| {
                Ok(OrganizationPo {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    base_url: row.get(3)?,
                    status: row.get(4)?,
                    created_by: row.get(5)?,
                    modified_by: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            })
            .map_err(|e| AppError::Internal(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(orgs)
    }

    fn update(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<(), AppError> {
        let conn = storage::get().conn();

        conn.execute(
            "UPDATE organizations SET name = ?1, description = ?2, base_url = ?3, modified_by = ?4, updated_at = ?5 WHERE id = ?6",
            rusqlite::params![
                org.name,
                org.description,
                org.base_url,
                ctx.uid(),
                current_timestamp(),
                org.id,
            ],
        )
            .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    fn delete(&self, ctx: RequestContext, id: &str) -> Result<(), AppError> {
        let conn = storage::get().conn();

        conn.execute(
            "UPDATE organizations SET status = 0, modified_by = ?1, updated_at = ?2 WHERE id = ?3 AND status != 0",
            rusqlite::params![ctx.uid(), current_timestamp(), id],
        )
            .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    fn count_all(&self, _ctx: RequestContext) -> Result<u64, AppError> {
        let conn = storage::get().conn();

        let mut stmt = conn
            .prepare("SELECT COUNT(*) FROM organizations WHERE status != 0")
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let count: i64 = stmt
            .query_row([], |row| row.get(0))
            .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(count as u64)
    }
}

fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
