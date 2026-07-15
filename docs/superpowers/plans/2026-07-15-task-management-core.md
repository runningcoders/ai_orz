# 任务管理核心功能实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为"任务管理可视化"方向三补全最核心的两个子功能：(1) 任务创建/编辑弹窗组件 (2) 任务详情页 + 路由。让用户可以在项目详情页发起新建任务、跳转到任务详情页查看完整信息、编辑任务、推进状态、更新进度。

**Architecture:**
- **TaskEditModal**：复用现有 `Modal` 组件，承载创建/编辑两种模式。表单字段映射后端 `CreateTaskRequest` / `UpdateTaskRequest`。
- **TaskDetail 页面**：独立页面 `frontend/src/pages/project/task_detail.rs`，路由 `/tasks/:id`。展示任务完整生命周期（基本信息、状态流转、进度更新、关联信息）。
- **项目详情页集成**：在 `ProjectDetail` 任务列表区域头部增加"新建任务"按钮；任务行可点击跳转到任务详情页。
- **数据源**：直接复用 `frontend/src/api/project.rs` 已有 API（`create_task` / `get_task` / `update_task` / `update_task_status` / `update_task_progress`），无需新增 API 客户端代码。

**Tech Stack:** Dioxus 0.7.9（WebAssembly）、chrono（时间格式化）、dioxus_router（路由跳转）、Signal 状态管理

---

## 调研结论

| 资源 | 状态 | 备注 |
|------|------|------|
| `CreateTaskRequest` DTO | ✅ 已存在 | `common/src/api/task.rs`，含 title/description/priority/tags/root_user_id/assignee_type/assignee_id/project_id/due_at/dependencies |
| `GetTaskResponse` DTO | ✅ 已存在 | 完整任务信息，含 progress、created_at、updated_at 等 |
| `UpdateTaskRequest` DTO | ✅ 已存在 | 局部更新字段 |
| `create_task` / `get_task` / `update_task` API 客户端 | ✅ 已存在 | `frontend/src/api/project.rs` |
| `update_task_status` / `update_task_progress` API 客户端 | ✅ 已存在 | 同上 |
| `list_agents` API 客户端 | ✅ 已存在 | `frontend/src/api/hr.rs` |
| `list_projects` API 客户端 | ✅ 已存在 | `frontend/src/api/project.rs` |
| `TaskStatus` 枚举 | ✅ 已存在 | `common/src/enums/task.rs`：Cancelled=0, PendingReview=1, Pending=2, InProgress=3, Completed=4, Archived=5 |
| `AssigneeType` 枚举 | ✅ 已存在 | User=0, Agent=1 |
| `Modal` 组件 | ✅ 已存在 | `frontend/src/components/modal.rs` |
| `use_toast` | ✅ 已存在 | 已全局注册 |
| `dioxus_router::Link` / `use_navigator` | ✅ 已存在 | `agent_detail.rs` 中已使用 |

**结论**：所有后端 API、前端客户端、DTO、枚举、组件、状态管理均已就绪。本计划**无需新增任何后端代码或 API 客户端**，仅做 UI 集成。

---

## 文件结构

| 文件 | 职责 | 操作 |
|------|------|------|
| `frontend/src/pages/project/task_detail.rs` | 任务详情页：基本信息、状态流转、进度更新、操作按钮 | 创建 |
| `frontend/src/pages/project/task_edit_modal.rs` | 任务创建/编辑弹窗组件 | 创建 |
| `frontend/src/pages/project/mod.rs` | 导出新模块 | 修改 |
| `frontend/src/pages/mod.rs` | 注册 `/tasks/:id` 路由 | 修改 |
| `frontend/src/pages/project/project_detail.rs` | 集成"新建任务"入口 + 任务行可点击 | 修改 |
| `frontend/index.html` | 任务详情页/弹窗相关 CSS 样式 | 修改 |

---

## Task 1: 任务创建/编辑弹窗组件

**Files:**
- Create: `frontend/src/pages/project/task_edit_modal.rs`
- Modify: `frontend/src/pages/project/mod.rs`

### Step 1.1: 创建 task_edit_modal.rs 骨架

在 `frontend/src/pages/project/task_edit_modal.rs` 创建组件文件：

