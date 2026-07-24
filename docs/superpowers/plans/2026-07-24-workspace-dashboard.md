# Workspace 工作台驾驶舱 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 `/workspace` 从 Canvas 试点 Demo 页升级为真实业务工作台驾驶舱，左侧 Project 列表 + 右侧 Agent 列表 + 中间 Canvas 关系图，展示运行时状态并支持点击跳转。

**Architecture:** 三栏布局（左 Project 浮层 / 中 Canvas 关系图 / 右 Agent 浮层）+ 顶部汇总状态条。中间区域通过 `WorkspaceView` 状态机切换三种视图：Global（默认 Project-Agent 关联）/ ProjectDetail（选中 Project 的 Task + Agent）/ AgentDetail（选中 Agent 的 Task + Project）。复用 CanvasScene + 力导向布局，节点颜色按状态区分，点击节点跳转详情页。

**Tech Stack:** Dioxus 0.7 + web-sys Canvas 2D + CanvasScene 组件 + Signal 状态管理 + reqwest API 调用

---

## 文件结构

### 新建文件
- `frontend/src/pages/workspace.rs` — **重写**，从 Demo 改为真实业务工作台
- `frontend/src/components/workspace_graph.rs` — **新建**，workspace 专用 Canvas 关系图组件，直接用 CanvasScene + 业务节点构造逻辑
- `frontend/src/hooks/use_workspace_data.rs` — **新建**，数据加载 hook，封装 list_projects + list_agents + list_tasks 调用和 Signal 管理

### 修改文件
- `frontend/src/components/canvas_scene.rs` — CanvasNode 加 `node_type: Option<String>` 字段
- `frontend/src/components/relation_graph.rs` — CanvasNode 构造点同步加 node_type
- `frontend/src/pages/hr/agent_detail.rs` — CanvasNode 构造点同步加 node_type

### 不修改
- `frontend/src/components/force_layout.rs` — 力导向布局算法不变
- `frontend/src/components/particles.rs` — 粒子系统不变
- 后端代码 — 第一期不改动后端

---

## 关键设计

### WorkspaceView 状态机

```rust
enum WorkspaceView {
    Global,                // 默认：所有 Project ↔ Agent 关联
    ProjectDetail(String), // 选中某 Project：其 Task + Agent
    AgentDetail(String),    // 选中某 Agent：其 Task + Project
}
```

### 节点颜色规范

| 实体 | 状态 | 颜色 | 说明 |
|------|------|------|------|
| Project | Active | `#10b981` 绿色 | 活跃 |
| Project | InProgress | `#3b82f6` 蓝色 | 进行中 |
| Project | PendingReview | `#f59e0b` 橙色 | 待评审 |
| Project | Completed | `#6b7280` 灰色 | 已完成 |
| Project | 其他 | `#9ca3af` 浅灰 | 默认 |
| Agent | Idle (runtime_state=0) | `#10b981` 绿色 | 空闲 |
| Agent | Resting (runtime_state=1) | `#f59e0b` 橙色 | 休息中 |
| Agent | Busy (runtime_state=2) | `#ef4444` 红色 | 忙碌 |
| Task | InProgress | `#3b82f6` 蓝色 | 进行中 |
| Task | Completed | `#6b7280` 灰色 | 已完成 |
| Task | 其他 | `#9ca3af` 浅灰 | 默认 |

### 节点类型标识

`CanvasNode.node_type` 字段值：`"project"` / `"agent"` / `"task"`，用于点击回调判断跳转目标。

### CanvasNode 扩展

```rust
pub struct CanvasNode {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub radius: f64,
    pub label: String,
    pub color: String,
    pub node_type: Option<String>,  // 新增：节点类型标识
}
```

---

### Task 1: 扩展 CanvasNode 加 node_type 字段

**Files:**
- Modify: `frontend/src/components/canvas_scene.rs:22-30`
- Modify: `frontend/src/components/relation_graph.rs`（CanvasNode 构造点）
- Modify: `frontend/src/pages/hr/agent_detail.rs`（如有 CanvasNode 直接构造）
- Modify: `frontend/src/pages/workspace.rs`（sample_nodes 构造点，后续会被重写但先保持编译通过）

- [ ] **Step 1: 修改 CanvasNode 结构定义**

在 `frontend/src/components/canvas_scene.rs` 中：

