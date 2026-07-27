//! Full test environment initialization.
//!
//! Uses `ai_orz::service::init()` (the aggregated service-layer init) instead
//! of manually listing 30+ DAO/DAL/Domain init calls — mirrors the `main.rs`
//! startup flow. Storage is isolated to a tempdir to avoid polluting the dev
//! environment (same pattern as `tests/http_handler_macro_test.rs`).
//!
//! 【关键设计】所有集成测试共享同一个全局数据库（因为 `storage::init` 用 `OnceLock`
//! 全局单例，第二次调用是 no-op）。用 `tokio::sync::OnceCell` 串行化初始化，
//! 只有第一个测试会真正执行 init，后续测试直接复用。测试间靠唯一 ID（uuid::now_v7）
//! 隔离数据，不依赖 DB 初始状态。

use ai_orz::pkg::RequestContext;
use common::config::{DatabaseConfig, StatsConfig, VectorStoreType};
use sqlx::SqlitePool;

/// 全局初始化 cell，确保所有 init 操作只执行一次。
static ENV_INIT: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();

/// Initialize all pkg + service singletons + return a test RequestContext.
///
/// Idempotent: 第一次调用真正执行初始化，后续调用直接返回（所有底层 `init`
/// 函数都基于 `OnceLock::set`，二次调用 no-op）。所有集成测试共享同一个全局
/// Storage 单例，测试间靠唯一 ID 隔离数据。
pub async fn init_full_test_env(_pool: SqlitePool) -> RequestContext {
    ENV_INIT
        .get_or_init(|| async {
            // 1. Load global AppConfig (idempotent; reads `.ai_orz/ai_orz.toml`)
            let _ = ai_orz::config::init();

            // 2. pkg::storage — isolate to a tempdir + InMemory vector store to avoid
            //    polluting the dev `.ai_orz/` directory. Pattern proven by
            //    `tests/http_handler_macro_test.rs::ensure_storage_initialized`.
            //    用 InMemory 而非 default LanceDb，避免 block_in_place 要求 multi-thread runtime
            let tmp = tempfile::tempdir().expect("创建临时目录失败");
            let db_config = DatabaseConfig {
                vector_store_type: VectorStoreType::InMemory,
                ..Default::default()
            };
            let stats_config = StatsConfig::default();
            ai_orz::pkg::storage::init(tmp.path(), &db_config, &stats_config).await;
            // Leak the tempdir so the SQLite file stays alive for the test process lifetime.
            std::mem::forget(tmp);

            // 3. pkg::jwt — test-only secret (1 hour expiry is plenty for any test)
            ai_orz::pkg::jwt::init_jwt("test-jwt-secret-do-not-use-in-prod", 1);

            // 4. pkg::tool_tracing — agent creation writes trace files
            let trace_dir = std::env::temp_dir().join("ai_orz_integration_test_trace");
            let _ = std::fs::create_dir_all(&trace_dir);
            ai_orz::pkg::tool_tracing::logger::ToolCallLogger::init(trace_dir);

            // 5. pkg::tool_registry — register builtin tools (idempotent via registry set)
            ai_orz::pkg::tool_registry::builtin::register_all(
                ai_orz::pkg::tool_registry::get_registry(),
            );

            // 6. service layer — one-line replacement for 30+ manual DAO/DAL/Domain init calls.
            //    Internally calls: dao::init_all() + dal::init_all() + domain::init_all().
            ai_orz::service::init();
        })
        .await;

    // Note: `request_context_test_support::new_test_ctx` is `#[cfg(test)]` and not
    // accessible from integration tests. Use the global Storage singleton (initialized
    // above by `storage::init`) + `RequestContext::from_storage`, matching the pattern
    // in `tests/http_handler_macro_test.rs` which also relies on the global singleton.
    // The `pool` parameter is retained for `#[sqlx::test]` API compatibility but
    // is NOT used by handlers (handlers access the global storage singleton).
    RequestContext::from_storage("test-integration-user", ai_orz::pkg::storage::get().clone())
}
