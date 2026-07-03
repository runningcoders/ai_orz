//! SQLite 存储模块
//!
//! 基于 sqlx 连接池管理，不再使用全局单例，支持依赖注入和测试隔离
//!
//! 向量存储后端：
//! - SqliteVssStore: 基于 SQLite VSS 扩展（需要系统依赖）
//! - InMemoryVectorStore: 纯 Rust 内存实现（推荐，零系统依赖）
//! - HnswStore: HNSW 高性能近似最近邻索引（V2 优化）

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::sync::Arc;
use once_cell::sync::OnceCell;

use crate::pkg::stats::Stats;

/// 向量存储抽象 Trait
pub mod vector;
pub use vector::{SqliteVssStore, VectorStore};

/// 纯 Rust 内存向量存储
mod mem_vector;
pub use mem_vector::InMemoryVectorStore;

/// HNSW 高性能近似最近邻索引（V2 优化）
mod hnsw;
pub use hnsw::HnswStore;

/// LanceDB 高性能嵌入式向量数据库
mod lance;
pub use lance::LanceVectorStore;

/// 统一存储门面（可克隆，内部 Arc，零成本克隆）
#[derive(Clone, Debug)]
pub struct Storage {
    inner: Arc<StorageInner>,
}

#[derive(Debug)]
struct StorageInner {
    sqlite: SqlitePool,
    vector: Arc<dyn VectorStore>,
    stats: OnceCell<Stats>,
}

impl Storage {
    /// 创建存储实例，初始化连接池，自动运行 migrations
    /// 根据配置自动选择向量存储后端
    ///
    /// # 参数
    /// - base_data_path: 基础数据目录根路径
    /// - db_config: 数据库配置片段（从完整 Config 中取出传入）
    pub async fn new(
        base_data_path: &Path,
        db_config: &common::config::DatabaseConfig,
        stats_config: &common::config::StatsConfig,
    ) -> Result<Self> {
        let db_path = base_data_path.join(&db_config.db_file_name);
        let connection_url = format!("sqlite://{}", db_path.display());

        let sqlite = SqlitePoolOptions::new()
            .max_connections(5) // SQLite 单文件写并发有限，不需要太多连接
            .connect(&connection_url)
            .await?;

        // 运行所有 migrations，自动建表/升级
        sqlx::migrate!("./migrations").run(&sqlite).await.map_err(Into::<common::error::Error>::into)?;

        // 根据配置选择向量存储后端
        let vector: Arc<dyn VectorStore> = match db_config.vector_store_type {
            common::config::VectorStoreType::InMemory => {
                let vectors_dir = base_data_path.join("vectors");
                Arc::new(InMemoryVectorStore::with_path(&vectors_dir)?)
            }
            common::config::VectorStoreType::Hnsw => Arc::new(HnswStore::new()?),
            common::config::VectorStoreType::LanceDb => {
                let lance_dir = base_data_path.join("vectors_lance");
                Arc::new(LanceVectorStore::new(&lance_dir)?)
            }
            common::config::VectorStoreType::SqliteVss => {
                let vector_db_path = base_data_path.join(&db_config.vector_db_file_name);
                Arc::new(SqliteVssStore::new(vector_db_path.to_str().unwrap_or_default()).await?)
            }
        };

        // 初始化 Stats DuckDB
        let stats_db_path = base_data_path.join(&stats_config.db_file_name);
        let stats = Stats::open(
            stats_db_path.to_str().unwrap_or_default(),
            stats_config.batch_size,
        ).await?;
        stats.initialize_default()?;

        let mut inner = StorageInner {
            sqlite,
            vector,
            stats: OnceCell::new(),
        };
        // Safety: we just created it, so set is ok
        inner.stats.set(stats).expect("stats already initialized");

        Ok(Self {
            inner: Arc::new(inner),
        })
    }

    /// 创建使用 SQLite VSS 后端的存储（需要系统依赖，保留用于向后兼容）
    pub async fn with_sqlite_vss(
        base_data_path: &Path,
        db_config: &common::config::DatabaseConfig,
        stats_config: &common::config::StatsConfig,
    ) -> Result<Self> {
        let mut db_config = db_config.clone();
        db_config.vector_store_type = common::config::VectorStoreType::SqliteVss;
        Self::new(base_data_path, &db_config, stats_config).await
    }

    /// 获取 SQLite 连接池
    pub fn sqlite(&self) -> &SqlitePool {
        &self.inner.sqlite
    }

    /// 获取 SQLite 连接池（别名，向后兼容）
    pub fn sqlite_pool(&self) -> &SqlitePool {
        &self.inner.sqlite
    }

    /// 获取向量存储
    pub fn vector(&self) -> &Arc<dyn VectorStore> {
        &self.inner.vector
    }

    /// 获取向量存储（兼容旧代码）
    pub fn vector_store(&self) -> &Arc<dyn VectorStore> {
        &self.inner.vector
    }

    /// 获取 Stats 统计模块
    pub fn stats(&self) -> &Stats {
        self.inner.stats.get().expect("Stats not initialized")
    }

    /// 安全获取 Stats 统计模块（返回 Option，避免未初始化时 panic）
    pub fn stats_opt(&self) -> Option<&Stats> {
        self.inner.stats.get()
    }

    /// 初始化 Stats（首次设置，不可重复）
    /// 生产代码由 `Storage::new()` 内部调用，测试代码可通过此方法注入
    pub fn init_stats(&self, stats: Stats) -> common::error::Result<()> {
        self.inner.stats.set(stats).map_err(|_| {
            common::error::Error::internal("Stats already initialized")
        })
    }
}

use std::path::Path;
use std::sync::OnceLock;
use common::error::Result;

/// 全局 Storage 单例（向后兼容）
static STORAGE_INSTANCE: OnceLock<Storage> = OnceLock::new();

/// 获取全局 Storage 单例（向后兼容）
pub fn get() -> &'static Storage {
    STORAGE_INSTANCE
        .get()
        .expect("Storage 尚未初始化，请先调用 storage::init()")
}

/// 初始化全局 Storage（由 main.rs 调用，只调用一次）
pub async fn init(
    base_data_path: &Path,
    db_config: &common::config::DatabaseConfig,
    stats_config: &common::config::StatsConfig,
) {
    if STORAGE_INSTANCE.get().is_none() {
        let storage = Storage::new(base_data_path, db_config, stats_config)
            .await
            .expect("初始化 Storage 失败");
        let _ = STORAGE_INSTANCE.set(storage);
    }
}

/// 测试辅助工具（仅在 test 编译时可用）
#[cfg(test)]
pub mod test_support;
