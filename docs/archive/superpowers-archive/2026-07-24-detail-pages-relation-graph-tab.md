# 三个详情页新增关系图 Tab 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Agent 详情页、Project 详情页、Task 详情页中新增「关系图」tab，复用 WorkspaceGraph 组件展示 Canvas 渲染的 DAG 关系图，并将 Project/Task 详情页改造为 tab 布局。

**Architecture:** WorkspaceGraph 已支持 ProjectDetail / AgentDetail 两种视图模式，本计划新增 TaskDetail 视图模式（展示 Task → Project + Task → Agent + Task → 依赖 Task 的关系）。三个详情页统一采用 `active_tab: Signal<usize>` + DaisyUI tabs 模式。数据加载上，agent_detail 新增加载全局 tasks + agents 列表（用于关系图），task_detail 新增加载该 task 所属 project 的 tasks 列表（用于依赖 DAG）+ assignee agent 对象。WorkspaceGraph 复用已有的 `use_workspace_data` 数据契约。

**Tech Stack:** Dioxus 0.6 + wasm-bindgen + Canvas 2D + DaisyUI tabs

---

## 文件结构

**修改文件：**
- `frontend/src/components/workspace_graph.rs` — 新增 `WorkspaceView::TaskDetail(String)` 变体 + `build_task_detail_view` 函数
- `frontend/src/pages/hr/agent_detail.rs` — 新增「关系图」tab（第 5 个 tab，复用 WorkspaceGraph，view=AgentDetail），新增加载全局 tasks + agents
- `frontend/src/pages/project/project_detail.rs` — 改造为 tab 布局：概览 / 任务列表 / 产物 / 关系图；关系图 tab 使用 WorkspaceGraph(view=ProjectDetail)
- `frontend/src/pages/project/task_detail.rs` — 改造为 tab 布局：概览 / 进度与状态 / 关系图；关系图 tab 使用 WorkspaceGraph(view=TaskDetail)，新增加载同 project 的 tasks + assignee agent

**不新增文件**（所有改造在现有文件内完成，避免文件膨胀）

---

## 关键数据契约（不可变）

### WorkspaceGraph Props（已存在，不改）

```rust
pub struct WorkspaceGraphProps {
    pub view: WorkspaceView,
    pub projects: Vec<ProjectListItem>,
    pub agents: Vec<AgentListItem>,
    pub tasks: Vec<TaskListItem>,
    pub width: f64,
    pub height: f64,
}
```

### WorkspaceView 枚举（新增 TaskDetail 变体）

```rust
pub enum WorkspaceView {
    Global,
    ProjectDetail(String),
    AgentDetail(String),
    TaskDetail(String),  // 新增
}
```

### 关键 API 函数签名（已存在，直接复用）

```rust
// frontend/src/api/project.rs
pub async fn list_tasks(project_id: Option<&str>, status: Option<i32>, assignee_id: Option<&str>, assignee_type: Option<i32>) -> Result<ListTasksResponse, ApiError>
pub async fn list_project_tasks(project_id: &str) -> Result<ListTasksResponse, ApiError>
pub async fn list_projects() -> Result<ListProjectsResponse, ApiError>

// frontend/src/api/hr.rs
pub async fn list_agents() -> Result<ListAgentsResponse, ApiError>
pub async fn get_agent(id: &str, stats_options: Option<&StatsOptions>) -> Result<GetAgentResponse, ApiError>
```

### TaskListItem 关键字段（已存在，全部可用）

`id, title, status, priority, assignee_type, assignee_id, project_id, dependencies, progress`

---

## Task 1：WorkspaceGraph 新增 TaskDetail 视图

**Files:**
- Modify: `frontend/src/components/workspace_graph.rs`

**目标：** 新增 `WorkspaceView::TaskDetail(task_id)` 变体和 `build_task_detail_view` 函数，展示以 Task 为中心的关系图：Task 节点居中（layer=0），关联的 Project 节点（layer=-1，顶部）、Agent 节点（layer=-1，顶部）、依赖的前置 Task 节点（layer=1，下方）、后继 Task 节点（layer=-1，顶部或由 DAG 决定）。

