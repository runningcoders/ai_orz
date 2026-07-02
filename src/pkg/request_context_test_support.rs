//! RequestContext 测试辅助工具
//!
//! 提供测试专用的 RequestContext 构建方法，与生产代码完全隔离。
//! 仅在 `#[cfg(test)]` 编译时可用。

use crate::pkg::request_context::RequestContext;
use crate::pkg::stats::Stats;
use crate::pkg::storage;
use sqlx::sqlite::SqlitePool;

/// 创建测试用 RequestContext（无 Stats）
pub fn new_test_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    let storage = storage::test_support::create_test_storage(pool);
    RequestContext::from_storage(user_id, storage)
}

/// 创建带 Stats 的测试用 RequestContext
pub fn new_test_ctx_with_stats(user_id: &str, pool: SqlitePool, stats: Stats) -> RequestContext {
    let storage = storage::test_support::create_test_storage(pool);
    storage.init_stats(stats).expect("init stats failed");
    RequestContext::from_storage(user_id, storage)
}

/// 从全局 Storage 创建测试用 RequestContext（使用全局单例）
pub fn new_test_ctx_from_global(user_id: &str) -> RequestContext {
    RequestContext::new(Some(user_id.to_string()), None)
}
