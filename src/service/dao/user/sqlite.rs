//! User DAO SQLite 实现

use crate::error::AppError;
use crate::models::user::UserPo;
use crate::pkg::constants::utils;
use crate::pkg::storage;
use crate::pkg::RequestContext;
use crate::service::dao::user::UserDaoTrait;
use std::sync::{Arc, OnceLock};

// ==================== 单例管理 ====================

static USER_DAO: OnceLock<Arc<dyn UserDaoTrait>> = OnceLock::new();

/// 获取 User DAO 单例
pub fn dao() -> Arc<dyn UserDaoTrait> {
    USER_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = USER_DAO.set(Arc::new(UserDaoImpl::new()));
}

// ==================== 实现 ====================

pub struct UserDaoImpl;

impl UserDaoImpl {
    pub fn new() -> Self {
        Self
    }
}

impl UserDaoTrait for UserDaoImpl {
    fn insert(&self, _ctx: RequestContext, user: &UserPo) -> Result<(), AppError> {
        let conn = storage::get().conn();

        conn.execute(
            "INSERT INTO users (id, organization_id, username, display_name, email, password_hash, role, status, created_by, modified_by, created_at, updated_at) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                user.id,
                user.organization_id,
                user.username,
                user.display_name,
                user.email,
                user.password_hash,
                user.role,
                user.status,
                user.created_by,
                user.modified_by,
                user.created_at,
                user.updated_at,
            ],
        )
            .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    fn find_by_id(&self, _ctx: RequestContext, id: &str) -> Result<Option<UserPo>, AppError> {
        let conn = storage::get().conn();

        let mut stmt = conn
            .prepare(
                "SELECT id, organization_id, username, display_name, email, password_hash, role, status, created_by, modified_by, created_at, updated_at 
                 FROM users WHERE id = ?1 AND status != 0",
            )
            .map_err(|e| AppError::Internal(e.to_string()))?;

        match stmt.query_row([id], |row| {
            Ok(UserPo {
                id: row.get(0)?,
                organization_id: row.get(1)?,
                username: row.get(2)?,
                display_name: row.get(3)?,
                email: row.get(4)?,
                password_hash: row.get(5)?,
                role: row.get(6)?,
                status: row.get(7)?,
                created_by: row.get(8)?,
                modified_by: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        }) {
            Ok(u) => Ok(Some(u)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Internal(e.to_string())),
        }
    }

    fn find_by_username(&self, _ctx: RequestContext, username: &str) -> Result<Option<UserPo>, AppError> {
        let conn = storage::get().conn();

        let mut stmt = conn
            .prepare(
                "SELECT id, organization_id, username, display_name, email, password_hash, role, status, created_by, modified_by, created_at, updated_at 
                 FROM users WHERE username = ?1 AND status != 0",
            )
            .map_err(|e| AppError::Internal(e.to_string()))?;

        match stmt.query_row([username], |row| {
            Ok(UserPo {
                id: row.get(0)?,
                organization_id: row.get(1)?,
                username: row.get(2)?,
                display_name: row.get(3)?,
                email: row.get(4)?,
                password_hash: row.get(5)?,
                role: row.get(6)?,
                status: row.get(7)?,
                created_by: row.get(8)?,
                modified_by: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        }) {
            Ok(u) => Ok(Some(u)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Internal(e.to_string())),
        }
    }

    fn find_by_organization_id(&self, _ctx: RequestContext, org_id: &str) -> Result<Vec<UserPo>, AppError> {
        let conn = storage::get().conn();

        let mut stmt = conn
            .prepare(
                "SELECT id, organization_id, username, display_name, email, password_hash, role, status, created_by, modified_by, created_at, updated_at 
                 FROM users WHERE organization_id = ?1 AND status != 0 ORDER BY created_at DESC",
            )
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let users = stmt
            .query_map([org_id], |row| {
                Ok(UserPo {
                    id: row.get(0)?,
                    organization_id: row.get(1)?,
                    username: row.get(2)?,
                    display_name: row.get(3)?,
                    email: row.get(4)?,
                    password_hash: row.get(5)?,
                    role: row.get(6)?,
                    status: row.get(7)?,
                    created_by: row.get(8)?,
                    modified_by: row.get(9)?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                })
            })
            .map_err(|e| AppError::Internal(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(users)
    }

    fn update(&self, ctx: RequestContext, user: &UserPo) -> Result<(), AppError> {
        let conn = storage::get().conn();

        conn.execute(
            "UPDATE users SET organization_id = ?1, username = ?2, display_name = ?3, email = ?4, password_hash = ?5, role = ?6, modified_by = ?7, updated_at = ?8 WHERE id = ?9",
            rusqlite::params![
                user.organization_id,
                user.username,
                user.display_name,
                user.email,
                user.password_hash,
                user.role,
                ctx.uid(),
                utils::current_timestamp(),
                user.id,
            ],
        )
            .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    fn delete(&self, ctx: RequestContext, id: &str) -> Result<(), AppError> {
        let conn = storage::get().conn();

        conn.execute(
            "UPDATE users SET status = 0, modified_by = ?1, updated_at = ?2 WHERE id = ?3 AND status != 0",
            rusqlite::params![ctx.uid(), utils::current_timestamp(), id],
        )
            .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    fn exists_by_username(&self, _ctx: RequestContext, username: &str) -> Result<bool, AppError> {
        let conn = storage::get().conn();

        let mut stmt = conn
            .prepare("SELECT COUNT(*) FROM users WHERE username = ?1 AND status != 0")
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let count: i64 = stmt
            .query_row([username], |row| row.get(0))
            .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(count > 0)
    }

    fn count_by_organization_id(&self, _ctx: RequestContext, org_id: &str) -> Result<u64, AppError> {
        let conn = storage::get().conn();

        let mut stmt = conn
            .prepare("SELECT COUNT(*) FROM users WHERE organization_id = ?1 AND status != 0")
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let count: i64 = stmt
            .query_row([org_id], |row| row.get(0))
            .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(count as u64)
    }
}
