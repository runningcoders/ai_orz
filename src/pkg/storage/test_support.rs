//! Storage 测试辅助工具
//!
//! 提供测试专用的 Storage 构建方法，与生产代码完全隔离。
//! 仅在 `#[cfg(test)]` 编译时可用。

use super::*;
use crate::pkg::stats::Stats;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

/// 从已有的 SQLite pool 创建测试用 Storage（内存向量存储，无 Stats）
pub fn create_test_storage(pool: SqlitePool) -> Storage {
    let temp_dir = tempfile::tempdir().expect("创建临时目录失败");
    let vector =
        Arc::new(InMemoryVectorStore::with_path(temp_dir.path()).expect("创建测试向量存储失败"));
    Storage {
        inner: Arc::new(StorageInner {
            sqlite: pool,
            vector,
            stats: OnceCell::new(),
        }),
    }
}

/// 创建带 Stats 的测试用 Storage
pub async fn create_test_storage_with_stats(pool: SqlitePool, stats: Stats) -> Storage {
    let storage = create_test_storage(pool);
    storage.init_stats(stats).expect("init stats failed");
    storage
}

/// 初始化全局 Storage 单例（测试专用，内存数据库）
pub async fn init_for_test() {
    if STORAGE_INSTANCE.get().is_none() {
        let sqlite = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("创建测试内存数据库失败");

        sqlx::migrate!("./migrations")
            .run(&sqlite)
            .await
            .expect("运行 migrations 失败");

        let storage = create_test_storage(sqlite);
        let _ = STORAGE_INSTANCE.set(storage);
    }
}