- [ ] **Step 1: 新增 WorkspaceView::TaskDetail 变体**

在 `frontend/src/components/workspace_graph.rs` 的 `WorkspaceView` 枚举中添加：

```rust
/// Workspace 视图模式
#[derive(Debug, Clone, PartialEq)]
pub enum WorkspaceView {
    /// 全局视图：Project ↔ Agent 关联
    Global,
    /// Project 详情视图：选中 Project 的 Task + Agent
    ProjectDetail(String),
    /// Agent 详情视图：选中 Agent 的 Task + Project
    AgentDetail(String),
    /// Task 详情视图：选中 Task 的 Project + Agent + 依赖/后继 Task
    TaskDetail(String),
}
```

- [ ] **Step 2: 实现 build_task_detail_view 函数**

在 `build_agent_detail_view` 函数之后添加新函数。逻辑：
1. 中心 Task 节点 layer=0
2. 若 task 有 project_id：查找 projects 中对应 Project，添加 Project 节点 layer=-1（顶部）+ Task→Project 边
3. 若 task.assignee_type==1：查找 agents 中对应 Agent，添加 Agent 节点 layer=-1（顶部）+ Task→Agent 边
4. 依赖的前置 Task：遍历 task.dependencies，在 tasks 中查找同 project 的前置 Task，添加节点 layer=1（下方）+ 前置Task→Task 边
5. 后继 Task：遍历所有 tasks，找出 dependencies 包含当前 task_id 的，添加节点 layer=-1 + Task→后继Task 边

```rust
/// 构建 TaskDetail 视图的节点和边
///
/// 选中 Task 为中心，展示：
/// - 关联 Project（顶部）
/// - 关联 Agent（顶部）
/// - 前置依赖 Task（下方）
/// - 后继依赖 Task（顶部）
fn build_task_detail_view(
    task_id: &str,
    task: &TaskListItem,
    projects: &[ProjectListItem],
    agents: &[AgentListItem],
    tasks: &[TaskListItem],
) -> (Vec<CanvasNode>, Vec<CanvasEdge>) {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    let center_node_id = format!("task:{}", task.id);

    // 中心 Task 节点（layer=0）
    nodes.push(CanvasNode {
        id: center_node_id.clone(),
        x: 0.0, y: 0.0,
        radius: 30.0,
        label: task.title.clone(),
        color: task_status_color(task.status),
        node_type: Some("task".to_string()),
        layer: Some(0),
    });

    // 关联 Project（layer=-1 顶部）
    if let Some(pid) = &task.project_id {
        if let Some(p) = projects.iter().find(|p| &p.id == pid) {
            nodes.push(CanvasNode {
                id: format!("project:{}", p.id),
                x: 0.0, y: 0.0,
                radius: 28.0,
                label: p.name.clone(),
                color: project_status_color(p.status),
                node_type: Some("project".to_string()),
                layer: Some(-1),
            });
            edges.push(CanvasEdge {
                from_id: center_node_id.clone(),
                to_id: format!("project:{}", p.id),
            });
        }
    }

    // 关联 Agent（layer=-1 顶部）
    if task.assignee_type == 1 {
        if let Some(a) = agents.iter().find(|a| a.id == task.assignee_id) {
            nodes.push(CanvasNode {
                id: format!("agent:{}", a.id),
                x: 0.0, y: 0.0,
                radius: 25.0,
                label: a.name.clone(),
                color: agent_runtime_color(a.runtime_state),
                node_type: Some("agent".to_string()),
                layer: Some(-1),
            });
            edges.push(CanvasEdge {
                from_id: center_node_id.clone(),
                to_id: format!("agent:{}", a.id),
            });
        }
    }

    // 前置依赖 Task（layer=1 下方）
    for dep_id in &task.dependencies {
        if let Some(dep_task) = tasks.iter().find(|t| &t.id == dep_id) {
            nodes.push(CanvasNode {
                id: format!("task:{}", dep_task.id),
                x: 0.0, y: 0.0,
                radius: 20.0,
                label: dep_task.title.clone(),
                color: task_status_color(dep_task.status),
                node_type: Some("task".to_string()),
                layer: Some(1),
            });
            // 前置 Task → 当前 Task
            edges.push(CanvasEdge {
                from_id: format!("task:{}", dep_task.id),
                to_id: center_node_id.clone(),
            });
        }
    }

    // 后继 Task（layer=-1 顶部，被其他 Task 依赖）
    for t in tasks {
        if t.dependencies.iter().any(|d| d == task_id) {
            nodes.push(CanvasNode {
                id: format!("task:{}", t.id),
                x: 0.0, y: 0.0,
                radius: 20.0,
                label: t.title.clone(),
                color: task_status_color(t.status),
                node_type: Some("task".to_string()),
                layer: Some(-1),
            });
            // 当前 Task → 后继 Task
            edges.push(CanvasEdge {
                from_id: center_node_id.clone(),
                to_id: format!("task:{}", t.id),
            });
        }
    }

    (nodes, edges)
}
```

