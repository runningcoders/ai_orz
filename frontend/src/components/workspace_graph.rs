//! Workspace 专用 Canvas 关系图组件
//!
//! 根据 WorkspaceView 状态机切换三种视图：
//! - Global：所有 Project ↔ Agent 的关联（通过 Task.assignee_id 和 Task.project_id 推断）
//! - ProjectDetail：选中 Project 的 Task + Agent 节点
//! - AgentDetail：选中 Agent 的 Task + Project 节点
//!
//! 节点颜色按状态区分（见颜色规范表），点击节点跳转对应详情页。

use crate::components::canvas_scene::{CanvasEdge, CanvasNode, CanvasScene};
use common::api::{AgentListItem, ProjectListItem, TaskListItem};
use dioxus::prelude::*;
use dioxus_router::use_navigator;

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

/// WorkspaceGraph Props
#[derive(Props, Clone)]
pub struct WorkspaceGraphProps {
    /// 当前视图模式
    pub view: WorkspaceView,
    /// 图数据中的 Project 列表（按视图过滤后的）
    pub projects: Vec<ProjectListItem>,
    /// 图数据中的 Agent 列表（按视图过滤后的）
    pub agents: Vec<AgentListItem>,
    /// 图数据中的 Task 列表（按视图过滤后的）
    pub tasks: Vec<TaskListItem>,
    /// Canvas 宽度
    pub width: f64,
    /// Canvas 高度
    pub height: f64,
    /// 非中心节点点击时的视图切换回调
    pub on_view_change: Option<EventHandler<WorkspaceView>>,
    /// 是否自适应父容器尺寸（HUD 全屏背景模式，覆盖 width/height）
    #[props(default = false)]
    pub auto_size: bool,
}

impl PartialEq for WorkspaceGraphProps {
    fn eq(&self, other: &Self) -> bool {
        self.view == other.view
            && self.projects == other.projects
            && self.agents == other.agents
            && self.tasks == other.tasks
            && self.width == other.width
            && self.height == other.height
        // on_view_change 不参与比较（EventHandler 无法比较）
    }
}

/// Project 状态颜色
fn project_status_color(status: i32) -> String {
    match status {
        1 => "#10b981".to_string(), // Active 绿
        3 => "#3b82f6".to_string(), // InProgress 蓝
        2 => "#f59e0b".to_string(), // PendingReview 橙
        4 => "#6b7280".to_string(), // Completed 灰
        _ => "#9ca3af".to_string(), // 其他浅灰
    }
}

/// Agent 运行时状态颜色（基于 runtime_state）
fn agent_runtime_color(runtime_state: i32) -> String {
    match runtime_state {
        0 => "#10b981".to_string(), // Idle 绿
        1 => "#f59e0b".to_string(), // Resting 橙
        2 => "#ef4444".to_string(), // Busy 红
        _ => "#9ca3af".to_string(),
    }
}

/// Task 状态颜色
fn task_status_color(status: i32) -> String {
    match status {
        1 => "#3b82f6".to_string(), // InProgress 蓝
        2 => "#6b7280".to_string(), // Completed 灰
        _ => "#9ca3af".to_string(),
    }
}

