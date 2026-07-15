use dioxus::prelude::*;
use std::collections::HashSet;

use crate::api::hr::search_memory_with_traversal;
use crate::components::button::Button;
use crate::components::graph::{calculate_layout, expand_layout, Graph, GraphEdge, GraphNode};
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use common::api::MemoryResult;

/// 从搜索结果构建图谱节点和边
fn build_graph_from_results(
    results: &[MemoryResult],
) -> (Vec<GraphNode>, Vec<GraphEdge>) {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut seen_node_ids = HashSet::new();

    for item in results {
        match item.memory_type.as_str() {
            "knowledge_node" | "short_term" | "trace" => {
                if seen_node_ids.insert(item.id.clone()) {
                    nodes.push(GraphNode {
                        id: item.id.clone(),
                        label: item.content.chars().take(20).collect::<String>(),
                        node_type: item.memory_type.clone(),
                        x: 0.0,
                        y: 0.0,
                    });
                }
            }
            "relation" => {
                if let (Some(src), Some(tgt)) = (&item.source_node_id, &item.target_node_id) {
                    // 确保源和目标节点都存在于节点列表中
                    if seen_node_ids.insert(src.clone()) {
                        nodes.push(GraphNode {
                            id: src.clone(),
                            label: src.chars().take(8).collect::<String>(),
                            node_type: "knowledge_node".to_string(),
                            x: 0.0,
                            y: 0.0,
                        });
                    }
                    if seen_node_ids.insert(tgt.clone()) {
                        nodes.push(GraphNode {
                            id: tgt.clone(),
                            label: tgt.chars().take(8).collect::<String>(),
                            node_type: "knowledge_node".to_string(),
                            x: 0.0,
                            y: 0.0,
                        });
                    }
                    let label = item.relation_type.as_deref().unwrap_or("").to_string();
                    edges.push(GraphEdge {
                        source: src.clone(),
                        target: tgt.clone(),
                        label,
                    });
                }
            }
            _ => {}
        }
    }

    (nodes, edges)
}

fn type_label(t: &str) -> &'static str {
    match t {
        "knowledge_node" => "知识节点",
        "short_term" => "短期记忆",
        "trace" => "调用记录",
        "relation" => "关系",
        _ => "未知",
    }
}

fn type_badge_class(t: &str) -> &'static str {
    match t {
        "knowledge_node" => "badge badge-primary",
        "short_term" => "badge badge-success",
        "trace" => "badge badge-warning",
        "relation" => "badge badge-accent",
        _ => "badge badge-neutral",
    }
}

