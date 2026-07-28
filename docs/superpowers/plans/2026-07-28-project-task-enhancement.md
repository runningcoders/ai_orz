# Project & Task Enhancement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为项目详情接口增加按需返回的 Mermaid 任务依赖图（含状态着色），并为 Project/Task 详情接口增加按需返回的 Artifact 列表，同时落地一个通用的图形渲染组件 `pkg/utils/graph`。

**Architecture:**
- 新建通用图形组件 `pkg/utils/graph`，定义 `Graph`/`GraphNode`/`GraphLine`/`GraphRenderer` 抽象，Mermaid 只是其中一种 Renderer 实现。任何实体只要能转换为 `GraphNodeData` 就能渲染成图。
- 在 Domain 层（`ProjectDomainImpl`）聚合 `task_dal` 和 `artifact_dal`，按需注入 `task_graph` 和 `artifacts` 到 Project 实体。Domain 层负责业务编排，DAL 层保持单一职责。
- 复用现有 `ArtifactDetail` DTO（已含 id 等所有关键字段），不新建 `ArtifactSummary`，避免重复定义。
- 按需返回模式（`with_task_graph` / `with_artifacts` option），不破坏现有接口契约。

**Tech Stack:** Rust, async-trait, sqlx, serde, existing layered architecture (handler → domain → dal → dao)

---

## File Structure

### 新建文件
- `src/pkg/utils/mod.rs` — utils 模块入口
- `src/pkg/utils/graph/mod.rs` — graph 模块入口，导出公共 API
- `src/pkg/utils/graph/types.rs` — `Graph`/`GraphNodeData`/`GraphLine` 数据结构
- `src/pkg/utils/graph/renderer.rs` — `GraphRenderer` trait + `MermaidRenderer` 实现
- `src/pkg/utils/graph/graph_test.rs` — graph 组件测试（`#[cfg(test)]`）
- `src/service/domain/project/task_graph.rs` — Project Domain 子模块：基于 Task 列表构建 Graph 并渲染 mermaid
- `src/service/domain/project/task_graph_test.rs` — task_graph 测试（`#[cfg(test)]`）

### 修改文件
- `src/pkg/mod.rs` — 注册 `utils` 模块
- `src/models/project.rs` — Project 实体加 `task_graph` 和 `artifacts` 字段
- `src/models/task.rs` — Task 实体加 `artifacts` 字段
- `src/service/dal/project.rs` — `ProjectFetchOptions` 加 `with_task_graph` / `with_artifacts` 字段
- `src/service/dal/task.rs` — `TaskFetchOptions` 加 `with_artifacts` 字段
- `src/service/domain/project/mod.rs` — 注册 `task_graph` 子模块
- `src/service/domain/project/service.rs` — `get_project` 不再纯透传，按 options 注入 task_graph 和 artifacts
- `src/service/domain/project/task.rs` — `get_task` 按 options 注入 artifacts（如果存在 get_task 方法）
- `common/src/api/project.rs` — `GetProjectRequest` 加 `with_task_graph` / `with_artifacts` 参数；`GetProjectResponse` 加 `task_graph` / `artifacts` 字段
- `common/src/api/task.rs` — `GetTaskRequest` 加 `with_artifacts` 参数；`GetTaskResponse` 加 `artifacts` 字段
- `src/handlers/project/projects/get_project.rs` — 传递新 options 字段
- `src/handlers/project/projects/response.rs` — `to_detail` 映射新字段
- `src/handlers/project/tasks/get_task.rs` — 传递新 options 字段（如果存在）
- `src/handlers/project/tasks/response.rs` — `to_detail` 映射新字段（如果存在）

---

## Task 1: 通用图形组件 - 数据结构

**Files:**
- Create: `src/pkg/utils/mod.rs`
- Create: `src/pkg/utils/graph/mod.rs`
- Create: `src/pkg/utils/graph/types.rs`
- Modify: `src/pkg/mod.rs`

- [ ] **Step 1: 注册 utils 模块到 pkg**

修改 `src/pkg/mod.rs`，在合适位置加入：

```rust
pub mod utils;
```

- [ ] **Step 2: 创建 utils 模块入口**

创建 `src/pkg/utils/mod.rs`：

```rust
//! 通用工具模块
//!
//! 提供 Graph 渲染等通用能力

pub mod graph;
```

- [ ] **Step 3: 创建 graph 模块入口**

创建 `src/pkg/utils/graph/mod.rs`：

```rust
//! 通用图形渲染组件
//!
//! 设计理念：
//! - 定义 Graph/Node/Line 抽象，任何实体只要能转换为 GraphNodeData 即可渲染成图
//! - Renderer trait 支持多种输出格式，Mermaid 只是其中一种实现
//! - 不绑定任何业务实体，纯通用组件

mod types;
mod renderer;

pub use types::{Graph, GraphNodeData, GraphLine, GraphNode};
pub use renderer::{GraphRenderer, MermaidRenderer, MermaidDirection};

#[cfg(test)]
mod graph_test;
```

- [ ] **Step 4: 创建 graph 数据结构**

创建 `src/pkg/utils/graph/types.rs`：

```rust
//! Graph 数据结构定义

/// 图节点数据
///
/// 表示图中的一个节点，由 ID、标签、可选分类组成。
/// 分类可用于渲染时着色等样式区分（如任务状态 done/doing/todo）。
#[derive(Debug, Clone)]
pub struct GraphNodeData {
    /// 节点唯一 ID（用于边的引用）
    pub id: String,
    /// 节点显示标签
    pub label: String,
    /// 节点分类（可选，用于样式区分）
    pub category: Option<String>,
}

impl GraphNodeData {
    /// 创建新节点
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            category: None,
        }
    }

    /// 设置节点分类
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }
}

/// 图边
///
/// 表示两个节点之间的有向连接。
#[derive(Debug, Clone)]
pub struct GraphLine {
    /// 起点节点 ID
    pub from: String,
    /// 终点节点 ID
    pub to: String,
    /// 边标签（可选）
    pub label: Option<String>,
}

impl GraphLine {
    /// 创建新边
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            label: None,
        }
    }

    /// 设置边标签
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// 图数据结构
///
/// 由节点列表和边列表组成，是 Renderer 的输入。
#[derive(Debug, Clone, Default)]
pub struct Graph {
    /// 节点列表
    pub nodes: Vec<GraphNodeData>,
    /// 边列表
    pub lines: Vec<GraphLine>,
}

impl Graph {
    /// 创建空图
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加节点
    pub fn add_node(&mut self, node: GraphNodeData) -> &mut Self {
        self.nodes.push(node);
        self
    }

    /// 添加边
    pub fn add_line(&mut self, line: GraphLine) -> &mut Self {
        self.lines.push(line);
        self
    }

    /// 根据 ID 查找节点
    pub fn find_node(&self, id: &str) -> Option<&GraphNodeData> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// 节点数量
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// 边数量
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// 渲染图
    pub fn render(&self, renderer: &dyn super::GraphRenderer) -> String {
        renderer.render(self)
    }
}

/// GraphNode trait
///
/// 实体实现此 trait 即可转换为图节点。
/// 这是可选的便捷接口，调用方也可以直接构造 GraphNodeData。
pub trait GraphNode {
    /// 转换为图节点数据
    fn to_graph_node(&self) -> GraphNodeData;
}
```