/// 构建 Global 视图的节点和边
///
/// Project 节点 + Agent 节点，通过 Task 关联（Task.project_id → Project, Task.assignee_id → Agent）
fn build_global_view(
    projects: &[ProjectListItem],
    agents: &[AgentListItem],
    tasks: &[TaskListItem],
) -> (Vec<CanvasNode>, Vec<CanvasEdge>) {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // Project 节点
    for p in projects {
        nodes.push(CanvasNode {
            id: format!("project:{}", p.id),
            x: 0.0,
            y: 0.0,
            radius: 28.0,
            label: p.name.clone(),
            color: project_status_color(p.status),
            node_type: Some("project".to_string()),
            layer: None,
        });
    }

    // Agent 节点
    for a in agents {
        nodes.push(CanvasNode {
            id: format!("agent:{}", a.id),
            x: 0.0,
            y: 0.0,
            radius: 25.0,
            label: a.name.clone(),
            color: agent_runtime_color(a.runtime_state),
            node_type: Some("agent".to_string()),
            layer: None,
        });
    }

    // 通过 Task 推断 Project ↔ Agent 关联
    // 每个 Task 有 project_id 和 assignee_id，如果 assignee_type=1（Agent），则建立 Project → Agent 边
    let mut edge_set: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    for t in tasks {
        if let Some(pid) = &t.project_id
            && t.assignee_type == 1
        {
            // Agent
            let from = format!("project:{}", pid);
            let to = format!("agent:{}", t.assignee_id);
            if from != to {
                edge_set.insert((from, to));
            }
        }
    }
    for (from, to) in edge_set {
        edges.push(CanvasEdge {
            from_id: from,
            to_id: to,
        });
    }

    (nodes, edges)
}

/// 构建 ProjectDetail 视图的节点和边
///
/// 选中 Project 的 Task 节点 + 关联 Agent 节点，Task → Agent 边
/// Task 节点使用 Kahn 拓扑排序分层布局，展示 DAG 依赖关系
fn build_project_detail_view(
    project_id: &str,
    project: &ProjectListItem,
    agents: &[AgentListItem],
    tasks: &[TaskListItem],
    graph_width: f64,
    graph_height: f64,
) -> (Vec<CanvasNode>, Vec<CanvasEdge>) {
    use crate::components::layered_layout::{LayeredLayoutConfig, compute_layered_layout};

    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    let project_node_id = format!("project:{}", project.id);

    // 中心 Project 节点（layer=0，位于顶部）
    nodes.push(CanvasNode {
        id: project_node_id.clone(),
        x: 0.0,
        y: 0.0,
        radius: 35.0,
        label: project.name.clone(),
        color: project_status_color(project.status),
        node_type: Some("project".to_string()),
        layer: Some(0),
    });

    // 该 Project 的 Task 节点
    let project_tasks: Vec<&TaskListItem> = tasks
        .iter()
        .filter(|t| t.project_id.as_deref() == Some(project_id))
        .collect();

    // 构建 Task ID 列表和依赖映射（仅同项目内的依赖）
    let task_ids: Vec<String> = project_tasks.iter().map(|t| t.id.clone()).collect();
    let task_id_set: std::collections::HashSet<&str> =
        task_ids.iter().map(|s| s.as_str()).collect();
    let mut deps_map: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for t in &project_tasks {
        let in_project_deps: Vec<String> = t
            .dependencies
            .iter()
            .filter(|d| task_id_set.contains(d.as_str()))
            .cloned()
            .collect();
        deps_map.insert(t.id.clone(), in_project_deps);
    }

    // 计算分层布局（Task 节点从 layer=1 开始，给 Project 留 layer=0）
    let config = LayeredLayoutConfig {
        width: graph_width,
        height: graph_height,
        top_margin: 160.0,
        layer_height: 80.0,
        side_margin: 60.0,
    };
    let task_positions = compute_layered_layout(&task_ids, &deps_map, &config);

    // 添加 Task 节点（位置来自分层布局，layer +1 因为 Project 占了 layer 0）
    for t in &project_tasks {
        let (layer, x, y) = task_positions
            .get(&t.id)
            .copied()
            .unwrap_or((0, 350.0, 160.0));
        nodes.push(CanvasNode {
            id: format!("task:{}", t.id),
            x,
            y,
            radius: 20.0,
            label: t.title.clone(),
            color: task_status_color(t.status),
            node_type: Some("task".to_string()),
            layer: Some(layer + 1),
        });
        // Project → Task 边
        edges.push(CanvasEdge {
            from_id: project_node_id.clone(),
            to_id: format!("task:{}", t.id),
        });
    }

    // === Task → Task 依赖边（基于 dependencies）===
    for t in &project_tasks {
        for dep_id in &t.dependencies {
            if task_id_set.contains(dep_id.as_str()) {
                edges.push(CanvasEdge {
                    from_id: format!("task:{}", dep_id),
                    to_id: format!("task:{}", t.id),
                });
            }
        }
    }

    // 关联 Agent 节点（去重：该 Project 的 Task 分配到的 Agent）
    let agent_ids: std::collections::HashSet<String> = project_tasks
        .iter()
        .filter(|t| t.assignee_type == 1)
        .map(|t| t.assignee_id.clone())
        .collect();

    for aid in agent_ids {
        if let Some(a) = agents.iter().find(|a| a.id == aid) {
            nodes.push(CanvasNode {
                id: format!("agent:{}", a.id),
                x: 0.0,
                y: 0.0,
                radius: 25.0,
                label: a.name.clone(),
                color: agent_runtime_color(a.runtime_state),
                node_type: Some("agent".to_string()),
                layer: None,
            });
        }
    }

    // Task → Agent 边
    for t in &project_tasks {
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
        x: 0.0,
        y: 0.0,
        radius: 35.0,
        label: agent.name.clone(),
        color: agent_runtime_color(agent.runtime_state),
        node_type: Some("agent".to_string()),
        layer: None,
    });

    // 该 Agent 的 Task 节点
    let agent_tasks: Vec<&TaskListItem> = tasks
        .iter()
        .filter(|t| t.assignee_type == 1 && t.assignee_id == agent_id)
        .collect();

    for t in &agent_tasks {
        nodes.push(CanvasNode {
            id: format!("task:{}", t.id),
            x: 0.0,
            y: 0.0,
            radius: 20.0,
            label: t.title.clone(),
            color: task_status_color(t.status),
            node_type: Some("task".to_string()),
            layer: None,
        });
        // Agent → Task 边
        edges.push(CanvasEdge {
            from_id: format!("agent:{}", agent.id),
            to_id: format!("task:{}", t.id),
        });
    }

    // 关联 Project 节点（去重）
    let project_ids: std::collections::HashSet<String> = agent_tasks
        .iter()
        .filter_map(|t| t.project_id.clone())
        .collect();

    for pid in project_ids {
        if let Some(p) = projects.iter().find(|p| p.id == pid) {
            nodes.push(CanvasNode {
                id: format!("project:{}", p.id),
                x: 0.0,
                y: 0.0,
                radius: 28.0,
                label: p.name.clone(),
                color: project_status_color(p.status),
                node_type: Some("project".to_string()),
                layer: None,
            });
        }
    }

    // Task → Project 边
    for t in &agent_tasks {
        if let Some(pid) = &t.project_id {
            edges.push(CanvasEdge {
                from_id: format!("task:{}", t.id),
                to_id: format!("project:{}", pid),
            });
        }
    }

    (nodes, edges)
}

