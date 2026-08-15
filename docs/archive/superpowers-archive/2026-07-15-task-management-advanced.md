# 独立任务管理页面 + 看板视图实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 构建独立的任务管理页面（`/tasks`），支持全局任务列表查询、多维度筛选、列表视图和看板视图（按状态分列，不支持拖拽），与已完成的任务详情页和创建弹窗形成完整闭环。

**Architecture:**
- **后端**：新增 `GET /api/v1/tasks` Handler，复用现有 `TaskManage::list()` Domain 方法，支持 `project_id`、`status`、`assignee_id`、`assignee_type` 查询参数
- **前端 API**：新增 `list_tasks()` 客户端函数
- **前端页面**：`TaskList` 组件，包含筛选栏（项目/状态/负责人）、视图切换（列表/看板）、任务卡片、统计概览
- **看板视图**：按 TaskStatus 枚举值（待审核/待处理/进行中/已完成/已归档）分列展示任务卡片，点击卡片跳转到详情页

**Tech Stack:** Dioxus 0.7.9、dioxus_router、chrono、Tailwind/Mistral CSS 设计系统

---

## 文件结构

| 文件 | 职责 | 操作 |
|------|------|------|
| `src/handlers/project/task/list_tasks.rs` | 后端 Handler：`GET /api/v1/tasks` 全局任务列表 | 创建 |
| `src/handlers/project/task/mod.rs` | 注册新路由 | 修改 |
| `frontend/src/api/project.rs` | 前端 API 客户端：新增 `list_tasks()` | 修改 |
| `frontend/src/pages/project/tasks.rs` | 任务管理页面（列表视图 + 看板视图） | 创建 |
| `frontend/src/pages/project/mod.rs` | 导出新模块 | 修改 |
| `frontend/src/pages/mod.rs` | 注册 `/tasks` 路由 | 修改 |
| `frontend/index.html` | 看板视图相关 CSS 样式 | 修改 |

---

## 调研结论

| 资源 | 状态 | 备注 |
|------|------|------|
| `TaskManage::list()` Domain 方法 | ✅ 已存在 | `src/service/domain/project/mod.rs:213-221`，支持 project_id/assignee_type/assignee_id/status/limit |
| `TaskListItem` DTO | ✅ 已存在 | `common/src/api/task.rs:73-103`，含 id/title/status/priority/tags/assignee_type/assignee_id/project_id/progress |
| `TaskStatus` 枚举 | ✅ 已存在 | `common/src/enums/task.rs`：Cancelled=0, PendingReview=1, Pending=2, InProgress=3, Completed=4, Archived=5 |
| `AssigneeType` 枚举 | ✅ 已存在 | User=0, Agent=1 |
| `list_project_tasks` API 客户端 | ✅ 已存在 | `frontend/src/api/project.rs:37-39` |
| `get_task` API 客户端 | ✅ 已存在 | `frontend/src/api/project.rs:41-43` |
| 看板视图 | ❌ 需新增 | 按状态分列，不支持拖拽 |
| 全局任务列表 API | ❌ 需新增 | 后端 Handler + 前端客户端 |

**结论**：后端 Domain 能力已就绪，只需新增 Handler 和前端页面。

---

## Task 1: 后端 Handler - 全局任务列表

**Files:**
- Create: `src/handlers/project/task/list_tasks.rs`
- Modify: `src/handlers/project/task/mod.rs`

### Step 1.1: 创建 list_tasks.rs