- [ ] **Step 5: 编译验证**

Run: `cargo build -p ai-orz 2>&1 | tail -20`
Expected: BUILD SUCCEEDED（graph 模块还未被引用，但应该能编译通过）

- [ ] **Step 6: Commit**

```bash
git add src/pkg/mod.rs src/pkg/utils/
git commit -m "feat(pkg): add generic graph rendering component - data structures"
```

---

## Task 2: 通用图形组件 - Mermaid Renderer

**Files:**
- Create: `src/pkg/utils/graph/renderer.rs`
- Create: `src/pkg/utils/graph/graph_test.rs`

- [ ] **Step 1: 编写失败测试**

创建 `src/pkg/utils/graph/graph_test.rs`：

```rust
//! Graph 组件测试

use super::*;

#[test]
fn test_empty_graph_renders_empty_mermaid() {
    let graph = Graph::new();
    let renderer = MermaidRenderer::new(MermaidDirection::LR);
    let result = graph.render(&renderer);
    assert!(result.contains("flowchart LR"));
}

#[test]
fn test_single_node_renders_correctly() {
    let mut graph = Graph::new();
    graph.add_node(GraphNodeData::new("t1", "Task 1"));
    let renderer = MermaidRenderer::new(MermaidDirection::LR);
    let result = graph.render(&renderer);
    assert!(result.contains("flowchart LR"));
    assert!(result.contains("t1[\"Task 1\"]"));
}

#[test]
fn test_node_with_category_gets_style_class() {
    let mut graph = Graph::new();
    graph.add_node(GraphNodeData::new("t1", "Task 1").with_category("done"));
    let renderer = MermaidRenderer::new(MermaidDirection::LR);
    let result = graph.render(&renderer);
    assert!(result.contains("class t1 done"));
}

#[test]
fn test_line_renders_arrow() {
    let mut graph = Graph::new();
    graph.add_node(GraphNodeData::new("t1", "Task 1"));
    graph.add_node(GraphNodeData::new("t2", "Task 2"));
    graph.add_line(GraphLine::new("t1", "t2"));
    let renderer = MermaidRenderer::new(MermaidDirection::LR);
    let result = graph.render(&renderer);
    assert!(result.contains("t1 --> t2"));
}

#[test]
fn test_line_with_label_renders_labeled_arrow() {
    let mut graph = Graph::new();
    graph.add_node(GraphNodeData::new("t1", "Task 1"));
    graph.add_node(GraphNodeData::new("t2", "Task 2"));
    graph.add_line(GraphLine::new("t1", "t2").with_label("blocks"));
    let renderer = MermaidRenderer::new(MermaidDirection::LR);
    let result = graph.render(&renderer);
    assert!(result.contains("t1 -- blocks --> t2"));
}

#[test]
fn test_td_direction() {
    let graph = Graph::new();
    let renderer = MermaidRenderer::new(MermaidDirection::TD);
    let result = graph.render(&renderer);
    assert!(result.contains("flowchart TD"));
}

#[test]
fn test_node_label_escaped() {
    let mut graph = Graph::new();
    graph.add_node(GraphNodeData::new("t1", "Task \"quote\" [bracket]"));
    let renderer = MermaidRenderer::new(MermaidDirection::LR);
    let result = graph.render(&renderer);
    // 引号需要转义，避免破坏 mermaid 语法
    assert!(result.contains("\\\""));
}

#[test]
fn test_orphan_line_target_rendered_as_external() {
    // 边指向一个图中不存在的节点 ID（如跨项目依赖）
    // 应该将该节点渲染为外部节点
    let mut graph = Graph::new();
    graph.add_node(GraphNodeData::new("t1", "Task 1"));
    graph.add_line(GraphLine::new("t1", "external_task_id"));
    let renderer = MermaidRenderer::new(MermaidDirection::LR);
    let result = graph.render(&renderer);
    // 外部节点应该被自动补出来
    assert!(result.contains("external_task_id"));
}
```

- [ ] **Step 2: 运行测试验证失败**

Run: `cargo test -p ai-orz lib::pkg::utils::graph::graph_test -- --nocapture 2>&1 | tail -20`
Expected: FAIL（renderer 模块还未实现）

- [ ] **Step 3: 实现 MermaidRenderer**

创建 `src/pkg/utils/graph/renderer.rs`：