```rust
/// Canvas 渲染节点（通用数据结构，业务场景填充字段）
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CanvasNode {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub radius: f64,
    pub label: String,
    pub color: String,
    /// 节点类型标识（如 "project"/"agent"/"task"），用于点击回调判断
    pub node_type: Option<String>,
}
```

- [ ] **Step 2: 修改 canvas_scene.rs 中所有 CanvasNode 构造点**

搜索 `canvas_scene.rs` 中所有 `CanvasNode {` 构造点，为每个加 `node_type: None,`。

特别关注 `use_effect` 中的 `merged.push(CanvasNode { ... })` 处（props 同步逻辑），加 `node_type: new_node.node_type.clone(),`。

`Default` impl 中加 `node_type: None,`。

- [ ] **Step 3: 修改 relation_graph.rs 中 CanvasNode 构造点**

在 `frontend/src/components/relation_graph.rs` 中，中心节点和关联节点构造处：

```rust
// 中心节点
let mut nodes = vec![CanvasNode {
    id: center_id.clone(),
    x: 0.0,
    y: 0.0,
    radius: 35.0,
    label: center_name,
    color: center_color,
    node_type: center_kind.clone(),  // 新增
}];
for item in &related {
    nodes.push(CanvasNode {
        id: item.id.clone(),
        x: 0.0,
        y: 0.0,
        radius: 22.0,
        label: item.name.clone(),
        color: related_color.clone(),
        node_type: item.kind.clone(),  // 新增
    });
}
```

- [ ] **Step 4: 修改 workspace.rs sample_nodes 构造点**

在 `frontend/src/pages/workspace.rs` 的 `sample_nodes()` 函数中，每个 `CanvasNode { ... }` 加 `node_type: None,`（后续 Task 会重写此文件，但先保持编译通过）。

- [ ] **Step 5: 验证编译**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check --release 2>&1 | tail -20`
Expected: 编译通过，无错误（可能有既有警告）

- [ ] **Step 6: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/components/canvas_scene.rs frontend/src/components/relation_graph.rs frontend/src/pages/workspace.rs frontend/src/pages/hr/agent_detail.rs
git commit -m "refactor(canvas): CanvasNode 新增 node_type 字段用于节点类型标识"
```

---

### Task 2: 新建 use_workspace_data hook

**Files:**
- Create: `frontend/src/hooks/use_workspace_data.rs`
- Modify: `frontend/src/hooks/mod.rs`（添加 mod 声明）

**职责**：封装 list_projects + list_agents + list_tasks 的调用和 Signal 管理，提供数据给 workspace 页面。

- [ ] **Step 1: 在 hooks/mod.rs 添加模块声明**

查看 `frontend/src/hooks/mod.rs` 当前内容，添加 `pub mod use_workspace_data;`。

- [ ] **Step 2: 创建 use_workspace_data.rs**

```rust
//! Workspace 工作台数据加载 hook
//!
//! 封装 list_projects + list_agents + list_tasks 的调用和 Signal 管理。
//! 提供 refresh() 方法供定时刷新。

use dioxus::prelude::*;
use common::api::{AgentListItem, ProjectListItem, TaskListItem};
use crate::api::{hr::list_agents, project::list_projects};
use crate::api::project::list_tasks;
use crate::store::toast::use_toast;

/// Workspace 工作台数据
#[derive(Debug, Clone, Default)]
pub struct WorkspaceData {
    pub projects: Vec<ProjectListItem>,
    pub agents: Vec<AgentListItem>,
    pub tasks: Vec<TaskListItem>,
}

/// Workspace 数据加载 hook
///
/// 返回 (data_signal, refresh_fn)，调用方可在 use_effect 中触发首次加载，
/// 也可在交互时调用 refresh() 重新加载。
pub fn use_workspace_data() -> (Signal<Option<WorkspaceData>>, impl FnMut()) {
    let data: Signal<Option<WorkspaceData>> = use_signal(|| None);
    let toast = use_toast();
    let mut refresh_count = use_signal(|| 0u32);

    let load = move || {
        spawn(async move {
            let projects = list_projects().await.unwrap_or_default();
            let agents = list_agents().await.unwrap_or_default();
            // 加载所有任务（不传过滤参数）
            let tasks = list_tasks(None, None, None, None).await.unwrap_or_default();

            // 失败时 toast 提示但不阻断
            if projects.is_empty() && agents.is_empty() {
                toast.info("暂无 Project 和 Agent 数据");
            }

            data.set(Some(WorkspaceData { projects, agents, tasks }));
            refresh_count += 1;
        });
    };

    // 首次加载
    use_effect(move || {
        load();
    });

    // 返回 refresh 函数
    (data, move || { load(); })
}
```

