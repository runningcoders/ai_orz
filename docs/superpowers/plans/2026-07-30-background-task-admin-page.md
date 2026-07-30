# 后台任务管理页面 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为通用后台任务模块增加管理页面，支持列表查看（按类型/状态筛选 + 客户端分页）、查看任务详情、清理已完成任务、自动刷新列表。取消功能后续再扩展。

**Architecture:**
- **后端**：registry 增加 `list_all_progress()` 方法；新增 2 个 handler（list_tasks、cleanup_tasks）；无需改动现有任务对象和 trait。
- **前端**：新增 `/system/tasks` 页面，列表展示所有任务快照，支持按类型/状态筛选 + 客户端分页，使用 `use_future` 实现 3 秒自动轮询（组件卸载自动取消，筛选变化自动重启），点击任务行弹窗查看详情（result/error），清理已完成任务按钮。

**Tech Stack:** Rust + tokio + axum + Dioxus (frontend)

---

## File Structure

### 新建文件
- `src/handlers/system/task_list.rs` — list_tasks handler
- `src/handlers/system/task_cleanup.rs` — cleanup_tasks handler
- `frontend/src/pages/system/tasks.rs` — 后台任务管理页面
- `frontend/src/api/background_task.rs` — 后台任务管理 API 封装

### 修改文件
- `common/src/api/background_task.rs` — 新增 ListTasksRequest/Response、CleanupTasksRequest/Response
- `common/src/api/mod.rs` — re-export 新增 DTO
- `src/pkg/background_task/registry.rs` — 增加 list_all_progress 方法
- `src/handlers/system/mod.rs` — 注册 task_list、task_cleanup 模块
- `src/router.rs` — 新增 list/cleanup 路由
- `frontend/src/api/mod.rs` — 注册 background_task API 模块
- `frontend/src/pages/system/mod.rs` — 注册 tasks 页面模块
- `frontend/src/pages/mod.rs` — 注册 SystemTasks 路由
- `frontend/src/layouts/navbar.rs` — 系统管理菜单添加"后台任务"链接

---

## Task 1: 新增列表/清理 API DTO

**Files:**
- Modify: `common/src/api/background_task.rs`
- Modify: `common/src/api/mod.rs`

- [ ] **Step 1: 在 `common/src/api/background_task.rs` 末尾新增 DTO**

```rust
/// 后台任务列表查询请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListTasksRequest {
    /// 按任务类型筛选（可选，字符串匹配 task_type 字段）
    #[param(source = "query")]
    pub task_type: Option<String>,
    /// 按状态筛选（可选）
    #[param(source = "query")]
    pub status: Option<TaskStatus>,
}

/// 后台任务列表响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListTasksResponse {
    /// 任务进度快照列表（按 started_at 降序）
    pub tasks: Vec<TaskProgressSnapshot>,
    /// 总数
    pub total: usize,
}

/// 清理已完成任务请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct CleanupTasksRequest {
    /// 每个类型保留的最近已完成任务数量（默认 10）
    #[param(source = "query")]
    pub max_count: Option<usize>,
}

/// 清理已完成任务响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CleanupTasksResponse {
    /// 清理的任务数量
    pub cleaned: usize,
}
```

- [ ] **Step 2: 在 `common/src/api/mod.rs` re-export 新增 DTO**

更新现有的 `pub use background_task::{...}` 行，追加新类型：

```rust
pub use background_task::{
    CleanupTasksRequest, CleanupTasksResponse, GetTaskProgressRequest, ListTasksRequest,
    ListTasksResponse, TaskIdResponse, TaskProgressSnapshot, TaskStatus, TaskType,
};
```

- [ ] **Step 3: 验证编译**

Run: `cargo check -p common`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add common/src/api/background_task.rs common/src/api/mod.rs
git commit -m "feat(common): add task list/cleanup DTOs"
```

---

## Task 2: registry 增加 list_all_progress 方法

**Files:**
- Modify: `src/pkg/background_task/registry.rs`

- [ ] **Step 1: 在 `impl BackgroundTaskRegistry` 中追加 list_all_progress 方法**

```rust
    /// 列出所有任务的进度快照
    ///
    /// 遍历注册中心中所有任务，调用 `progress()` 获取快照。
    /// 任务数量不大时性能可接受。
    pub async fn list_all_progress(&self) -> Vec<TaskProgressSnapshot> {
        let guard = self.tasks.read().await;
        guard.values().map(|t| t.progress()).collect()
    }