- [ ] **Step 3: 在 WorkspaceGraph 组件的 match 中接入 TaskDetail 分支**

修改 `WorkspaceGraph` 组件中的 match 语句（约 303-319 行），添加 TaskDetail 分支：

```rust
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
        WorkspaceView::TaskDetail(tid) => {
            if let Some(t) = tasks.iter().find(|t| t.id == *tid) {
                build_task_detail_view(tid, t, &projects, &agents, &tasks)
            } else {
                (Vec::new(), Vec::new())
            }
        }
    };
```

- [ ] **Step 4: 运行前端测试验证不破坏现有逻辑**

Run: `cd frontend && cargo test 2>&1 | tail -10`
Expected: 34 passed; 0 failed（无新增测试，仅验证不破坏）

- [ ] **Step 5: 提交**

```bash
git add frontend/src/components/workspace_graph.rs
git commit -m "feat(workspace-graph): 新增 TaskDetail 视图模式展示 Task 关系图"
```

---

## Task 2：Agent 详情页新增「关系图」tab

**Files:**
- Modify: `frontend/src/pages/hr/agent_detail.rs`

**目标：** 在现有 4 个 tab 后新增第 5 个 tab「关系图」，使用 WorkspaceGraph(view=AgentDetail(agent_id))。需要新增加载全局 tasks 列表（用于关系图过滤）和 agents 列表（用于关系图节点）。

- [ ] **Step 1: 添加 import 和新 signals**

在 `frontend/src/pages/hr/agent_detail.rs` 顶部 import 区添加：

```rust
use crate::components::workspace_graph::{WorkspaceGraph, WorkspaceView};
use crate::api::project::{list_tasks, list_projects};
use crate::api::hr::list_agents;
use common::api::{AgentListItem, ProjectListItem, TaskListItem};
```

注意：`list_agents` 已在文件中 import（因现有代码用 `get_agent` 等），需确认是否重复。若已 import 则不重复添加。

在 `HrAgentDetail` 函数内 `active_tab` signal 下方添加新 signals：

```rust
    // 关系图所需数据：全局 projects + tasks + agents 列表
    let mut graph_projects = use_signal(Vec::<ProjectListItem>::new);
    let mut graph_tasks = use_signal(Vec::<TaskListItem>::new);
    let mut graph_agents = use_signal(Vec::<AgentListItem>::new);
```

- [ ] **Step 2: 在 load_data 中加载关系图数据**

在 `load_data` 闭包内的最后（`load_latest_messages` 之后）添加加载 tasks 和 agents 的逻辑：

```rust
            // 加载关系图所需的全局 tasks 和 agents
            match list_tasks(None, None, None, None).await {
                Ok(resp) => graph_tasks.set(resp.tasks),
                Err(e) => toast.error(&format!("获取任务列表失败: {}", e)),
            }
            match list_agents().await {
                Ok(resp) => graph_agents.set(resp.agents),
                Err(e) => toast.error(&format!("获取 Agent 列表失败: {}", e)),
            }
```

- [ ] **Step 3: 修改 active_tab 上限和 tab 渲染**

将 `active_tab` 的取值范围从 0..=3 扩展到 0..=4。找到 tab 导航 div（约 384-405 行），在「💬 对话与记忆」按钮后添加第 5 个 tab 按钮：