**注意**：`list_projects` 和 `list_agents` 的实际签名需先确认。如果 `list_projects()` 返回的是 `ListProjectsResponse` 而非 `Vec<ProjectListItem>`，需要 `.projects` 字段提取。`list_tasks` 签名是 `list_tasks(project_id: Option<String>, status: Option<i32>, assignee_id: Option<String>, assignee_type: Option<i32>)`。

- [ ] **Step 3: 验证 API 函数签名**

Run: 检查 `frontend/src/api/project.rs` 中 `list_projects` 和 `list_tasks` 的实际签名，以及 `frontend/src/api/hr.rs` 中 `list_agents` 的签名。

如果签名不符（如 `list_projects` 返回 `ListProjectsResponse` 而非 `Vec`），调整 Step 2 的代码。

- [ ] **Step 4: 调整代码适配实际 API 签名**

根据 Step 3 的检查结果，修正 `use_workspace_data.rs` 中的 API 调用代码。

常见调整：
- `list_projects()` 可能返回 `Result<ListProjectsResponse, ApiError>`，需 `.projects` 提取
- `list_agents()` 可能返回 `Result<ListAgentsResponse, ApiError>`，需 `.agents` 提取
- `list_tasks(...)` 可能返回 `Result<ListTasksResponse, ApiError>`，需 `.tasks` 提取

- [ ] **Step 5: 验证编译**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check --release 2>&1 | tail -20`
Expected: 编译通过

- [ ] **Step 6: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/hooks/mod.rs frontend/src/hooks/use_workspace_data.rs
git commit -m "feat(workspace): 新增 use_workspace_data hook 封装数据加载"
```

---

### Task 3: 新建 WorkspaceGraph 组件

**Files:**
- Create: `frontend/src/components/workspace_graph.rs`
- Modify: `frontend/src/components/mod.rs`（添加 mod 声明）

**职责**：workspace 专用 Canvas 关系图组件，直接用 CanvasScene，根据 `WorkspaceView` 状态机切换三种视图模式，构造对应的 nodes/edges。

- [ ] **Step 1: 在 components/mod.rs 添加模块声明**

在 `frontend/src/components/mod.rs` 添加 `pub mod workspace_graph;`。

- [ ] **Step 2: 创建 workspace_graph.rs**