```rust
//! Handler: GET /api/v1/tasks - List all tasks with filters

use super::response;
use common::error::Result;
use common::api::{ApiResponse, ListTasksResponse, TaskListItem};
use crate::pkg::RequestContext;

/// 查询参数
#[derive(Debug, serde::Deserialize)]
pub struct QueryParams {
    project_id: Option<String>,
    status: Option<i32>,
    assignee_id: Option<String>,
    assignee_type: Option<i32>,
    limit: Option<usize>,
}

pub async fn list_tasks(
    ctx: RequestContext,
    query: axum::extract::Query<QueryParams>,
) -> Result<axum::Json<ApiResponse<ListTasksResponse>>> {
    let project_id = query.project_id.as_deref();
    let status = query.status.map(common::enums::TaskStatus::from_i32);
    let assignee_id = query.assignee_id.as_deref();
    let assignee_type = query.assignee_type.map(common::enums::AssigneeType::from_i32);

    let tasks = crate::service::domain::project::domain()
        .task_manage()
        .list(ctx, project_id, assignee_type, assignee_id, status, query.limit)
        .await?;

    let items: Vec<TaskListItem> = tasks.into_iter().map(|t| TaskListItem {
        id: t.po.id,
        title: t.po.title,
        description: t.po.description,
        status: t.po.status.to_i32(),
        priority: t.po.priority,
        tags: t.po.tags,
        root_user_id: t.po.root_user_id,
        assignee_type: t.po.assignee_type.to_i32(),
        assignee_id: t.po.assignee_id,
        project_id: t.po.project_id,
        thinking_depth: t.po.thinking_depth,
        progress: t.po.progress,
        created_at: t.po.created_at,
        updated_at: t.po.updated_at,
    }).collect();

    response::success(ListTasksResponse { tasks: items })
}
```

### Step 1.2: 在 mod.rs 中注册路由

读取 `src/handlers/project/task/mod.rs` 确认路由注册模式：

```bash
grep -n "pub fn router" src/handlers/project/task/mod.rs
```

假设路由注册模式类似：

```rust
// 在 router 函数中添加
let router = Router::new()
    .route("/tasks", get(list_tasks))  // 新增
    .route("/tasks/:id", get(get_task))
    .route("/tasks/:id", put(update_task))
    .route("/tasks/:id/status", put(update_task_status))
    .route("/tasks/:id/progress", put(update_task_progress))
    .route("/tasks", post(create_task))
    .route("/projects/:project_id/tasks", get(list_project_tasks))
    .route("/agents/:agent_id/tasks", get(list_agent_tasks));
```

### Step 1.3: 验证编译

```bash
cd /Users/aman/Technology/rust/ai_orz && cargo check --lib 2>&1 | grep -E "^error|Finished"
```

**预期**：0 错误。

---

## Task 2: 前端 API 客户端 - list_tasks

**Files:**
- Modify: `frontend/src/api/project.rs`

### Step 2.1: 添加 list_tasks 函数

在 `frontend/src/api/project.rs` 的"任务管理"区域（约第 35 行后）添加：

```rust
pub async fn list_tasks(
    project_id: Option<&str>,
    status: Option<i32>,
    assignee_id: Option<&str>,
    assignee_type: Option<i32>,
) -> Result<ListTasksResponse, String> {
    let mut url = "/api/v1/tasks".to_string();
    let mut params = Vec::new();
    if let Some(pid) = project_id {
        params.push(format!("project_id={}", pid));
    }
    if let Some(s) = status {
        params.push(format!("status={}", s));
    }
    if let Some(aid) = assignee_id {
        params.push(format!("assignee_id={}", aid));
    }
    if let Some(at) = assignee_type {
        params.push(format!("assignee_type={}", at));
    }
    if !params.is_empty() {
        url.push('?');
        url.push_str(&params.join("&"));
    }
    api_get(&url).await
}
```

### Step 2.2: 验证编译

```bash
cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check 2>&1 | grep -E "^error|Finished"
```

**预期**：0 错误。

---

## Task 3: 任务管理页面组件

**Files:**
- Create: `frontend/src/pages/project/tasks.rs`
- Modify: `frontend/src/pages/project/mod.rs`

### Step 3.1: 创建 tasks.rs