```

- [ ] **Step 2: 验证编译**

Run: `cargo check --lib`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/pkg/background_task/registry.rs
git commit -m "feat(pkg): add list_all_progress to BackgroundTaskRegistry"
```

---

## Task 3: 后端 handler + 路由注册

**Files:**
- Create: `src/handlers/system/task_list.rs`
- Create: `src/handlers/system/task_cleanup.rs`
- Modify: `src/handlers/system/mod.rs`
- Modify: `src/router.rs`

- [ ] **Step 1: 创建 `src/handlers/system/task_list.rs`**

```rust
//! GET /api/v1/system/tasks - 列出所有后台任务
//!
//! 支持按 task_type 和 status 筛选。返回 TaskProgressSnapshot 列表（按 started_at 降序）。
//! 客户端分页：后端返回全部匹配任务，前端自行分页（任务数量通常不大）。

use crate::pkg::RequestContext;
use crate::service::domain::system;
use ai_orz_macros::generate_http_handler;
use common::api::{ListTasksRequest, ListTasksResponse};
use common::error::Result;

#[generate_http_handler]
pub async fn list_tasks(
    _ctx: RequestContext,
    params: ListTasksRequest,
) -> Result<ListTasksResponse> {
    let mut snapshots = system::domain()
        .background_task_registry()
        .list_all_progress()
        .await;

    // 按 task_type 筛选（字符串匹配）
    if let Some(ref task_type_str) = params.task_type {
        snapshots.retain(|s| &s.task_type == task_type_str);
    }

    // 按 status 筛选
    if let Some(status) = params.status {
        snapshots.retain(|s| s.status == status);
    }

    // 按 started_at 降序排序（最新的在前）
    snapshots.sort_by(|a, b| b.started_at.cmp(&a.started_at));

    let total = snapshots.len();
    Ok(ListTasksResponse {
        tasks: snapshots,
        total,
    })
}
```

- [ ] **Step 2: 创建 `src/handlers/system/task_cleanup.rs`**

```rust
//! POST /api/v1/system/tasks/cleanup - 清理已完成的旧任务
//!
//! 保留每个 task_type 最近 max_count 个已完成/失败的任务，其余移除。
//! 运行中或等待中的任务不受影响。

use crate::pkg::RequestContext;
use crate::service::domain::system;
use ai_orz_macros::generate_http_handler;
use common::api::{CleanupTasksRequest, CleanupTasksResponse, TaskStatus};
use common::error::Result;

#[generate_http_handler]
pub async fn cleanup_tasks(
    _ctx: RequestContext,
    params: CleanupTasksRequest,
) -> Result<CleanupTasksResponse> {
    let max_count = params.max_count.unwrap_or(10);

    // 清理前统计已完成/失败的任务数量
    let before = system::domain()
        .background_task_registry()
        .list_all_progress()
        .await;
    let before_count = before
        .iter()
        .filter(|p| {
            p.status == TaskStatus::Completed || p.status == TaskStatus::Failed
        })
        .count();

    // 执行清理
    system::domain()
        .background_task_registry()
        .cleanup_finished(max_count)
        .await;

    // 清理后统计
    let after = system::domain()
        .background_task_registry()
        .list_all_progress()
        .await;
    let after_count = after
        .iter()
        .filter(|p| {
            p.status == TaskStatus::Completed || p.status == TaskStatus::Failed
        })
        .count();

    let cleaned = before_count.saturating_sub(after_count);
    Ok(CleanupTasksResponse { cleaned })
}
```

- [ ] **Step 3: 在 `src/handlers/system/mod.rs` 注册新模块**

在现有 `pub mod task_progress;` 附近添加：

```rust
pub mod task_cleanup;
pub mod task_list;
```