```rust
//! Workspace 专用 Canvas 关系图组件
//!
//! 根据 WorkspaceView 状态机切换三种视图：
//! - Global：所有 Project ↔ Agent 的关联（通过 Task.assignee_id 和 Task.project_id 推断）
//! - ProjectDetail：选中 Project 的 Task + Agent 节点
//! - AgentDetail：选中 Agent 的 Task + Project 节点
//!
//! 节点颜色按状态区分（见颜色规范表），点击节点跳转对应详情页。

use dioxus::prelude::*;
use dioxus_router::use_navigator;
use common::api::{AgentListItem, ProjectListItem, TaskListItem};
use crate::components::canvas_scene::{CanvasEdge, CanvasNode, CanvasScene};

/// Workspace 视图模式
#[derive(Debug, Clone, PartialEq)]
pub enum WorkspaceView {
    /// 全局视图：Project ↔ Agent 关联
    Global,
    /// Project 详情视图：选中 Project 的 Task + Agent
    ProjectDetail(String),
    /// Agent 详情视图：选中 Agent 的 Task + Project
    AgentDetail(String),
}

/// WorkspaceGraph Props
#[derive(Props, Clone, PartialEq)]
pub struct WorkspaceGraphProps {
    /// 当前视图模式
    pub view: WorkspaceView,
    /// 全部 Project 列表
    pub projects: Vec<ProjectListItem>,
    /// 全部 Agent 列表
    pub agents: Vec<AgentListItem>,
    /// 全部 Task 列表
    pub tasks: Vec<TaskListItem>,
    /// Canvas 宽度
    pub width: f64,
    /// Canvas 高度
    pub height: f64,
}

/// Project 状态颜色
fn project_status_color(status: i32) -> String {
    match status {
        1 => "#10b981".to_string(),   // Active 绿
        3 => "#3b82f6".to_string(),   // InProgress 蓝
        2 => "#f59e0b".to_string(),   // PendingReview 橙
        4 => "#6b7280".to_string(),   // Completed 灰
        _ => "#9ca3af".to_string(),   // 其他浅灰
    }
}

/// Agent 运行时状态颜色（基于 runtime_state）
fn agent_runtime_color(runtime_state: i32) -> String {
    match runtime_state {
        0 => "#10b981".to_string(),   // Idle 绿
        1 => "#f59e0b".to_string(),   // Resting 橙
        2 => "#ef4444".to_string(),    // Busy 红
        _ => "#9ca3af".to_string(),
    }
}

/// Task 状态颜色
fn task_status_color(status: i32) -> String {
    match status {
        1 => "#3b82f6".to_string(),   // InProgress 蓝
        2 => "#6b7280".to_string(),   // Completed 灰
        _ => "#9ca3af".to_string(),
    }
}

/// 构建 Global 视图的节点和边
///
/// Project 节点 + Agent 节点，通过 Task 关联（Task.project_id → Project, Task.assignee_id → Agent）
fn build_global_view(projects: &[ProjectListItem], agents: &[AgentListItem], tasks: &[TaskListItem]) -> (Vec<CanvasNode>, Vec<CanvasEdge>) {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // Project 节点
    for p in projects {
        nodes.push(CanvasNode {
            id: format!("project:{}", p.id),
            x: 0.0, y: 0.0,
            radius: 28.0,
            label: p.name.clone(),
            color: project_status_color(p.status),
            node_type: Some("project".to_string()),
        });
    }

    // Agent 节点
    for a in agents {
        nodes.push(CanvasNode {
            id: format!("agent:{}", a.id),
            x: 0.0, y: 0.0,
            radius: 25.0,
            label: a.name.clone(),
            color: agent_runtime_color(a.runtime_state),
            node_type: Some("agent".to_string()),
        });
    }

    // 通过 Task 推断 Project ↔ Agent 关联
    // 每个 Task 有 project_id 和 assignee_id，如果 assignee_type=1（Agent），则建立 Project → Agent 边
    let mut edge_set: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    for t in tasks {
        if let Some(pid) = &t.project_id {
            if t.assignee_type == 1 { // Agent
                let from = format!("project:{}", pid);
                let to = format!("agent:{}", t.assignee_id);
                if from != to {
                    edge_set.insert((from, to));
                }
            }
        }
    }
    for (from, to) in edge_set {
        edges.push(CanvasEdge { from_id: from, to_id: to });
    }

    (nodes, edges)
}

/// 构建 ProjectDetail 视图的节点和边
///
/// 选中 Project 的 Task 节点 + 关联 Agent 节点，Task → Agent 边
fn build_project_detail_view(
    project_id: &str,
    project: &ProjectListItem,
    agents: &[AgentListItem],
    tasks: &[TaskListItem],
) -> (Vec<CanvasNode>, Vec<CanvasEdge>) {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // 中心 Project 节点
    nodes.push(CanvasNode {
        id: format!("project:{}", project.id),
        x: 0.0, y: 0.0,
        radius: 35.0,
        label: project.name.clone(),
        color: project_status_color(project.status),
        node_type: Some("project".to_string()),
    });

    // 该 Project 的 Task 节点
    let project_tasks: Vec<&TaskListItem> = tasks.iter()
        .filter(|t| t.project_id.as_deref() == Some(project_id))
        .collect();

    for t in project_tasks {
        nodes.push(CanvasNode {
            id: format!("task:{}", t.id),
            x: 0.0, y: 0.0,
            radius: 20.0,
            label: t.title.clone(),
            color: task_status_color(t.status),
            node_type: Some("task".to_string()),
        });
        // Project → Task 边
        edges.push(CanvasEdge {
            from_id: format!("project:{}", project.id),
            to_id: format!("task:{}", t.id),
        });
    }

    // 关联 Agent 节点（去重：该 Project 的 Task 分配到的 Agent）
    let agent_ids: std::collections::HashSet<String> = project_tasks.iter()
        .filter(|t| t.assignee_type == 1)
        .map(|t| t.assignee_id.clone())
        .collect();

    for aid in agent_ids {
        if let Some(a) = agents.iter().find(|a| a.id == aid) {
            nodes.push(CanvasNode {
                id: format!("agent:{}", a.id),
                x: 0.0, y: 0.0,
                radius: 25.0,
                label: a.name.clone(),
                color: agent_runtime_color(a.runtime_state),
                node_type: Some("agent".to_string()),
            });
        }
    }

    // Task → Agent 边
    for t in project_tasks {
        if t.assignee_type == 1 {
            edges.push(CanvasEdge {
                from_id: format!("task:{}", t.id),
                to_id: format!("agent:{}", t.assignee_id),
            });
        }
    }

    (nodes, edges)
}

/// 构建 AgentDetail 视图的节点和边
///
/// 选中 Agent 的 Task 节点 + 关联 Project 节点
fn build_agent_detail_view(
    agent_id: &str,
    agent: &AgentListItem,
    projects: &[ProjectListItem],
    tasks: &[TaskListItem],
) -> (Vec<CanvasNode>, Vec<CanvasEdge>) {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // 中心 Agent 节点
    nodes.push(CanvasNode {
        id: format!("agent:{}", agent.id),
        x: 0.0, y: 0.0,
        radius: 35.0,
        label: agent.name.clone(),
        color: agent_runtime_color(agent.runtime_state),
        node_type: Some("agent".to_string()),
    });

    // 该 Agent 的 Task 节点
    let agent_tasks: Vec<&TaskListItem> = tasks.iter()
        .filter(|t| t.assignee_type == 1 && t.assignee_id == agent_id)
        .collect();

    for t in agent_tasks {
        nodes.push(CanvasNode {
            id: format!("task:{}", t.id),
            x: 0.0, y: 0.0,
            radius: 20.0,
            label: t.title.clone(),
            color: task_status_color(t.status),
            node_type: Some("task".to_string()),
        });
        // Agent → Task 边
        edges.push(CanvasEdge {
            from_id: format!("agent:{}", agent.id),
            to_id: format!("task:{}", t.id),
        });
    }

    // 关联 Project 节点（去重）
    let project_ids: std::collections::HashSet<String> = agent_tasks.iter()
        .filter_map(|t| t.project_id.clone())
        .collect();

    for pid in project_ids {
        if let Some(p) = projects.iter().find(|p| p.id == pid) {
            nodes.push(CanvasNode {
                id: format!("project:{}", p.id),
                x: 0.0, y: 0.0,
                radius: 28.0,
                label: p.name.clone(),
                color: project_status_color(p.status),
                node_type: Some("project".to_string()),
            });
            // Task → Project 边
            // 注意：这里不直接连 Agent → Project，而是通过 Task 间接关联
        }
    }

    // Task → Project 边
    for t in agent_tasks {
        if let Some(pid) = &t.project_id {
            edges.push(CanvasEdge {
                from_id: format!("task:{}", t.id),
                to_id: format!("project:{}", pid),
            });
        }
    }

    (nodes, edges)
}

#[component]
pub fn WorkspaceGraph(props: WorkspaceGraphProps) -> Element {
    let view = props.view.clone();
    let projects = props.projects.clone();
    let agents = props.agents.clone();
    let tasks = props.tasks.clone();
    let width = props.width;
    let height = props.height;

    // 根据视图模式构建节点和边
    let (nodes, edges) = match &view {
        WorkspaceView::Global => build_global_view(&projects, &agents, &tasks),
        WorkspaceView::ProjectDetail(pid) => {
            if let Some(p) = projects.iter().find(|p| p.id == *pid) {
                build_project_detail_view(pid, p, &agents, &tasks)
            } else {
                (Vec::new(), Vec::new())
            }
        }
        WorkspaceView::AgentDetail(aid) => {
            if let Some(a) = agents.iter().find(|a| a.id == *aid) {
                build_agent_detail_view(aid, a, &projects, &tasks)
            } else {
                (Vec::new(), Vec::new())
            }
        }
    };

    // 节点 id → 类型 查找表，供点击回调判断跳转
    let mut click_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for n in &nodes {
        if let Some(nt) = &n.node_type {
            // 提取真实 ID（去掉 "project:"/"agent:"/"task:" 前缀）
            let real_id = n.id.splitn(2, ':').nth(1).unwrap_or(&n.id).to_string();
            click_map.insert(n.id.clone(), (nt.clone(), real_id));
        }
    }

    let navigator = use_navigator();

    let node_count = nodes.len();

    rsx! {
        div { class: "flex flex-col items-center w-full",
            if node_count == 0 {
                div { class: "text-center py-12",
                    div { class: "text-5xl mb-4 opacity-30", "📊" }
                    div { class: "text-base-content/70", "暂无数据可展示" }
                }
            } else {
                CanvasScene {
                    width: width,
                    height: height,
                    nodes: nodes,
                    edges: edges,
                    enable_force_layout: true,
                    enable_data_flow_particles: true,
                    enable_glow_particles: true,
                    enable_background_particles: true,
                    enable_birth_death_particles: true,
                    on_node_click: Some(EventHandler::new(move |node_id: String| {
                        if let Some((nt, real_id)) = click_map.get(&node_id) {
                            match nt.as_str() {
                                "project" => navigator.push(format!("/projects/{}", real_id)),
                                "agent" => navigator.push(format!("/hr/agents/{}", real_id)),
                                "task" => navigator.push(format!("/tasks/{}", real_id)),
                                _ => {}
                            }
                        }
                    })),
                }
            }
        }
    }
}
```

