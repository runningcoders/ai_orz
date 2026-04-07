//! SQLite 特定实现
//!
//! 包含 SQLite 特定的初始化和建表逻辑
//! 预留扩展其他数据库，保持解耦

use rusqlite::Connection;
use crate::pkg::storage::sql;

/// 初始化 SQLite 数据库，创建所有表
pub fn init_db(conn: &mut Connection) -> Result<(), String> {
    let tables = [
        sql::SQLITE_CREATE_TABLE_AGENTS,
        sql::SQLITE_CREATE_TABLE_MODEL_PROVIDERS,
        sql::SQLITE_CREATE_TABLE_ORGANIZATIONS,
        sql::SQLITE_CREATE_TABLE_TASKS,
        sql::SQLITE_CREATE_TABLE_SHORT_TERM_MEMORY_INDEX,
        sql::SQLITE_CREATE_TABLE_LONG_TERM_KNOWLEDGE_NODE,
        sql::SQLITE_CREATE_TABLE_KNOWLEDGE_REFERENCE,
    ];

    for table_sql in tables {
        conn.execute(table_sql, ())
            .map_err(|e| format!("创建表失败: {}", e))?;
    }

    Ok(())
}
