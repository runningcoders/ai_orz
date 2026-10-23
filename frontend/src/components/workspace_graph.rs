//! Workspace 专用 Canvas 关系图组件
//!
//! 根据 WorkspaceView 状态机切换三种视图：
//! - Global：所有 Project ↔ Agent 的关联（通过 Task.assignee_id 和 Task.project_id 推断）
//! - ProjectDetail：选中 Project 的 Task + Agent 节点
//! - AgentDetail：选中 Agent 的 Task + Project 节点
//!
//! 节点颜色按状态区分（见颜色规范表），点击节点跳转对应详情页。

use crate::components::canvas_scene::{CanvasEdge, CanvasNode, CanvasScene, card_size};
use crate::components::node_card;
use crate::utils::dagre_layout::{DagreEdge, DagreNode, dagre_layout};
use common::api::{AgentListItem, ProjectListItem, TaskListItem};
use dioxus::prelude::*;
use dioxus_router::use_navigator;

/// 工作台节点统一构造：有正文/标签 → 矩形信息卡，否则保持圆形
///
/// 渐进增强与渲染器 `CanvasNode::is_card` 的判定共用一条路径：DTO 里的
/// description / tags 原样下发，数据够就升级成卡片，不够不硬撑空卡片。
/// 卡片节点的 radius 必须是**外接圆半径**（力导向碰撞避让按它算最小中心距，
/// 填成小圆半径 168px 宽的卡片会互相压叠），此处在构造点收敛修一次。
fn workspace_node(
    id: String,
    label: String,
    color: String,
    node_type: &str,
    circle_radius: f64,
    description: Option<&str>,
    tags: Vec<String>,
) -> CanvasNode {
    let mut node = CanvasNode {
        id,
        radius: circle_radius,
        label,
        color,
        node_type: Some(node_type.to_string()),
        description: description.unwrap_or("").trim().to_string(),
        tags,
        ..Default::default()
    };
    if node.is_card() {
        let body_lines = node_card::body_lines(node.summary.as_deref(), &node.description);
        let w = node_card::box_width(&node.label, body_lines.len());
        let h = node_card::box_height(body_lines.len(), !node.tags.is_empty());
        node.radius = w.max(h) / 2.0;
    }
    node
}

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
    /// 轻量概览模式：只画「圆点 + 边」，不渲染节点信息卡。
    ///
    /// 对标知识图谱的全局点线态：节点数量很大时（如 Agent 视图「全部」），
    /// 每个节点画一张矩形信息卡会导致整页卡顿，降级为圆点即可保持可读与流畅。
    #[props(default = false)]
    pub simple: bool,
}

impl PartialEq for WorkspaceGraphProps {
    fn eq(&self, other: &Self) -> bool {
        self.view == other.view
            && self.projects == other.projects
            && self.agents == other.agents
            && self.tasks == other.tasks
            && self.width == other.width
            && self.height == other.height
            && self.simple == other.simple
        // on_view_change 不参与比较（EventHandler 无法比较）
    }
}

/// Project 状态颜色
fn project_status_color(status: i32) -> String {
    match status {
        1 => "#3b82f6".to_string(), // InProgress 蓝
        2 => "#6b7280".to_string(), // Completed 灰
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

/// 轻量概览模式下圆点的统一半径（按节点类型归一）。
///
/// 信息卡节点的 `radius` 存的是**卡片外接圆半径**（可达上百像素），直接拿来当圆点
/// 会让圆点巨大且互压；概览态丢弃「中心节点放大」的差异化，按类型取统一小半径，
/// 换取一致的视觉密度（与知识图谱全局点线态同一取向）。
fn simple_dot_radius(node_type: Option<&str>) -> f64 {
    match node_type {
        Some("project") => 28.0,
        Some("agent") => 25.0,
        Some("task") => 20.0,
        _ => 20.0,
    }
}

/// Task 节点标签：业务 tags 之外追加进度胶囊（有进度才有信息量，0% 不占位）
fn task_tags(t: &TaskListItem) -> Vec<String> {
    let mut tags = t.tags.clone();
    if t.progress > 0 {
        tags.push(format!("进度 {}%", t.progress));
    }
    tags
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
        nodes.push(workspace_node(
            format!("project:{}", p.id),
            p.name.clone(),
            project_status_color(p.status),
            "project",
            28.0,
            p.description.as_deref(),
            p.tags.clone(),
        ));
    }

    // Agent 节点
    for a in agents {
        nodes.push(workspace_node(
            format!("agent:{}", a.id),
            a.name.clone(),
            agent_runtime_color(a.runtime_state),
            "agent",
            25.0,
            a.description.as_deref(),
            a.roles.clone(),
        ));
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
            ..Default::default()
        });
    }

    (nodes, edges)
}

