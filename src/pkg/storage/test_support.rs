//! Storage 测试辅助工具
//!
//! 提供测试专用的 Storage 构建方法，与生产代码完全隔离。
//! 仅在 `#[cfg(test)]` 编译时可用。

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

use super::*;
use crate::pkg::stats::Stats;

/// 全局测试库所在的进程级临时目录（故意不清理：单例要活到进程结束）
fn global_db_dir() -> &'static std::path::PathBuf {
    static DIR: once_cell::sync::Lazy<std::path::PathBuf> = once_cell::sync::Lazy::new(|| {
        let path = std::env::temp_dir().join(format!("ai_orz_test_storage_{}", std::process::id()));
        std::fs::create_dir_all(&path).expect("创建测试全局库目录失败");
        path
    });
    &DIR
}

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

/// 初始化串行门锁：`init_for_test` 常被多个并行用例同时调用。
///
/// ⚠️ 不能用 checklist 式的 `if get().is_none()` 就完事：两个调用者会各自
/// `connect` + `migrate`，在**同一个文件库**上并发跑 migrations → `SQLITE_BUSY`
/// / 「表已存在」→ `.expect()` 直接 panic。
static INIT_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// 初始化全局 Storage 单例（测试专用）
///
/// ⚠️ **必须用临时文件库，不能用 `sqlite::memory:`**：SQLite 的内存库是
/// **按连接私有**的 —— migrations 只在连接池开出的第 1 条连接上执行过，一旦并发
/// 压力让池开出第 2 条连接，那条连接看到的是一张表都没有的新库。症状正是「单跑全绿、
/// 并跑全量偶发 `no such table`」的幽灵 flake（`RequestContext::new_system()` 路径，
/// 如 lark/email 监听器的 `ensure_listener_for`）。改用文件库后所有连接共享同一份
/// 数据，语义与生产 `Storage::new` 一致。
pub async fn init_for_test() {
    let _guard = INIT_LOCK.lock().await;
    if STORAGE_INSTANCE.get().is_none() {
        let sqlite = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(global_db_dir().join("global.db"))
                    .create_if_missing(true),
            )
            .await
            .expect("创建测试数据库失败");

        sqlx::migrate!("./migrations")
            .run(&sqlite)
            .await
            .expect("运行 migrations 失败");

        let storage = create_test_storage(sqlite);
        let _ = STORAGE_INSTANCE.set(storage);
    }
}