```rust
//! 任务创建/编辑弹窗
//!
//! 支持两种模式：
//! - mode = "create"：创建新任务，assignee_type 默认 Agent
//! - mode = "edit"：编辑已有任务，预填充表单字段

use dioxus::prelude::*;
use dioxus_router::use_navigator;

use crate::api::hr::list_agents;
use crate::api::project::{create_task, get_task, list_projects, update_task};
use crate::components::modal::Modal;
use crate::store::toast::use_toast;
use common::api::{
    CreateTaskRequest, GetTaskResponse, ListAgentsResponseItem, ListProjectsResponseItem,
    UpdateTaskRequest,
};
use common::enums::AssigneeType;

/// 弹窗模式
#[derive(Debug, Clone, PartialEq)]
pub enum TaskEditMode {
    /// 在指定项目下创建任务
    Create { project_id: Option<String> },
    /// 编辑已有任务
    Edit { task_id: String },
}

/// 弹窗 Props
#[derive(Props, Clone, PartialEq)]
pub struct TaskEditModalProps {
    /// 弹窗模式（创建/编辑）
    pub mode: TaskEditMode,
    /// 是否显示
    pub show: bool,
    /// 关闭回调
    pub on_close: EventHandler<()>,
    /// 提交成功回调（用于通知父组件刷新）
    pub on_success: EventHandler<GetTaskResponse>,
}

#[component]
pub fn TaskEditModal(props: TaskEditModalProps) -> Element {
    // 表单状态
    let mut title = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut priority = use_signal(|| 0i32);
    let mut tags_input = use_signal(String::new); // 逗号分隔的字符串
    let mut due_at = use_signal(String::new); // ISO8601 字符串
    let mut assignee_type = use_signal(|| AssigneeType::Agent);
    let mut assignee_id = use_signal(String::new);
    let mut project_id = use_signal(String::new);
    let mut dependencies_input = use_signal(String::new); // 逗号分隔的 task id 列表

    // 下拉数据
    let mut projects = use_signal(Vec::<ListProjectsResponseItem>::new);
    let mut agents = use_signal(Vec::<ListAgentsResponseItem>::new);

    // 提交状态
    let mut submitting = use_signal(|| false);
    let mut loading_data = use_signal(|| true);

    let toast = use_toast();
    let navigator = use_navigator();

    // 编辑模式：加载已有任务数据
    let mode_for_load = props.mode.clone();
    let show_for_load = props.show;
    use_effect(move || {
        if !show_for_load {
            return;
        }
        loading_data.set(true);

        // 加载项目下拉数据
        let pid_initial = match &mode_for_load {
            TaskEditMode::Create { project_id } => project_id.clone().unwrap_or_default(),
            TaskEditMode::Edit { .. } => String::new(),
        };

        spawn(async move {
            // 加载项目列表
            match list_projects().await {
                Ok(list) => {
                    projects.set(list.projects);
                    if !pid_initial.is_empty() {
                        project_id.set(pid_initial.clone());
                    } else if let Some(first) = list.projects.first() {
                        project_id.set(first.id.clone());
                    }
                }
                Err(e) => toast.error(&e),
            }
            // 加载 Agent 列表
            match list_agents().await {
                Ok(resp) => {
                    agents.set(resp.agents);
                    // 默认选择第一个 Agent
                    if let Some(first) = resp.agents.first() {
                        assignee_id.set(first.id.clone());
                    }
                }
                Err(e) => toast.error(&e),
            }

            // 编辑模式：加载任务详情
            if let TaskEditMode::Edit { task_id } = &mode_for_load {
                match get_task(task_id).await {
                    Ok(t) => {
                        title.set(t.title);
                        description.set(t.description.unwrap_or_default());
                        priority.set(t.priority);
                        tags_input.set(t.tags.join(","));
                        due_at.set(format_timestamp(t.due_at));
                        assignee_type.set(if t.assignee_type == 0 {
                            AssigneeType::User
                        } else {
                            AssigneeType::Agent
                        });
                        assignee_id.set(t.assignee_id);
                        project_id.set(t.project_id.unwrap_or_default());
                        dependencies_input.set(t.dependencies.join(","));
                    }
                    Err(e) => toast.error(&e),
                }
            }
            loading_data.set(false);
        });
    });

    // 提交
    let mode_for_submit = props.mode.clone();
    let on_success = props.on_success;
    let on_close = props.on_close;
    let handle_submit = move |_| {
        let title_val = title();
        if title_val.trim().is_empty() {
            toast.error("任务标题不能为空");
            return;
        }
        submitting.set(true);
        let mode_clone = mode_for_submit.clone();
        spawn(async move {
            let result = match mode_clone {
                TaskEditMode::Create { .. } => {
                    let req = CreateTaskRequest {
                        title: title_val.trim().to_string(),
                        description: if description().is_empty() {
                            None
                        } else {
                            Some(description())
                        },
                        priority: Some(priority()),
                        tags: parse_csv(&tags_input()),
                        root_user_id: None,
                        assignee_type: Some(assignee_type()),
                        assignee_id: if assignee_id().is_empty() {
                            "default".to_string()
                        } else {
                            assignee_id()
                        },
                        project_id: if project_id().is_empty() {
                            None
                        } else {
                            Some(project_id())
                        },
                        due_at: parse_timestamp(&due_at()),
                        dependencies: parse_csv(&dependencies_input()),
                    };
                    create_task(req).await
                }
                TaskEditMode::Edit { task_id } => {
                    let req = UpdateTaskRequest {
                        id: task_id.clone(),
                        title: Some(title_val.trim().to_string()),
                        description: Some(if description().is_empty() {
                            String::new()
                        } else {
                            description()
                        }),
                        priority: Some(priority()),
                        tags: parse_csv(&tags_input()),
                        due_at: parse_timestamp(&due_at()),
                        dependencies: parse_csv(&dependencies_input()),
                    };
                    update_task(&task_id, req).await
                }
            };
            submitting.set(false);
            match result {
                Ok(t) => {
                    toast.success(if matches!(mode_clone, TaskEditMode::Create { .. }) {
                        "任务已创建"
                    } else {
                        "任务已更新"
                    });
                    on_success.call(t);
                    on_close.call(());
                }
                Err(e) => toast.error(&e),
            }
        });
    };

    let is_create = matches!(props.mode, TaskEditMode::Create { .. });
    let title_label = if is_create { "新建任务" } else { "编辑任务" };

    rsx! {
        Modal {
            title: title_label.to_string(),
            show: props.show,
            on_close: move |_| on_close.call(()),
            footer: Some(rsx! {
                div { class: "modal-footer-actions",
                    button {
                        class: "btn btn-secondary",
                        disabled: submitting(),
                        onclick: move |_| on_close.call(()),
                        "取消"
                    }
                    button {
                        class: "btn btn-primary",
                        disabled: submitting() || loading_data(),
                        onclick: handle_submit,
                        if submitting() { "提交中..." } else { if is_create { "创建" } else { "保存" } }
                    }
                }
            }),
            if loading_data() {
                div { class: "modal-body-stack",
                    p { class: "text-muted", "加载中..." }
                }
            } else {
                div { class: "modal-body-stack",
                    // 标题
                    div {
                        label { class: "form-label", "标题 *" }
                        input {
                            class: "form-input",
                            r#type: "text",
                            placeholder: "请输入任务标题",
                            value: "{title}",
                            oninput: move |e| title.set(e.value().clone()),
                        }
                    }
                    // 描述
                    div {
                        label { class: "form-label", "描述" }
                        textarea {
                            class: "form-input",
                            placeholder: "请输入任务描述（可选）",
                            value: "{description}",
                            oninput: move |e| description.set(e.value().clone()),
                            rows: 3,
                        }
                    }
                    // 优先级
                    div {
                        label { class: "form-label", "优先级（0-10）" }
                        input {
                            class: "form-input",
                            r#type: "number",
                            min: "0",
                            max: "10",
                            value: "{priority}",
                            oninput: move |e| {
                                if let Ok(v) = e.value().parse::<i32>() {
                                    priority.set(v.clamp(0, 10));
                                }
                            },
                        }
                    }
                    // 标签
                    div {
                        label { class: "form-label", "标签（逗号分隔）" }
                        input {
                            class: "form-input",
                            r#type: "text",
                            placeholder: "例如：urgent, frontend, bug",
                            value: "{tags_input}",
                            oninput: move |e| tags_input.set(e.value().clone()),
                        }
                    }
                    // 截止时间
                    div {
                        label { class: "form-label", "截止时间（Unix 毫秒时间戳）" }
                        input {
                            class: "form-input",
                            r#type: "text",
                            placeholder: "例如：1720454400000",
                            value: "{due_at}",
                            oninput: move |e| due_at.set(e.value().clone()),
                        }
                    }
                    // 分配对象类型
                    div {
                        label { class: "form-label", "分配对象类型" }
                        select {
                            class: "form-input",
                            value: "{assignee_type()}",
                            onchange: move |e| {
                                let v = e.value().as_str();
                                if v == "User" {
                                    assignee_type.set(AssigneeType::User);
                                } else {
                                    assignee_type.set(AssigneeType::Agent);
                                }
                            },
                            option { value: "Agent", "Agent" }
                            option { value: "User", "User" }
                        }
                    }
                    // 分配对象 ID
                    div {
                        label { class: "form-label", "分配对象" }
                        if matches!(assignee_type(), AssigneeType::Agent) {
                            select {
                                class: "form-input",
                                value: "{assignee_id}",
                                onchange: move |e| assignee_id.set(e.value().clone()),
                                option { value: "", "请选择 Agent" }
                                for agent in agents.read().iter() {
                                    option { value: "{agent.id}", "{agent.name}" }
                                }
                            }
                        } else {
                            input {
                                class: "form-input",
                                r#type: "text",
                                placeholder: "请输入用户 ID",
                                value: "{assignee_id}",
                                oninput: move |e| assignee_id.set(e.value().clone()),
                            }
                        }
                    }
                    // 关联项目
                    div {
                        label { class: "form-label", "关联项目" }
                        select {
                            class: "form-input",
                            value: "{project_id}",
                            onchange: move |e| project_id.set(e.value().clone()),
                            option { value: "", "无（独立任务）" }
                            for p in projects.read().iter() {
                                option { value: "{p.id}", "{p.name}" }
                            }
                        }
                    }
                    // 前置任务
                    div {
                        label { class: "form-label", "前置任务 ID（逗号分隔）" }
                        input {
                            class: "form-input",
                            r#type: "text",
                            placeholder: "例如：task_id_1, task_id_2",
                            value: "{dependencies_input}",
                            oninput: move |e| dependencies_input.set(e.value().clone()),
                        }
                    }
                }
            }
        }
    }
}

/// 解析逗号分隔字符串为 Vec<String>（过滤空值）
fn parse_csv(s: &str) -> Option<Vec<String>> {
    let items: Vec<String> = s
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if items.is_empty() {
        None
    } else {
        Some(items)
    }
}

/// 解析时间戳字符串为 i64（毫秒）
fn parse_timestamp(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    s.parse::<i64>().ok()
}

/// 格式化时间戳为可编辑字符串
fn format_timestamp(ts: Option<i64>) -> String {
    ts.map(|t| t.to_string()).unwrap_or_default()
}
```

