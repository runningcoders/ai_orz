//! SQLite 存储模块
//!
//! 基于 sqlx 连接池管理，不再使用全局单例，支持依赖注入和测试隔离
//!
//! 向量存储后端：
//! - SqliteVssStore: 基于 SQLite VSS 扩展（需要系统依赖）
//! - InMemoryVectorStore: 纯 Rust 内存实现（推荐，零系统依赖）
//! - HnswStore: HNSW 高性能近似最近邻索引（V2 优化）

use crate::error::Result;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::sync::Arc;

/// 向量存储抽象 Trait
pub mod vector;
pub use vector::{VectorStore, SqliteVssStore};

/// 纯 Rust 内存向量存储
mod mem_vector;
pub use mem_vector::InMemoryVectorStore;

/// HNSW 高性能近似最近邻索引（V2 优化）
mod hnsw;
pub use hnsw::HnswStore;

/// 统一存储门面（可克隆，内部 Arc，零成本克隆）
#[derive(Clone, Debug)]
pub struct Storage {
    inner: Arc<StorageInner>,
}

#[derive(Debug)]
struct StorageInner {
    sqlite: SqlitePool,
    vector: Arc<dyn VectorStore>,
}

impl Storage {
    /// 从已有的 SQLite pool 创建 Storage（测试专用，保证隔离性）
    /// 使用纯 Rust 内存向量存储
    pub fn with_sqlite_pool(sqlite: SqlitePool) -> Self {
        // 测试场景使用内存向量存储
        let vector = Arc::new(
            InMemoryVectorStore::new(std::env::temp_dir().join("ai_orz_test_vectors"))
                .expect("创建测试向量存储失败")
        );
        Self {
            inner: Arc::new(StorageInner { sqlite, vector }),
        }
    }
    
    /// 创建存储实例，初始化连接池，自动运行 migrations
    /// 默认使用纯 Rust 内存向量存储（零系统依赖）
    pub async fn new(db_path: &str, vector_base_path: &str) -> Result<Self> {
        // SQLite 连接 URL 格式：sqlite:path 不需要双斜杠
        // 双斜杠会导致相对路径解析错误，把当前目录当成域名解析了
        let connection_url = if db_path == ":memory:" {
            "sqlite::memory:".to_string()
        } else {
            format!("sqlite:{}", db_path)
        };

        let sqlite = SqlitePoolOptions::new()
            .max_connections(5) // SQLite 单文件写并发有限，不需要太多连接
            .connect(&connection_url)
            .await?;

        // 运行所有 migrations，自动建表/升级
        sqlx::migrate!("./migrations").run(&sqlite).await?;

        // 初始化向量存储（默认使用纯 Rust 内存实现）
        let vector: Arc<dyn VectorStore> = Arc::new(
            InMemoryVectorStore::new(vector_base_path)?
        );

        Ok(Self {
            inner: Arc::new(StorageInner { sqlite, vector }),
        })
    }
    
    /// 创建使用 SQLite VSS 后端的存储（需要系统依赖）
    pub async fn with_sqlite_vss(db_path: &str, vector_db_path: &str) -> Result<Self> {
        let connection_url = if db_path == ":memory:" {
            "sqlite::memory:".to_string()
        } else {
            format!("sqlite:{}", db_path)
        };

        let sqlite = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&connection_url)
            .await?;

        sqlx::migrate!("./migrations").run(&sqlite).await?;

        // 使用 SQLite VSS 后端
        let vector: Arc<dyn VectorStore> = Arc::new(
            SqliteVssStore::new(vector_db_path).await?
        );

        Ok(Self {
            inner: Arc::new(StorageInner { sqlite, vector }),
        })
    }

    /// 获取 SQLite 连接池引用
    pub fn sqlite_pool(&self) -> &SqlitePool {
        &self.inner.sqlite
    }

    /// 获取 SQLite 连接池的 owned clone（便宜，因为内部是 Arc）
    pub fn sqlite_pool_owned(&self) -> SqlitePool {
        self.inner.sqlite.clone()
    }
    
    /// 向后兼容：旧代码调用 pool_owned()
    #[deprecated = "Use sqlite_pool_owned() instead"]
    pub fn pool_owned(&self) -> SqlitePool {
        self.sqlite_pool_owned()
    }

    /// 获取向量存储（统一 Trait 接口）
    pub fn vector(&self) -> Arc<dyn VectorStore> {
        self.inner.vector.clone()
    }
}

/// 全局存储实例
static STORAGE: std::sync::OnceLock<Storage> = std::sync::OnceLock::new();

/// 初始化存储（测试专用，内存数据库）
pub async fn init_for_test() {
    init("sqlite::memory:", "sqlite::memory:").await;
}

/// 初始化存储
pub async fn init(db_path: &str, vector_db_path: &str) {
    let storage = Storage::new(db_path, vector_db_path).await.unwrap();
    let _ = STORAGE.set(storage);
}

/// 获取存储实例
pub fn get() -> &'static Storage {
    STORAGE
        .get()
        .expect("存储未初始化，请先调用 storage::init()")
}
