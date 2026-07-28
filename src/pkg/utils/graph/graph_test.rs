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
