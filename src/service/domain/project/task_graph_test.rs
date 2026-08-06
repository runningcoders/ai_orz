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
        execution_plan: None,
        execution_result: None,
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

#[test]
fn test_complex_dag_full_flow() {
    // 集成测试：构造一个 4 任务的复杂 DAG
    //
    // 依赖结构：
    //   t1 (已完成) --> t2 (进行中) --> t3 (待开始)
    //                  t2 (进行中) --> t4 (待开始)
    //                  t3 (待开始) --> t4 (待开始)
    //
    // 预期 mermaid 边：
    //   t1 --> t2
    //   t2 --> t3
    //   t2 --> t4
    //   t3 --> t4
    let tasks = vec![
        make_task("t1", "设计数据库", TaskStatus::Completed, vec![]),
        make_task("t2", "实现 API", TaskStatus::InProgress, vec!["t1"]),
        make_task("t3", "前端对接", TaskStatus::Pending, vec!["t2"]),
        make_task("t4", "测试", TaskStatus::Pending, vec!["t2", "t3"]),
    ];

    let mermaid = build_task_graph_mermaid(&tasks, MermaidDirection::LR);

    // 验证图方向
    assert!(mermaid.contains("flowchart LR"));

    // 验证节点定义
    assert!(mermaid.contains("t1[\"设计数据库\"]"));
    assert!(mermaid.contains("t2[\"实现 API\"]"));
    assert!(mermaid.contains("t3[\"前端对接\"]"));
    assert!(mermaid.contains("t4[\"测试\"]"));

    // 验证依赖边（执行流向：前置任务指向后继任务）
    assert!(mermaid.contains("t1 --> t2"));
    assert!(mermaid.contains("t2 --> t3"));
    assert!(mermaid.contains("t2 --> t4"));
    assert!(mermaid.contains("t3 --> t4"));

    // 验证状态着色（基于 task_status_to_category 的实际映射）
    // Completed -> "done", InProgress -> "doing", Pending -> "todo"
    assert!(mermaid.contains("class t1 done"));
    assert!(mermaid.contains("class t2 doing"));
    assert!(mermaid.contains("class t3 todo"));
    assert!(mermaid.contains("class t4 todo"));
}