```rust
//! 任务管理页面 - 列表视图 + 看板视图

use dioxus::prelude::*;
use dioxus_router::use_navigator;

use crate::api::project::{list_projects, list_tasks};
use crate::components::state::{EmptyState, Loading};
use crate::store::toast::use_toast;
use common::api::{ListProjectsResponseItem, ListTasksResponse, TaskListItem};

#[derive(Debug, Clone, PartialEq)]
pub enum ViewMode {
    List,
    Board,
}

fn task_status_badge(status: i32) -> &'static str {
    match status {
        0 => "badge badge-error",       // 已取消
        1 => "badge badge-warning",     // 待审核
        2 => "badge badge-info",        // 待处理
        3 => "badge badge-primary",     // 进行中
        4 => "badge badge-success",     // 已完成
        5 => "badge badge-neutral",     // 已归档
        _ => "badge badge-neutral",
    }
}

fn task_status_text(status: i32) -> &'static str {
    match status {
        0 => "已取消",
        1 => "待审核",
        2 => "待处理",
        3 => "进行中",
        4 => "已完成",
        5 => "已归档",
        _ => "未知",
    }
}

fn progress_bar_class(progress: i32) -> &'static str {
    match progress {
        0..=25 => "overview-progress-fill warning",
        26..=50 => "overview-progress-fill primary",
        51..=75 => "overview-progress-fill accent",
        76..=100 => "overview-progress-fill success",
        _ => "overview-progress-fill",
    }
}

fn format_time(timestamp: i64) -> String {
    use chrono::{Local, TimeZone};
    Local
        .timestamp_opt(timestamp / 1000, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| timestamp.to_string())
}

#[component]
pub fn TaskList() -> Element {
    let mut tasks = use_signal(Vec::<TaskListItem>::new);
    let mut projects = use_signal(Vec::<ListProjectsResponseItem>::new);
    let mut loading = use_signal(|| true);
    let mut view_mode = use_signal(|| ViewMode::Board);

    // 筛选状态
    let mut filter_project_id = use_signal(String::new);
    let mut filter_status = use_signal(|| -1i32);
    let mut filter_assignee_type = use_signal(|| -1i32);

    let toast = use_toast();
    let navigator = use_navigator();

    // 加载数据
    let load_data = move || {
        loading.set(true);
        let pid = filter_project_id();
        let status = if filter_status() >= 0 { Some(filter_status()) } else { None };
        let at = if filter_assignee_type() >= 0 { Some(filter_assignee_type()) } else { None };
        spawn(async move {
            match list_tasks(
                if pid.is_empty() { None } else { Some(&pid) },
                status,
                None,
                at,
            ).await {
                Ok(resp) => tasks.set(resp.tasks),
                Err(e) => toast.error(&e),
            }
            match list_projects().await {
                Ok(list) => projects.set(list.projects),
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    };

    use_effect(move || {
        load_data();
    });

    // 筛选变化时重新加载
    let pid_for_filter = filter_project_id();
    let status_for_filter = filter_status();
    let at_for_filter = filter_assignee_type();
    use_effect(move || {
        if !loading() {
            load_data();
        }
    }, [pid_for_filter, status_for_filter, at_for_filter]);

    let tasks_list = tasks.read().clone();
    let projects_list = projects.read().clone();

    // 统计数据
    let total = tasks_list.len();
    let completed = tasks_list.iter().filter(|t| t.status == 4).count();
    let in_progress = tasks_list.iter().filter(|t| t.status == 3).count();
    let pending = tasks_list.iter().filter(|t| t.status == 2 || t.status == 1).count();

    // 看板数据分组
    let board_groups = [
        (1, "待审核"),
        (2, "待处理"),
        (3, "进行中"),
        (4, "已完成"),
        (5, "已归档"),
    ];

    let filtered_tasks_by_status = |status: i32| {
        tasks_list.iter().filter(|t| t.status == status).collect::<Vec<_>>()
    };

    rsx! {
        div { class: "page-header",
            h1 { class: "page-title", "任务管理" }
            div { class: "page-header-actions",
                button {
                    class: "btn btn-secondary",
                    onclick: move |_| view_mode.set(ViewMode::List),
                    if matches!(view_mode(), ViewMode::List) { class: "btn btn-secondary active" }
                    "列表视图"
                }
                button {
                    class: "btn btn-secondary",
                    onclick: move |_| view_mode.set(ViewMode::Board),
                    if matches!(view_mode(), ViewMode::Board) { class: "btn btn-secondary active" }
                    "看板视图"
                }
            }
        }

        // 统计概览
        div { class: "card",
            div { class: "card-header",
                h2 { class: "card-title", "任务概览" }
            }
            div { class: "overview-grid",
                div { class: "overview-item",
                    div { class: "overview-label", "任务总数" }
                    div { class: "overview-stat-value", "{total}" }
                }
                div { class: "overview-item",
                    div { class: "overview-label", "进行中" }
                    div { class: "overview-stat-value primary", "{in_progress}" }
                }
                div { class: "overview-item",
                    div { class: "overview-label", "待处理" }
                    div { class: "overview-stat-value warning", "{pending}" }
                }
                div { class: "overview-item",
                    div { class: "overview-label", "已完成" }
                    div { class: "overview-stat-value success", "{completed}" }
                }
            }
        }

        // 筛选栏
        div { class: "card",
            div { class: "card-header",
                h2 { class: "card-title", "筛选条件" }
            }
            div { class: "filter-row",
                div { class: "filter-item",
                    label { class: "form-label", "项目" }
                    select {
                        class: "form-input",
                        value: "{filter_project_id}",
                        onchange: move |e| filter_project_id.set(e.value().clone()),
                        option { value: "", "全部项目" }
                        for p in projects_list.iter() {
                            option { value: "{p.id}", "{p.name}" }
                        }
                    }
                }
                div { class: "filter-item",
                    label { class: "form-label", "状态" }
                    select {
                        class: "form-input",
                        value: "{filter_status}",
                        onchange: move |e| {
                            if let Ok(v) = e.value().parse::<i32>() {
                                filter_status.set(v);
                            }
                        },
                        option { value: "-1", "全部状态" }
                        option { value: "1", "待审核" }
                        option { value: "2", "待处理" }
                        option { value: "3", "进行中" }
                        option { value: "4", "已完成" }
                        option { value: "5", "已归档" }
                    }
                }
                div { class: "filter-item",
                    label { class: "form-label", "负责人类型" }
                    select {
                        class: "form-input",
                        value: "{filter_assignee_type}",
                        onchange: move |e| {
                            if let Ok(v) = e.value().parse::<i32>() {
                                filter_assignee_type.set(v);
                            }
                        },
                        option { value: "-1", "全部" }
                        option { value: "0", "用户" }
                        option { value: "1", "Agent" }
                    }
                }
            }
        }

        // 视图内容
        if loading() {
            div { class: "card", Loading {} }
        } else if tasks_list.is_empty() {
            div { class: "card", EmptyState { icon: "📋".to_string(), message: "暂无任务".to_string() } }
        } else if matches!(view_mode(), ViewMode::List) {
            // 列表视图
            div { class: "card",
                div { class: "card-header",
                    h2 { class: "card-title", "任务列表" }
                }
                table { class: "table",
                    thead { tr {
                        th { "标题" }
                        th { "状态" }
                        th { "优先级" }
                        th { "进度" }
                        th { "负责人" }
                        th { "项目" }
                        th { "更新时间" }
                    }}
                    tbody {
                        for t in tasks_list.iter() {
                            {
                                let tid = t.id.clone();
                                let t_title = t.title.clone();
                                let t_status = t.status;
                                let t_priority = t.priority;
                                let t_progress = t.progress;
                                let t_assignee_type = t.assignee_type;
                                let t_assignee_id = t.assignee_id.clone();
                                let t_project_id = t.project_id.clone();
                                let t_updated_at = t.updated_at;
                                rsx! {
                                    tr {
                                        key: "{tid}",
                                        class: "table-row-clickable",
                                        onclick: move |_| navigator.push(format!("/tasks/{}", tid)),
                                        td { "{t_title}" }
                                        td { span { class: "{task_status_badge(t_status)}", "{task_status_text(t_status)}" } }
                                        td { "{t_priority}" }
                                        td {
                                            div { class: "progress-cell",
                                                div { class: "progress-bar",
                                                    div { class: "progress-bar-fill", style: "width: {t_progress}%;" }
                                                }
                                                span { class: "text-muted text-mono progress-text", "{t_progress}%" }
                                            }
                                        }
                                        td {
                                            "{if t_assignee_type == 0 { \"用户\" } else { \"Agent\" }}: {t_assignee_id}"
                                        }
                                        td {
                                            if let Some(pid) = &t_project_id {
                                                span { class: "text-mono", "{pid}" }
                                            } else {
                                                span { class: "text-muted", "无" }
                                            }
                                        }
                                        td { span { class: "text-mono text-muted", "{format_time(t_updated_at)}" } }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            // 看板视图
            div { class: "kanban-board",
                for (status, title) in board_groups.iter() {
                    let group_tasks = filtered_tasks_by_status(*status);
                    if group_tasks.is_empty() {
                        continue;
                    }
                    div { class: "kanban-column",
                        div { class: "kanban-column-header",
                            span { class: "{task_status_badge(*status)}", "{title}" }
                            span { class: "kanban-column-count", "{group_tasks.len()}" }
                        }
                        div { class: "kanban-column-content",
                            for t in group_tasks {
                                {
                                    let tid = t.id.clone();
                                    let t_title = t.title.clone();
                                    let t_progress = t.progress;
                                    let t_priority = t.priority;
                                    let t_tags = t.tags.clone();
                                    rsx! {
                                        div {
                                            key: "{tid}",
                                            class: "kanban-card",
                                            onclick: move |_| navigator.push(format!("/tasks/{}", tid)),
                                            div { class: "kanban-card-header",
                                                h3 { class: "kanban-card-title", "{t_title}" }
                                                div { class: "kanban-card-meta",
                                                    if t_priority > 0 {
                                                        span { class: "badge badge-warning", "优先级 {t_priority}" }
                                                    }
                                                }
                                            }
                                            if !t_tags.is_empty() {
                                                div { class: "kanban-card-tags",
                                                    for tag in t_tags.iter() {
                                                        span { class: "badge badge-neutral tag-item", "{tag}" }
                                                    }
                                                }
                                            }
                                            div { class: "kanban-card-progress",
                                                div { class: "progress-bar",
                                                    div { class: "{progress_bar_class(t_progress)}", style: "width: {t_progress}%;" }
                                                }
                                                span { class: "text-muted text-mono", "{t_progress}%" }
                                            }
                                        }
                                    }
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

### Step 3.2: 在 mod.rs 中导出

修改 `frontend/src/pages/project/mod.rs`：

```rust
pub mod artifacts;
pub mod project_detail;
pub mod projects;
pub mod task_detail;
pub mod task_edit_modal;
pub mod tasks;       // 新增
```

### Step 3.3: 验证编译

```bash
cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check 2>&1 | grep -E "^error|Finished"
```

**预期**：0 错误。

---

## Task 4: 注册路由

**Files:**
- Modify: `frontend/src/pages/mod.rs`

### Step 4.1: 导入组件

在 `frontend/src/pages/mod.rs` 顶部 import 区域（约第 33 行后）添加：

```rust
use crate::pages::project::tasks::TaskList;
```

### Step 4.2: 添加路由

在 `Route` 枚举中（约第 91 行后，`Project` 模块路由区域）添加：

```rust
    // Project 模块
    #[route("/projects")]
    ProjectList {},
    #[route("/projects/:id")]
    ProjectDetail { id: String },
    #[route("/projects/artifacts")]
    ProjectArtifacts {},
    #[route("/tasks")]
    TaskList {},              // 新增
    #[route("/tasks/:id")]
    TaskDetail { id: String },