```rust
//! Graph 渲染器实现

use super::types::{Graph, GraphLine, GraphNodeData};

/// 图渲染器 trait
///
/// 定义图的输出格式。任何实现此 trait 的渲染器都可以将 Graph 转换为字符串。
/// 例如 MermaidRenderer 输出 Mermaid 语法，未来可扩展 PlantUmlRenderer、DotRenderer 等。
pub trait GraphRenderer: Send + Sync {
    /// 渲染图为字符串
    fn render(&self, graph: &Graph) -> String;
}

/// Mermaid 方向
#[derive(Debug, Clone, Copy, Default)]
pub enum MermaidDirection {
    /// 左到右
    #[default]
    LR,
    /// 上到下
    TD,
}

impl MermaidDirection {
    fn as_str(&self) -> &'static str {
        match self {
            MermaidDirection::LR => "LR",
            MermaidDirection::TD => "TD",
        }
    }
}

/// Mermaid 渲染器
///
/// 输出 Mermaid flowchart 语法的字符串，可直接嵌入 Markdown 等文本中。
///
/// 节点样式：
/// - 有 category 的节点会生成 `class <id> <category>` 样式类
/// - 调用方可在 Markdown 中配合自定义 CSS 使用这些 class
///
/// 外部节点处理：
/// - 边引用了图中不存在的节点 ID 时，自动补出该节点（标签为 `(external) <id>`）
#[derive(Debug, Clone)]
pub struct MermaidRenderer {
    /// 图方向
    pub direction: MermaidDirection,
}

impl MermaidRenderer {
    /// 创建渲染器
    pub fn new(direction: MermaidDirection) -> Self {
        Self { direction }
    }

    /// 使用默认方向（LR）创建渲染器
    pub fn default_lr() -> Self {
        Self::new(MermaidDirection::LR)
    }

    /// 转义节点标签中的特殊字符
    fn escape_label(label: &str) -> String {
        label.replace('\\', "\\\\").replace('"', "\\\"")
    }

    /// 收集所有被边引用但不在 nodes 列表中的节点 ID（外部节点）
    fn collect_external_nodes(graph: &Graph) -> Vec<String> {
        let existing: std::collections::HashSet<&str> =
            graph.nodes.iter().map(|n| n.id.as_str()).collect();
        let mut external: Vec<String> = Vec::new();
        for line in &graph.lines {
            if !existing.contains(line.from.as_str()) {
                external.push(line.from.clone());
            }
            if !existing.contains(line.to.as_str()) {
                external.push(line.to.clone());
            }
        }
        external
    }
}

impl GraphRenderer for MermaidRenderer {
    fn render(&self, graph: &Graph) -> String {
        let mut out = String::new();
        out.push_str(&format!("flowchart {}\n", self.direction.as_str()));

        // 节点定义
        for node in &graph.nodes {
            let label = Self::escape_label(&node.label);
            out.push_str(&format!("    {}[\"{}\"]\n", node.id, label));
        }

        // 外部节点（边引用但未在 nodes 中的节点）
        let external_nodes = Self::collect_external_nodes(graph);
        for ext_id in &external_nodes {
            out.push_str(&format!(
                "    {}[\"(external) {}\"]\n",
                ext_id,
                Self::escape_label(ext_id)
            ));
        }

        // 边定义
        for line in &graph.lines {
            match &line.label {
                Some(label) => {
                    out.push_str(&format!("    {} -- {} --> {}\n", line.from, label, line.to));
                }
                None => {
                    out.push_str(&format!("    {} --> {}\n", line.from, line.to));
                }
            }
        }

        // 样式类（基于 category）
        let mut classes_seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for node in &graph.nodes {
            if let Some(cat) = &node.category {
                if classes_seen.insert(cat.as_str()) {
                    out.push_str(&format!("    class {} {}\n", node.id, cat));
                } else {
                    // 同一 category 的其它节点也要加上 class 声明
                    out.push_str(&format!("    class {} {}\n", node.id, cat));
                }
            }
        }

        out
    }
}
```

- [ ] **Step 4: 运行测试验证通过**

Run: `cargo test -p ai-orz lib::pkg::utils::graph::graph_test -- --nocapture 2>&1 | tail -30`
Expected: PASS（所有 8 个测试通过）

- [ ] **Step 5: Commit**

```bash
git add src/pkg/utils/graph/
git commit -m "feat(pkg): implement MermaidRenderer for graph component"
```

---

## Task 3: Project Domain - Task Graph 构建器

**Files:**
- Create: `src/service/domain/project/task_graph.rs`
- Create: `src/service/domain/project/task_graph_test.rs`
- Modify: `src/service/domain/project/mod.rs`

- [ ] **Step 1: 注册 task_graph 子模块**

修改 `src/service/domain/project/mod.rs`，在 `mod artifact;` 附近加入：

```rust
mod task;
mod task_graph;
```

并在 `#[cfg(test)]` 区域加入：

```rust
#[cfg(test)]
mod task_graph_test;
```

- [ ] **Step 2: 编写失败测试**

创建 `src/service/domain/project/task_graph_test.rs`：

```rust
//! Task Graph 构建器测试

use crate::models::task::{Task, TaskPo};
use crate::pkg::utils::graph::MermaidDirection;
use common::enums::{AssigneeType, TaskStatus};
use common::constants::utils;

use super::task_graph::build_task_graph_mermaid;

fn make_task(id: &str, title: &str, status: TaskStatus, deps: Vec<&str>) -> Task {
    Task::from_po(TaskPo {
        id: id.to_string(),
        title: title.to_string(),
        description: String::new(),
        status,
        priority: 0,
        tags: "[]".to_string(),
        due_at: None,
        start_at: None,
        end_at: None,
        dependencies: if deps.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&deps).unwrap())
        },
        root_user_id: "u1".to_string(),
        assignee_type: AssigneeType::User,
        assignee_id: "u1".to_string(),
        project_id: Some("p1".to_string()),
        thinking_depth: 0,
        progress: 0,
        created_by: "u1".to_string(),
        modified_by: "u1".to_string(),
        created_at: utils::current_timestamp_ms(),
        updated_at: utils::current_timestamp_ms(),
    })
}

#[test]
fn test_empty_tasks_renders_empty_graph() {
    let tasks: Vec<Task> = vec![];
    let result = build_task_graph_mermaid(&tasks, MermaidDirection::LR);
    assert!(result.contains("flowchart LR"));
    // 空图不应该有节点定义
    assert!(!result.contains("[\""));
}

#[test]
fn test_single_task_no_deps() {
    let tasks = vec![make_task("t1", "Task 1", TaskStatus::Todo, vec![])];
    let result = build_task_graph_mermaid(&tasks, MermaidDirection::LR);
    assert!(result.contains("t1[\"Task 1\"]"));
    assert!(!result.contains("-->"));
}

#[test]
fn test_dependency_renders_arrow_in_correct_direction() {
    // t2 依赖 t1，意味着 t1 是 t2 的前置，图上应该是 t1 --> t2
    let tasks = vec![
        make_task("t1", "Task 1", TaskStatus::Done, vec![]),
        make_task("t2", "Task 2", TaskStatus::Todo, vec!["t1"]),
    ];
    let result = build_task_graph_mermaid(&tasks, MermaidDirection::LR);
    assert!(result.contains("t1 --> t2"));
}

#[test]
fn test_status_category_applied() {
    let tasks = vec![
        make_task("t1", "Task 1", TaskStatus::Done, vec![]),
        make_task("t2", "Task 2", TaskStatus::Doing, vec![]),
        make_task("t3", "Task 3", TaskStatus::Todo, vec![]),
    ];
    let result = build_task_graph_mermaid(&tasks, MermaidDirection::LR);
    assert!(result.contains("class t1 done"));
    assert!(result.contains("class t2 doing"));
    assert!(result.contains("class t3 todo"));
}

#[test]
fn test_cross_project_dependency_rendered_as_external() {
    // t2 依赖一个不在当前任务列表中的 task（跨项目依赖）
    let tasks = vec![make_task("t1", "Task 1", TaskStatus::Todo, vec!["external_task_id"])];
    let result = build_task_graph_mermaid(&tasks, MermaidDirection::LR);
    assert!(result.contains("external_task_id"));
    assert!(result.contains("(external)"));
}

#[test]
fn test_multiple_dependencies() {
    // t3 依赖 t1 和 t2
    let tasks = vec![
        make_task("t1", "Task 1", TaskStatus::Done, vec![]),
        make_task("t2", "Task 2", TaskStatus::Done, vec![]),
        make_task("t3", "Task 3", TaskStatus::Todo, vec!["t1", "t2"]),
    ];
    let result = build_task_graph_mermaid(&tasks, MermaidDirection::LR);
    assert!(result.contains("t1 --> t3"));
    assert!(result.contains("t2 --> t3"));
}
```