- [ ] **Step 3: 验证编译**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check --release 2>&1 | tail -20`
Expected: 编译通过

- [ ] **Step 4: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/components/mod.rs frontend/src/components/workspace_graph.rs
git commit -m "feat(workspace): 新增 WorkspaceGraph 组件支持三种视图模式"
```

---

### Task 4: 重写 workspace.rs 工作台页面

**Files:**
- Modify: `frontend/src/pages/workspace.rs`（完整重写）

**职责**：三栏布局 + 顶部汇总状态条 + WorkspaceView 状态机切换。

- [ ] **Step 1: 重写 workspace.rs**

```rust
//! 工作台页面（驾驶舱）
//!
//! 三栏布局：左侧 Project 列表浮层 / 中间 Canvas 关系图 / 右侧 Agent 列表浮层
//! 顶部汇总状态条：项目数 / Agent 数 / 活跃任务 / 忙碌 Agent
//! 中间区域通过 WorkspaceView 状态机切换三种视图：
//! - Global：Project ↔ Agent 关联（默认）
//! - ProjectDetail：选中 Project 的 Task + Agent
//! - AgentDetail：选中 Agent 的 Task + Project

use dioxus::prelude::*;
use dioxus_router::use_navigator;

use crate::components::workspace_graph::{WorkspaceGraph, WorkspaceView};
use crate::hooks::use_workspace_data::use_workspace_data;
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;

/// Project 状态标签
fn project_status_label(status: i32) -> &'static str {
    match status {
        1 => "活跃",
        2 => "待评审",
        3 => "进行中",
        4 => "已完成",
        5 => "已归档",
        _ => "未知",
    }
}

/// Agent 运行时状态标签
fn agent_runtime_label(runtime_state: i32) -> &'static str {
    match runtime_state {
        0 => "空闲",
        1 => "休息中",
        2 => "忙碌",
        _ => "未知",
    }
}

/// Agent 运行时状态颜色 class（用于 badge）
fn agent_runtime_badge_class(runtime_state: i32) -> &'static str {
    match runtime_state {
        0 => "badge badge-success",
        1 => "badge badge-warning",
        2 => "badge badge-error",
        _ => "badge badge-ghost",
    }
}

#[component]
pub fn Workspace() -> Element {
    let (data_signal, refresh) = use_workspace_data();
    let mut current_view = use_signal(|| WorkspaceView::Global);
    let navigator = use_navigator();
    let toast = use_toast();

    let data = data_signal.read().clone();

    rsx! {
        AppLayout {
            div { class: "flex flex-col h-full gap-4",
                // === 顶部汇总状态条 ===
                {data.as_ref().map(|d| {
                    let project_count = d.projects.len();
                    let agent_count = d.agents.len();
                    let active_task_count = d.tasks.iter().filter(|t| t.status == 1).count();
                    let busy_agent_count = d.agents.iter().filter(|a| a.runtime_state == 2).count();

                    rsx! {
                        div { class: "grid grid-cols-2 md:grid-cols-4 gap-3",
                            div { class: "stat bg-base-100 rounded-lg shadow-sm",
                                div { class: "stat-title", "项目" }
                                div { class: "stat-value text-primary", "{project_count}" }
                            }
                            div { class: "stat bg-base-100 rounded-lg shadow-sm",
                                div { class: "stat-title", "Agent" }
                                div { class: "stat-value text-info", "{agent_count}" }
                            }
                            div { class: "stat bg-base-100 rounded-lg shadow-sm",
                                div { class: "stat-title", "活跃任务" }
                                div { class: "stat-value text-secondary", "{active_task_count}" }
                            }
                            div { class: "stat bg-base-100 rounded-lg shadow-sm",
                                div { class: "stat-title", "忙碌 Agent" }
                                div { class: "stat-value text-error", "{busy_agent_count}" }
                            }
                        }
                    }
                })}

                // === 三栏布局：左 Project / 中 Canvas / 右 Agent ===
                div { class: "flex gap-4 flex-1 min-h-0",
                    // 左侧 Project 列表浮层
                    {data.as_ref().map(|d| {
                        rsx! {
                            div { class: "w-64 flex-shrink-0 bg-base-100 rounded-lg shadow-md overflow-y-auto",
                                div { class: "p-3 sticky top-0 bg-base-100 border-b border-base-200 z-10",
                                    div { class: "flex justify-between items-center",
                                        h3 { class: "text-sm font-semibold", "项目列表" }
                                        button {
                                            class: "btn btn-ghost btn-xs",
                                            onclick: move |_| { current_view.set(WorkspaceView::Global); },
                                            "全局"
                                        }
                                    }
                                }
                                div { class: "divide-y divide-base-200",
                                    for p in d.projects.iter() {
                                        {
                                            let pid = p.id.clone();
                                            let is_selected = matches!(*current_view.read(), WorkspaceView::ProjectDetail(ref id) if id == &pid);
                                            rsx! {
                                                button {
                                                    class: "w-full text-left p-3 hover:bg-base-200 transition-colors {if is_selected { \"bg-base-200\" } else { \"\" }}",
                                                    onclick: move |_| {
                                                        current_view.set(WorkspaceView::ProjectDetail(pid.clone()));
                                                    },
                                                    div { class: "flex justify-between items-start",
                                                        span { class: "text-sm font-medium truncate", "{p.name}" }
                                                        span { class: "badge badge-xs badge-ghost ml-2",
                                                            "{project_status_label(p.status)}"
                                                        }
                                                    }
                                                    if !p.tags.is_empty() {
                                                        div { class: "flex flex-wrap gap-1 mt-1",
                                                            for tag in p.tags.iter().take(2) {
                                                                span { class: "badge badge-xs badge-ghost", "{tag}" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if d.projects.is_empty() {
                                        div { class: "p-4 text-center text-sm text-base-content/50",
                                            "暂无项目"
                                        }
                                    }
                                }
                            }
                        }
                    })}

                    // 中间 Canvas 关系图
                    div { class: "flex-1 bg-base-100 rounded-lg shadow-md p-4 min-w-0",
                        {data.as_ref().map(|d| {
                            rsx! {
                                WorkspaceGraph {
                                    view: current_view.read().clone(),
                                    projects: d.projects.clone(),
                                    agents: d.agents.clone(),
                                    tasks: d.tasks.clone(),
                                    width: 700.0,
                                    height: 500.0,
                                }
                            }
                        })}
                        {data.is_none().then(|| rsx! {
                            div { class: "flex items-center justify-center h-full",
                                span { class: "loading loading-spinner loading-lg" }
                            }
                        })}
                    }

                    // 右侧 Agent 列表浮层
                    {data.as_ref().map(|d| {
                        rsx! {
                            div { class: "w-64 flex-shrink-0 bg-base-100 rounded-lg shadow-md overflow-y-auto",
                                div { class: "p-3 sticky top-0 bg-base-100 border-b border-base-200 z-10",
                                    div { class: "flex justify-between items-center",
                                        h3 { class: "text-sm font-semibold", "Agent 列表" }
                                        button {
                                            class: "btn btn-ghost btn-xs",
                                            onclick: move |_| { current_view.set(WorkspaceView::Global); },
                                            "全局"
                                        }
                                    }
                                }
                                div { class: "divide-y divide-base-200",
                                    for a in d.agents.iter() {
                                        {
                                            let aid = a.id.clone();
                                            let is_selected = matches!(*current_view.read(), WorkspaceView::AgentDetail(ref id) if id == &aid);
                                            rsx! {
                                                button {
                                                    class: "w-full text-left p-3 hover:bg-base-200 transition-colors {if is_selected { \"bg-base-200\" } else { \"\" }}",
                                                    onclick: move |_| {
                                                        current_view.set(WorkspaceView::AgentDetail(aid.clone()));
                                                    },
                                                    div { class: "flex justify-between items-start",
                                                        span { class: "text-sm font-medium truncate", "{a.name}" }
                                                        span { class: "badge badge-xs ml-2 {agent_runtime_badge_class(a.runtime_state)}",
                                                            "{agent_runtime_label(a.runtime_state)}"
                                                        }
                                                    }
                                                    div { class: "text-xs text-base-content/60 mt-1",
                                                        "{a.kind}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if d.agents.is_empty() {
                                        div { class: "p-4 text-center text-sm text-base-content/50",
                                            "暂无 Agent"
                                        }
                                    }
                                }
                            }
                        }
                    })}
                }

                // === 底部图例 + 刷新按钮 ===
                div { class: "flex justify-between items-center text-xs text-base-content/70",
                    div { class: "flex gap-4",
                        span { class: "flex items-center gap-1",
                            span { class: "w-3 h-3 rounded-full bg-success" }
                            "空闲/活跃"
                        }
                        span { class: "flex items-center gap-1",
                            span { class: "w-3 h-3 rounded-full bg-warning" }
                            "休息/待评审"
                        }
                        span { class: "flex items-center gap-1",
                            span { class: "w-3 h-3 rounded-full bg-error" }
                            "忙碌"
                        }
                        span { class: "flex items-center gap-1",
                            span { class: "w-3 h-3 rounded-full bg-info" }
                            "进行中"
                        }
                    }
                    button {
                        class: "btn btn-ghost btn-xs",
                        onclick: move |_| { refresh(); toast.info("已刷新数据"); },
                        "🔄 刷新"
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 2: 验证编译**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check --release 2>&1 | tail -30`
Expected: 编译通过，可能有少量警告