```

### Step 4.3: 验证编译

```bash
cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check 2>&1 | grep -E "^error|Finished"
```

**预期**：0 错误。

---

## Task 5: CSS 样式补充

**Files:**
- Modify: `frontend/index.html`

### Step 5.1: 添加看板视图 CSS

在 `frontend/index.html` 的 `</style>` 标签之前（约第 1786 行后）添加：

```css
      /* ===== 页面头部操作区 ===== */
      .page-header-actions {
        display: flex;
        gap: var(--space-2);
      }

      /* ===== 筛选行 ===== */
      .filter-row {
        display: flex;
        flex-wrap: wrap;
        gap: var(--space-4);
        padding: var(--space-4);
      }

      .filter-item {
        min-width: 150px;
        flex: 1;
        max-width: 250px;
      }

      /* ===== 看板视图 ===== */
      .kanban-board {
        display: flex;
        gap: var(--space-4);
        overflow-x: auto;
        padding: var(--space-4) 0;
      }

      .kanban-column {
        flex: 0 0 300px;
        background-color: var(--color-surface-secondary);
        border-radius: var(--radius-lg);
        display: flex;
        flex-direction: column;
      }

      .kanban-column-header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        padding: var(--space-3);
        border-bottom: 1px solid var(--color-border);
      }

      .kanban-column-count {
        background-color: var(--color-primary);
        color: white;
        font-size: var(--font-xs);
        font-weight: bold;
        padding: 2px 8px;
        border-radius: var(--radius-full);
      }

      .kanban-column-content {
        flex: 1;
        padding: var(--space-3);
        display: flex;
        flex-direction: column;
        gap: var(--space-3);
        overflow-y: auto;
      }

      .kanban-card {
        background-color: var(--color-surface);
        border-radius: var(--radius-md);
        padding: var(--space-3);
        cursor: pointer;
        transition: box-shadow 0.15s, transform 0.15s;
        border: 1px solid var(--color-border);
      }

      .kanban-card:hover {
        box-shadow: var(--shadow-md);
        transform: translateY(-2px);
      }

      .kanban-card-header {
        display: flex;
        justify-content: space-between;
        align-items: flex-start;
        gap: var(--space-2);
        margin-bottom: var(--space-2);
      }

      .kanban-card-title {
        font-size: var(--font-sm);
        font-weight: 500;
        color: var(--color-text-primary);
        margin: 0;
        flex: 1;
        line-height: 1.4;
      }

      .kanban-card-meta {
        flex-shrink: 0;
      }

      .kanban-card-tags {
        display: flex;
        flex-wrap: wrap;
        gap: var(--space-1);
        margin-bottom: var(--space-2);
      }

      .kanban-card-progress {
        display: flex;
        align-items: center;
        gap: var(--space-2);
      }

      .kanban-card-progress .progress-bar {
        flex: 1;
        height: 6px;
        border-radius: var(--radius-full);
        background-color: var(--color-border);
        overflow: hidden;
      }

      .kanban-card-progress .progress-bar > div {
        height: 100%;
        border-radius: var(--radius-full);
        transition: width 0.3s;
      }