### Step 1.2: 验证 ListAgentsResponseItem 字段名

在 `common/src/api/hr.rs` 找到 `ListAgentsResponse` 定义（可能使用 `agents: Vec<...>` 字段），确认字段名是 `id` 和 `name`。如不同请调整 Step 1.1 的 `{agent.id}` / `{agent.name}`。

```bash
grep -n "pub struct ListAgentsResponse" common/src/api/hr.rs
```

### Step 1.3: 验证 ListProjectsResponseItem 字段名

在 `common/src/api/project.rs` 找到 `ListProjectsResponseItem` 定义，确认字段名是 `id` 和 `name`。

```bash
grep -n "pub struct ListProjectsResponseItem\|pub struct ListProjectsResponse " common/src/api/project.rs
```

### Step 1.4: 在 mod.rs 中导出

修改 `frontend/src/pages/project/mod.rs`：

```rust
//! 项目模块页面

pub mod artifacts;
pub mod project_detail;
pub mod projects;
pub mod task_detail;       // 新增
pub mod task_edit_modal;   // 新增
```

### Step 1.5: 验证编译

```bash
cd frontend && cargo check
```

**预期**：0 错误。如有 ListAgentsResponseItem / ListProjectsResponseItem 字段名错误，修正后重试。

---

## Task 2: 任务详情页