- [ ] **Step 3: 修复编译错误（如有）**

常见问题：
- `list_projects` / `list_agents` / `list_tasks` 返回类型不符 → 调整 use_workspace_data.rs
- `ProjectListItem` / `AgentListItem` / `TaskListItem` 字段名不符 → 对照 common crate 调整
- `WorkspaceView` PartialEq 缺失 → 确认已 derive PartialEq

- [ ] **Step 4: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/pages/workspace.rs
git commit -m "feat(workspace): 重写工作台为驾驶舱三栏布局 + Canvas 关系图"
```

---

### Task 5: 验证整体编译 + 清理警告

**Files:**
- 可能微调：`frontend/src/hooks/use_workspace_data.rs`
- 可能微调：`frontend/src/components/workspace_graph.rs`
- 可能微调：`frontend/src/pages/workspace.rs`

- [ ] **Step 1: 完整 release 编译**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check --release 2>&1 | tail -30`
Expected: 编译通过

- [ ] **Step 2: 检查并修复新增警告**

检查是否有：
- unused import 警告 → 删除未用 import
- unused variable 警告 → 加 `_` 前缀或删除
- dead_code 警告 → 如为预留 API 加 `#[allow(dead_code)]`

- [ ] **Step 3: 后端测试验证（确保无回归）**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo test --workspace --lib 2>&1 | tail -20`
Expected: 所有测试通过（本次只改前端，后端不应受影响）

- [ ] **Step 4: Commit（如有修复）**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add -A
git commit -m "fix(workspace): 清理编译警告"
```

