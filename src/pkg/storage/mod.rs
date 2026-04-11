//! SQLite 存储模块
//! 
//! 基于 sqlx 连接池管理，不再使用全局单例，支持依赖注入和测试隔离

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use crate::error::Result;

/// 数据库连接池包装
#[derive(Clone, Debug)]
pub struct Storage {
    pool: SqlitePool,
}

impl Storage {
    /// 创建存储实例，初始化连接池，自动运行 migrations
     async fn new(db_path: &str) -> Result<Self> {
        // SQLite 连接 URL 需要是 sqlite://路径 格式
        let connection_url = if db_path == ":memory:" {
            "sqlite::memory:".to_string()
        } else {
            format!("sqlite://{}", db_path)
        };

        let pool = SqlitePoolOptions::new()
            .max_connections(5) // SQLite 单文件写并发有限，不需要太多连接
            .connect(&connection_url)
            .await?;

        // 运行所有 migrations，自动建表/升级
        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self {
            pool
        })
    }

    /// 获取连接池引用
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// 获取连接池的 owned clone（便宜，因为内部是 Arc）
    pub fn pool_owned(&self) -> SqlitePool {
        self.pool.clone()
    }
}

/// 全局存储实例
static STORAGE: std::sync::OnceLock<Storage> = std::sync::OnceLock::new();

/// 初始化存储
pub async fn init(db_path: &str) {
    let storage = Storage::new(db_path).await.unwrap();
    let _ = STORAGE.set(storage);
}

/// 获取存储实例
pub fn get() -> &'static Storage {
    STORAGE
        .get()
        .expect("存储未初始化，请先调用 storage::init()")
}