- [ ] **Step 4: 在 `src/router.rs` 的 `system_routes()` 中添加路由**

在现有的 `/tasks/{task_id}/progress` 路由之后添加：

```rust
// 后台任务管理
.route(
    "/tasks",
    get(handlers::system::task_list::list_tasks_handler),
)
.route(
    "/tasks/cleanup",
    post(handlers::system::task_cleanup::cleanup_tasks_handler),
)
```

- [ ] **Step 5: 验证编译**

Run: `cargo check --lib --bin ai_orz`
Expected: PASS

- [ ] **Step 6: 运行测试**

Run: `cargo test --test preset_skills_test`
Expected: 4 passed

- [ ] **Step 7: Commit**

```bash
git add src/handlers/system/ src/router.rs
git commit -m "feat(system): add task list/cleanup handlers"
```

---

## Task 4: 前端 API 封装

**Files:**
- Create: `frontend/src/api/background_task.rs`
- Modify: `frontend/src/api/mod.rs`

- [ ] **Step 1: 创建 `frontend/src/api/background_task.rs`**

```rust
//! 后台任务管理 API
//!
//! 封装后台任务列表查询、清理、进度查询接口。

use crate::api::{api_get, api_post};
use common::api::{
    CleanupTasksResponse, ListTasksRequest, ListTasksResponse, TaskProgressSnapshot,
};
use gloo_net::Error as ApiError;

/// 查询后台任务进度
///
/// `GET /api/v1/system/tasks/{task_id}/progress`
pub async fn get_task_progress(task_id: &str) -> Result<TaskProgressSnapshot, ApiError> {
    api_get(&format!("/api/v1/system/tasks/{}/progress", task_id)).await
}

/// 列出所有后台任务（支持筛选）
///
/// `GET /api/v1/system/tasks?task_type=xxx&status=xxx`
pub async fn list_tasks(req: &ListTasksRequest) -> Result<ListTasksResponse, ApiError> {
    let qs = crate::api::build_query_string(&[
        ("task_type", req.task_type.clone()),
        (
            "status",
            req.status.map(|s| serde_json::to_string(&s).unwrap_or_default()),
        ),
    ]);
    api_get(&format!("/api/v1/system/tasks{}", qs)).await
}

/// 清理已完成的旧任务
///
/// `POST /api/v1/system/tasks/cleanup?max_count=10`
pub async fn cleanup_tasks(max_count: Option<usize>) -> Result<CleanupTasksResponse, ApiError> {
    let qs =
        crate::api::build_query_string(&[("max_count", max_count.map(|v| v.to_string()))]);
    let body = serde_json::json!({});
    api_post(&format!("/api/v1/system/tasks/cleanup{}", qs), &body).await
}
```

- [ ] **Step 2: 在 `frontend/src/api/mod.rs` 注册模块**

```rust
pub mod background_task;
```

- [ ] **Step 3: Commit**

```bash
git add frontend/src/api/background_task.rs frontend/src/api/mod.rs
git commit -m "feat(frontend): add background task management API"
```

---

## Task 5: 前端任务管理页面

**Files:**
- Create: `frontend/src/pages/system/tasks.rs`

页面功能：
- 列表展示所有任务（类型、状态 badge、步骤进度、开始时间、耗时）
- 按类型筛选（下拉选择）、按状态筛选（下拉选择）
- 客户端分页（每页 20 条）
- 使用 `use_future` 实现 3 秒自动轮询（组件卸载自动取消，筛选变化自动重启并立即加载）
- 点击任务行弹窗查看详情（result JSON、error、step_message）
- 清理已完成任务按钮（带确认弹窗）

**轮询实现关键点：** 使用 `use_future` 而非 `use_effect` + `spawn`。`use_future` 会在闭包中读取的信号变化时自动重启（取消旧 future，启动新 future），组件卸载时自动取消。这样筛选变化时会立即重新加载，且不会产生多个并行轮询循环。

- [ ] **Step 1: 创建 `frontend/src/pages/system/tasks.rs`**

