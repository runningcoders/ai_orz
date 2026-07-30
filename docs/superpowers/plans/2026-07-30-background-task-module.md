# 通用后台任务模块 + Seed 异步化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 创建通用后台任务模块（自包含任务对象 + pkg 注册中心 + system domain 统一管理），将现有 initialize_system 和向量重建任务收编统一管理，并将 seed 导出/恢复操作改造成异步任务，提供分阶段进度查询；**现有业务进度查询接口**（`get_initialize_progress` / `get_rebuild_progress`）切换为调用 system domain 获取基础 `TaskProgressSnapshot`，再装饰为各自业务响应 DTO，保持向后兼容。

**Architecture:**
- **pkg 层** 提供 `BackgroundTaskRegistry`（注册中心，全局 HashMap 存 `Arc<dyn BackgroundTask>`）和 `BackgroundTask` trait（自包含任务对象：自己持有进度状态、自己写入 result 并置为成功）。`run(&self)` 签名保证 dyn compatible，任务只执行一次的语义由 registry 的 `register`（spawn 一次）保证。
- **任务对象** 自包含：持有 ctx/params + 进度状态字段（status/step/total/message/result/error），`run` 方法内部更新自己的字段，外部通过 `progress()` 读取快照。
- **system domain** 通过 trait 默认实现暴露 `background_task_registry()`（委托 pkg 全局单例），提供统一进度查询 handler `GET /api/v1/system/tasks/{task_id}/progress` 返回通用 `TaskProgressSnapshot`。
- **装饰模式（关键设计）**：现有业务进度查询接口 `get_initialize_progress` / `get_rebuild_progress` 不再自己维护任务状态，而是调用 `system::domain().background_task_registry()` 获取基础 `TaskProgressSnapshot`，在其上映射业务状态枚举、解析 result JSON，装饰为 `InitProgressResponse` / `RebuildProgressResponse`。这样既统一了任务状态存储，又保持业务接口向后兼容（前端和测试无需改动业务接口契约）。
- **handler 层** 完成跨 domain 编排，包装为任务对象，通过 `pkg::background_task::registry().register(task)` 注册运行。
- **前端** 通用 `TaskProgress` 组件接收 `TaskProgressSnapshot`，seed 管理页面和 reception 页面复用；业务页面也可继续使用装饰后的业务接口。

**Tech Stack:** Rust + tokio + axum + async_trait + once_cell + serde_json + Dioxus (frontend)

---

## File Structure

### 新建文件
- `common/src/api/background_task.rs` — 通用后台任务 API DTO（TaskStatus, TaskType, TaskProgressSnapshot, TaskIdResponse, GetTaskProgressRequest）
- `src/pkg/background_task/mod.rs` — 模块入口（BackgroundTask trait, `registry()` 全局访问）
- `src/pkg/background_task/registry.rs` — BackgroundTaskRegistry 实现（HashMap 存 Arc 引用 + register/get/list/cleanup）
- `src/handlers/system/task_progress.rs` — 统一进度查询 handler（返回通用 `TaskProgressSnapshot`）
- `src/handlers/finance/model_provider/rebuild_vectors_task.rs` — RebuildVectorsTask 任务对象
- `frontend/src/components/task_progress.rs` — 通用进度条组件

### 修改文件
- `common/src/api/mod.rs` — 注册 background_task API 模块
- `src/pkg/mod.rs` — 注册 background_task 模块
- `src/service/domain/system/mod.rs` — SystemDomain trait 加 `background_task_registry()` 默认实现（委托 pkg 全局单例）
- `src/handlers/organization/initialize_system.rs` — 删除 INIT_TASKS static，新增 InitializeSystemTask 任务对象；`get_initialize_progress` 改为调用 system domain 获取基础信息后装饰为 InitProgressResponse
- `src/service/domain/finance/mod.rs` — 删除 RebuildTask 结构和 rebuild_task 字段
- `src/service/domain/finance/model_provider.rs` — 删除 start_rebuild_task/run_rebuild_task，保留各 DAL 的 rebuild_vectors 基础动作
- `src/handlers/finance/model_provider/rebuild_progress.rs` — 改为调用 system domain 获取基础信息后装饰为 RebuildProgressResponse
- `src/handlers/finance/model_provider/mod.rs` — 注册 rebuild_vectors_task 模块
- `src/handlers/system/seed/save.rs` — 新增 SeedSaveTask，改为异步提交
- `src/handlers/system/seed/load.rs` — 新增 SeedLoadTask，改为异步提交
- `src/handlers/system/seed/apply_default.rs` — 新增 SeedApplyDefaultTask，改为异步提交
- `src/handlers/system/seed/mod.rs` — assemble_snapshot_from_db/apply_snapshot_to_db 增加进度回调参数
- `src/handlers/system/mod.rs` — 注册 task_progress handler
- `src/router.rs` — 新增统一进度查询路由
- `frontend/src/api/seed.rs` — 新增异步提交和进度查询 API
- `frontend/src/api/auth.rs` — get_initialize_progress 改用统一接口
- `frontend/src/pages/system/seed.rs` — 集成进度条
- `frontend/src/pages/reception.rs` — 统一使用 TaskProgress 组件
- `frontend/src/components/mod.rs` — 注册 task_progress 组件
- `tests/common/factories/user_factory.rs` — 轮询逻辑保持使用装饰后的业务接口（验证向后兼容）

---

## Task 1: 创建通用后台任务 API DTO