```

### Step 5.2: 验证编译

```bash
cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check 2>&1 | grep -E "^error|Finished"
```

**预期**：0 错误。

---

## Task 6: 端到端验证

### Step 6.1: 后端测试

```bash
cd /Users/aman/Technology/rust/ai_orz && cargo test --lib --no-fail-fast 2>&1 | tail -10
```

**预期**：测试 100% 通过（不新增后端测试，但需确保现有测试不受影响）。

### Step 6.2: 前端编译检查

```bash
cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check 2>&1 | grep -E "^error|Finished"
```

**预期**：0 错误。

### Step 6.3: 手动验证清单

1. **访问 `/tasks`** → 页面加载成功，显示统计概览、筛选栏、看板视图（默认）
2. **切换视图** → 列表视图和看板视图切换正常
3. **筛选功能**：
   - 选择项目 → 任务列表更新
   - 选择状态 → 任务列表更新
   - 选择负责人类型 → 任务列表更新
4. **任务卡片点击** → 跳转到 `/tasks/{id}` 详情页
5. **看板视图分组**：待审核/待处理/进行中/已完成/已归档 五列正确显示
6. **任务进度条**：卡片内进度条颜色根据进度变化（橙色/蓝色/紫色/绿色）

---

## 风险与注意事项

| 风险 | 应对 |
|------|------|
| 后端路由冲突 | `GET /api/v1/tasks` 与现有的 `GET /api/v1/tasks/{id}` 不冲突（路径不同） |
| 前端筛选状态同步问题 | 使用 `use_effect` 监听筛选参数变化，自动重新加载 |
| 看板列横向滚动 | CSS 使用 `overflow-x: auto` 支持横向滚动 |
| 任务卡片过多导致性能问题 | 当前实现简单，后续可添加分页或虚拟滚动 |
| `filter_assignee_type` 类型转换 | 使用 `parse::<i32>()` 确保类型安全 |

---

## 完成标准

- [ ] Task 1: 后端 Handler 创建成功，`GET /api/v1/tasks` 可访问
- [ ] Task 2: 前端 API 客户端 `list_tasks()` 可用
- [ ] Task 3: `TaskList` 页面可独立编译
- [ ] Task 4: 路由 `/tasks` 注册成功
- [ ] Task 5: CSS 样式美观，符合项目设计系统
- [ ] Task 6: 端到端验证通过，后端测试 100% 通过，前端 0 编译错误