```rust
//! 后台任务管理页面
//!
//! 功能：
//! - 列表展示所有后台任务（按 started_at 降序）
//! - 按类型/状态筛选 + 客户端分页（每页 20 条）
//! - 使用 use_future 实现 3 秒自动轮询（卸载自动取消，筛选变化自动重启）
//! - 点击任务行弹窗查看详情（result/error）
//! - 清理已完成任务

use crate::api::background_task::{cleanup_tasks, list_tasks};
use crate::components::toast::use_toast;
use crate::layouts::AppLayout;
use common::api::{ListTasksRequest, TaskProgressSnapshot, TaskStatus};
use dioxus::prelude::*;

const PAGE_SIZE: usize = 20;
const POLL_INTERVAL_MS: u32 = 3000;

/// 后台任务管理页面
#[component]
pub fn SystemTasks() -> Element {
    let toast = use_toast();
    let mut tasks = use_signal(Vec::<TaskProgressSnapshot>::new);
    let mut loading = use_signal(|| true);
    let mut filter_type = use_signal(|| None::<String>);
    let mut filter_status = use_signal(|| None::<TaskStatus>);
    let mut current_page = use_signal(|| 0usize);
    let mut detail_task = use_signal(|| None::<TaskProgressSnapshot>);
    let mut show_cleanup_confirm = use_signal(|| false);

    // use_future：初始加载 + 持续轮询
    // 读取 filter_type/filter_status 会建立依赖，筛选变化时自动重启 future
    // 组件卸载时自动取消，不会产生多个并行循环
    use_future(move || async move {
        let req = ListTasksRequest {
            task_type: filter_type(),
            status: filter_status(),
        };
        // 立即加载一次
        match list_tasks(&req).await {
            Ok(resp) => {
                tasks.set(resp.tasks);
                loading.set(false);
            }
            Err(e) => {
                toast.error(&format!("加载任务列表失败: {}", e));
                loading.set(false);
            }
        }
        // 持续轮询
        loop {
            gloo_timers::future::TimeoutFuture::new(POLL_INTERVAL_MS).await;
            let req = ListTasksRequest {
                task_type: filter_type(),
                status: filter_status(),
            };
            if let Ok(resp) = list_tasks(&req).await {
                tasks.set(resp.tasks);
            }
        }
    });

    // 客户端分页
    let total = tasks().len();
    let total_pages = (total + PAGE_SIZE - 1) / PAGE_SIZE;
    let page = current_page().min(total_pages.saturating_sub(1));
    let start = page * PAGE_SIZE;
    let end = (start + PAGE_SIZE).min(total);
    let page_tasks: Vec<TaskProgressSnapshot> = tasks()[start..end].to_vec();

    // 统计
    let running_count = tasks().iter().filter(|t| t.status == TaskStatus::Running).count();
    let completed_count = tasks().iter().filter(|t| t.status == TaskStatus::Completed).count();
    let failed_count = tasks().iter().filter(|t| t.status == TaskStatus::Failed).count();

    // 清理已完成任务
    let on_cleanup = move |_| {
        spawn(async move {
            match cleanup_tasks(Some(10)).await {
                Ok(resp) => {
                    toast.success(&format!("已清理 {} 个任务", resp.cleaned));
                    show_cleanup_confirm.set(false);
                    // 立即刷新列表
                    let req = ListTasksRequest {
                        task_type: filter_type(),
                        status: filter_status(),
                    };
                    if let Ok(resp) = list_tasks(&req).await {
                        tasks.set(resp.tasks);
                    }
                }
                Err(e) => toast.error(&format!("清理失败: {}", e)),
            }
        });
    };

    rsx! {
        AppLayout {
            div { class: "card bg-base-100 shadow-md",
                div { class: "card-body",
                    // 标题栏
                    div { class: "flex items-center justify-between",
                        h2 { class: "card-title", "后台任务管理" }
                        div { class: "flex gap-2",
                            button {
                                class: "btn btn-warning btn-sm",
                                onclick: move |_| show_cleanup_confirm.set(true),
                                "清理已完成"
                            }
                        }
                    }

                    // 统计卡片
                    div { class: "stats stats-horizontal shadow mt-4",
                        div { class: "stat",
                            div { class: "stat-title", "运行中" }
                            div { class: "stat-value text-primary", "{running_count}" }
                        }
                        div { class: "stat",
                            div { class: "stat-title", "已完成" }
                            div { class: "stat-value text-success", "{completed_count}" }
                        }
                        div { class: "stat",
                            div { class: "stat-title", "已失败" }
                            div { class: "stat-value text-error", "{failed_count}" }
                        }
                    }

                    // 筛选栏
                    div { class: "flex gap-4 mt-4",
                        select {
                            class: "select select-bordered select-sm",
                            onchange: move |e| {
                                let val = e.value();
                                filter_type.set(if val.is_empty() { None } else { Some(val) });
                                current_page.set(0);
                            },
                            option { value: "", "全部类型" }
                            option { value: "initialize_system", "系统初始化" }
                            option { value: "rebuild_vectors", "向量重建" }
                            option { value: "seed_save", "Seed 导出" }
                            option { value: "seed_load", "Seed 导入" }
                            option { value: "seed_apply_default", "应用默认 Seed" }
                        }
                        select {
                            class: "select select-bordered select-sm",
                            onchange: move |e| {
                                let val = e.value();
                                filter_status.set(match val.as_str() {
                                    "pending" => Some(TaskStatus::Pending),
                                    "running" => Some(TaskStatus::Running),
                                    "completed" => Some(TaskStatus::Completed),
                                    "failed" => Some(TaskStatus::Failed),
                                    _ => None,
                                });
                                current_page.set(0);
                            },
                            option { value: "", "全部状态" }
                            option { value: "pending", "等待中" }
                            option { value: "running", "运行中" }
                            option { value: "completed", "已完成" }
                            option { value: "failed", "已失败" }
                        }
                    }

                    // 任务列表
                    if loading() {
                        div { class: "flex justify-center py-8",
                            span { class: "loading loading-spinner loading-lg" }
                        }
                    } else if page_tasks.is_empty() {
                        div { class: "text-center py-8 text-base-content/50",
                            "暂无后台任务"
                        }
                    } else {
                        div { class: "overflow-x-auto mt-4",
                            table { class: "table table-zebra",
                                thead {
                                    tr {
                                        th { "类型" }
                                        th { "状态" }
                                        th { "进度" }
                                        th { "开始时间" }
                                        th { "耗时" }
                                    }
                                }
                                tbody {
                                    for t in page_tasks.iter() {
                                        {
                                            let task_id = t.task_id.clone();
                                            let task_id_click = task_id.clone();
                                            let status_class = match t.status {
                                                TaskStatus::Pending => "badge badge-ghost",
                                                TaskStatus::Running => "badge badge-primary",
                                                TaskStatus::Completed => "badge badge-success",
                                                TaskStatus::Failed => "badge badge-error",
                                            };
                                            let status_label = match t.status {
                                                TaskStatus::Pending => "等待中",
                                                TaskStatus::Running => "运行中",
                                                TaskStatus::Completed => "已完成",
                                                TaskStatus::Failed => "已失败",
                                            };
                                            let type_label = match t.task_type.as_str() {
                                                "initialize_system" => "系统初始化",
                                                "rebuild_vectors" => "向量重建",
                                                "seed_save" => "Seed 导出",
                                                "seed_load" => "Seed 导入",
                                                "seed_apply_default" => "应用默认 Seed",
                                                _ => t.task_type.as_str(),
                                            };
                                            let started_time = chrono::DateTime::from_timestamp_millis(t.started_at)
                                                .map(|dt| dt.format("%m-%d %H:%M:%S").to_string())
                                                .unwrap_or_else(|| "-".to_string());
                                            let duration = if let Some(finished) = t.finished_at {
                                                finished - t.started_at
                                            } else {
                                                chrono::Utc::now().timestamp_millis() - t.started_at
                                            };
                                            let duration_str = if duration < 1000 {
                                                format!("{}ms", duration)
                                            } else if duration < 60_000 {
                                                format!("{:.1}s", duration as f64 / 1000.0)
                                            } else {
                                                format!("{:.1}m", duration as f64 / 60_000.0)
                                            };

                                            rsx! {
                                                tr {
                                                    class: "cursor-pointer hover",
                                                    key: "{task_id}",
                                                    onclick: move |_| {
                                                        if let Some(t) = tasks().iter().find(|t| t.task_id == task_id_click) {
                                                            detail_task.set(Some(t.clone()));
                                                        }
                                                    },
                                                    td { {type_label} }
                                                    td {
                                                        span { class: "{status_class}", {status_label} }
                                                    }
                                                    td {
                                                        if t.total_steps > 0 {
                                                            "{t.current_step}/{t.total_steps}"
                                                        } else {
                                                            "-"
                                                        }
                                                        br {}
                                                        small { class: "text-base-content/60",
                                                            "{t.step_message}"
                                                        }
                                                    }
                                                    td { {started_time} }
                                                    td { {duration_str} }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // 分页
                        if total_pages > 1 {
                            div { class: "flex justify-center mt-4",
                                div { class: "join",
                                    button {
                                        class: "join-item btn btn-sm",
                                        disabled: page == 0,
                                        onclick: move |_| current_page.set(page.saturating_sub(1)),
                                        "«"
                                    }
                                    button { class: "join-item btn btn-sm", "第 {page + 1} 页 / {total_pages}" }
                                    button {
                                        class: "join-item btn btn-sm",
                                        disabled: page + 1 >= total_pages,
                                        onclick: move |_| current_page.set(page + 1),
                                        "»"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 详情弹窗
            if let Some(detail) = detail_task() {
                rsx! {
                    div {
                        class: "modal modal-open",
                        onclick: move |_| detail_task.set(None),
                        div {
                            class: "modal-box",
                            onclick: |e| e.stop_propagation(),
                            h3 { class: "font-bold text-lg", "任务详情" }
                            div { class: "py-4 space-y-2",
                                div { class: "flex gap-2",
                                    span { class: "font-semibold", "任务 ID:" }
                                    span { class: "font-mono text-sm", "{detail.task_id}" }
                                }
                                div { class: "flex gap-2",
                                    span { class: "font-semibold", "类型:" }
                                    span { "{detail.task_type}" }
                                }
                                div { class: "flex gap-2",
                                    span { class: "font-semibold", "状态:" }
                                    span { "{detail.status:?}" }
                                }
                                div { class: "flex gap-2",
                                    span { class: "font-semibold", "步骤:" }
                                    span { "{detail.current_step} / {detail.total_steps}" }
                                }
                                div { class: "flex gap-2",
                                    span { class: "font-semibold", "当前描述:" }
                                    span { "{detail.step_message}" }
                                }
                                if let Some(err) = &detail.error {
                                    div { class: "alert alert-error",
                                        span { class: "font-semibold", "错误信息:" }
                                        span { "{err}" }
                                    }
                                }
                                if let Some(result) = &detail.result {
                                    div {
                                        span { class: "font-semibold", "结果:" }
                                        pre { class: "bg-base-200 p-2 rounded mt-1 text-xs overflow-x-auto",
                                            {serde_json::to_string_pretty(result).unwrap_or_default()}
                                        }
                                    }
                                }
                            }
                            div { class: "modal-action",
                                button {
                                    class: "btn",
                                    onclick: move |_| detail_task.set(None),
                                    "关闭"
                                }
                            }
                        }
                    }
                }
            }

            // 清理确认弹窗
            if show_cleanup_confirm() {
                rsx! {
                    div {
                        class: "modal modal-open",
                        onclick: move |_| show_cleanup_confirm.set(false),
                        div {
                            class: "modal-box",
                            onclick: |e| e.stop_propagation(),
                            h3 { class: "font-bold text-lg", "确认清理" }
                            p { class: "py-4", "将清理已完成的旧任务，每个类型保留最近 10 个。运行中的任务不受影响。" }
                            div { class: "modal-action",
                                button {
                                    class: "btn btn-ghost",
                                    onclick: move |_| show_cleanup_confirm.set(false),
                                    "取消"
                                }
                                button {
                                    class: "btn btn-warning",
                                    onclick: move |_| on_cleanup(()),
                                    "确认清理"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/pages/system/tasks.rs
git commit -m "feat(frontend): add background task management page"
```