**Files:**
- Create: `common/src/api/background_task.rs`
- Modify: `common/src/api/mod.rs`

- [x] **Step 1: 创建 `common/src/api/background_task.rs`**

```rust
//! 通用后台任务 API DTO
//!
//! 统一管理所有后台异步任务（初始化、向量重建、seed 导出/导入等）的进度查询。

use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 后台任务状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// 等待开始
    Pending,
    /// 运行中
    Running,
    /// 已完成
    Completed,
    /// 已失败
    Failed,
}

/// 后台任务类型标识（前端按此区分展示文案）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    /// 系统初始化
    InitializeSystem,
    /// 向量索引重建
    RebuildVectors,
    /// Seed 导出
    SeedSave,
    /// Seed 导入
    SeedLoad,
    /// 应用默认 Seed
    SeedApplyDefault,
}

impl TaskType {
    /// 转换为字符串标识
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InitializeSystem => "initialize_system",
            Self::RebuildVectors => "rebuild_vectors",
            Self::SeedSave => "seed_save",
            Self::SeedLoad => "seed_load",
            Self::SeedApplyDefault => "seed_apply_default",
        }
    }
}

/// 任务进度快照（从任务对象读取的当前状态）
///
/// 通用接口返回此结构。业务 handler 可在此基础上装饰为各自的响应 DTO。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TaskProgressSnapshot {
    /// 任务 ID
    pub task_id: String,
    /// 任务类型字符串
    pub task_type: String,
    /// 任务状态
    pub status: TaskStatus,
    /// 当前步骤编号（1-based，0 表示尚未开始）
    pub current_step: usize,
    /// 总步骤数
    pub total_steps: usize,
    /// 当前步骤描述（人类可读）
    pub step_message: String,
    /// 开始时间戳（毫秒）
    pub started_at: i64,
    /// 结束时间戳（毫秒，运行中为 None）
    pub finished_at: Option<i64>,
    /// 失败时的错误信息
    pub error: Option<String>,
    /// 任务结果（完成时，JSON 序列化的业务结果）
    pub result: Option<serde_json::Value>,
}

/// 异步提交响应（统一返回 task_id）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TaskIdResponse {
    /// 任务 ID
    pub task_id: String,
}

/// 进度查询请求（task_id 从 path 提取）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetTaskProgressRequest {
    /// 任务 ID
    #[param(source = "path")]
    pub task_id: String,
}
```

- [x] **Step 2: 在 `common/src/api/mod.rs` 注册模块**

添加 `pub mod background_task;` 和 pub use 语句：
```rust
pub use background_task::{
    GetTaskProgressRequest, TaskIdResponse, TaskProgressSnapshot, TaskStatus, TaskType,
};
```

- [x] **Step 3: 验证编译**

Run: `cargo check -p common`
Expected: PASS

- [x] **Step 4: Commit**

```bash
git add common/src/api/background_task.rs common/src/api/mod.rs
git commit -m "feat(common): add generic background task API DTOs"
```

---

## Task 2: 创建 pkg/background_task 模块（trait + registry）

**Files:**
- Create: `src/pkg/background_task/mod.rs`
- Create: `src/pkg/background_task/registry.rs`
- Modify: `src/pkg/mod.rs`

- [x] **Step 1: 创建 `src/pkg/background_task/mod.rs`**

关键点：`run(&self)` 签名（非 `self: Arc<Self>`）保证 trait 是 dyn compatible，registry 通过 `Arc<dyn BackgroundTask>` 存储分发。任务只执行一次的语义由 registry 的 `register`（spawn 一次）保证，任务对象内部用 `Mutex` + `AtomicUsize` 实现可变性。

```rust
//! 通用后台任务模块（pkg 层注册中心）
//!
//! 定义后台任务生命周期契约（BackgroundTask trait）和注册中心（BackgroundTaskRegistry）。
//! 任务对象自包含：自己持有进度状态字段，run 方法内部更新，外部通过 progress() 读取快照。
//! 任意层均可通过 `registry()` 注册任务。

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
```

- [x] **Step 2: 创建 `src/pkg/background_task/registry.rs`**