#[component]
pub fn HrKnowledgeGraph() -> Element {
    let mut keyword = use_signal(String::new);
    let mut nodes = use_signal(Vec::<GraphNode>::new);
    let mut edges = use_signal(Vec::<GraphEdge>::new);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut expanded_nodes = use_signal(HashSet::<String>::new);
    let mut selected_node_id = use_signal(|| None::<String>);
    let mut selected_node_data = use_signal(|| None::<MemoryResult>);
    let mut search_history = use_signal(Vec::<String>::new);
    let mut highlighted_node_ids = use_signal(Vec::<String>::new);
    // 缓存搜索结果的 detail map，用于侧边栏展示
    let mut detail_map = use_signal(|| std::collections::HashMap::<String, MemoryResult>::new());

    let mut handle_search = move |_| {
        let kw = keyword().clone();
        if kw.is_empty() {
            return;
        }
        loading.set(true);
        error.set(String::new());
        expanded_nodes.set(HashSet::new());
        selected_node_id.set(None);
        selected_node_data.set(None);

        let mut history = search_history.read().clone();
        if !history.contains(&kw) {
            history.insert(0, kw.clone());
            if history.len() > 10 {
                history.pop();
            }
            search_history.set(history);
        }

        spawn(async move {
            match search_memory_with_traversal(&kw, &[], 1).await {
                Ok(data) => {
                    let mut map = std::collections::HashMap::new();
                    let mut highlights = Vec::new();
                    for item in &data.results {
                        if item.memory_type != "relation" {
                            map.insert(item.id.clone(), item.clone());
                            highlights.push(item.id.clone());
                        }
                    }
                    detail_map.set(map);
                    highlighted_node_ids.set(highlights);

                    let (new_nodes, new_edges) = build_graph_from_results(&data.results);
                    if new_nodes.is_empty() {
                        error.set("未找到匹配的知识节点".to_string());
                        nodes.set(Vec::new());
                        edges.set(Vec::new());
                    } else {
                        let laid = calculate_layout(&new_nodes, None);
                        nodes.set(laid);
                        edges.set(new_edges);
                    }
                }
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    };

    let handle_node_click = move |node_id: String| {
        // 选中节点
        selected_node_id.set(Some(node_id.clone()));

        // 从缓存中获取详情
        if let Some(detail) = detail_map.read().get(&node_id) {
            selected_node_data.set(Some(detail.clone()));
        }

        // 如果已展开过，跳过
        if expanded_nodes.read().contains(&node_id) {
            return;
        }

        // 展开关联
        loading.set(true);
        let seed_ids = vec![node_id.clone()];
        spawn(async move {
            match search_memory_with_traversal("", &seed_ids, 1).await {
                Ok(data) => {
                    // 更新详情缓存
                    let mut map = detail_map.read().clone();
                    for item in &data.results {
                        if item.memory_type != "relation" {
                            map.insert(item.id.clone(), item.clone());
                        }
                    }
                    detail_map.set(map);

                    // 提取新节点（排除已存在的）
                    let existing_ids: HashSet<String> = nodes.read().iter().map(|n| n.id.clone()).collect();
                    let (mut new_nodes, new_edges) = build_graph_from_results(&data.results);
                    new_nodes.retain(|n| !existing_ids.contains(&n.id));

                    if !new_nodes.is_empty() {
                        let current_nodes = nodes.read().clone();
                        let current_edges = edges.read().clone();
                        let updated_nodes = expand_layout(&current_nodes, &new_nodes, &seed_ids[0]);
                        let mut updated_edges = current_edges;
                        // 去重添加新边
                        let existing_edge_keys: HashSet<(String, String)> = updated_edges.iter()
                            .map(|e| (e.source.clone(), e.target.clone()))
                            .collect();
                        for e in new_edges {
                            let key = (e.source.clone(), e.target.clone());
                            if !existing_edge_keys.contains(&key) {
                                updated_edges.push(e);
                            }
                        }
                        nodes.set(updated_nodes);
                        edges.set(updated_edges);
                    }

                    expanded_nodes.write().insert(seed_ids[0].clone());
                }
                Err(_) => {}
            }
            loading.set(false);
        });
    };

    let current_nodes = nodes.read().clone();
    let current_edges = edges.read().clone();
    let selected_id = selected_node_id.read().clone();
    let selected_detail = selected_node_data.read().clone();

    rsx! {
        AppLayout {
            div { class: "card",
                h2 { class: "card-title", "知识图谱" }
                div { class: "space-y-4",
                    div { class: "flex gap-2",
                        input {
                            class: "form-input flex-1",
                            value: "{keyword}",
                            oninput: move |e| keyword.set(e.value()),
                            placeholder: "搜索知识节点...",
                            onkeydown: move |evt| {
                                if evt.key() == Key::Enter {
                                    handle_search(());
                                }
                            }
                        }
                        Button {
                            onclick: move |_| handle_search(()),
                            "搜索"
                        }
                    }
                    if !search_history().is_empty() {
                        {
                            let history_list = search_history().clone();
                            rsx! {
                                div { class: "flex flex-wrap gap-2",
                                    span { class: "text-xs text-muted", "搜索历史:" }
                                    for kw in history_list.into_iter() {
                                        button {
                                            class: "btn btn-xs btn-ghost",
                                            onclick: move |_| {
                                                keyword.set(kw.clone());
                                                handle_search(());
                                            },
                                            "{kw}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "space-y-2",
                        div { class: "flex items-center gap-4 text-sm",
                            span { class: "text-muted", "点击节点展开关联知识" }
                            // 节点类型图例
                            div { class: "flex items-center gap-3",
                                span { class: "graph-legend-item",
                                    span { class: "graph-legend-dot", style: "background: #3b82f6;" }
                                    "知识节点"
                                }
                                span { class: "graph-legend-item",
                                    span { class: "graph-legend-dot", style: "background: #10b981;" }
                                    "短期记忆"
                                }
                                span { class: "graph-legend-item",
                                    span { class: "graph-legend-dot", style: "background: #f59e0b;" }
                                    "调用记录"
                                }
                            }
                        }
                        // 关系类型图例
                        div { class: "graph-legend-section",
                            h4 { class: "graph-legend-title", "关系类型" }
                            div { class: "flex flex-wrap gap-3",
                                span { class: "graph-legend-item",
                                    span { class: "graph-legend-line", style: "background: #ef4444;" }
                                    "属于"
                                }
                                span { class: "graph-legend-item",
                                    span { class: "graph-legend-line dashed", style: "background: #3b82f6;" }
                                    "引用"
                                }
                                span { class: "graph-legend-item",
                                    span { class: "graph-legend-line", style: "background: #10b981;" }
                                    "包含"
                                }
                                span { class: "graph-legend-item",
                                    span { class: "graph-legend-line", style: "background: #f59e0b;" }
                                    "关联"
                                }
                                span { class: "graph-legend-item",
                                    span { class: "graph-legend-line", style: "background: #8b5cf6;" }
                                    "派生"
                                }
                                span { class: "graph-legend-item",
                                    span { class: "graph-legend-line dashed", style: "background: #ec4899;" }
                                    "依赖"
                                }
                            }
                        }
                    }
                }
            }

            if loading() {
                Loading {}
            } else if !error().is_empty() && current_nodes.is_empty() {
                EmptyState { message: "{error()}" }
            } else if current_nodes.is_empty() {
                EmptyState { message: "开始搜索知识节点".to_string() }
            } else {
                div { class: "graph-container",
                    // 图谱区域
                    div { class: "graph-main",
                        div { class: "card",
                            h3 { class: "card-title", "图谱视图 ({current_nodes.len()} 节点, {current_edges.len()} 关系)" }
                            Graph {
                                nodes: current_nodes,
                                edges: current_edges,
                                selected_node_id: selected_id,
                                highlighted_node_ids: Some(highlighted_node_ids()),
                                on_node_click: handle_node_click,
                            }
                        }
                    }

                    // 详情侧边栏
                    if let Some(detail) = &selected_detail {
                        div { class: "graph-detail-panel",
                            div { class: "card",
                                div { class: "card-header",
                                    h3 { class: "card-title", "节点详情" }
                                    button {
                                        class: "btn btn-ghost btn-sm",
                                        onclick: move |_| {
                                            selected_node_id.set(None);
                                            selected_node_data.set(None);
                                        },
                                        "✕"
                                    }
                                }
                                div { class: "space-y-4",
                                    div { class: "detail-grid",
                                        div {
                                            label { class: "form-label", "类型" }
                                            span { class: "{type_badge_class(&detail.memory_type)}", "{type_label(&detail.memory_type)}" }
                                        }
                                        div {
                                            label { class: "form-label", "匹配分数" }
                                            if let Some(score) = detail.score {
                                                span { class: "text-mono", "{score:.4}" }
                                            } else {
                                                span { class: "text-muted", "N/A" }
                                            }
                                        }
                                    }

                                    div {
                                        label { class: "form-label", "内容" }
                                        div { class: "detail-content-box",
                                            p { "{detail.content}" }
                                        }
                                    }

                                    if let Some(summary) = &detail.summary {
                                        div {
                                            label { class: "form-label", "摘要" }
                                            div { class: "detail-content-box text-muted",
                                                p { "{summary}" }
                                            }
                                        }
                                    }

                                    if detail.memory_type == "relation" {
                                        div { class: "detail-grid",
                                            if let Some(source) = &detail.source_node_id {
                                                div {
                                                    label { class: "form-label", "源节点" }
                                                    span { class: "text-mono text-sm", "{source}" }
                                                }
                                            }
                                            if let Some(target) = &detail.target_node_id {
                                                div {
                                                    label { class: "form-label", "目标节点" }
                                                    span { class: "text-mono text-sm", "{target}" }
                                                }
                                            }
                                            if let Some(rel_type) = &detail.relation_type {
                                                div {
                                                    label { class: "form-label", "关系类型" }
                                                    span { class: "text-mono", "{rel_type}" }
                                                }
                                            }
                                        }
                                    }

                                    div { class: "border-t pt-4",
                                        label { class: "form-label", "ID" }
                                        span { class: "text-mono text-muted text-sm break-all", "{detail.id}" }
                                    }

                                    div { class: "flex gap-2",
                                        button {
                                            class: "btn btn-sm btn-outline",
                                            onclick: move |_| {
                                                selected_node_id.set(None);
                                                selected_node_data.set(None);
                                            },
                                            "关闭"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