/// 构建 TaskDetail 视图的节点和边
///
/// 选中 Task 为中心，展示：
/// - 关联 Project（顶部，layer=-1）
/// - 关联 Agent（顶部，layer=-1）
/// - 前置依赖 Task（下方，layer=1）
/// - 后继依赖 Task（顶部，layer=-1）
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
        x: 0.0,
        y: 0.0,
        radius: 30.0,
        label: task.title.clone(),
        color: task_status_color(task.status),
        node_type: Some("task".to_string()),
        layer: Some(0),
    });

    // 关联 Project（layer=-1 顶部）
    if let Some(pid) = &task.project_id
        && let Some(p) = projects.iter().find(|p| &p.id == pid)
    {
        nodes.push(CanvasNode {
            id: format!("project:{}", p.id),
            x: 0.0,
            y: 0.0,
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

    // 关联 Agent（layer=-1 顶部）
    if task.assignee_type == 1
        && let Some(a) = agents.iter().find(|a| a.id == task.assignee_id)
    {
        nodes.push(CanvasNode {
            id: format!("agent:{}", a.id),
            x: 0.0,
            y: 0.0,
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

    // 前置依赖 Task（layer=1 下方）
    for dep_id in &task.dependencies {
        if let Some(dep_task) = tasks.iter().find(|t| &t.id == dep_id) {
            nodes.push(CanvasNode {
                id: format!("task:{}", dep_task.id),
                x: 0.0,
                y: 0.0,
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
                x: 0.0,
                y: 0.0,
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

#[component]
pub fn WorkspaceGraph(props: WorkspaceGraphProps) -> Element {
    let view = props.view.clone();
    let projects = props.projects.clone();
    let agents = props.agents.clone();
    let tasks = props.tasks.clone();

    // 自适应容器尺寸（HUD 全屏背景模式）：挂载后测量父容器真实尺寸
    let mut measured = use_signal(|| None::<(f64, f64)>);
    let (width, height) = if props.auto_size {
        (*measured.read()).unwrap_or((props.width, props.height))
    } else {
        (props.width, props.height)
    };

    // 根据视图模式构建节点和边
    let (nodes, edges) = match &view {
        WorkspaceView::Global => build_global_view(&projects, &agents, &tasks),
        WorkspaceView::ProjectDetail(pid) => {
            if let Some(p) = projects.iter().find(|p| p.id == *pid) {
                build_project_detail_view(pid, p, &agents, &tasks, width, height)
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

    // 节点 id → (类型, 真实ID) 查找表，供点击回调判断跳转
    let mut click_map: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();
    for n in &nodes {
        if let Some(nt) = &n.node_type {
            let real_id =
                n.id.split_once(':')
                    .map(|x| x.1)
                    .unwrap_or(&n.id)
                    .to_string();
            click_map.insert(n.id.clone(), (nt.clone(), real_id));
        }
    }

    // 当前视图的中心节点 ID（点击中心节点 → 跳转详情页；点击非中心节点 → 切换视图）
    let center_node_id = match &view {
        WorkspaceView::Global => None, // Global 无中心节点
        WorkspaceView::ProjectDetail(pid) => Some(format!("project:{}", pid)),
        WorkspaceView::AgentDetail(aid) => Some(format!("agent:{}", aid)),
        WorkspaceView::TaskDetail(tid) => Some(format!("task:{}", tid)),
    };

    let navigator = use_navigator();
    let on_view_change = props.on_view_change;

    let node_count = nodes.len();

    rsx! {
        div { class: "relative flex flex-col items-center w-full h-full",
            onmounted: move |evt: MountedEvent| {
                if let Some(el) = evt.data().downcast::<web_sys::Element>() {
                    let rect = el.get_bounding_client_rect();
                    let w = rect.width();
                    let h = rect.height();
                    if w > 0.0 && h > 0.0 {
                        measured.set(Some((w, h)));
                    }
                }
            },
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
                    transparent: props.auto_size,
                    enable_force_layout: true,
                    enable_data_flow_particles: true,
                    enable_glow_particles: true,
                    enable_background_particles: true,
                    enable_birth_death_particles: true,
                    on_node_click: Some(EventHandler::new(move |node_id: String| {
                        if let Some((nt, real_id)) = click_map.get(&node_id) {
                            // 中心节点点击 → 跳转详情页
                            if center_node_id.as_deref() == Some(&node_id) {
                                match nt.as_str() {
                                    "project" => { navigator.push(format!("/projects/{}", real_id)); }
                                    "agent" => { navigator.push(format!("/hr/agents/{}", real_id)); }
                                    "task" => { navigator.push(format!("/tasks/{}", real_id)); }
                                    _ => {}
                                }
                                return;
                            }
                            // 非中心节点点击 → 切换视图
                            if let Some(on_change) = &on_view_change {
                                let new_view = match nt.as_str() {
                                    "project" => WorkspaceView::ProjectDetail(real_id.clone()),
                                    "agent" => WorkspaceView::AgentDetail(real_id.clone()),
                                    "task" => WorkspaceView::TaskDetail(real_id.clone()),
                                    _ => return,
                                };
                                on_change.call(new_view);
                            }
                        }
                    })),
                }
            }
        }
    }
}