```rust
//! 后台任务注册中心
//!
//! 全局 HashMap 存 `Arc<dyn BackgroundTask>`，提供注册/查询/列表/清理。
//! 任意层可通过 `registry()` 访问。

use crate::pkg::background_task::BackgroundTask;
use common::api::{TaskProgressSnapshot, TaskStatus, TaskType};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 后台任务注册中心
pub struct BackgroundTaskRegistry {
    tasks: RwLock<HashMap<String, Arc<dyn BackgroundTask>>>,
}

impl BackgroundTaskRegistry {
    /// 创建新的注册中心
    pub fn new() -> Self {
        Self {
            tasks: RwLock::new(HashMap::new()),
        }
    }

    /// 注册任务，spawn 执行，返回 task_id
    pub async fn register(&self, task: Arc<dyn BackgroundTask>) -> String {
        let task_id = task.task_id().to_string();
        {
            let mut guard = self.tasks.write().await;
            guard.insert(task_id.clone(), task.clone());
        }
        let task_clone = task.clone();
        tokio::spawn(async move {
            let result = task_clone.run().await;
            if let Err(e) = result {
                tracing::error!("后台任务 {} 执行失败: {}", task_clone.task_id(), e);
            }
        });
        task_id
    }

    /// 获取任务对象引用
    pub async fn get(&self, task_id: &str) -> Option<Arc<dyn BackgroundTask>> {
        let guard = self.tasks.read().await;
        guard.get(task_id).cloned()
    }

    /// 查询进度快照
    pub async fn get_progress(&self, task_id: &str) -> Option<TaskProgressSnapshot> {
        let guard = self.tasks.read().await;
        guard.get(task_id).map(|t| t.progress())
    }

    /// 列出指定类型的所有任务
    pub async fn list_by_type(&self, task_type: TaskType) -> Vec<Arc<dyn BackgroundTask>> {
        let guard = self.tasks.read().await;
        guard.values().filter(|t| t.task_type() == task_type).cloned().collect()
    }

    /// 列出所有任务
    pub async fn list_all(&self) -> Vec<Arc<dyn BackgroundTask>> {
        let guard = self.tasks.read().await;
        guard.values().cloned().collect()
    }

    /// 列出指定类型的进度快照
    pub async fn list_progress_by_type(&self, task_type: TaskType) -> Vec<TaskProgressSnapshot> {
        let guard = self.tasks.read().await;
        guard.values().filter(|t| t.task_type() == task_type).map(|t| t.progress()).collect()
    }

    /// 清理已完成的旧任务，保留每个类型最近 max_count 个已完成任务
    pub async fn cleanup_finished(&self, max_count: usize) {
        let mut guard = self.tasks.write().await;
        let mut by_type: HashMap<String, Vec<(String, i64)>> = HashMap::new();
        for (id, task) in guard.iter() {
            let p = task.progress();
            if p.status == TaskStatus::Completed || p.status == TaskStatus::Failed {
                let finished = p.finished_at.unwrap_or(0);
                by_type.entry(p.task_type).or_default().push((id.clone(), finished));
            }
        }
        let mut to_remove = Vec::new();
        for (_type, mut list) in by_type {
            list.sort_by(|a, b| b.1.cmp(&a.1));
            for (id, _) in list.into_iter().skip(max_count) {
                to_remove.push(id);
            }
        }
        for id in to_remove {
            guard.remove(&id);
        }
    }
}

impl Default for BackgroundTaskRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```

- [x] **Step 3: 在 `src/pkg/mod.rs` 注册模块**

```rust
pub mod background_task;
```

- [x] **Step 4: 验证编译**

Run: `cargo check --lib`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add src/pkg/background_task/ src/pkg/mod.rs
git commit -m "feat(pkg): add BackgroundTask trait and Registry"
```

---

## Task 3: system domain 暴露 registry + 统一进度查询 handler

**Files:**
- Modify: `src/service/domain/system/mod.rs`
- Create: `src/handlers/system/task_progress.rs`
- Modify: `src/handlers/system/mod.rs`
- Modify: `src/router.rs`

- [x] **Step 1: 在 SystemDomain trait 添加 registry 访问方法（默认实现）**

在 `src/service/domain/system/mod.rs` 中，trait 方法提供默认实现，直接返回 pkg 全局单例。SystemDomainImpl 无需新增字段，各业务层也可直接调用 `pkg::background_task::registry()` 注册任务。

```rust
use crate::pkg::background_task::BackgroundTaskRegistry;

pub trait SystemDomain: Send + Sync {
    fn cron_manager(&self) -> &dyn CronManager;
    fn backup_manager(&self) -> &dyn BackupManager;
    fn log_query(&self) -> &dyn LogQuery;
    fn aop_monitor(&self) -> &dyn AopMonitor;
    fn aop_stats(&self) -> &dyn AopStats;
    /// 通用后台任务注册中心（委托 pkg 全局单例）
    fn background_task_registry(&self) -> &'static BackgroundTaskRegistry {
        crate::pkg::background_task::registry()
    }
}
```

- [x] **Step 2: 创建 `src/handlers/system/task_progress.rs`**

统一进度查询 handler，返回通用 `TaskProgressSnapshot`。业务 handler 可在此基础上装饰为各自的响应 DTO。

```rust
//! 统一后台任务进度查询
//!
//! 所有后台任务共用此接口，前端通过 task_id 查询进度。
//! 业务 handler 可在此基础上装饰为各自的响应 DTO。

use crate::pkg::RequestContext;
use crate::service::domain::system;
use ai_orz_macros::generate_http_handler;
use common::api::{GetTaskProgressRequest, TaskProgressSnapshot};
use common::error::{Error, Result};