**Files:**
- Create: `frontend/src/pages/project/task_detail.rs`

### Step 2.1: 创建 task_detail.rs 完整文件

在 `frontend/src/pages/project/task_detail.rs` 创建：

```rust
//! 任务详情页 - 基本信息、状态流转、进度更新、操作按钮

use dioxus::prelude::*;
use dioxus_router::use_navigator;

use crate::api::project::{
    get_task, update_task_progress, update_task_status,
};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::store::toast::use_toast;
use common::api::GetTaskResponse;

fn status_badge(status: i32) -> &'static str {
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

fn status_text(status: i32) -> &'static str {
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

fn format_timestamp(ts: Option<i64>) -> String {
    use chrono::{Local, TimeZone};
    ts.map(|t| {
        Local
            .timestamp_opt(t / 1000, 0)
            .single()
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| t.to_string())
    })
    .unwrap_or_else(|| "—".to_string())
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

#[component]
pub fn TaskDetail(id: String) -> Element {
    let mut task = use_signal(|| None::<GetTaskResponse>);
    let mut loading = use_signal(|| true);

    // 进度更新弹窗
    let mut show_progress_modal = use_signal(|| false);
    let mut new_progress = use_signal(|| 0i32);
    let mut updating_progress = use_signal(|| false);

    let toast = use_toast();
    let navigator = use_navigator();

    // 初始加载
    let id_for_load = id.clone();
    use_effect(move || {
        loading.set(true);
        let id_clone = id_for_load.clone();
        spawn(async move {
            match get_task(&id_clone).await {
                Ok(t) => {
                    new_progress.set(t.progress);
                    task.set(Some(t));
                }
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    });

    // 状态切换
    let change_status = move |new_status: i32| {
        let id_clone = id.clone();
        spawn(async move {
            match update_task_status(&id_clone, new_status).await {
                Ok(_) => {
                    toast.success("任务状态已更新");
                    if let Ok(t) = get_task(&id_clone).await {
                        new_progress.set(t.progress);
                        task.set(Some(t));
                    }
                }
                Err(e) => toast.error(&e),
            }
        });
    };

    // 打开进度弹窗
    let open_progress_modal = move |_| {
        if let Some(t) = task.read().as_ref() {
            new_progress.set(t.progress);
        }
        show_progress_modal.set(true);
    };

    // 提交进度更新
    let submit_progress = move |_| {
        let id_clone = id.clone();
        let progress_val = new_progress();
        updating_progress.set(true);
        spawn(async move {
            match update_task_progress(&id_clone, progress_val).await {
                Ok(t) => {
                    toast.success("进度已更新");
                    task.set(Some(t));
                    show_progress_modal.set(false);
                }
                Err(e) => toast.error(&e),
            }
            updating_progress.set(false);
        });
    };

    // 返回项目（如有关联）
    let back_to_project = move |_| {
        if let Some(t) = task.read().as_ref() {
            if let Some(pid) = &t.project_id {
                navigator.push(format!("/projects/{}", pid));
            } else {
                navigator.push("/projects".to_string());
            }
        } else {
            navigator.push("/projects".to_string());
        }
    };

    rsx! {
        div { class: "page-header",
            button {
                class: "btn btn-secondary btn-sm",
                onclick: back_to_project,
                "← 返回项目"
            }
        }
        if loading() {
            div { class: "card", Loading {} }
        } else if let Some(t) = task.read().as_ref() {
            // 区域 1：基本信息
            div { class: "card",
                div { class: "card-header",
                    h2 { class: "card-title", "{t.title}" }
                    span { class: "{status_badge(t.status)}", "{status_text(t.status)}" }
                }
                div { class: "detail-grid",
                    div {
                        label { class: "form-label", "描述" }
                        if let Some(desc) = &t.description {
                            if desc.is_empty() {
                                span { class: "text-muted", "暂无描述" }
                            } else {
                                "{desc}"
                            }
                        } else {
                            span { class: "text-muted", "暂无描述" }
                        }
                    }
                    div {
                        label { class: "form-label", "优先级" }
                        span { "{t.priority}" }
                    }
                    div {
                        label { class: "form-label", "分配对象" }
                        span {
                            "{if t.assignee_type == 0 { \"用户\" } else { \"Agent\" } }: {t.assignee_id}"
                        }
                    }
                    div {
                        label { class: "form-label", "根用户" }
                        span { class: "text-mono", "{t.root_user_id}" }
                    }
                    if let Some(pid) = &t.project_id {
                        div {
                            label { class: "form-label", "所属项目" }
                            span { class: "text-mono", "{pid}" }
                        }
                    }
                    div {
                        label { class: "form-label", "创建者" }
                        span { class: "text-mono", "{t.created_by}" }
                    }
                    div {
                        label { class: "form-label", "创建时间" }
                        span { class: "text-mono text-muted", "{format_timestamp(Some(t.created_at))}" }
                    }
                    div {
                        label { class: "form-label", "更新时间" }
                        span { class: "text-mono text-muted", "{format_timestamp(Some(t.updated_at))}" }
                    }
                    if let Some(due) = t.due_at {
                        div {
                            label { class: "form-label", "截止时间" }
                            span { class: "text-mono", "{format_timestamp(Some(due))}" }
                        }
                    }
                    if let Some(start) = t.start_at {
                        div {
                            label { class: "form-label", "开始时间" }
                            span { class: "text-mono", "{format_timestamp(Some(start))}" }
                        }
                    }
                    if let Some(end) = t.end_at {
                        div {
                            label { class: "form-label", "结束时间" }
                            span { class: "text-mono", "{format_timestamp(Some(end))}" }
                        }
                    }
                }
            }

            // 区域 2：标签和依赖
            if !t.tags.is_empty() || !t.dependencies.is_empty() {
                div { class: "card",
                    div { class: "card-header",
                        h2 { class: "card-title", "标签与依赖" }
                    }
                    div { class: "detail-card-body",
                        if !t.tags.is_empty() {
                            div { class: "detail-section",
                                label { class: "form-label", "标签" }
                                div { class: "tag-list",
                                    for tag in t.tags.iter() {
                                        span { class: "badge badge-neutral tag-item", "{tag}" }
                                    }
                                }
                            }
                        }
                        if !t.dependencies.is_empty() {
                            div { class: "detail-section",
                                label { class: "form-label", "前置任务" }
                                ul { class: "dependency-list",
                                    for dep in t.dependencies.iter() {
                                        li { class: "text-mono", "{dep}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 区域 3：进度管理
            div { class: "card",
                div { class: "card-header",
                    h2 { class: "card-title", "进度管理" }
                }
                div { class: "detail-card-body",
                    div { class: "detail-section",
                        div { class: "progress-section",
                            div { class: "overview-progress",
                                div { class: "overview-progress-bar",
                                    div { class: "{progress_bar_class(t.progress)}", style: "width: {t.progress}%;" }
                                }
                                span { class: "overview-progress-text", "{t.progress}%" }
                            }
                        }
                    }
                    div { class: "detail-action-row",
                        button {
                            class: "btn btn-primary",
                            onclick: open_progress_modal,
                            "更新进度"
                        }
                    }
                }
            }

            // 区域 4：状态流转
            div { class: "card",
                div { class: "card-header",
                    h2 { class: "card-title", "状态流转" }
                }
                div { class: "detail-card-body",
                    div { class: "detail-action-row",
                        if t.status != 1 {
                            button {
                                class: "btn btn-warning",
                                onclick: move |_| change_status(1),
                                "送审"
                            }
                        }
                        if t.status != 2 {
                            button {
                                class: "btn btn-info",
                                onclick: move |_| change_status(2),
                                "待处理"
                            }
                        }
                        if t.status != 3 {
                            button {
                                class: "btn btn-primary",
                                onclick: move |_| change_status(3),
                                "开始"
                            }
                        }
                        if t.status != 4 {
                            button {
                                class: "btn btn-accent",
                                onclick: move |_| change_status(4),
                                "完成"
                            }
                        }
                        if t.status != 0 && t.status != 5 {
                            button {
                                class: "btn btn-error",
                                onclick: move |_| change_status(0),
                                "取消"
                            }
                        }
                        if t.status != 5 {
                            button {
                                class: "btn btn-secondary",
                                onclick: move |_| change_status(5),
                                "归档"
                            }
                        }
                    }
                }
            }

            // 进度更新弹窗
            Modal {
                title: "更新进度".to_string(),
                show: show_progress_modal(),
                on_close: move |_| show_progress_modal.set(false),
                footer: Some(rsx! {
                    div { class: "modal-footer-actions",
                        button {
                            class: "btn btn-secondary",
                            disabled: updating_progress(),
                            onclick: move |_| show_progress_modal.set(false),
                            "取消"
                        }
                        button {
                            class: "btn btn-primary",
                            disabled: updating_progress(),
                            onclick: submit_progress,
                            if updating_progress() { "更新中..." } else { "更新" }
                        }
                    }
                }),
                div { class: "modal-body-stack",
                    div {
                        label { class: "form-label", "进度（0-100）" }
                        input {
                            class: "form-input",
                            r#type: "number",
                            min: "0",
                            max: "100",
                            value: "{new_progress}",
                            oninput: move |e| {
                                if let Ok(v) = e.value().parse::<i32>() {
                                    new_progress.set(v.clamp(0, 100));
                                }
                            },
                        }
                    }
                    div {
                        label { class: "form-label", "预览" }
                        div { class: "overview-progress",
                            div { class: "overview-progress-bar",
                                div { class: "{progress_bar_class(new_progress())}", style: "width: {new_progress()}%;" }
                            }
                            span { class: "overview-progress-text", "{new_progress()}%" }
                        }
                    }
                }
            }
        } else {
            div { class: "card", EmptyState { icon: "❓".to_string(), message: "任务不存在".to_string() } }
        }
    }
}
```