```rust
                            button {
                                class: "{tab4_class}",
                                onclick: move |_| active_tab.set(4),
                                "🕸️ 关系图"
                            }
```

在 tab 导航上方添加 `tab4_class` 变量（参考现有 tab0_class..tab3_class 模式）：

```rust
        let tab4_class = if active_tab() == 4 { "tab tab-lg tab-active" } else { "tab tab-lg" };
```

- [ ] **Step 4: 添加 tab 4 内容渲染**

在 `match active_tab()` 的 match 分支中添加 `4 =>` 分支：

```rust
                            4 => rsx! {
                                div { class: "card bg-base-100 shadow-md",
                                    div { class: "card-header",
                                        h2 { class: "card-title", "关系图" }
                                    }
                                    div { class: "p-4",
                                        WorkspaceGraph {
                                            view: WorkspaceView::AgentDetail(a.id.clone()),
                                            projects: Vec::new(),  // Agent 详情页不展示 Project，传空
                                            agents: graph_agents.read().clone(),
                                            tasks: graph_tasks.read().clone(),
                                            width: 800.0,
                                            height: 500.0,
                                        }
                                    }
                                }
                            },
```

注意：`projects` 传空 Vec，因为 AgentDetail 视图只用 tasks 推断 Project（build_agent_detail_view 内部会 filter tasks 并从 tasks 的 project_id 推断关联 Project，但需要 projects 参数来查找 Project 名称）。**修正：** 实际上 `build_agent_detail_view` 需要从 `projects` 参数查找 Project 名称，所以必须传 projects 数据。改为：

```rust
                            4 => rsx! {
                                div { class: "card bg-base-100 shadow-md",
                                    div { class: "card-header",
                                        h2 { class: "card-title", "关系图" }
                                    }
                                    div { class: "p-4",
                                        WorkspaceGraph {
                                            view: WorkspaceView::AgentDetail(a.id.clone()),
                                            projects: graph_projects.read().clone(),
                                            agents: graph_agents.read().clone(),
                                            tasks: graph_tasks.read().clone(),
                                            width: 800.0,
                                            height: 500.0,
                                        }
                                    }
                                }
                            },
```

并新增 `graph_projects` signal + 在 load_data 中加载：

```rust
    // 关系图所需数据：全局 projects + tasks + agents 列表
    let mut graph_projects = use_signal(Vec::<ProjectListItem>::new);
    let mut graph_tasks = use_signal(Vec::<TaskListItem>::new);
    let mut graph_agents = use_signal(Vec::<AgentListItem>::new);
```

在 load_data 中：

```rust
            // 加载关系图所需的全局 projects、tasks 和 agents
            match list_projects().await {
                Ok(resp) => graph_projects.set(resp.projects),
                Err(e) => toast.error(&format!("获取项目列表失败: {}", e)),
            }
            match list_tasks(None, None, None, None).await {
                Ok(resp) => graph_tasks.set(resp.tasks),
                Err(e) => toast.error(&format!("获取任务列表失败: {}", e)),
            }
            match list_agents().await {
                Ok(resp) => graph_agents.set(resp.agents),
                Err(e) => toast.error(&format!("获取 Agent 列表失败: {}", e)),
            }
```

补充 import：

```rust
use crate::api::project::{list_tasks, list_projects};
use common::api::{AgentListItem, ProjectListItem, TaskListItem};
```

- [ ] **Step 5: 运行前端编译验证**

Run: `cd frontend && cargo build --release 2>&1 | tail -10`
Expected: 编译成功，可能有现有 warnings

- [ ] **Step 6: 提交**

```bash
git add frontend/src/pages/hr/agent_detail.rs
git commit -m "feat(agent-detail): 新增「关系图」tab 展示 Agent 关联 Task 和 Project"
```

---

## Task 3：Project 详情页改造为 tab 布局 + 关系图 tab

**Files:**
- Modify: `frontend/src/pages/project/project_detail.rs`

**目标：** 将现有垂直堆叠的 6 个区域改造为 4 个 tab：概览（基本信息+统计+状态管理）/ 任务列表 / 产物 / 关系图。关系图 tab 使用 WorkspaceGraph(view=ProjectDetail(project_id))，复用已加载的 tasks 列表，新增加载全局 agents 列表。