#[generate_http_handler]
pub async fn get_task_progress(
    _ctx: RequestContext,
    params: GetTaskProgressRequest,
) -> Result<TaskProgressSnapshot> {
    match system::domain().background_task_registry().get_progress(&params.task_id).await {
        Some(snapshot) => Ok(snapshot),
        None => Err(Error::not_found(format!("任务不存在: {}", params.task_id))),
    }
}
```

- [x] **Step 3: 在 `src/handlers/system/mod.rs` 注册模块**

```rust
pub mod task_progress;
```

- [x] **Step 4: 在 `src/router.rs` 的 `system_routes()` 中添加路由**

在 seed routes 之后添加：
```rust
.route(
    "/tasks/{task_id}/progress",
    get(handlers::system::task_progress::get_task_progress_handler),
)
```

- [x] **Step 5: 验证编译**

Run: `cargo check --lib --bin ai_orz`
Expected: PASS

- [x] **Step 6: Commit**

```bash
git add src/service/domain/system/mod.rs src/handlers/system/task_progress.rs src/handlers/system/mod.rs src/router.rs
git commit -m "feat(system): add unified task progress query endpoint"
```

---

## Task 4: 迁移 initialize_system 到通用模块 + 装饰现有进度接口

**Files:**
- Modify: `src/handlers/organization/initialize_system.rs`

**关键设计（装饰模式）：** `get_initialize_progress` 不再自己维护任务状态，而是调用 `system::domain().background_task_registry().get_progress()` 获取基础 `TaskProgressSnapshot`，然后：
1. 将 `TaskStatus` 映射为业务 `InitStatus`
2. 解析 `result` JSON 字段为 `InitializeSystemResponse`
3. 包装为 `InitProgressResponse` 返回

这样前端和测试工厂仍可使用原有的 `/organization/initialize/progress?task_id=` 接口，无需改动业务接口契约。

- [x] **Step 1: 改造 initialize_system.rs**

删除：
- `InitTaskState` struct 及 impl
- `INIT_TASKS` static 和 `init_task_store()` 函数
- `run_initialize_task` 和 `update_progress` 辅助函数

新增 `InitializeSystemTask` 自包含任务对象：

```rust
pub struct InitializeSystemTask {
    task_id: String,
    ctx: RequestContext,
    params: InitializeSystemRequest,
    // 进度状态字段（任务体内部更新，外部通过 progress() 读取）
    status: Mutex<TaskStatus>,
    current_step: AtomicUsize,
    total_steps: usize,
    step_message: Mutex<String>,
    started_at: i64,
    finished_at: Mutex<Option<i64>>,
    error: Mutex<Option<String>>,
    result: Mutex<Option<serde_json::Value>>,
}

impl InitializeSystemTask {
    pub fn new(ctx: RequestContext, params: InitializeSystemRequest) -> Self {
        let total_steps = if params.embedding_model.is_some() { 5 } else { 4 };
        Self {
            task_id: uuid::Uuid::new_v4().to_string(),
            ctx, params,
            status: Mutex::new(TaskStatus::Pending),
            current_step: AtomicUsize::new(0),
            total_steps,
            step_message: Mutex::new("等待开始".to_string()),
            started_at: chrono::Utc::now().timestamp_millis(),
            finished_at: Mutex::new(None),
            error: Mutex::new(None),
            result: Mutex::new(None),
        }
    }

    fn set_step(&self, step: usize, message: &str) {
        self.current_step.store(step, Ordering::SeqCst);
        *self.step_message.lock().unwrap() = message.to_string();
    }

    fn set_completed(&self, result: serde_json::Value) {
        *self.status.lock().unwrap() = TaskStatus::Completed;
        *self.finished_at.lock().unwrap() = Some(chrono::Utc::now().timestamp_millis());
        *self.step_message.lock().unwrap() = "初始化完成".to_string();
        *self.result.lock().unwrap() = Some(result);
    }

    fn set_failed(&self, error: String) {
        *self.status.lock().unwrap() = TaskStatus::Failed;
        *self.finished_at.lock().unwrap() = Some(chrono::Utc::now().timestamp_millis());
        *self.step_message.lock().unwrap() = "初始化失败".to_string();
        *self.error.lock().unwrap() = Some(error);
    }
}

#[async_trait]
impl BackgroundTask for InitializeSystemTask {
    fn task_id(&self) -> &str { &self.task_id }
    fn task_type(&self) -> TaskType { TaskType::InitializeSystem }

    fn progress(&self) -> TaskProgressSnapshot {
        TaskProgressSnapshot {
            task_id: self.task_id.clone(),
            task_type: self.task_type().as_str().to_string(),
            status: *self.status.lock().unwrap(),
            current_step: self.current_step.load(Ordering::SeqCst),
            total_steps: self.total_steps,
            step_message: self.step_message.lock().unwrap().clone(),
            started_at: self.started_at,
            finished_at: *self.finished_at.lock().unwrap(),
            error: self.error.lock().unwrap().clone(),
            result: self.result.lock().unwrap().clone(),
        }
    }

