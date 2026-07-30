//! 通用后台任务模块（pkg 层注册中心）
//!
//! 定义后台任务生命周期契约（BackgroundTask trait）和注册中心（BackgroundTaskRegistry）。
//! 任务对象自包含：自己持有进度状态字段，run 方法内部更新，外部通过 progress() 读取快照。
//! 任意层均可通过 `registry()` 注册任务。
//!
//! 使用方式：
//! ```ignore
//! // 1. 实现任务对象（自包含进度状态）
//! struct MyTask {
//!     task_id: String,
//!     ctx: RequestContext,
//!     status: Mutex<TaskStatus>,
//!     current_step: AtomicUsize,
//!     // ...
//! }
//! #[async_trait]
//! impl BackgroundTask for MyTask {
//!     fn task_id(&self) -> &str { &self.task_id }
//!     fn task_type(&self) -> TaskType { TaskType::SeedSave }
//!     fn progress(&self) -> TaskProgressSnapshot { /* 从字段读取 */ }
//!     async fn run(self: Arc<Self>) -> Result<serde_json::Value> {
//!         self.set_step(1, "步骤1");
//!         // 调用 domain 基础动作...
//!         self.set_completed(serde_json::json!({"done": true}));
//!         Ok(serde_json::json!({"done": true}))
//!     }
//! }
//!
//! // 2. 注册运行
//! let task = Arc::new(MyTask::new(ctx));
//! let task_id = registry().register(task).await;
//!
//! // 3. 前端轮询 GET /api/v1/system/tasks/{task_id}/progress
//! ```

mod registry;

pub use registry::BackgroundTaskRegistry;

use async_trait::async_trait;
use common::api::{TaskProgressSnapshot, TaskType};
use common::error::Result;
use once_cell::sync::OnceCell;

/// 后台任务生命周期契约
///
/// 实现者负责：
/// - 持有任务进度状态字段（status/step/total/message/result/error）
/// - `progress()` 返回当前状态快照
/// - `run()` 执行业务逻辑，内部更新进度字段，末尾写入 result 并置状态为 Completed
///
/// 注意：`run` 使用 `&self` 而非 `self` 以保证 trait 是 dyn compatible，
/// registry 通过 `Arc<dyn BackgroundTask>` 存储和分发。任务只执行一次的语义
/// 由 registry 的 `register` 方法（spawn 一次）保证。
#[async_trait]
pub trait BackgroundTask: Send + Sync + 'static {
    /// 任务唯一 ID
    fn task_id(&self) -> &str;

    /// 任务类型
    fn task_type(&self) -> TaskType;

    /// 当前进度快照（从对象内部字段读取）
    fn progress(&self) -> TaskProgressSnapshot;

    /// 执行任务
    ///
    /// 任务体内部更新自己的进度字段，末尾写入 result 并置状态为 Completed。
    /// 返回值用于 registry 记录最终结果。
    async fn run(&self) -> Result<serde_json::Value>;
}

static REGISTRY: OnceCell<BackgroundTaskRegistry> = OnceCell::new();

/// 获取全局后台任务注册中心
pub fn registry() -> &'static BackgroundTaskRegistry {
    REGISTRY.get_or_init(BackgroundTaskRegistry::new)
}
