//! OrganizationDao SQLite 实现

use crate::error::AppError;
use crate::models::organization::OrganizationPo;
use crate::pkg::storage;
use crate::pkg::RequestContext;
use crate::service::dao::org::OrganizationDaoTrait;
use std::sync::{Arc, OnceLock};

// ==================== 单例 ====================

static ORG_DAO: OnceLock<Arc<dyn OrganizationDaoTrait>> = OnceLock::new();

/// 获取 OrganizationDao 单例
pub fn dao() -> Arc<dyn OrganizationDaoTrait> {
    ORG_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = ORG_DAO.set(Arc::new(OrganizationDaoImpl::new()));
}

// ==================== 实现 ====================

struct OrganizationDaoImpl;

impl OrganizationDaoImpl {
    fn new() -> Self {
        Self
    }
}

impl OrganizationDaoTrait for OrganizationDaoImpl {
    fn insert(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<(), AppError> {
        let conn = storage::get().conn();
        let now = current_timestamp();

        conn.execute(
            "INSERT INTO organizations (id, name, description, status, created_by, modified_by, created_at, updated_at) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                org.id,
                org.name,
                org.description,
                org.status,
                ctx.uid(),  // 创建者
                ctx.uid(),  // 修改者
                now,
                now,
            ],
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    fn find_by_id(
        &self,
        _ctx: RequestContext,
        id: &str,
    ) -> Result<Option<OrganizationPo>, AppError> {
        let conn = storage::get().conn();

        let mut stmt = conn
            .prepare(
                "SELECT id, name, description, status, created_by, modified_by, created_at, updated_at 
                 FROM organizations WHERE id = ?1 AND status != 0",
            )
            .map_err(|e| AppError::Internal(e.to_string()))?;

        match stmt.query_row([id], |row| {
            Ok(OrganizationPo {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                status: row.get(3)?,
                created_by: row.get(4)?,
                modified_by: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        }) {
            Ok(o) => Ok(Some(o)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Internal(e.to_string())),
        }
    }

    fn find_all(&self, _ctx: RequestContext) -> Result<Vec<OrganizationPo>, AppError> {
        let conn = storage::get().conn();

        let mut stmt = conn
            .prepare(
                "SELECT id, name, description, status, created_by, modified_by, created_at, updated_at 
                 FROM organizations WHERE status != 0 ORDER BY id DESC",
            )
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let orgs = stmt
            .query_map([], |row| {
                Ok(OrganizationPo {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    status: row.get(3)?,
                    created_by: row.get(4)?,
                    modified_by: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
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
            "UPDATE organizations SET name = ?1, description = ?2, modified_by = ?3, updated_at = ?4 WHERE id = ?5",
            rusqlite::params![
                org.name,
                org.description,
                ctx.uid(),           // 修改者
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
}

fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