### Step 2.2: 验证编译

```bash
cd frontend && cargo check
```

**预期**：0 错误。

---

## Task 3: 注册路由

**Files:**
- Modify: `frontend/src/pages/mod.rs`

### Step 3.1: 在 mod.rs 顶部导入新组件

修改 `frontend/src/pages/mod.rs`，在 import 区域（约 32 行后）添加：

```rust
use crate::pages::project::task_detail::TaskDetail;
```

### Step 3.2: 在 Route 枚举中添加新路由

在 `frontend/src/pages/mod.rs` 的 `Route` 枚举中（约 88 行后，`Project` 模块路由区域）添加：

```rust
    // Project 模块
    #[route("/projects")]
    ProjectList {},
    #[route("/projects/:id")]
    ProjectDetail { id: String },
    #[route("/projects/artifacts")]
    ProjectArtifacts {},
    #[route("/tasks/:id")]
    TaskDetail { id: String },  // 新增
```

### Step 3.3: 验证编译

```bash
cd frontend && cargo check
```

**预期**：0 错误。

---

## Task 4: 项目详情页集成入口

**Files:**
- Modify: `frontend/src/pages/project/project_detail.rs`

### Step 4.1: 添加 TaskEditModal 导入和状态

在文件顶部 import 区域（约 1-14 行）修改：