    async fn run(&self) -> Result<serde_json::Value> {
        *self.status.lock().unwrap() = TaskStatus::Running;
        self.set_step(1, "正在创建组织和超级管理员");
        match self.run_steps().await {
            Ok(resp) => {
                let v = serde_json::to_value(&resp)
                    .map_err(|e| Error::internal(format!("序列化结果失败: {}", e)))?;
                self.set_completed(v.clone());
                Ok(v)
            }
            Err(e) => {
                self.set_failed(e.to_string());
                Err(e)
            }
        }
    }
}
```

`run_steps` 方法从原 `run_initialize_steps` 迁移，每步通过 `set_step` 更新进度：创建组织+Owner → chat provider → embedding provider（可选）→ 同步内置工具 → 导入预置技能。

- [x] **Step 2: 改造 `initialize_system` handler**

```rust
#[generate_http_handler]
pub async fn initialize_system(
    ctx: RequestContext,
    params: InitializeSystemRequest,
) -> Result<TaskIdResponse> {
    let task = Arc::new(InitializeSystemTask::new(ctx, params));
    let task_id = registry().register(task).await;
    Ok(TaskIdResponse { task_id })
}
```

- [x] **Step 3: 改造 `get_initialize_progress` handler（装饰模式核心）**

调用 system domain 获取基础 `TaskProgressSnapshot`，装饰为 `InitProgressResponse`：

```rust
#[generate_http_handler]
pub async fn get_initialize_progress(
    _ctx: RequestContext,
    params: GetInitProgressRequest,
) -> Result<InitProgressResponse> {
    // 1. 从 system domain 获取基础任务信息
    let snapshot = system::domain()
        .background_task_registry()
        .get_progress(&params.task_id)
        .await
        .ok_or_else(|| Error::not_found(format!("初始化任务不存在: {}", params.task_id)))?;

    // 2. 装饰为业务响应 DTO（状态映射 + result 解析）
    Ok(InitProgressResponse {
        task_id: snapshot.task_id,
        status: match snapshot.status {
            TaskStatus::Pending => InitStatus::Pending,
            TaskStatus::Running => InitStatus::Running,
            TaskStatus::Completed => InitStatus::Completed,
            TaskStatus::Failed => InitStatus::Failed,
        },
        current_step: snapshot.current_step,
        total_steps: snapshot.total_steps,
        step_message: snapshot.step_message,
        started_at: snapshot.started_at,
        finished_at: snapshot.finished_at,
        error: snapshot.error,
        result: snapshot.result.and_then(|v| serde_json::from_value(v).ok()),
    })
}
```

- [x] **Step 4: 验证编译**

Run: `cargo check --lib --bin ai_orz`
Expected: PASS

- [x] **Step 5: 运行现有测试（验证装饰模式向后兼容）**

Run: `cargo test --test preset_skills_test`
Expected: 4 passed（测试工厂 `poll_initialize_progress` 继续使用 `/organization/initialize/progress` 业务接口，装饰模式保证契约不变）

- [x] **Step 6: Commit**

```bash
git add src/handlers/organization/initialize_system.rs
git commit -m "refactor(initialize_system): migrate to self-contained BackgroundTask with decorator pattern"
```

---

## Task 5: 迁移向量重建到通用模块 + 装饰现有进度接口

**Files:**
- Modify: `src/service/domain/finance/mod.rs`
- Modify: `src/service/domain/finance/model_provider.rs`
- Modify: `src/handlers/finance/model_provider/rebuild_progress.rs`
- Create: `src/handlers/finance/model_provider/rebuild_vectors_task.rs`
- Modify: `src/handlers/finance/model_provider/mod.rs`

**关键设计（装饰模式）：** `get_rebuild_progress` 改为调用 `system::domain().background_task_registry().list_progress_by_type(TaskType::RebuildVectors)` 获取最近任务快照，装饰为 `RebuildProgressResponse`。

- [x] **Step 1: 在 finance domain 删除 RebuildTask 结构**

在 `src/service/domain/finance/mod.rs` 中：
- 删除 `RebuildTask` struct
- 删除 `FinanceDomainImpl.rebuild_task` 字段
- 删除 `ModelProviderManage::get_rebuild_progress` trait 方法
- 删除 `start_rebuild_task` 和 `run_rebuild_task` 方法

保留各 DAL 的 `rebuild_vectors(ctx)` 方法（这是 domain 层的基础动作）。

- [x] **Step 2: 创建 RebuildVectorsTask 任务对象（handler 层）**

`src/handlers/finance/model_provider/rebuild_vectors_task.rs`：自包含任务对象，`run` 方法遍历 7 个实体调用各 DAL 的 `rebuild_vectors(ctx)`，并发检查（同一时刻仅允许一个 RebuildVectors 任务运行）。

```rust
pub struct RebuildVectorsTask {
    task_id: String,
    ctx: RequestContext,
    status: Mutex<TaskStatus>,
    current_step: AtomicUsize,
    total_steps: usize,  // 7
    step_message: Mutex<String>,
    started_at: i64,
    finished_at: Mutex<Option<i64>>,
    error: Mutex<Option<String>>,
    result: Mutex<Option<serde_json::Value>>,
}

#[async_trait]
impl BackgroundTask for RebuildVectorsTask {
    fn task_id(&self) -> &str { &self.task_id }
    fn task_type(&self) -> TaskType { TaskType::RebuildVectors }
    fn progress(&self) -> TaskProgressSnapshot { /* 从字段读取 */ }

    async fn run(&self) -> Result<serde_json::Value> {
        // 并发检查（向量重建独占）
        let existing = crate::pkg::background_task::registry()
            .list_progress_by_type(TaskType::RebuildVectors).await;
        for p in existing {
            if p.status == TaskStatus::Running && p.task_id != self.task_id {
                return Err(Error::conflict(format!(
                    "向量重建任务正在执行中（task_id={}），请等待完成", p.task_id
                )));
            }
        }

        *self.status.lock().unwrap() = TaskStatus::Running;

        let entities: [(&str, &str); 7] = [
            ("agent", "Agent"), ("memory", "Memory"), ("skill", "Skill"),
            ("task", "Task"), ("project", "Project"), ("message", "Message"), ("tool", "Tool"),
        ];

        // 遍历调用 dal::agent::dal().rebuild_vectors(ctx) 等
        // 每步 set_step(i+1, &format!("正在重建 {} 向量索引", label))
        // 完成后 set_completed(json!({"rebuilt": true, "entities": 7}))
        // 失败时 set_failed(e.to_string())
    }
}
```

- [x] **Step 3: 改造 switch_embedding_provider 调用方式**

在 `switch_embedding_provider` handler 中，原来调用 `finance::domain().model_provider_manage().start_rebuild_task(ctx)` 的地方，改为：
```rust
use crate::handlers::finance::model_provider::rebuild_vectors_task::RebuildVectorsTask;
use crate::pkg::background_task::registry;
use std::sync::Arc;

