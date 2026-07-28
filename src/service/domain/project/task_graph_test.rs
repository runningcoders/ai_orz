//! Task Graph 构建器测试

use crate::models::task::{Task, TaskPo};
use crate::pkg::utils::graph::MermaidDirection;
use common::constants::utils;
use common::enums::{AssigneeType, TaskStatus};

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
    let tasks = vec![make_task("t1", "Task 1", TaskStatus::Pending, vec![])];
    let result = build_task_graph_mermaid(&tasks, MermaidDirection::LR);
    assert!(result.contains("t1[\"Task 1\"]"));
    assert!(!result.contains("-->"));
}

#[test]
fn test_dependency_renders_arrow_in_correct_direction() {
    // t2 依赖 t1，意味着 t1 是 t2 的前置，图上应该是 t1 --> t2
    let tasks = vec![
        make_task("t1", "Task 1", TaskStatus::Completed, vec![]),
        make_task("t2", "Task 2", TaskStatus::Pending, vec!["t1"]),
    ];
    let result = build_task_graph_mermaid(&tasks, MermaidDirection::LR);
    assert!(result.contains("t1 --> t2"));
}

#[test]
fn test_status_category_applied() {
    let tasks = vec![
        make_task("t1", "Task 1", TaskStatus::Completed, vec![]),
        make_task("t2", "Task 2", TaskStatus::InProgress, vec![]),
        make_task("t3", "Task 3", TaskStatus::Pending, vec![]),
    ];
    let result = build_task_graph_mermaid(&tasks, MermaidDirection::LR);
    assert!(result.contains("class t1 done"));
    assert!(result.contains("class t2 doing"));
    assert!(result.contains("class t3 todo"));
}

#[test]
fn test_cross_project_dependency_rendered_as_external() {
    // t2 依赖一个不在当前任务列表中的 task（跨项目依赖）
    let tasks = vec![make_task(
        "t1",
        "Task 1",
        TaskStatus::Pending,
        vec!["external_task_id"],
    )];
    let result = build_task_graph_mermaid(&tasks, MermaidDirection::LR);
    assert!(result.contains("external_task_id"));
    assert!(result.contains("(external)"));
}

#[test]
fn test_multiple_dependencies() {
    // t3 依赖 t1 和 t2
    let tasks = vec![
        make_task("t1", "Task 1", TaskStatus::Completed, vec![]),
        make_task("t2", "Task 2", TaskStatus::Completed, vec![]),
        make_task("t3", "Task 3", TaskStatus::Pending, vec!["t1", "t2"]),
    ];
    let result = build_task_graph_mermaid(&tasks, MermaidDirection::LR);
    assert!(result.contains("t1 --> t3"));
    assert!(result.contains("t2 --> t3"));
}
