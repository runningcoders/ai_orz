//! SQLite 存储模块

use rusqlite::Connection;
use std::sync::Mutex;
use std::cell::RefCell;

/// 线程本地数据库连接
thread_local! {
    static TEST_CONN: RefCell<Option<Connection>> = RefCell::new(None);
}

/// 数据库连接管理
pub struct Storage {
    conn: Mutex<Connection>,
}

impl Storage {
    /// 创建存储实例
    pub fn new(db_path: &str) -> Result<Self, String> {
        let conn = Connection::open(db_path)
            .map_err(|e| format!("打开数据库失败: {}", e))?;
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
    STORAGE.get().expect("存储未初始化，请先调用 storage::init()")
}

/// 测试用：获取内存数据库连接
#[cfg(test)]
pub fn test_conn() -> rusqlite::Connection {
    Connection::open_in_memory().expect("创建内存数据库失败")
}
