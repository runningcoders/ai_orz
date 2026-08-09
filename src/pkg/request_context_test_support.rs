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

/// 确保全局 base data 目录指向进程级共享临时目录（幂等，仅首次生效）
///
/// 通过 BASE_DATA_PATH_ENV 环境变量生效（AppConfig::base_data_path 每次读环境变量），
/// 供 shell_exec 等依赖 config::get().base_data_path() 的单测使用。
pub fn ensure_test_base_data_path() -> std::path::PathBuf {
    static DIR: once_cell::sync::Lazy<std::path::PathBuf> = once_cell::sync::Lazy::new(|| {
        let path = std::env::temp_dir().join(format!("ai_orz_test_base_{}", std::process::id()));
        std::fs::create_dir_all(&path).expect("创建测试 base data 目录失败");
        // SAFETY: Lazy 保证全进程仅设置一次，无并发写环境变量竞争
        unsafe { std::env::set_var(common::config::BASE_DATA_PATH_ENV, &path) };
        path
    });
    DIR.clone()
}

/// 确保 ToolCallLogger 单例已初始化（指向测试 base data 目录，幂等）
pub fn ensure_test_tool_call_logger() {
    crate::pkg::tool_tracing::logger::ToolCallLogger::init(ensure_test_base_data_path());
}