let task = Arc::new(RebuildVectorsTask::new(ctx));
let _task_id = registry().register(task).await;
```

- [x] **Step 4: 改造 `get_rebuild_progress` handler（装饰模式核心）**

调用 system domain 获取基础任务信息，装饰为 `RebuildProgressResponse`：

```rust
#[generate_http_handler]
pub async fn get_rebuild_progress(
    _ctx: RequestContext,
    _params: GetRebuildProgressRequest,
) -> Result<RebuildProgressResponse> {
    // 1. 从 system domain 获取最近一个 RebuildVectors 任务的基础信息
    let snapshots = system::domain()
        .background_task_registry()
        .list_progress_by_type(TaskType::RebuildVectors)
        .await;

    let snapshot = snapshots
        .into_iter()
        .max_by_key(|p| p.started_at)
        .ok_or_else(|| Error::not_found("没有向量重建任务"))?;

    // 2. 装饰为业务响应 DTO
    Ok(RebuildProgressResponse {
        task_id: snapshot.task_id,
        status: match snapshot.status {
            TaskStatus::Pending => RebuildStatus::Pending,
            TaskStatus::Running => RebuildStatus::Running,
            TaskStatus::Completed => RebuildStatus::Completed,
            TaskStatus::Failed => RebuildStatus::Failed,
        },
        current_entity: Some(snapshot.step_message),
        current_entity_index: snapshot.current_step,
        total_entities: snapshot.total_steps,
        processed_records: 0,
        total_records: 0,
        started_at: snapshot.started_at,
        finished_at: snapshot.finished_at,
        error: snapshot.error,
    })
}
```

- [x] **Step 5: 在 `src/handlers/finance/model_provider/mod.rs` 注册模块**

```rust
pub mod rebuild_vectors_task;
```

- [x] **Step 6: 验证编译和测试**

Run: `cargo check --lib --bin ai_orz && cargo test --test preset_skills_test`
Expected: PASS

- [x] **Step 7: Commit**

```bash
git add src/service/domain/finance/ src/handlers/finance/model_provider/
git commit -m "refactor(finance): migrate rebuild_vectors to self-contained BackgroundTask with decorator pattern"
```

---

## Task 6: 改造 seed save_seed 为异步

**Files:**
- Modify: `src/handlers/system/seed/save.rs`
- Modify: `src/handlers/system/seed/mod.rs`

- [x] **Step 1: 在 save.rs 新增 SeedSaveTask 任务对象**

实现 `BackgroundTask` trait，`run` 方法调用 `assemble_snapshot_from_db_with_progress`，在各阶段更新进度。导出阶段：组织(1) + 用户(2) + Provider(3) + Agent(4) + Skill(5) + 写文件(6)，共 6 步。

```rust
pub struct SeedSaveTask {
    task_id: String,
    ctx: RequestContext,
    params: SaveSeedRequest,
    status: Mutex<TaskStatus>,
    current_step: AtomicUsize,
    total_steps: usize,  // 6
    step_message: Mutex<String>,
    started_at: i64,
    finished_at: Mutex<Option<i64>>,
    error: Mutex<Option<String>>,
    result: Mutex<Option<serde_json::Value>>,
}

#[async_trait]
impl BackgroundTask for SeedSaveTask {
    fn task_id(&self) -> &str { &self.task_id }
    fn task_type(&self) -> TaskType { TaskType::SeedSave }
    fn progress(&self) -> TaskProgressSnapshot { /* 从字段读取 */ }

    async fn run(&self) -> Result<serde_json::Value> {
        *self.status.lock().unwrap() = TaskStatus::Running;
        // Step 1-5: assemble_snapshot_from_db_with_progress（进度回调更新 step）
        // Step 6: 写入文件
        // set_completed / set_failed
    }
}
```

- [x] **Step 2: 改造 save_seed handler**

```rust
#[generate_http_handler]
pub async fn save_seed(ctx: RequestContext, params: SaveSeedRequest) -> Result<TaskIdResponse> {
    super::check_super_admin(&ctx)?;
    let task = Arc::new(SeedSaveTask::new(ctx, params));
    let task_id = registry().register(task).await;
    Ok(TaskIdResponse { task_id })
}
```

- [x] **Step 3: 在 mod.rs 添加 assemble_snapshot_from_db_with_progress**

基于现有 `assemble_snapshot_from_db`，接收 `&dyn Fn(usize, &str) + Send + Sync` 进度回调参数，在各阶段调用 `progress(step, message)`。

- [x] **Step 4: 验证编译**

Run: `cargo check --lib --bin ai_orz`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add src/handlers/system/seed/
git commit -m "feat(seed): make save_seed async with progress tracking"
```

---

## Task 7: 改造 seed load_seed 为异步

**Files:**
- Modify: `src/handlers/system/seed/load.rs`
- Modify: `src/handlers/system/seed/mod.rs`

- [x] **Step 1: 新增 SeedLoadTask 任务对象**

类似 SeedSaveTask，`run` 方法读文件 → 解析快照 → 调用 `apply_snapshot_to_db_with_progress`，在 user/provider/agent/skill 各阶段更新进度（4 步）。DryRun 也走异步。

- [x] **Step 2: 改造 load_seed handler**

```rust
#[generate_http_handler]
pub async fn load_seed(ctx: RequestContext, params: LoadSeedRequest) -> Result<TaskIdResponse> {
    super::check_super_admin(&ctx)?;
    let task = Arc::new(SeedLoadTask::new(ctx, params));
    let task_id = registry().register(task).await;
    Ok(TaskIdResponse { task_id })
}
```

- [x] **Step 3: 在 mod.rs 添加 apply_snapshot_to_db_with_progress**