---

### Task 6: 推送 + 总结

- [ ] **Step 1: 推送到远程**

Run: `cd /Users/aman/Technology/rust/ai_orz && git push origin main`
Expected: 推送成功

- [ ] **Step 2: 更新文档（可选）**

如果需要，在 `docs/frontend_roadmap.md` 更新 workspace 工作台完成状态。

---

## 验收标准

- [ ] `/workspace` 页面显示三栏布局：左 Project 列表 / 中 Canvas 关系图 / 右 Agent 列表
- [ ] 顶部汇总状态条显示 4 个统计数字
- [ ] 默认中间显示 Global 视图（Project ↔ Agent 关联）
- [ ] 点击左侧 Project → 中间切换为该 Project 的 Task + Agent 视图
- [ ] 点击右侧 Agent → 中间切换为该 Agent 的 Task + Project 视图
- [ ] 点击"全局"按钮 → 中间回到 Global 视图
- [ ] 点击中间 Canvas 节点 → 跳转对应详情页（Project → /projects/:id，Agent → /hr/agents/:id，Task → /tasks/:id）
- [ ] 节点颜色按状态区分（绿/橙/红/蓝/灰）
- [ ] 粒子效果正常（数据流/辉光/背景/诞生消亡）
- [ ] 力导向布局自动分布节点
- [ ] 拖拽节点可重新布局
- [ ] 编译通过，后端测试无回归

## 第二期预告（待后端支持）

- 后端在 `TaskListItem` 加 `dependencies: Vec<String>` 字段
- ProjectDetail 视图升级为真正的 Task DAG 分层布局
- 实现拓扑排序分层算法
- Agent 运行时状态实时刷新（SSE 或轮询）