- [ ] **Step 3: 运行测试验证失败**

Run: `cargo test -p ai-orz service::domain::project::task_graph_test -- --nocapture 2>&1 | tail -20`
Expected: FAIL（task_graph 模块还未实现）

- [ ] **Step 4: 实现 task_graph 构建器**

创建 `src/service/domain/project/task_graph.rs`：

```rust
//! Task Graph 构建器
//!
//! 基于 Task 列表的 dependencies 字段构建有向无环图（DAG），
//! 并渲染为 Mermaid 字符串供前端/文档直接使用。
//!
//! 依赖方向说明：
//! - Task.dependencies 字段存储"前置任务 ID 列表"，即 A.dependencies 含 B 表示 A 依赖 B
//! - 在图上，前置任务应该在前面（视觉上靠左/靠上），所以画 B --> A（B 指向 A）
//! - 这样图的箭头方向表示"执行流向"：B 完成后才能执行 A

use crate::models::task::Task;
use crate::pkg::utils::graph::{Graph, GraphLine, GraphNodeData, MermaidDirection, MermaidRenderer};
use common::enums::TaskStatus;

/// 基于 Task 列表构建 Mermaid 任务依赖图
///
/// 参数：
/// - `tasks`: 同一个项目内的所有任务
/// - `direction`: 图方向
///
/// 返回 Mermaid flowchart 语法的字符串。
pub fn build_task_graph_mermaid(tasks: &[Task], direction: MermaidDirection) -> String {
    let graph = build_task_graph(tasks);
    let renderer = MermaidRenderer::new(direction);
    graph.render(&renderer)
}

/// 基于 Task 列表构建 Graph 数据结构
fn build_task_graph(tasks: &[Task]) -> Graph {
    let mut graph = Graph::new();

    // 添加所有任务为节点
    for task in tasks {
        let category = task_status_to_category(&task.po.status);
        let node = GraphNodeData::new(task.po.id.clone(), task.po.title.clone())
            .with_category(category);
        graph.add_node(node);
    }

    // 添加依赖边
    // task.dependencies 含 dep_id 表示 task 依赖 dep_id
    // 图上画 dep_id --> task（dep 在前，task 在后）
    for task in tasks {
        let deps = task.po.get_dependencies();
        for dep_id in deps {
            let line = GraphLine::new(dep_id, task.po.id.clone());
            graph.add_line(line);
        }
    }

    graph
}

/// 将 TaskStatus 转换为 mermaid 样式分类
fn task_status_to_category(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Todo => "todo",
        TaskStatus::Doing => "doing",
        TaskStatus::Done => "done",
        TaskStatus::Cancelled => "cancelled",
        TaskStatus::Failed => "failed",
    }
}
```

- [ ] **Step 5: 运行测试验证通过**

Run: `cargo test -p ai-orz service::domain::project::task_graph_test -- --nocapture 2>&1 | tail -30`
Expected: PASS（所有 6 个测试通过）

- [ ] **Step 6: Commit**

```bash
git add src/service/domain/project/mod.rs src/service/domain/project/task_graph.rs src/service/domain/project/task_graph_test.rs
git commit -m "feat(domain): add task graph builder with mermaid rendering"
```

---

## Task 4: Project/Task 实体扩展

**Files:**
- Modify: `src/models/project.rs`
- Modify: `src/models/task.rs`

- [ ] **Step 1: Project 实体加 task_graph 和 artifacts 字段**

修改 `src/models/project.rs`，在 `Project` 结构体中加入两个字段：

```rust
use common::api::ArtifactDetail;

/// Project 业务实体
///
/// 聚合所有相关信息：项目基本信息 + 任务列表
/// 这是 Domain 层返回给上层的类型
#[derive(Debug, Clone)]
pub struct Project {
    /// 底层持久化对象
    pub po: ProjectPo,
    /// 搜索匹配元信息（搜索场景下由 DAL 层填充）
    pub search_match: Option<crate::models::vector::SearchMatchInfo>,
    /// 统计数据（由 DAL 层按需注入）
    pub stats: Option<ProjectStats>,
    /// 模型调用统计数据（由 DAL 层按需注入）
    pub model_call_stats: Option<ModelCallStats>,
    /// 任务依赖图（Mermaid 字符串，由 Domain 层按需注入）
    pub task_graph: Option<String>,
    /// 产物列表（由 Domain 层按需注入）
    pub artifacts: Option<Vec<ArtifactDetail>>,
}
```

同时更新 `from_po` 方法：