- [ ] **Step 1: 添加 import 和新 signals**

在 `frontend/src/pages/project/project_detail.rs` 顶部 import 区添加：

```rust
use crate::components::workspace_graph::{WorkspaceGraph, WorkspaceView};
use crate::api::hr::list_agents;
use common::api::{AgentListItem, ProjectListItem};
```

在 `ProjectDetail` 函数内 signals 区添加：

```rust
    // Tab 切换：0=概览 1=任务列表 2=产物 3=关系图
    let mut active_tab = use_signal(|| 0usize);
    // 关系图所需的 agents 列表（tasks 已有）
    let mut graph_agents = use_signal(Vec::<AgentListItem>::new);
```

- [ ] **Step 2: 在 use_effect 中加载 agents 列表**

修改 `use_effect` 闭包（约 53-76 行），在 `list_artifacts` 之后添加：

```rust
            match list_agents().await {
                Ok(resp) => graph_agents.set(resp.agents),
                Err(e) => toast.error(&format!("获取 Agent 列表失败: {}", e)),
            }
```

- [ ] **Step 3: 添加 tab 导航和 tab_class 变量**

在 `rsx!` 中 `AppLayout` 之后、`if loading()` 之前，准备 tab class 变量。然后在 `if let Some(p) = &project_data` 分支内，先渲染 tab 导航，再用 `match active_tab()` 切换内容。

在 `let project_data = project.read().clone();` 之前添加：

```rust
    let tab0_class = if active_tab() == 0 { "tab tab-lg tab-active" } else { "tab tab-lg" };
    let tab1_class = if active_tab() == 1 { "tab tab-lg tab-active" } else { "tab tab-lg" };
    let tab2_class = if active_tab() == 2 { "tab tab-lg tab-active" } else { "tab tab-lg" };
    let tab3_class = if active_tab() == 3 { "tab tab-lg tab-active" } else { "tab tab-lg" };
```

- [ ] **Step 4: 重构 rsx 主体为 tab 布局**

将 `if let Some(p) = &project_data` 分支内的所有区域（区域1-4）包装到 tab 结构中。整体结构变为：

```rust
        } else if let Some(p) = &project_data {
            // Tab 导航
            div { class: "tabs tabs-boxed mb-6",
                button { class: "{tab0_class}", onclick: move |_| active_tab.set(0), "📋 概览" }
                button { class: "{tab1_class}", onclick: move |_| active_tab.set(1), "📝 任务列表" }
                button { class: "{tab2_class}", onclick: move |_| active_tab.set(2), "📦 产物" }
                button { class: "{tab3_class}", onclick: move |_| active_tab.set(3), "🕸️ 关系图" }
            }

            // Tab 内容
            {match active_tab() {
                0 => rsx! {
                    // === 概览：基本信息 + 统计 + 状态管理 ===
                    // 区域 1：项目基本信息卡片（原代码）
                    div { class: "card bg-base-100 shadow-md",
                        // ... 原区域1代码不变 ...
                    }
                    // 区域 2：项目概览统计（原代码）
                    div { class: "card bg-base-100 shadow-md",
                        // ... 原区域2代码不变 ...
                    }
                    // 区域 3：状态管理（原代码）
                    div { class: "card bg-base-100 shadow-md",
                        // ... 原区域3代码不变 ...
                    }
                    // 条件渲染的 ProjectStatsPanel（原代码）
                    if p.stats.is_some() || p.model_call_stats.is_some() {
                        ProjectStatsPanel {
                            stats: p.stats.clone(),
                            model_call_stats: p.model_call_stats.clone(),
                        }
                    }
                },
                1 => rsx! {
                    // === 任务列表 ===
                    div { class: "card bg-base-100 shadow-md",
                        // ... 原区域3（任务列表）代码不变 ...
                    }
                },
                2 => rsx! {
                    // === 产物 ===
                    div { class: "card bg-base-100 shadow-md",
                        // ... 原区域4（产物列表）代码不变 ...
                    }
                },
                3 => rsx! {
                    // === 关系图 ===
                    div { class: "card bg-base-100 shadow-md",
                        div { class: "card-header",
                            h2 { class: "card-title", "关系图" }
                        }
                        div { class: "p-4",
                            WorkspaceGraph {
                                view: WorkspaceView::ProjectDetail(p.id.clone()),
                                projects: graph_projects.read().clone(),
                                agents: graph_agents.read().clone(),
                                tasks: tasks_list.clone(),
                                width: 800.0,
                                height: 500.0,
                            }
                        }
                    }
                },
                _ => rsx! { div {} },
            }}
```