---

## Task 6: 前端路由 + 导航注册

**Files:**
- Modify: `frontend/src/pages/system/mod.rs`
- Modify: `frontend/src/pages/mod.rs`
- Modify: `frontend/src/layouts/navbar.rs`

- [ ] **Step 1: 在 `frontend/src/pages/system/mod.rs` 注册模块**

```rust
pub mod tasks;
```

- [ ] **Step 2: 在 `frontend/src/pages/mod.rs` 注册路由**

在 `use` 语句中添加：
```rust
use crate::pages::system::tasks::SystemTasks;
```

在 `Route` 枚举中添加（在 `SystemSeed` 附近）：
```rust
#[route("/system/tasks")]
SystemTasks {},
```

- [ ] **Step 3: 在 `frontend/src/layouts/navbar.rs` 系统菜单中添加链接**

在系统管理下拉菜单的 Admin 权限区域，`SystemSeed` 链接之后添加：

```rust
li {
    Link {
        to: Route::SystemTasks {},
        onclick: move |_| {
            hr_menu_open.set(false);
            finance_menu_open.set(false);
            project_menu_open.set(false);
            system_menu_open.set(false);
            user_menu_open.set(false);
        },
        "后台任务"
    }
}
```

- [ ] **Step 4: Commit**

```bash
git add frontend/src/pages/system/mod.rs frontend/src/pages/mod.rs frontend/src/layouts/navbar.rs
git commit -m "feat(frontend): register SystemTasks route and nav link"
```