```rust
impl Project {
    /// 从 PO 创建 Project
    pub fn from_po(po: ProjectPo) -> Self {
        Self {
            po,
            search_match: None,
            stats: None,
            model_call_stats: None,
            task_graph: None,
            artifacts: None,
        }
    }
}
```

注意检查 Project 中其他构造方法（如 `new`），同步加入新字段为 `None`。

- [ ] **Step 2: Task 实体加 artifacts 字段**

修改 `src/models/task.rs`，在 `Task` 结构体中加入字段：

```rust
use common::api::ArtifactDetail;

/// Task 业务实体
///
/// 聚合所有相关信息：任务基本信息 + 产物列表
/// 这是 Domain 层返回给上层的类型
#[derive(Debug, Clone)]
pub struct Task {
    /// 底层持久化对象
    pub po: TaskPo,
    /// 搜索匹配元信息（搜索场景下由 DAL 层填充）
    pub search_match: Option<SearchMatchInfo>,
    /// 统计数据（由 DAL 层按需注入）
    pub stats: Option<TaskStats>,
    /// 模型调用统计数据（由 DAL 层按需注入）
    pub model_call_stats: Option<ModelCallStats>,
    /// 产物列表（由 Domain 层按需注入）
    pub artifacts: Option<Vec<ArtifactDetail>>,
}
```

同时更新 `from_po` 方法：

```rust
impl Task {
    /// 从 PO 创建 Task
    pub fn from_po(po: TaskPo) -> Self {
        Self {
            po,
            search_match: None,
            stats: None,
            model_call_stats: None,
            artifacts: None,
        }
    }
}
```

注意检查 Task 中其他构造方法，同步加入新字段为 `None`。

- [ ] **Step 3: 编译验证**

Run: `cargo build -p ai-orz 2>&1 | tail -30`
Expected: BUILD SUCCEEDED（如果有编译错误，是因为其他地方构造 Project/Task 时缺少新字段，需要同步更新）

- [ ] **Step 4: 修复所有编译错误**

如果 Step 3 失败，根据错误信息逐个修复构造 Project/Task 的地方，为新字段加上 `None`。

可能的位置：
- `src/service/dal/project.rs` 中 `ProjectDalImpl::get_project` 等方法
- `src/service/dal/task.rs` 中 `TaskDalImpl::get_task` 等方法
- 测试代码中构造 Project/Task 的地方

- [ ] **Step 5: Commit**

```bash
git add src/models/project.rs src/models/task.rs src/service/dal/
git commit -m "feat(models): add task_graph and artifacts fields to Project/Task entities"
```

---

## Task 5: ProjectFetchOptions / TaskFetchOptions 扩展

**Files:**
- Modify: `src/service/dal/project.rs`
- Modify: `src/service/dal/task.rs`

- [ ] **Step 1: ProjectFetchOptions 加新字段**

修改 `src/service/dal/project.rs`，在 `ProjectFetchOptions` 结构体加入：

```rust
/// Project 附带信息获取选项
#[derive(Debug, Clone, Default)]
pub struct ProjectFetchOptions {
    /// 是否加载统计信息（ProjectStats: 事件次数汇总）
    pub with_stats: Option<bool>,
    /// 是否加载模型调用统计（ModelCallStats: token + 时序）
    pub with_model_call_stats: Option<bool>,
    /// 统计时间范围（毫秒），None 表示全部历史
    pub stats_time_range: Option<(i64, i64)>,
    /// 时序查询粒度，None 时默认 Daily
    pub stats_interval: Option<StatsInterval>,
    /// 是否加载任务依赖图（Mermaid 字符串）
    pub with_task_graph: Option<bool>,
    /// 是否加载产物列表（ArtifactDetail）
    pub with_artifacts: Option<bool>,
}
```

- [ ] **Step 2: TaskFetchOptions 加新字段**

修改 `src/service/dal/task.rs`，在 `TaskFetchOptions` 结构体加入：

```rust
/// Task 附带信息获取选项
#[derive(Debug, Clone, Default)]
pub struct TaskFetchOptions {
    /// 是否加载统计信息（TaskStats: 事件次数汇总）
    pub with_stats: Option<bool>,
    /// 是否加载模型调用统计（ModelCallStats: token + 时序）
    pub with_model_call_stats: Option<bool>,
    /// 统计时间范围（毫秒），None 表示全部历史
    pub stats_time_range: Option<(i64, i64)>,
    /// 时序查询粒度，None 时默认 Daily
    pub stats_interval: Option<StatsInterval>,
    /// 是否加载产物列表（ArtifactDetail）
    pub with_artifacts: Option<bool>,
}
```

注意：实际字段以现有 TaskFetchOptions 为准，保留现有字段，仅添加 `with_artifacts`。

- [ ] **Step 3: 编译验证**

Run: `cargo build -p ai-orz 2>&1 | tail -10`
Expected: BUILD SUCCEEDED

- [ ] **Step 4: Commit**

```bash
git add src/service/dal/project.rs src/service/dal/task.rs
git commit -m "feat(dal): add with_task_graph and with_artifacts options"
```

---

## Task 6: Domain 层聚合 - Project get_project 注入

**Files:**
- Modify: `src/service/domain/project/service.rs`

- [ ] **Step 1: 实现 get_project 聚合逻辑**

修改 `src/service/domain/project/service.rs`，将 `get_project` 从纯透传改为聚合：