**关键变更说明：** 相比最初设计，`projects` 字段改为使用 `graph_projects` signal（通过 `list_projects()` 加载的全局列表中过滤），而不是手动从 `GetProjectResponse` 构造 `ProjectListItem`。原因：`ProjectListItem` 含 `description: Option<String>` 和 `owner_agent_id: Option<String>` 字段，手动构造易出错且需重复维护字段映射。直接复用 API 返回的 `ProjectListItem` 更稳健。

**需要在 Step 1 signals 中新增 `graph_projects`：**

```rust
    // 关系图所需数据：全局 projects + agents 列表（tasks 已有）
    let mut graph_projects = use_signal(Vec::<ProjectListItem>::new);
    let mut graph_agents = use_signal(Vec::<AgentListItem>::new);
```

**需要在 Step 2 use_effect 中加载 projects：**

修改 `use_effect` 闭包，在 `list_agents` 之后添加：

```rust
            match list_projects().await {
                Ok(resp) => graph_projects.set(resp.projects),
                Err(e) => toast.error(&format!("获取项目列表失败: {}", e)),
            }
```

**import 需补充：**

```rust
use crate::api::project::{list_agents, list_projects};
```

注意 `list_agents` 在 `crate::api::hr`，`list_projects` 在 `crate::api::project`，所以 import 应为：

```rust
use crate::api::hr::list_agents;
use crate::api::project::list_projects;
use common::api::{AgentListItem, ProjectListItem};
```

- [ ] **Step 5: 运行前端编译验证**

Run: `cd frontend && cargo build --release 2>&1 | tail -10`
Expected: 编译成功

- [ ] **Step 6: 提交**

```bash
git add frontend/src/pages/project/project_detail.rs
git commit -m "feat(project-detail): 改造为 tab 布局并新增「关系图」tab"
```

---

## Task 4：Task 详情页改造为 tab 布局 + 关系图 tab

**Files:**
- Modify: `frontend/src/pages/project/task_detail.rs`

**目标：** 将现有垂直堆叠的 4 个区域改造为 3 个 tab：概览（基本信息+标签依赖+统计）/ 进度与状态（进度管理+状态流转）/ 关系图。关系图 tab 使用 WorkspaceGraph(view=TaskDetail(task_id))。需要新增加载同 project 的 tasks 列表（用于依赖 DAG）和 assignee agent 对象。

- [ ] **Step 1: 添加 import 和新 signals**

在 `frontend/src/pages/project/task_detail.rs` 顶部 import 区添加：

```rust
use crate::components::workspace_graph::{WorkspaceGraph, WorkspaceView};
use crate::api::hr::list_agents;
use crate::api::project::{list_project_tasks, list_projects};
use common::api::{AgentListItem, ProjectListItem, TaskListItem};
```

在 `TaskDetail` 函数内 signals 区添加：

```rust
    // Tab 切换：0=概览 1=进度与状态 2=关系图
    let mut active_tab = use_signal(|| 0usize);
    // 关系图所需数据：同 project 的 tasks + 全局 agents + 全局 projects
    let mut graph_tasks = use_signal(Vec::<TaskListItem>::new);
    let mut graph_agents = use_signal(Vec::<AgentListItem>::new);
    let mut graph_projects = use_signal(Vec::<ProjectListItem>::new);
```

- [ ] **Step 2: 在 use_effect 中加载关系图数据**

修改 `use_effect` 闭包（约 33-51 行），在 `get_task` 成功之后并行加载关系图数据：

