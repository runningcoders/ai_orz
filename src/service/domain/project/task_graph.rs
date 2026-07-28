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
        TaskStatus::Cancelled => "cancelled",
        TaskStatus::PendingReview => "pending_review",
        TaskStatus::Pending => "todo",
        TaskStatus::InProgress => "doing",
        TaskStatus::Completed => "done",
        TaskStatus::Archived => "archived",
    }
}