```rust
use crate::models::artifact::Artifact;
use crate::pkg::utils::graph::MermaidDirection;
use crate::service::dao::task::TaskQuery;
use common::api::ArtifactDetail;
use common::enums::TaskStatus;

use super::task_graph::build_task_graph_mermaid;

/// ... 保留其他 imports ...

#[async_trait::async_trait]
impl super::ProjectManage for ProjectDomainImpl {
    // ... 其他方法不变 ...

    /// 根据 ID 获取项目（带附带信息选项）
    ///
    /// Domain 层聚合：在 DAL 返回基础 Project 后，按 options 注入：
    /// - task_graph: 调用 task_dal 查询项目任务，用 graph 组件生成 mermaid
    /// - artifacts: 调用 artifact_dal 查询项目级产物列表
    async fn get_project(
        &self,
        ctx: RequestContext,
        id: &str,
        options: crate::service::dal::project::ProjectFetchOptions,
    ) -> Result<Option<Project>> {
        // 先调 DAL 拿基础 project（含 stats / model_call_stats）
        let mut project = self.project_dal.get_project(ctx.clone(), id, options.clone()).await?;

        if let Some(project) = project.as_mut() {
            // 注入 task_graph
            if options.with_task_graph.unwrap_or(false) {
                let tasks = self.task_dal
                    .list_by_project(ctx.clone(), id)
                    .await?;
                let mermaid = build_task_graph_mermaid(&tasks, MermaidDirection::LR);
                project.task_graph = Some(mermaid);
            }

            // 注入 artifacts
            if options.with_artifacts.unwrap_or(false) {
                let artifacts = self.artifact_dal
                    .list_by_project(ctx.clone(), id)
                    .await?;
                project.artifacts = Some(
                    artifacts.iter().map(artifact_to_detail).collect::<Result<Vec<_>>>()?
                );
            }
        }

        Ok(project)
    }

    // ... 其他方法不变 ...
}

/// 将 Artifact 业务实体转换为 ArtifactDetail DTO
fn artifact_to_detail(artifact: &Artifact) -> Result<ArtifactDetail> {
    Ok(ArtifactDetail {
        id: artifact.po.id.clone(),
        project_id: artifact.po.project_id.clone(),
        task_id: artifact.po.task_id.clone(),
        name: artifact.po.name.clone(),
        description: artifact.po.description.clone(),
        file_type: artifact.po.file_type,
        source_type: artifact.po.source_type,
        file_path: artifact.po.file_meta.file_path.clone(),
        mime_type: artifact.po.file_meta.mime_type.clone(),
        file_size: artifact.po.file_meta.file_size,
        tags: artifact.po.tags(),
        status: artifact.po.status,
        created_by: artifact.po.created_by.clone(),
        modified_by: artifact.po.modified_by.clone(),
        created_at: artifact.po.created_at,
        updated_at: artifact.po.updated_at,
    })
}
```

注意：
1. `ProjectDomainImpl` 已持有 `task_dal` 和 `artifact_dal`（见 `mod.rs:50`），无需新增依赖。
2. 需要确认 `TaskDal` 是否有 `list_by_project` 方法。如果没有，使用 `TaskDal::query` 配合 `TaskQuery { project_id: Some(id), .. }`。
3. 需要确认 `ArtifactDal::list_by_project` 返回的类型（应该是 `Vec<Artifact>` 业务实体）。
4. `options.clone()` 需要 `ProjectFetchOptions` 实现 `Clone`（已有 `#[derive(Clone)]`）。

- [ ] **Step 2: 确认 TaskDal 的查询方法**

Run: `cargo build -p ai-orz 2>&1 | grep -E "error\[|warning.*task_dal" | head -20`

如果 `list_by_project` 方法不存在，改用 `query`：

```rust
// 替代方案：使用 TaskDal::query
let task_query = TaskQuery {
    project_id: Some(id.to_string()),
    ..Default::default()
};
let tasks = self.task_dal.query(ctx.clone(), task_query).await?;
```

- [ ] **Step 3: 编译验证**

Run: `cargo build -p ai-orz 2>&1 | tail -20`
Expected: BUILD SUCCEEDED

- [ ] **Step 4: Commit**

```bash
git add src/service/domain/project/service.rs
git commit -m "feat(domain): inject task_graph and artifacts in get_project"
```

---

## Task 7: Domain 层聚合 - Task get_task 注入 artifacts

**Files:**
- Modify: `src/service/domain/project/task.rs`

- [ ] **Step 1: 找到 TaskManage::get_task 方法**

读取 `src/service/domain/project/task.rs`，找到现有的 `get_task` 或 `get` 方法。

Run: `grep -n "async fn get" src/service/domain/project/task.rs`

- [ ] **Step 2: 实现 get_task 聚合 artifacts**

在 TaskManage trait 实现中，找到按 options 获取 task 的方法（通常叫 `get_task`），加入 artifacts 注入：

```rust
async fn get_task(
    &self,
    ctx: RequestContext,
    id: &str,
    options: crate::service::dal::task::TaskFetchOptions,
) -> Result<Option<Task>> {
    // 先调 DAL 拿基础 task
    let mut task = self.task_dal.get_task(ctx.clone(), id, options.clone()).await?;

    if let Some(task) = task.as_mut() {
        // 注入 artifacts
        if options.with_artifacts.unwrap_or(false) {
            if let Some(task_id) = task.po.id.as_str().into() {
                let artifacts = self.artifact_dal
                    .list_by_task(ctx.clone(), task_id)
                    .await?;
                task.artifacts = Some(
                    artifacts.iter().map(|a| ArtifactDetail {
                        id: a.po.id.clone(),
                        project_id: a.po.project_id.clone(),
                        task_id: a.po.task_id.clone(),
                        name: a.po.name.clone(),
                        description: a.po.description.clone(),
                        file_type: a.po.file_type,
                        source_type: a.po.source_type,
                        file_path: a.po.file_meta.file_path.clone(),
                        mime_type: a.po.file_meta.mime_type.clone(),
                        file_size: a.po.file_meta.file_size,
                        tags: a.po.tags(),
                        status: a.po.status,
                        created_by: a.po.created_by.clone(),
                        modified_by: a.po.modified_by.clone(),
                        created_at: a.po.created_at,
                        updated_at: a.po.updated_at,
                    }).collect::<Vec<_>>()
                );
            }
        }
    }

    Ok(task)
}
```

注意：
1. 实际方法签名以现有代码为准。
2. `ProjectDomainImpl` 已持有 `artifact_dal`，无需新增依赖。
3. 如果 `list_by_task` 方法签名不匹配，按实际签名调整。
4. 考虑提取 `artifact_to_detail` 为公共函数（参考 Task 6），避免重复代码。

- [ ] **Step 3: 提取 artifact_to_detail 公共函数**

如果 Task 6 和 Task 7 都需要 `artifact_to_detail`，将其提取到 `src/service/domain/project/mod.rs` 或 `src/service/domain/project/artifact.rs` 中作为 pub(crate) 函数。

- [ ] **Step 4: 编译验证**

Run: `cargo build -p ai-orz 2>&1 | tail -20`
Expected: BUILD SUCCEEDED

- [ ] **Step 5: Commit**

