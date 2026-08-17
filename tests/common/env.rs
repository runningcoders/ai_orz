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
            // 0. 隔离基础数据目录：每个测试二进制进程独占一个 tempdir 作为
            //    AI_ORZ_BASE_PATH，避免 cargo test --test '*' 并行跑多个集成
            //    测试二进制时并发写共享目录 `.ai_orz/ai_orz.toml` 造成
            //    TOML 文件部分写入→解析失败→`a2a_server` 回退 Default(enabled=false)
            //    → tasks/send 返回 METHOD_NOT_FOUND 的 CI 偶现失败。
            //
            // 安全性：set_var 标记 unsafe 是因为跨线程读/写 env 会导致 UB。
            // 此处位于 ENV_INIT OnceCell 的 get_or_init 闭包内，是进程启动后
            // 所有测试代码执行之前的单例初始化，没有其他线程在并发读写 env，
            // 因此是安全的。后续所有 env::var 读取都会在 set 完成后发生。
            let base_tmp = tempfile::tempdir().expect("创建 base data 临时目录失败");
            unsafe {
                std::env::set_var(
                    common::config::BASE_DATA_PATH_ENV,
                    base_tmp.path().as_os_str(),
                );
            }
            // Leak：进程生命周期内保持目录存在，测试结束由 OS 回收 tempdir。
            std::mem::forget(base_tmp);

            // 1. Load global AppConfig (idempotent; writes default to isolated base path)
            //    显式 unwrap：配置加载失败必须立刻终止，静默忽略会导致后续
            //    config::get() 拿到空 OnceLock 发生无关 panic，定位成本极高。
            ai_orz::config::init().expect("集成测试 config 初始化失败");

            // 2. pkg::storage — isolate to a tempdir + InMemory vector store to avoid
            //    polluting the dev `.ai_orz/` directory. Pattern proven by
            //    `tests/http_handler_macro_test.rs::ensure_storage_initialized`.
            //    用 InMemory 而非 default LanceDb，避免 block_in_place 要求 multi-thread runtime
            let tmp = tempfile::tempdir().expect("创建 storage 临时目录失败");
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

            // 7. AOP 基础设施 — 先注册生产者和消费者（注册到 registry，还没真正开始
            //    轮询/消费 worker），对齐真实 ai_orz::run() 的启动顺序。
            ai_orz::producer::init()
                .await
                .expect("producer init should succeed");
            ai_orz::consumer::init()
                .await
                .expect("consumer init should succeed");

            // 8. 第二阶段：service 基础数据（幂等注入 DB 默认值）
            //    对齐真实启动流程：producer/consumer 注册完毕 → init_base_data。
            //    目前内容：system domain 的 2 条系统级 cron triggers
            //    （agent_rest 4h + project_followup 1h）。
            //    缺少这一步会导致 system_cron_triggers_test 的 baseline 断言拿到 0。
            ai_orz::service::init_base_data().await;
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