---

## Task 7: cargo fmt + 最终验证

- [ ] **Step 1: 格式化**

Run: `cargo fmt --all && cd frontend && cargo fmt && cd ..`

- [ ] **Step 2: 编译验证**

Run: `cargo check --lib --bin ai_orz && cd frontend && cargo check`
Expected: PASS

- [ ] **Step 3: 测试验证**

Run: `cargo test --test preset_skills_test`
Expected: 4 passed

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "chore: fmt + verify background task admin page"
```

---

## Self-Review Notes

### 设计决策记录

1. **取消功能暂不实现**：取消功能需要改造 trait + 5 个任务对象（增加 cancelled 字段 + 检查点），复杂度较高。本期先不做，后续扩展时再增加 `cancel()` / `is_cancelled()` trait 方法 + 协作式取消检查点。

2. **客户端分页**：后台任务数量通常不大（每类型保留最近若干个），后端返回全部匹配任务，前端用 Dioxus 信号管理分页。避免 registry 的 list_all_progress 增加分页复杂度。

3. **use_future 轮询**：使用 Dioxus `use_future` 而非 `use_effect` + `spawn`。`use_future` 读取的信号变化时自动重启（取消旧 future，启动新 future），组件卸载时自动取消。筛选变化时立即重新加载并重启轮询，不会产生多个并行循环。

4. **list_all_progress**：新增 registry 方法返回 `Vec<TaskProgressSnapshot>`，handler 层做筛选和排序。每次调用遍历所有任务调用 `progress()`，任务数量不大时性能可接受。

5. **清理接口返回清理数量**：通过 before/after 统计差值计算清理数量，前端展示清理结果。`cleanup_finished` 保留每个类型最近 max_count 个已完成/失败任务。

6. **详情弹窗**：点击任务行弹窗显示完整信息（task_id、result JSON、error、step_message），复用 DaisyUI modal 组件。

### 后续扩展点（取消功能）

当需要实现取消功能时：
1. `TaskStatus` 增加 `Cancelled` 变体
2. `BackgroundTask` trait 增加 `fn cancel(&self) {}` 和 `fn is_cancelled(&self) -> bool { false }` 默认实现
3. registry 增加 `cancel_task(task_id)` 方法
4. 每个任务对象增加 `cancelled: AtomicBool` 字段，`run` 方法在关键步骤检查
5. 装饰接口（InitStatus/RebuildStatus）增加 Cancelled 映射
6. 新增 `POST /tasks/{task_id}/cancel` handler
7. 前端增加取消按钮