```bash
git add src/service/domain/project/task.rs src/service/domain/project/mod.rs src/service/domain/project/artifact.rs
git commit -m "feat(domain): inject artifacts in get_task"
```

---

## Task 8: API DTO 扩展 - Project

**Files:**
- Modify: `common/src/api/project.rs`

- [ ] **Step 1: GetProjectRequest 加新参数**

修改 `common/src/api/project.rs`，在 `GetProjectRequest` 中加入：

```rust
/// Get project detail request.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetProjectRequest {
    /// Project ID.
    #[param(source = "path")]
    pub id: String,

    /// 是否加载统计信息（ProjectStats: 事件次数汇总）
    #[param(source = "query")]
    pub with_stats: Option<bool>,

    /// 是否加载模型调用统计（ModelCallStats: token + 时序）
    #[param(source = "query")]
    pub with_model_call_stats: Option<bool>,

    /// 统计时间范围起始（毫秒），与 stats_time_end 配合使用
    #[param(source = "query")]
    pub stats_time_start: Option<i64>,

    /// 统计时间范围结束（毫秒），与 stats_time_start 配合使用
    #[param(source = "query")]
    pub stats_time_end: Option<i64>,

    /// 时序查询粒度：hourly / daily
    #[param(source = "query")]
    pub stats_interval: Option<String>,

    /// 是否加载任务依赖图（Mermaid 字符串）
    #[param(source = "query")]
    pub with_task_graph: Option<bool>,

    /// 是否加载产物列表
    #[param(source = "query")]
    pub with_artifacts: Option<bool>,
}
```

注意：保留现有字段，仅添加最后两个新字段。

- [ ] **Step 2: GetProjectResponse 加新字段**

修改 `GetProjectResponse`：

```rust
/// Project detail response.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetProjectResponse {
    // ... 现有字段保持不变 ...

    /// 任务依赖图（Mermaid 字符串），按需返回（with_task_graph=true 时填充）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_graph: Option<String>,

    /// 产物列表，按需返回（with_artifacts=true 时填充）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<Vec<ArtifactDetail>>,
}
```

注意：需要在文件顶部 import `ArtifactDetail`：

```rust
use crate::api::ArtifactDetail;
```

- [ ] **Step 3: 编译验证**

Run: `cargo build -p ai-orz-common 2>&1 | tail -10`
Expected: BUILD SUCCEEDED

- [ ] **Step 4: Commit**

```bash
git add common/src/api/project.rs
git commit -m "feat(api): add with_task_graph/with_artifacts to GetProjectRequest/Response"
```

---

## Task 9: API DTO 扩展 - Task

**Files:**
- Modify: `common/src/api/task.rs`

- [ ] **Step 1: GetTaskRequest 加 with_artifacts 参数**

修改 `common/src/api/task.rs`，在 `GetTaskRequest` 中加入：

```rust
/// Get task detail request.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetTaskRequest {
    /// Task ID.
    #[param(source = "path")]
    pub id: String,

    // ... 现有字段保持不变 ...

    /// 是否加载产物列表
    #[param(source = "query")]
    pub with_artifacts: Option<bool>,
}
```

注意：保留现有所有字段，仅添加 `with_artifacts`。

- [ ] **Step 2: GetTaskResponse 加 artifacts 字段**

修改 `GetTaskResponse`：

```rust
/// Task detail response.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetTaskResponse {
    // ... 现有字段保持不变 ...

    /// 产物列表，按需返回（with_artifacts=true 时填充）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<Vec<ArtifactDetail>>,
}
```

注意：需要在文件顶部 import `ArtifactDetail`：

```rust
use crate::api::ArtifactDetail;
```

- [ ] **Step 3: 编译验证**

Run: `cargo build -p ai-orz-common 2>&1 | tail -10`
Expected: BUILD SUCCEEDED

- [ ] **Step 4: Commit**

```bash
git add common/src/api/task.rs
git commit -m "feat(api): add with_artifacts to GetTaskRequest/Response"
```

---

## Task 10: Handler 层适配 - Project

**Files:**
- Modify: `src/handlers/project/projects/get_project.rs`
- Modify: `src/handlers/project/projects/response.rs`

- [ ] **Step 1: get_project handler 传递新 options**

修改 `src/handlers/project/projects/get_project.rs`：

```rust
let options = ProjectFetchOptions {
    with_stats: params.with_stats,
    with_model_call_stats: params.with_model_call_stats,
    stats_time_range: match (params.stats_time_start, params.stats_time_end) {
        (Some(start), Some(end)) => Some((start, end)),
        _ => None,
    },
    stats_interval: params.stats_interval.as_deref().and_then(|s| {
        match s.to_lowercase().as_str() {
            "hourly" => Some(StatsInterval::Hourly),
            "daily" => Some(StatsInterval::Daily),
            _ => None,
        }
    }),
    with_task_graph: params.with_task_graph,
    with_artifacts: params.with_artifacts,
};
```

- [ ] **Step 2: response::to_detail 映射新字段**

修改 `src/handlers/project/projects/response.rs`：

```rust
pub(super) fn to_detail(project: &Project) -> GetProjectResponse {
    GetProjectResponse {
        // ... 现有字段保持不变 ...
        stats: project.stats.clone(),
        model_call_stats: project.model_call_stats.clone(),
        task_graph: project.task_graph.clone(),
        artifacts: project.artifacts.clone(),
    }
}
```

- [ ] **Step 3: 编译验证**

Run: `cargo build -p ai-orz 2>&1 | tail -20`
Expected: BUILD SUCCEEDED

- [ ] **Step 4: 运行现有测试确保不破坏**

Run: `cargo test -p ai-orz --lib handlers::project 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/handlers/project/projects/
git commit -m "feat(handler): wire up task_graph and artifacts in get_project response"
```

---

## Task 11: Handler 层适配 - Task

**Files:**
- Modify: `src/handlers/project/tasks/get_task.rs`
- Modify: `src/handlers/project/tasks/response.rs`

- [ ] **Step 1: 找到 get_task handler**

确认 `src/handlers/project/tasks/get_task.rs` 存在并读取其内容。

Run: `cat src/handlers/project/tasks/get_task.rs`

- [ ] **Step 2: get_task handler 传递 with_artifacts**

修改 `src/handlers/project/tasks/get_task.rs`，在构造 `TaskFetchOptions` 时加入：