/// 构建 ProjectDetail 视图的节点和边
///
/// 选中 Project 的 Task 节点 + 关联 Agent 节点，Task → Agent 边
/// Task 节点使用 dagre 分层布局，展示 DAG 依赖关系
fn build_project_detail_view(
    project_id: &str,
    project: &ProjectListItem,
    agents: &[AgentListItem],
    tasks: &[TaskListItem],
    graph_width: f64,
    graph_height: f64,
) -> (Vec<CanvasNode>, Vec<CanvasEdge>) {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    let project_node_id = format!("project:{}", project.id);

    // 中心 Project 节点（坐标由函数末尾的 dagre 布局写回）
    nodes.push(workspace_node(
        project_node_id.clone(),
        project.name.clone(),
        project_status_color(project.status),
        "project",
        35.0,
        project.description.as_deref(),
        project.tags.clone(),
    ));

    // 该 Project 的 Task 节点
    let project_tasks: Vec<&TaskListItem> = tasks
        .iter()
        .filter(|t| t.project_id.as_deref() == Some(project_id))
        .collect();

    // 同项目内的 Task ID 集合（跨项目依赖不入图：与本项目无关的依赖画上来
    // 只会多出悬空端点）
    let task_ids: Vec<String> = project_tasks.iter().map(|t| t.id.clone()).collect();
    let task_id_set: std::collections::HashSet<&str> =
        task_ids.iter().map(|s| s.as_str()).collect();

    for t in &project_tasks {
        nodes.push(workspace_node(
            format!("task:{}", t.id),
            t.title.clone(),
            task_status_color(t.status),
            "task",
            20.0,
            t.description.as_deref(),
            task_tags(t),
        ));
        // Project → Task 边
        edges.push(CanvasEdge {
            from_id: project_node_id.clone(),
            to_id: format!("task:{}", t.id),
            ..Default::default()
        });
    }

    // === Task → Task 依赖边（基于 dependencies）===
    for t in &project_tasks {
        for dep_id in &t.dependencies {
            if task_id_set.contains(dep_id.as_str()) {
                edges.push(CanvasEdge {
                    from_id: format!("task:{}", dep_id),
                    to_id: format!("task:{}", t.id),
                    ..Default::default()
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
            nodes.push(workspace_node(
                format!("agent:{}", a.id),
                a.name.clone(),
                agent_runtime_color(a.runtime_state),
                "agent",
                25.0,
                a.description.as_deref(),
                a.roles.clone(),
            ));
        }
    }

    // Task → Agent 边
    for t in &project_tasks {
        if t.assignee_type == 1 {
            edges.push(CanvasEdge {
                from_id: format!("task:{}", t.id),
                to_id: format!("agent:{}", t.assignee_id),
                ..Default::default()
            });
        }
    }

    // ---- dagre 分层布局（取代旧「Kahn 初排 + 层约束力松弛」两层皮）----
    //
    // 旧链路的问题：Kahn 均分定初始坐标（层距写死 80px、不知卡片宽高）→ 力导向
    // 带 layer 约束松弛，稳态不重叠但排布随机不可复现、边仍是穿卡直线、数据变化
    // 后新旧布局坐标系不一致。dagre 一次给出：按传入尺寸算的层距/同层距（同层
    // 不重叠由坐标层面保证）+ 边 waypoints 折线路由（绕开中间卡）。
    //
    // 尺寸按「看到的形状」传：信息卡用实际卡宽高（`canvas_scene::card_size`，
    // 与碰撞避让同一份几何，不许各算一套），圆点（simple 降级态）用直径。
    let dagre_nodes: Vec<DagreNode> = nodes
        .iter()
        .map(|n| {
            let (w, h) = if n.is_card() {
                card_size(n)
            } else {
                (n.radius * 2.0, n.radius * 2.0)
            };
            DagreNode {
                id: n.id.clone(),
                width: w,
                height: h,
            }
        })
        .collect();
    let dagre_edges: Vec<DagreEdge> = edges
        .iter()
        .map(|e| DagreEdge {
            source: e.from_id.clone(),
            target: e.to_id.clone(),
        })
        .collect();
    let layout = dagre_layout(&dagre_nodes, &dagre_edges, graph_width, graph_height, false);

    // 写回节点坐标
    for n in &mut nodes {
        if let Some((x, y)) = layout.positions.get(&n.id) {
            n.x = *x;
            n.y = *y;
        }
    }
    // 写回边路由：dagre waypoints → 静态折线（渲染层走 `CanvasEdge::points`）。
    // 自环本项目不会出现；真出现时 dagre 不产路由，边退化为直线兜底。
    for e in &mut edges {
        let key = (e.from_id.clone(), e.to_id.clone());
        if let Some(pts) = layout.routes.get(&key) {
            e.points = Some(pts.clone());
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
    nodes.push(workspace_node(
        format!("agent:{}", agent.id),
        agent.name.clone(),
        agent_runtime_color(agent.runtime_state),
        "agent",
        35.0,
        agent.description.as_deref(),
        agent.roles.clone(),
    ));

    // 该 Agent 的 Task 节点
    let agent_tasks: Vec<&TaskListItem> = tasks
        .iter()
        .filter(|t| t.assignee_type == 1 && t.assignee_id == agent_id)
        .collect();

    for t in &agent_tasks {
        nodes.push(workspace_node(
            format!("task:{}", t.id),
            t.title.clone(),
            task_status_color(t.status),
            "task",
            20.0,
            t.description.as_deref(),
            task_tags(t),
        ));
        // Agent → Task 边
        edges.push(CanvasEdge {
            from_id: format!("agent:{}", agent.id),
            to_id: format!("task:{}", t.id),
            ..Default::default()
        });
    }

    // 关联 Project 节点（去重）
    let project_ids: std::collections::HashSet<String> = agent_tasks
        .iter()
        .filter_map(|t| t.project_id.clone())
        .collect();

    for pid in project_ids {
        if let Some(p) = projects.iter().find(|p| p.id == pid) {
            nodes.push(workspace_node(
                format!("project:{}", p.id),
                p.name.clone(),
                project_status_color(p.status),
                "project",
                28.0,
                p.description.as_deref(),
                p.tags.clone(),
            ));
        }
    }

    // Task → Project 边
    for t in &agent_tasks {
        if let Some(pid) = &t.project_id {
            edges.push(CanvasEdge {
                from_id: format!("task:{}", t.id),
                to_id: format!("project:{}", pid),
                ..Default::default()
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
    nodes.push({
        let mut n = workspace_node(
            center_node_id.clone(),
            task.title.clone(),
            task_status_color(task.status),
            "task",
            30.0,
            task.description.as_deref(),
            task_tags(task),
        );
        n.layer = Some(0);
        n
    });

    // 关联 Project（layer=-1 顶部）
    if let Some(pid) = &task.project_id
        && let Some(p) = projects.iter().find(|p| &p.id == pid)
    {
        nodes.push({
            let mut n = workspace_node(
                format!("project:{}", p.id),
                p.name.clone(),
                project_status_color(p.status),
                "project",
                28.0,
                p.description.as_deref(),
                p.tags.clone(),
            );
            n.layer = Some(-1);
            n
        });
        edges.push(CanvasEdge {
            from_id: center_node_id.clone(),
            to_id: format!("project:{}", p.id),
            ..Default::default()
        });
    }

    // 关联 Agent（layer=-1 顶部）
    if task.assignee_type == 1
        && let Some(a) = agents.iter().find(|a| a.id == task.assignee_id)
    {
        nodes.push({
            let mut n = workspace_node(
                format!("agent:{}", a.id),
                a.name.clone(),
                agent_runtime_color(a.runtime_state),
                "agent",
                25.0,
                a.description.as_deref(),
                a.roles.clone(),
            );
            n.layer = Some(-1);
            n
        });
        edges.push(CanvasEdge {
            from_id: center_node_id.clone(),
            to_id: format!("agent:{}", a.id),
            ..Default::default()
        });
    }

    // 前置依赖 Task（layer=1 下方）
    for dep_id in &task.dependencies {
        if let Some(dep_task) = tasks.iter().find(|t| &t.id == dep_id) {
            nodes.push({
                let mut n = workspace_node(
                    format!("task:{}", dep_task.id),
                    dep_task.title.clone(),
                    task_status_color(dep_task.status),
                    "task",
                    20.0,
                    dep_task.description.as_deref(),
                    task_tags(dep_task),
                );
                n.layer = Some(1);
                n
            });
            // 前置 Task → 当前 Task
            edges.push(CanvasEdge {
                from_id: format!("task:{}", dep_task.id),
                to_id: center_node_id.clone(),
                ..Default::default()
            });
        }
    }

    // 后继 Task（layer=-1 顶部，被其他 Task 依赖）
    for t in tasks {
        if t.dependencies.iter().any(|d| d == task_id) {
            nodes.push({
                let mut n = workspace_node(
                    format!("task:{}", t.id),
                    t.title.clone(),
                    task_status_color(t.status),
                    "task",
                    20.0,
                    t.description.as_deref(),
                    task_tags(t),
                );
                n.layer = Some(-1);
                n
            });
            // 当前 Task → 后继 Task
            edges.push(CanvasEdge {
                from_id: center_node_id.clone(),
                to_id: format!("task:{}", t.id),
                ..Default::default()
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
    let (mut nodes, edges) = match &view {
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

    // 轻量概览模式：把信息卡降级为「圆点 + 边」，只保留 id / 类型 / 颜色 / 分层。
    //
    // 与知识图谱「全局点线态」同一套表达（数据驱动：无正文即圆点），此处直接对
    // 已构建的节点做一次降级，避免在 4 个 build_* 与十余处调用点重复分支。
    // 半径必须按类型归一：卡片节点的 radius 是外接圆半径，留着会让圆点巨大互压。
    if props.simple {
        for n in &mut nodes {
            n.description.clear();
            n.summary = None;
            n.tags.clear();
            n.radius = simple_dot_radius(n.node_type.as_deref());
        }
    }

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
                    // ProjectDetail 走 dagre 静态布局：坐标即权威，力导向会把
                    // 精心算好并 fit 进画布的坐标重新推散（无边界 + 碰撞只保不
                    // 重叠，不保 fit）。其余视图没有静态布局，仍靠力导向松弛。
                    enable_force_layout: !matches!(&view, WorkspaceView::ProjectDetail(_)),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 有正文/标签 → 信息卡，radius 必须是卡片外接圆半径
    /// （力导向碰撞避让按它算最小中心距，填成小圆半径卡片会互相压叠）
    #[test]
    fn workspace_node_with_body_becomes_card_with_circumscribed_radius() {
        let node = workspace_node(
            "task:1".to_string(),
            "订单状态机".to_string(),
            "#3b82f6".to_string(),
            "task",
            20.0,
            Some("描述订单从创建到完成的流转，足够长以撑起两行正文区域"),
            vec!["订单".to_string(), "进度 40%".to_string()],
        );
        assert!(node.is_card());
        let body_lines = node_card::body_lines(node.summary.as_deref(), &node.description);
        let (w, h) = (
            node_card::box_width(&node.label, body_lines.len()),
            node_card::box_height(body_lines.len(), !node.tags.is_empty()),
        );
        assert_eq!(node.radius, w.max(h) / 2.0, "卡片半径应为外接圆半径");
        assert!(node.radius > 20.0, "卡片半径应大于原圆形半径");
    }

    /// 无正文无标签 → 保持圆形（渐进增强：数据不够不硬撑空卡片）
    #[test]
    fn workspace_node_without_body_stays_circle() {
        let node = workspace_node(
            "agent:1".to_string(),
            "客服 Agent".to_string(),
            "#10b981".to_string(),
            "agent",
            25.0,
            None,
            Vec::new(),
        );
        assert!(!node.is_card());
        assert_eq!(node.radius, 25.0, "圆形节点保持原半径");
    }

    /// Task 标签组装：业务 tags 在前，进度胶囊仅在 >0 时追加
    #[test]
    fn task_tags_appends_progress_when_positive() {
        let t = sample_task(0);
        assert!(!task_tags(&t).iter().any(|s| s.contains("进度")));
        let t = sample_task(40);
        let tags = task_tags(&t);
        assert_eq!(tags.last().map(String::as_str), Some("进度 40%"));
    }

    fn sample_task(progress: i32) -> TaskListItem {
        TaskListItem {
            id: "t1".to_string(),
            title: "T".to_string(),
            description: None,
            status: 1,
            priority: 0,
            tags: vec!["后端".to_string()],
            root_user_id: "u".to_string(),
            assignee_type: 1,
            assignee_id: "a".to_string(),
            project_id: None,
            thinking_depth: 0,
            progress,
            created_at: 0,
            updated_at: 0,
            dependencies: Vec::new(),
        }
    }

    /// ProjectDetail 走 dagre：坐标全部落进画布、卡片两两不重叠、依赖边有
    /// 折线路由、前置任务排在依赖者上方
    ///
    /// 回归旧 Kahn 两层皮（层距写死 80px + 力导向松弛）：坐标不再随机、
    /// 不再有「同层槽宽 < 卡宽」的互压，边不再穿卡。
    #[test]
    fn project_detail_dagre_layout_fits_canvas_without_overlap() {
        let project = ProjectListItem {
            id: "p1".to_string(),
            name: "订单中台".to_string(),
            description: Some("支撑订单全链路".to_string()),
            status: 1,
            priority: 0,
            tags: vec!["核心".to_string()],
            root_user_id: "u".to_string(),
            owner_agent_id: None,
            created_at: 0,
            updated_at: 0,
        };
        let agent = AgentListItem {
            id: "a1".to_string(),
            name: "客服 Agent".to_string(),
            roles: vec!["客服".to_string()],
            description: Some("处理售后咨询".to_string()),
            kind: "local".to_string(),
            model_provider_id: String::new(),
            status: 1,
            created_at: 0,
            runtime_state: 0,
        };
        // 链式依赖：t1 ← t2 ← t3（t2 依赖 t1，t3 依赖 t2），t1 分配给 a1
        let mut t1 = sample_task(0);
        t1.id = "t1".to_string();
        t1.title = "梳理订单状态机".to_string();
        t1.description = Some("把创建、支付、履约、售后的状态流转定义清楚，避免二义性".to_string());
        t1.project_id = Some("p1".to_string());
        t1.assignee_id = "a1".to_string();
        let mut t2 = sample_task(50);
        t2.id = "t2".to_string();
        t2.title = "实现状态机引擎".to_string();
        t2.description = Some("按定义好的状态流转实现引擎与持久化".to_string());
        t2.project_id = Some("p1".to_string());
        t2.dependencies = vec!["t1".to_string()];
        let mut t3 = sample_task(0);
        t3.id = "t3".to_string();
        t3.title = "接入存量订单回放".to_string();
        t3.description = Some("用历史订单回放验证状态机正确性".to_string());
        t3.project_id = Some("p1".to_string());
        t3.dependencies = vec!["t2".to_string()];

        let (nodes, edges) =
            build_project_detail_view("p1", &project, &[agent], &[t1, t2, t3], 800.0, 600.0);

        // 节点：project + 3 task + 1 agent
        assert_eq!(nodes.len(), 5);
        // 全部落进画布（dagre 包围盒映射 + margin 24）
        for n in &nodes {
            assert!((0.0..=800.0).contains(&n.x), "{} x={} 飞出画布", n.id, n.x);
            assert!((0.0..=600.0).contains(&n.y), "{} y={} 飞出画布", n.id, n.y);
        }
        // 依赖方向：前置在上（y 更小）
        let y_of = |id: &str| nodes.iter().find(|n| n.id == id).unwrap().y;
        assert!(y_of("project:p1") < y_of("task:t1"));
        assert!(y_of("task:t1") < y_of("task:t2"));
        assert!(y_of("task:t2") < y_of("task:t3"));
        // 卡片两两不重叠（矩形判据：水平或垂直至少一个方向分离）
        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                let (a, b) = (&nodes[i], &nodes[j]);
                let (aw, ah) = if a.is_card() {
                    card_size(a)
                } else {
                    (a.radius * 2.0, a.radius * 2.0)
                };
                let (bw, bh) = if b.is_card() {
                    card_size(b)
                } else {
                    (b.radius * 2.0, b.radius * 2.0)
                };
                let separated =
                    (a.x - b.x).abs() >= (aw + bw) / 2.0 || (a.y - b.y).abs() >= (ah + bh) / 2.0;
                assert!(separated, "{} 与 {} 重叠", a.id, b.id);
            }
        }
        // 依赖边有折线路由（waypoints ≥ 2 点）
        let dep = edges
            .iter()
            .find(|e| e.from_id == "task:t1" && e.to_id == "task:t2")
            .expect("t1 → t2 依赖边应在场");
        assert!(
            dep.points.as_ref().is_some_and(|p| p.len() >= 2),
            "依赖边应带 dagre 折线路由"
        );
    }
}