```rust
//! 项目详情页 - 基本信息、状态管理、任务列表、产物列表

use dioxus::prelude::*;
use dioxus_router::use_navigator;

use crate::api::project::{
    create_artifact, delete_artifact, get_project, list_artifacts, list_project_tasks,
    update_project_status, update_task_status,
};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::pages::project::task_edit_modal::{TaskEditModal, TaskEditMode};
use crate::store::toast::use_toast;
use common::api::{ArtifactDetail, CreateArtifactRequest, GetProjectResponse, TaskListItem};
use common::enums::ArtifactSourceType;
```

### Step 4.2: 在 ProjectDetail 组件中增加状态

在 `pub fn ProjectDetail(id: String)` 函数体内（约 84 行后、`// 产物新增 Modal 状态` 之前）添加：

```rust
    // 任务创建 Modal 状态
    let mut show_task_modal = use_signal(|| false);
    let navigator = use_navigator();
```

### Step 4.3: 任务列表区域头部添加"新建任务"按钮

将现有"区域 3：任务列表"卡片（约 343 行）修改为：

```rust
            // 区域 3：任务列表
            div { class: "card",
                div { class: "card-header",
                    div { class: "card-header-row",
                        h2 { class: "card-title", "任务列表" }
                        button {
                            class: "btn btn-primary btn-sm",
                            onclick: move |_| show_task_modal.set(true),
                            "+ 新建任务"
                        }
                    }
                }
                if tasks_list.is_empty() {
                    EmptyState { icon: "📋".to_string(), message: "暂无任务".to_string() }
                } else {
                    // ... 现有的任务列表表格代码保持不变
                }
            }
```