基于现有 `apply_snapshot_to_db`，接收进度回调参数，在各阶段调用。

- [x] **Step 4: 验证编译**

Run: `cargo check --lib --bin ai_orz`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add src/handlers/system/seed/
git commit -m "feat(seed): make load_seed async with progress tracking"
```

---

## Task 8: 改造 seed apply_default 为异步

**Files:**
- Modify: `src/handlers/system/seed/apply_default.rs`

- [x] **Step 1: 新增 SeedApplyDefaultTask 任务对象**

类似 SeedLoadTask，但 snapshot 来源是 `embedded_default_snapshot()`。

- [x] **Step 2: 改造 apply_default handler**

```rust
#[generate_http_handler]
pub async fn apply_default(ctx: RequestContext, params: ApplyDefaultSeedRequest) -> Result<TaskIdResponse> {
    super::check_super_admin(&ctx)?;
    let task = Arc::new(SeedApplyDefaultTask::new(ctx, params));
    let task_id = registry().register(task).await;
    Ok(TaskIdResponse { task_id })
}
```

- [x] **Step 3: 验证编译**

Run: `cargo check --lib --bin ai_orz`
Expected: PASS

- [x] **Step 4: Commit**

```bash
git add src/handlers/system/seed/
git commit -m "feat(seed): make apply_default async with progress tracking"
```

---

## Task 9: 前端通用进度条组件

**Files:**
- Create: `frontend/src/components/task_progress.rs`
- Modify: `frontend/src/components/mod.rs`

- [x] **Step 1: 创建 `frontend/src/components/task_progress.rs`**

通用进度条组件，接收 `TaskProgressSnapshot`，显示 spinner/失败图标/完成图标、步骤描述、进度百分比、错误信息。复用 index.html 中已定义的 `.init-progress-*` 样式。

```rust
#[derive(Props, Clone, PartialEq)]
pub struct TaskProgressProps {
    pub progress: TaskProgressSnapshot,
    #[props(default = None)]
    pub on_cancel: Option<EventHandler<()>>,
}

#[component]
pub fn TaskProgress(props: TaskProgressProps) -> Element {
    let p = &props.progress;
    let pct = if p.total_steps > 0 {
        (p.current_step as f64 / p.total_steps as f64 * 100.0) as usize
    } else { 0 };
    let is_failed = p.status == TaskStatus::Failed;
    let is_running = matches!(p.status, TaskStatus::Pending | TaskStatus::Running);
    let is_completed = p.status == TaskStatus::Completed;

    rsx! {
        div { class: "init-progress-container",
            if is_failed { div { class: "init-progress-icon failed", "✗" } }
            else if is_running { div { class: "init-progress-spinner" } }
            else if is_completed { div { /* ✓ 完成图标 */ } }

            h3 { class: "init-progress-title",
                if is_failed { "任务失败" }
                else if is_completed { "任务完成" }
                else { "正在执行..." }
            }
            p { class: "init-progress-step", "{p.step_message}" }
            p { class: "init-progress-count", "步骤 {p.current_step} / {p.total_steps}" }
            progress { class: "progress progress-primary w-full", value: "{pct}", max: "100" }

            if is_failed {
                if let Some(err) = &p.error { p { class: "init-progress-error", "{err}" } }
                if let Some(on_cancel) = &props.on_cancel {
                    button { class: "btn btn-outline btn-sm mt-4",
                        onclick: move |_| on_cancel.call(()),
                        "返回"
                    }
                }
            }
        }
    }
}
```

- [x] **Step 2: 在 `frontend/src/components/mod.rs` 注册**

```rust
pub mod task_progress;
```

- [x] **Step 3: 验证编译**

Run: `cd frontend && cargo check`
Expected: PASS

- [x] **Step 4: Commit**

```bash
git add frontend/src/components/
git commit -m "feat(frontend): add reusable TaskProgress component"
```

---

## Task 10: 前端 seed 管理页面 + reception 页面集成进度条

**Files:**
- Modify: `frontend/src/api/seed.rs`
- Modify: `frontend/src/api/auth.rs`
- Modify: `frontend/src/pages/system/seed.rs`
- Modify: `frontend/src/pages/reception.rs`

- [x] **Step 1: 更新前端 API**

- `frontend/src/api/seed.rs`：save_seed/load_seed/apply_default 返回 `TaskIdResponse`，新增 `get_task_progress(task_id)` 调用 `GET /api/v1/system/tasks/{task_id}/progress`
- `frontend/src/api/auth.rs`：`get_initialize_progress` 改用统一接口 `GET /api/v1/system/tasks/{task_id}/progress`

- [x] **Step 2: seed 管理页面集成进度条**

保存/加载/应用默认操作改为提交后轮询进度，使用 `TaskProgress` 组件展示。

- [x] **Step 3: reception 页面使用 TaskProgress 组件**

替换 reception.rs 中的自定义进度条 UI，改用 `TaskProgress` 组件。

- [x] **Step 4: 验证编译**

Run: `cd frontend && cargo check`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add frontend/src/
git commit -m "feat(frontend): integrate TaskProgress into seed and reception pages"
```

---

## Task 11: 验证测试工厂（装饰模式向后兼容）

**Files:**
- Modify: `tests/common/factories/user_factory.rs`

**关键设计：** 测试工厂 `poll_initialize_progress` 继续使用装饰后的业务接口 `/organization/initialize/progress?task_id=`，验证装饰模式的向后兼容性（业务接口契约不变，前端和测试无需改动）。

