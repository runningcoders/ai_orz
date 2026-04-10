//! SQLite 特定实现
//!
//! 包含 SQLite 特定的初始化和建表逻辑
//! 预留扩展其他数据库，保持解耦

use rusqlite::Connection;
use crate::pkg::storage::sql;

/// 初始化 SQLite 数据库，创建所有表
pub fn init_db(conn: &mut Connection) -> Result<(), String> {
    // 创建所有表
    let tables = [
        sql::SQLITE_CREATE_TABLE_AGENTS,
        sql::SQLITE_CREATE_TABLE_MODEL_PROVIDERS,
        sql::SQLITE_CREATE_TABLE_ORGANIZATIONS,
        sql::SQLITE_CREATE_TABLE_USERS,
        sql::SQLITE_CREATE_TABLE_TASKS,
        sql::SQLITE_CREATE_TABLE_SHORT_TERM_MEMORY_INDEX,
        sql::SQLITE_CREATE_TABLE_LONG_TERM_KNOWLEDGE_NODE,
        sql::SQLITE_CREATE_TABLE_KNOWLEDGE_NODE_RELATION,
        sql::SQLITE_CREATE_TABLE_KNOWLEDGE_REFERENCE,
        sql::SQLITE_CREATE_TABLE_MESSAGES,
    ];

    for table_sql in tables {
        conn.execute(table_sql, ())
            .map_err(|e| format!("创建表失败: {}", e))?;
    }

    // 创建所有索引
    let indexes = [
        sql::SQLITE_CREATE_INDEX_USERS_ORGANIZATION_ID,
        sql::SQLITE_CREATE_INDEX_USERS_USERNAME,
        sql::SQLITE_CREATE_INDEX_SHORT_TERM_AGENT_ID,
        sql::SQLITE_CREATE_INDEX_SHORT_TERM_CREATED_AT,
        sql::SQLITE_CREATE_INDEX_SHORT_TERM_TAGS,
        sql::SQLITE_CREATE_INDEX_LTKN_AGENT_ID,
        sql::SQLITE_CREATE_INDEX_LTKN_NODE_TYPE,
        sql::SQLITE_CREATE_INDEX_KNR_SOURCE_NODE_ID,
        sql::SQLITE_CREATE_INDEX_KNR_TARGET_NODE_ID,
        sql::SQLITE_CREATE_INDEX_KR_KNOWLEDGE_ID,
        sql::SQLITE_CREATE_INDEX_KR_SHORT_TERM_ID,
        sql::SQLITE_CREATE_INDEX_KR_TRACE_ID,
        sql::SQLITE_CREATE_INDEX_MESSAGES_TASK_ID,
        sql::SQLITE_CREATE_INDEX_MESSAGES_FROM_ID,
        sql::SQLITE_CREATE_INDEX_MESSAGES_TO_ID,
        sql::SQLITE_CREATE_INDEX_MESSAGES_CREATED_AT,
    ];

    for index_sql in indexes {
        conn.execute(index_sql, ())
            .map_err(|e| format!("创建索引失败: {}", e))?;
    }

    Ok(())
}