### Step 4.4: 任务行点击跳转到详情页

在任务列表表格的 `tr` 元素中（约 371 行），将：

```rust
                                        tr { key: "{task_id}",
                                            td { "{task_title}" }
```

修改为：

```rust
                                        tr {
                                            key: "{task_id}",
                                            class: "table-row-clickable",
                                            onclick: {
                                                let tid = task_id.clone();
                                                move |_| {
                                                    navigator.push(format!("/tasks/{}", tid));
                                                }
                                            },
                                            td { "{task_title}" }
```

### Step 4.5: 在 ProjectDetail 末尾添加 TaskEditModal 组件

在 ProjectDetail 组件的最外层 `rsx!` 块的最后一个 `div` 之后、`</...>` 之前添加 TaskEditModal。位置：现有 Modal（"新增产物 Modal"）之后，ProjectDetail 函数的 rsx! 闭合标签之前。

```rust
            // 新建任务 Modal
            TaskEditModal {
                mode: TaskEditMode::Create { project_id: Some(id.clone()) },
                show: show_task_modal(),
                on_close: move |_| show_task_modal.set(false),
                on_success: move |_| {
                    show_task_modal.set(false);
                    // 刷新任务列表
                    let pid = id.clone();
                    spawn(async move {
                        if let Ok(resp) = list_project_tasks(&pid).await {
                            tasks.set(resp.tasks);
                        }
                    });
                },
            }
```

### Step 4.6: 验证编译

```bash
cd frontend && cargo check
```

**预期**：0 错误。

---

## Task 5: CSS 样式补充

**Files:**
- Modify: `frontend/index.html`

### Step 5.1: 添加任务详情页/弹窗所需样式

