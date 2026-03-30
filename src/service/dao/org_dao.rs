use crate::error::AppError;
use crate::models::organization::OrganizationPo;
use rusqlite::Connection;
use std::sync::Arc;

/// Organization DAO 接口
pub trait OrganizationDaoTrait: Send + Sync {
    fn insert(&self, conn: &Connection, org: &OrganizationPo) -> Result<(), AppError>;
    fn find_by_id(&self, conn: &Connection, id: &str) -> Result<Option<OrganizationPo>, AppError>;
    fn find_all(&self, conn: &Connection) -> Result<Vec<OrganizationPo>, AppError>;
    fn update(&self, conn: &Connection, org: &OrganizationPo) -> Result<(), AppError>;
    fn delete(&self, conn: &Connection, id: &str, deleted_by: &str) -> Result<(), AppError>;
}

/// Organization DAO 私有实现
struct OrganizationDaoImpl;

impl OrganizationDaoImpl {
    fn new() -> Self { Self }
}

impl OrganizationDaoTrait for OrganizationDaoImpl {
    fn insert(&self, conn: &Connection, org: &OrganizationPo) -> Result<(), AppError> {
        conn.execute(
            "INSERT INTO organizations (id, name, description, status, created_by, modified_by, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![org.id, org.name, org.description, org.status, org.created_by, org.modified_by, org.created_at, org.updated_at],
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    fn find_by_id(&self, conn: &Connection, id: &str) -> Result<Option<OrganizationPo>, AppError> {
        let mut stmt = conn.prepare("SELECT id, name, description, status, created_by, modified_by, created_at, updated_at FROM organizations WHERE id = ?1 AND status != 0")
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
            Ok(org) => Ok(Some(org)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Internal(e.to_string())),
        }
    }

    fn find_all(&self, conn: &Connection) -> Result<Vec<OrganizationPo>, AppError> {
        let mut stmt = conn.prepare("SELECT id, name, description, status, created_by, modified_by, created_at, updated_at FROM organizations WHERE status != 0 ORDER BY id DESC")
            .map_err(|e| AppError::Internal(e.to_string()))?;
        
        let orgs = stmt.query_map([], |row| {
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

    fn update(&self, conn: &Connection, org: &OrganizationPo) -> Result<(), AppError> {
        conn.execute(
            "UPDATE organizations SET name = ?1, description = ?2, modified_by = ?3, updated_at = ?4 WHERE id = ?5",
            rusqlite::params![org.name, org.description, org.modified_by, current_timestamp(), org.id],
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    fn delete(&self, conn: &Connection, id: &str, deleted_by: &str) -> Result<(), AppError> {
        conn.execute(
            "UPDATE organizations SET status = 0, modified_by = ?1, updated_at = ?2 WHERE id = ?3 AND status != 0",
            rusqlite::params![deleted_by, current_timestamp(), id],
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }
}

// ==================== 单例 ====================
use std::sync::OnceLock;

static ORG_DAO: OnceLock<Arc<dyn OrganizationDaoTrait>> = OnceLock::new();

pub fn get_org_dao() -> Arc<dyn OrganizationDaoTrait> {
    ORG_DAO.get().unwrap().clone()
}

pub(super) fn init() {
    let _ = ORG_DAO.set(Arc::new(OrganizationDaoImpl::new()));
}

fn current_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
