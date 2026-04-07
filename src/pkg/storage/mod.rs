//! SQLite 存储模块

use rusqlite::Connection;
use std::sync::Mutex;

pub mod sql;

/// 数据库连接管理
pub struct Storage {
    conn: Mutex<Connection>,
}

impl Storage {
    /// 创建存储实例
    pub fn new(db_path: &str) -> Result<Self, String> {
        let conn = Connection::open(db_path).map_err(|e| format!("打开数据库失败: {}", e))?;

        // 创建所有表
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

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// 获取数据库连接
    pub fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap()
    }
}

/// 全局存储实例
static STORAGE: std::sync::OnceLock<Storage> = std::sync::OnceLock::new();

/// 初始化存储
pub fn init(db_path: &str) -> Result<(), String> {
    let storage = Storage::new(db_path)?;
    STORAGE.set(storage).map_err(|_| "存储已初始化".to_string())
}

/// 获取存储实例
pub fn get() -> &'static Storage {
    STORAGE
        .get()
        .expect("存储未初始化，请先调用 storage::init()")
}

/// 测试用：获取内存数据库连接
#[cfg(test)]
pub fn test_conn() -> rusqlite::Connection {
    Connection::open_in_memory().expect("创建内存数据库失败")
}