在 `frontend/index.html` 的 `<style>` 标签中追加以下 CSS：

```css
/* 卡片头部行布局（标题 + 操作按钮） */
.card-header-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
}

/* 详情区域分区 */
.detail-section {
    margin-bottom: 16px;
}

.detail-section:last-child {
    margin-bottom: 0;
}

/* 可点击行 */
.table-row-clickable {
    cursor: pointer;
    transition: background-color 0.15s;
}

.table-row-clickable:hover {
    background-color: var(--surface-hover, rgba(0, 0, 0, 0.04));
}

/* 进度展示区域 */
.progress-section {
    margin-bottom: 16px;
}

/* 依赖列表 */
.dependency-list {
    list-style: disc;
    padding-left: 24px;
    margin: 0;
}

.dependency-list li {
    padding: 4px 0;
    color: var(--text-secondary, #666);
}

/* 页面头部（返回按钮） */
.page-header {
    margin-bottom: 16px;
    display: flex;
    align-items: center;
    gap: 12px;
}
```

### Step 5.2: 验证编译

```bash
cd frontend && cargo check
```

**预期**：0 错误。

---

## Task 6: 端到端验证

### Step 6.1: 后端测试

```bash
cd /Users/aman/Technology/rust/ai_orz && cargo test
```

**预期**：测试 100% 通过（不新增后端测试，但需确保现有测试不受影响）。

### Step 6.2: 前端编译检查

```bash
cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check
```

**预期**：0 错误，0 警告（warnings 允许存在但需检查是否有未使用导入）。

### Step 6.3: 手动验证清单

启动前后端服务后，按以下清单验证：

1. **任务创建**
   - 进入任意项目详情页
   - 点击"新建任务"按钮 → 弹窗打开
   - 填写标题、优先级、选择 Agent、确认项目已预选
   - 提交 → toast 提示"任务已创建"，弹窗关闭，任务列表自动刷新
   - 任务列表中可见新任务

2. **任务详情**
   - 在项目详情页点击任一任务行 → 跳转到 `/tasks/{id}` 详情页
   - 详情页显示完整信息：标题、状态、进度、标签、依赖等
   - 点击"更新进度" → 弹窗打开，可拖动或输入新值
   - 提交 → 进度条更新，toast 提示成功

3. **任务状态流转**
   - 在详情页点击"开始"按钮 → 状态变为"进行中"
   - 点击"完成"按钮 → 状态变为"已完成"，进度自动为 100%
   - 点击"取消"按钮 → 状态变为"已取消"

4. **任务编辑**
   - 在项目详情页（或后续任务详情页）打开编辑弹窗
   - 修改字段后提交 → 任务更新

5. **返回导航**
   - 在任务详情页点击"← 返回项目" → 跳回原项目（如有）或项目列表

---

## 风险与注意事项

### 已知风险

| 风险 | 应对 |
|------|------|
| `ListAgentsResponseItem` 字段名不匹配 | Step 1.2 已加 `grep` 验证 |
| `AssigneeType` 在 `common::enums` 路径下不存在 | 需先 `grep` 确认；如不存在则前端硬编码为 `AssigneeType::Agent` |
| `chrono` 依赖未在 frontend/Cargo.toml | 检查 `frontend/Cargo.toml`，如有需要添加 `chrono = { version = "0.4", features = ["serde"] }` |
| `use_navigator` 路径不对 | 已确认在 `agent_detail.rs` 中使用 `dioxus_router::use_navigator` |
| 任务列表行点击冲突 | 已加 `e.stop_propagation()` 的注意点：表格行默认无内嵌按钮冲突 |

### 实施建议

1. **优先实施顺序**：Task 1 → Task 2 → Task 3 → Task 4 → Task 5 → Task 6
2. **每个 Task 完成后立即 `cargo check`**，避免错误累积
3. **如遇 Dioxus 0.7 闭包所有权问题**：参考 `frontend/src/pages/hr/agent_detail.rs` 中的模式（Signal 克隆 + `let` 绑定）

---

## 完成标准

- [ ] Task 1: TaskEditModal 组件可独立编译，create/edit 模式均可用
- [ ] Task 2: TaskDetail 页面可独立访问 `/tasks/{id}`
- [ ] Task 3: 路由注册成功，浏览器可访问
- [ ] Task 4: 项目详情页"新建任务"按钮可用，任务行可点击跳转
- [ ] Task 5: CSS 样式美观，符合项目设计系统
- [ ] Task 6: 端到端验证通过，后端测试 100% 通过，前端 0 编译错误
