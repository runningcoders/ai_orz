//! Shared test infrastructure for HTTP API integration tests.
//!
//! Provides:
//! - `init_full_test_env` — full DAO/DAL/Domain initialization (extracted from a2a pattern)
//! - `TestApp` — wraps `axum::Router` with typed HTTP request helpers
//! - `factories` — test data factories returning business entities
//! - `assertions` — common API response assertions
//!
//! ---------------------------------------------------------------------------
//! 集成测试避坑参考：共享存储下的「污染免疫」断言纪律
//! （历史教训沉淀，新写用例前速读一遍；逐案细节见文末案例索引）
//! ---------------------------------------------------------------------------
//!
//! ## 存储模型事实（冲突域判断的前提）
//!
//! - 每个 `tests/*.rs` 是独立进程、自带独立 tempdir 存储 → **跨 binary 永不互相干扰**；
//! - 同一 binary 内的 `#[sqlx::test]` 用例默认**并行**执行，却共享同一个进程级
//!   Storage 单例与唯一的 Local 组织 → 所有冲突都发生在 binary 内部兄弟用例
//!   之间，且是真并发竞争，不是单纯执行顺序问题；
//! - `factories::bootstrap_and_login` 返回的 SuperAdmin 是「复用身份」：其数据
//!   在兄弟用例间持久累积、随时可能被并发写入。它适合做"系统已就绪"的前置
//!   条件、或与请求体强绑定的纯 4xx 校验；**绝不要用它做状态快照的断言主体**。
//!
//! ## 对共享身份敏感的四类断言（偶发失败高发区）
//!
//! 1. **空态 / 零计数**：`assert!(list.is_empty())` —— 兄弟用例写入一条即翻车；
//! 2. **精确条数 / 快照**：`assert_eq!(list.len(), 2)` —— 同理；
//! 3. **默认解析 / 回退顺序**：期望"无 X → 回退 Y"（如 find_default_credential），
//!    被抢先写入后会命中别人的数据；
//! 4. **全局唯一性假设**：如"全局只有一个启用的 embedding provider"。
//!
//! ## 修复纪律：断言主体要么是全新身份，要么能证明「额外数据不影响判定」
//!
//! **纪律 A —— 快照主体迁移到 `register_fresh_member` 全新成员**（适用于绝对
//! 断言，即四类中的 1/2/3）。以下三行是固定头部模板：
//!
//! ```ignore
//! // 前置条件：Local 组织已初始化
//! let _ = crate::common::factories::bootstrap_and_login(&app).await;
//! let (jwt, _member_id, _member_org) =
//!     crate::common::factories::register_fresh_member(&app).await;
//! ```
//!
//! 新成员名下业务数据从零开始，空态/计数断言由此获得确定性隔离。
//!
//! **纪律 B —— 改写成「防污染形状」的断言**（适用于验证查询/过滤语义本身，
//! 如四类中的 1/2 出现在分页过滤场景）：
//!
//! - 包含式替代计数：`list.iter().any(|m| m.contains(自建唯一标识))` 替代
//!   对返回总数的绝对断言；
//! - 正反双向配对：既 `any` 必须命中自建行，也 `!any` 不得混入他侧行；
//! - 实体分区：以本用例创建的 entity_id 作过滤键，避免扫组织级大列表后数总数。
//!
//! ## 类别 4 的专门解法：全局单例资源窗口整体串行
//!
//! 围绕"全局唯一启用 provider"维护窗口（建 provider → 操作 → 删 provider）
//! 的一组用例，必须用文件级 async Mutex 串行化，且锁获取放在配置早退之后
//! （缺 key 的跳过路径不得触碰锁）。已有先例可直接抄：
//! `tests/integration/message_vector_test.rs`（REAL_VECTOR_MUTEX）、
//! `tests/integration/model_provider_embedding_test.rs`（EMBEDDING_STATE_MUTEX）。
//!
//! ## 历史案例索引（问题形状 → 修复手法对照）
//!
//! | 案例 | 问题形状 | 修复 |
//! |------|----------|------|
//! | `tests/integration/web_search_tool_test.rs` | 空态断言 + 默认解析回退 | 纪律 A |
//! | `tests/integration/github_integration_test.rs` | 凭据生命周期精确条数 | 纪律 A |
//! | `tests/integration/lark_integration_test.rs` | 结尾"快照清空"断言 | 纪律 A |
//! | `tests/integration/memory_test.rs` | task_id 过滤精确计数 | 纪律 B |
//! | `tests/integration/message_vector_test.rs` | provider 窗口互相拆除 | 串行锁 |

pub mod app;
pub mod assertions;
pub mod env;
pub mod factories;

/// 集成测试对 common crate 的轻量 re-export（保持测试文件顶部 `use` 简短）。
/// 新增重导出放在这里，避免每个集成测试文件都写一长串 `use common::xxx;`
/// （它们的 common 是本 mod，不是原始 common crate）。
#[allow(unused_imports)]
pub mod types {
    pub use common::enums::OrganizationScope;
    pub use common::enums::UserRole;
}

#[allow(unused_imports)] // 公共 re-export，由各个 integration test 按需引用
pub use app::TestApp;
#[allow(unused_imports)]
pub use assertions::{assert_api_error, assert_api_ok};
pub use env::init_full_test_env;