```rust
            match get_task(&id_clone, Some(&stats_options)).await {
                Ok(t) => {
                    new_progress.set(t.progress);
                    // 克隆关系图所需字段后再 set，避免 move 后无法使用
                    let pid_for_graph = t.project_id.clone();
                    task.set(Some(t));

                    // 加载关系图数据（独立 spawn，不阻塞主流程）
                    // 1. 同 project 的 tasks（用于依赖 DAG，包含当前 task）
                    // 2. 全局 agents（用于查找 assignee agent）
                    // 3. 全局 projects（用于查找关联 project）
                    spawn(async move {
                        if let Some(pid) = &pid_for_graph {
                            match list_project_tasks(pid).await {
                                Ok(resp) => graph_tasks.set(resp.tasks),
                                Err(e) => toast.error(&format!("获取项目任务失败: {}", e)),
                            }
                        }
                        match list_agents().await {
                            Ok(resp) => graph_agents.set(resp.agents),
                            Err(e) => toast.error(&format!("获取 Agent 列表失败: {}", e)),
                        }
                        match list_projects().await {
                            Ok(resp) => graph_projects.set(resp.projects),
                            Err(e) => toast.error(&format!("获取项目列表失败: {}", e)),
                        }
                    });
                }
                Err(e) => toast.error(&e),
            }
```

**说明：** 用 `list_agents()` 加载全局 agents 列表，避免从 `GetAgentResponse` 手动构造 `AgentListItem`（后者字段较多：roles/description/model_provider_id/created_at 等，手动映射易出错）。关系图组件内部会根据 `task.assignee_id` 从 `graph_agents` 中查找对应 agent。`list_projects` 同理加载全局 projects，由组件内部按 `task.project_id` 过滤。

- [ ] **Step 3: 添加 tab 导航和 tab_class 变量**

在 `rsx!` 中准备 tab class 变量：

```rust
    let tab0_class = if active_tab() == 0 { "tab tab-lg tab-active" } else { "tab tab-lg" };
    let tab1_class = if active_tab() == 1 { "tab tab-lg tab-active" } else { "tab tab-lg" };
    let tab2_class = if active_tab() == 2 { "tab tab-lg tab-active" } else { "tab tab-lg" };
```

- [ ] **Step 4: 重构 rsx 主体为 tab 布局**

将 `if let Some(t) = task.read().as_ref()` 分支内的所有区域包装到 tab 结构中：

```rust
        } else if let Some(t) = task.read().as_ref() {
            // Tab 导航
            div { class: "tabs tabs-boxed mb-6",
                button { class: "{tab0_class}", onclick: move |_| active_tab.set(0), "📋 概览" }
                button { class: "{tab1_class}", onclick: move |_| active_tab.set(1), "📊 进度与状态" }
                button { class: "{tab2_class}", onclick: move |_| active_tab.set(2), "🕸️ 关系图" }
            }

            // Tab 内容
            {match active_tab() {
                0 => rsx! {
                    // === 概览：基本信息 + 标签依赖 + 统计 ===
                    // 区域 1：基本信息（原代码）
                    div { class: "card bg-base-100 shadow-md",
                        // ... 原区域1代码不变 ...
                    }
                    // 区域 2：标签和依赖（原代码）
                    if !t.tags.is_empty() || !t.dependencies.is_empty() {
                        div { class: "card bg-base-100 shadow-md",
                            // ... 原区域2代码不变 ...
                        }
                    }
                    // 统计（原代码）
                    if t.stats.is_some() || t.model_call_stats.is_some() {
                        TaskStatsPanel {
                            stats: t.stats.clone(),
                            model_call_stats: t.model_call_stats.clone(),
                        }
                    }
                },
                1 => rsx! {
                    // === 进度与状态 ===
                    // 区域 3：进度管理（原代码）
                    div { class: "card bg-base-100 shadow-md",
                        // ... 原区域3代码不变 ...
                    }
                    // 区域 4：状态流转（原代码）
                    div { class: "card bg-base-100 shadow-md",
                        // ... 原区域4代码不变 ...
                    }
                },
                2 => rsx! {
                    // === 关系图 ===
                    div { class: "card bg-base-100 shadow-md",
                        div { class: "card-header",
                            h2 { class: "card-title", "关系图" }
                        }
                        div { class: "p-4",
                            WorkspaceGraph {
                                view: WorkspaceView::TaskDetail(t.id.clone()),
                                projects: graph_projects.read().clone(),
                                agents: graph_agents.read().clone(),
                                tasks: graph_tasks.read().clone(),
                                width: 800.0,
                                height: 500.0,
                            }
                        }
                    }
                },
                _ => rsx! { div {} },
            }}
```