- [x] **Step 1: 确认测试工厂轮询逻辑**

`poll_initialize_progress` 保持使用 `/api/v1/organization/initialize/progress?task_id={}` 接口，解析装饰后的 `InitProgressResponse`（包含 `status` 和 `result` 字段）。由于装饰模式保证业务接口契约不变，测试工厂逻辑无需改动。

```rust
async fn poll_initialize_progress(app: &TestApp, task_id: &str) -> serde_json::Value {
    loop {
        let (status, body) = app
            .get(&format!("/api/v1/organization/initialize/progress?task_id={}", task_id))
            .await;
        let data = crate::common::assert_api_ok(status, &body);
        let status_str = data.get("status").and_then(|v| v.as_str()).expect("missing status");
        match status_str {
            "completed" => return data.get("result").expect("missing result").clone(),
            "failed" => panic!("系统初始化失败: {}", /* ... */),
            _ => tokio::time::sleep(std::time::Duration::from_millis(100)).await,
        }
    }
}
```

- [x] **Step 2: 运行测试**

Run: `cargo test --test preset_skills_test`
Expected: 4 passed（装饰模式保证业务接口向后兼容）

- [x] **Step 3: Commit（如有改动）**

```bash
git add tests/common/factories/user_factory.rs
git commit -m "test: verify decorator pattern backward compatibility"
```

---

## Task 12: cargo fmt + 最终验证 + 提交

- [x] **Step 1: 格式化**

Run: `cargo fmt --all && cd frontend && cargo fmt && cd ..`

- [x] **Step 2: 编译验证**

Run: `cargo check --all && cd frontend && cargo check`
Expected: PASS

- [x] **Step 3: 测试验证**

Run: `cargo test --test preset_skills_test`
Expected: 4 passed

- [x] **Step 4: Commit**

```bash
git add -A
git commit -m "chore: fmt + verify background task module"
```

---

## Self-Review Notes

### 设计决策记录

1. **自包含任务对象**：任务对象自己持有进度状态字段（status/step/total/message/result/error），`run` 方法内部更新自己的字段。外部通过 `progress()` 读取快照。避免外部 store 回写。

2. **注册中心放 pkg 层**：`BackgroundTaskRegistry` 在 `pkg/background_task/`，任意层可通过 `registry()` 注册。system domain 通过 trait 默认实现暴露 `background_task_registry()` 方法，但实际是委托 pkg 全局单例。

3. **system domain 提供统一管理 handler**：`GET /api/v1/system/tasks/{task_id}/progress` 返回通用 `TaskProgressSnapshot`。

4. **装饰模式（核心设计）**：
   - `get_initialize_progress` 调用 `system::domain().background_task_registry().get_progress()` 获取 `TaskProgressSnapshot`，装饰为 `InitProgressResponse`（映射 `TaskStatus` → `InitStatus`，解析 `result` JSON）
   - `get_rebuild_progress` 调用 `list_progress_by_type(RebuildVectors)` 获取最近任务，装饰为 `RebuildProgressResponse`
   - 装饰模式保证业务接口契约不变，前端和测试工厂无需改动业务接口调用

5. **保留已完成任务，最大数量清理**：`cleanup_finished(max_count)` 保留每个 task_type 最近 max_count 个已完成任务。

6. **registry 提供对象引用访问**：`get(task_id) -> Arc<dyn BackgroundTask>`、`list_by_type`、`list_all`，便于未来扩展管理操作（暂停/恢复/取消）。

7. **trait 预留扩展点**：`BackgroundTask` trait 当前只定义基础方法，未来可加 `pause()` / `resume()` / `cancel()` 带默认实现的可选方法。

8. **trait 加 Sync 约束**：因为 registry 持有 `Arc<dyn BackgroundTask>` 并发访问，需要 `Send + Sync`。

9. **统一时间单位为毫秒**：`chrono::Utc::now().timestamp_millis()`，与前端 JS `Date.now()` 一致。

10. **DryRun 也走异步**：为前端逻辑统一（所有 seed 操作都轮询进度），DryRun 虽快但前端轮询很快得到 Completed。

11. **`run(&self)` 而非 `run(self: Arc<Self>)`**：保证 trait 是 dyn compatible，registry 通过 `Arc<dyn BackgroundTask>` 存储分发。任务只执行一次的语义由 `register`（spawn 一次）保证，任务对象内部用 `Mutex` + `AtomicUsize` 实现可变性。

### 装饰模式的优势

- **向后兼容**：现有业务接口（`/organization/initialize/progress`、`/finance/model-providers/rebuild-progress`）契约不变，前端和测试无需改动
- **统一存储**：任务状态统一存储在 pkg 注册中心，避免分散在各个 handler 的 static 变量中
- **灵活查询**：既可通过统一接口 `GET /api/v1/system/tasks/{task_id}/progress` 查询通用快照，也可通过业务接口获取装饰后的业务响应
- **渐进迁移**：前端可逐步从业务接口迁移到统一接口，两者并存

### 潜在风险

- 任务对象的进度字段用 `Mutex` + `AtomicUsize`，`progress()` 读取时需要多次 lock。如果性能成为问题，可考虑用 `RwLock<TaskProgressSnapshot>` 整体替换。
- `registry()` 全局单例通过 `once_cell::OnceCell` 初始化，测试环境共享同一实例。测试工厂的 `BOOTSTRAP_MUTEX` 仍需保留以串行化 bootstrap。