```rust
let options = TaskFetchOptions {
    // ... 现有字段 ...
    with_artifacts: params.with_artifacts,
};
```

- [ ] **Step 3: response::to_detail 映射 artifacts**

修改 `src/handlers/project/tasks/response.rs`：

```rust
pub(super) fn to_detail(task: &Task) -> GetTaskResponse {
    GetTaskResponse {
        // ... 现有字段保持不变 ...
        artifacts: task.artifacts.clone(),
    }
}
```

- [ ] **Step 4: 编译验证**

Run: `cargo build -p ai-orz 2>&1 | tail -10`
Expected: BUILD SUCCEEDED

- [ ] **Step 5: Commit**

```bash
git add src/handlers/project/tasks/
git commit -m "feat(handler): wire up artifacts in get_task response"
```

---

## Task 12: 集成测试 - 完整流程

**Files:**
- Modify: `src/service/domain/project/project_test.rs`（或新建测试文件）

- [ ] **Step 1: 编写集成测试**

在 project_test.rs 中加入测试：

```rust
#[cfg(test)]
mod task_graph_integration_test {
    use super::*;
    use crate::models::task::{Task, TaskPo};
    use crate::pkg::utils::graph::MermaidDirection;
    use crate::service::domain::project::task_graph::build_task_graph_mermaid;
    use common::enums::{AssigneeType, TaskStatus};

    #[test]
    fn test_project_with_task_graph_full_flow() {
        // 构造一个有依赖关系的任务列表
        let tasks = vec![
            make_task("t1", "设计数据库", TaskStatus::Done, vec![]),
            make_task("t2", "实现 API", TaskStatus::Doing, vec!["t1"]),
            make_task("t3", "前端对接", TaskStatus::Todo, vec!["t2"]),
            make_task("t4", "测试", TaskStatus::Todo, vec!["t2", "t3"]),
        ];

        let mermaid = build_task_graph_mermaid(&tasks, MermaidDirection::LR);

        // 验证图结构
        assert!(mermaid.contains("flowchart LR"));
        assert!(mermaid.contains("t1[\"设计数据库\"]"));
        assert!(mermaid.contains("t1 --> t2"));
        assert!(mermaid.contains("t2 --> t3"));
        assert!(mermaid.contains("t2 --> t4"));
        assert!(mermaid.contains("t3 --> t4"));
        assert!(mermaid.contains("class t1 done"));
        assert!(mermaid.contains("class t2 doing"));
    }

    fn make_task(id: &str, title: &str, status: TaskStatus, deps: Vec<&str>) -> Task {
        // 复用 task_graph_test.rs 中的辅助函数，或重新定义
        Task::from_po(TaskPo {
            id: id.to_string(),
            title: title.to_string(),
            description: String::new(),
            status,
            priority: 0,
            tags: "[]".to_string(),
            due_at: None,
            start_at: None,
            end_at: None,
            dependencies: if deps.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&deps).unwrap())
            },
            root_user_id: "u1".to_string(),
            assignee_type: AssigneeType::User,
            assignee_id: "u1".to_string(),
            project_id: Some("p1".to_string()),
            thinking_depth: 0,
            progress: 0,
            created_by: "u1".to_string(),
            modified_by: "u1".to_string(),
            created_at: 0,
            updated_at: 0,
        })
    }
}
```

- [ ] **Step 2: 运行测试**

Run: `cargo test -p ai-orz --lib service::domain::project::project_test::task_graph_integration_test -- --nocapture 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 3: 运行所有相关测试确保无回归**

Run: `cargo test -p ai-orz --lib pkg::utils::graph service::domain::project handlers::project 2>&1 | tail -30`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/service/domain/project/project_test.rs
git commit -m "test: add integration tests for task graph and artifact injection"
```

---

## Task 13: 文档更新

**Files:**
- Modify: `docs/superpowers/specs/`（如果有相关 spec）

- [ ] **Step 1: 更新项目记忆**

在 `project_memory.md` 中加入新的硬约束：

```markdown
- Task Graph Mermaid 生成逻辑必须位于 src/service/domain/project/task_graph.rs，使用 pkg/utils/graph 通用组件
- pkg/utils/graph 是通用图形渲染组件，必须保持零业务依赖；MermaidRenderer 只是其中一种实现
- Project/Task 实体的 artifacts 字段复用 ArtifactDetail DTO，不新建 ArtifactSummary
```

- [ ] **Step 2: Commit**

```bash
git add /Users/aman/.trae-cn/memory/projects/-Users-aman-Technology-rust-ai-orz/project_memory.md
git commit -m "docs: update project memory with graph component constraints"
```

---

## Self-Review

### Spec coverage
- ✅ Mermaid 任务图按需返回：Task 1-3（graph 组件）、Task 6（domain 注入）、Task 8/10（API/handler）
- ✅ 通用图形组件 pkg/utils/graph：Task 1-2
- ✅ 同项目 + 任务状态着色：Task 3（status_to_category）
- ✅ Artifact 在详情接口暴露：Task 4-7、9-11
- ✅ ArtifactDetail 复用（含 ID 等关键字段）：Task 6/7（artifact_to_detail）
- ✅ Domain 层聚合（project domain）：Task 6-7

### Placeholder scan
- ✅ 所有代码块都是完整实现，无 TBD/TODO
- ✅ 所有文件路径都是绝对路径或相对项目根的路径
- ✅ 测试代码完整可运行

### Type consistency
- ✅ `Graph`/`GraphNodeData`/`GraphLine` 在 Task 1 定义，Task 2/3 使用一致
- ✅ `MermaidRenderer`/`MermaidDirection` 在 Task 2 定义，Task 3/6 使用一致
- ✅ `ProjectFetchOptions.with_task_graph`/`with_artifacts` 在 Task 5 定义，Task 6/8/10 使用一致
- ✅ `TaskFetchOptions.with_artifacts` 在 Task 5 定义，Task 7/9/11 使用一致
- ✅ `Project.task_graph`/`artifacts` 在 Task 4 定义，Task 6/10 使用一致
- ✅ `Task.artifacts` 在 Task 4 定义，Task 7/11 使用一致

---

## Execution Handoff

**Plan complete and saved to `docs/superpowers/plans/2026-07-28-project-task-enhancement.md`. Two execution options:**

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**