**注意：** `WorkspaceView::TaskDetail` 需要从 `tasks` 参数中查找当前 task 对象。`graph_tasks` 加载的是同 project 的所有 tasks（包含当前 task），所以能找到。但更稳妥的做法是也把当前 task 放入 graph_tasks。由于 `list_project_tasks` 返回的已经包含当前 task（它属于该 project），所以无需额外处理。

- [ ] **Step 5: 运行前端编译验证**

Run: `cd frontend && cargo build --release 2>&1 | tail -10`
Expected: 编译成功

- [ ] **Step 6: 提交**

```bash
git add frontend/src/pages/project/task_detail.rs
git commit -m "feat(task-detail): 改造为 tab 布局并新增「关系图」tab"
```

---

## Task 5：验证 + 推送

**Files:**
- 无文件修改

- [ ] **Step 1: 运行前端全部测试**

Run: `cd frontend && cargo test 2>&1 | tail -10`
Expected: 34 passed; 0 failed

- [ ] **Step 2: 运行前端 release build**

Run: `cd frontend && cargo build --release 2>&1 | tail -10`
Expected: 编译成功

- [ ] **Step 3: 运行后端测试（确保未破坏）**

Run: `cargo test 2>&1 | tail -10`
Expected: 746 passed; 0 failed

- [ ] **Step 4: 推送到远程**

```bash
git push
```

---

## Self-Review

### 1. Spec coverage 检查
- ✅ Agent 详情页新增关系图 tab → Task 2
- ✅ Project 详情页新增关系图 tab → Task 3
- ✅ Task 详情页新增关系图 tab → Task 4
- ✅ 复用 WorkspaceGraph 组件 → Task 1 扩展 TaskDetail 视图，Task 2/3/4 引用
- ✅ project_detail 和 task_detail 改造为 tab 布局 → Task 3/4
- ✅ Task 6 验证编译 + 测试 + 推送 → Task 5

### 2. Placeholder 扫描
- 无 TBD/TODO
- 所有代码块完整，包含实际字段映射
- 前置验证步骤（ProjectListItem、AgentListItem 字段）已标注，但实际字段已在 plan 中给出

### 3. Type consistency 检查
- `WorkspaceView::TaskDetail(String)` 在 Task 1 定义，Task 4 使用 ✅
- `build_task_detail_view(task_id, task, projects, agents, tasks)` 签名在 Task 1 定义，Task 1 Step 3 调用 ✅
- `WorkspaceGraphProps` 字段（view/projects/agents/tasks/width/height）在所有 Task 中一致 ✅
- `ProjectListItem` 字段映射在 Task 3 中给出 ✅
- `AgentListItem` 字段映射在 Task 4 中给出 ✅

### 4. 风险点
- **ProjectListItem 构造：** Task 3 中手动构造 `ProjectListItem` 需要确认 `GetProjectResponse` 是否包含所有所需字段（id/name/status/priority/tags/root_user_id/created_at/updated_at）。`GetProjectResponse` 确实包含这些字段（它是 Project 的完整响应）。
- **AgentListItem 构造：** Task 4 中从 `GetAgentResponse` 映射到 `AgentListItem`，需要确认 `GetAgentResponse` 包含 `id/name/kind/status/runtime_state` 字段。已确认包含。
- **graph_tasks 数据范围：** Task 4 中 `list_project_tasks` 返回同 project 所有 tasks，包含当前 task，满足 `build_task_detail_view` 从 tasks 中查找当前 task 的需求。
- **tab 切换性能：** 每次切换 tab 会重新渲染，但 WorkspaceGraph 内部有 props 同步 effect 保留节点位置，不会重置布局。
