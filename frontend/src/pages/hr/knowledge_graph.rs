use dioxus::prelude::*;
use std::collections::HashSet;

use crate::api::hr::search_memory_with_traversal;
use crate::components::button::Button;
use crate::components::graph::{calculate_layout, expand_layout, Graph, GraphEdge, GraphNode};
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
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
                    // 修复 L19：之前用 src.chars().take(8) 作 label（UUID 前 8 字符无意义），
                    // 改用 relation_type 作 label 更有意义，无 relation_type 时回退到 "节点"
                    let inferred_label = item.relation_type.clone().unwrap_or_else(|| "节点".to_string());
                    if seen_node_ids.insert(src.clone()) {
                        nodes.push(GraphNode {
                            id: src.clone(),
                            label: inferred_label.clone(),
                            node_type: "knowledge_node".to_string(),
                            x: 0.0,
                            y: 0.0,
                        });
                    }
                    if seen_node_ids.insert(tgt.clone()) {
                        nodes.push(GraphNode {
                            id: tgt.clone(),
                            label: inferred_label.clone(),
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
    let toast = use_toast();
    let mut expanded_nodes = use_signal(HashSet::<String>::new);
    let mut selected_node_id = use_signal(|| None::<String>);
    let mut selected_node_data = use_signal(|| None::<MemoryResult>);
    let mut search_history = use_signal(Vec::<String>::new);
    let mut highlighted_node_ids = use_signal(Vec::<String>::new);
    let mut detail_map = use_signal(|| std::collections::HashMap::<String, MemoryResult>::new());
    // 修复 M11：节点点击请求 ID，用于取消过期的并发请求结果（用户快速点击多个节点时）
    let mut click_request_id = use_signal(|| 0u32);

    let mut handle_search = move |_| {
        let kw = keyword().clone();
        if kw.is_empty() {
            return;
        }
        loading.set(true);
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
                        toast.error("未找到匹配的知识节点");
                        nodes.set(Vec::new());
                        edges.set(Vec::new());
                    } else {
                        let laid = calculate_layout(&new_nodes, None);
                        nodes.set(laid);
                        edges.set(new_edges);
                    }
                }
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    };

    let handle_node_click = move |node_id: String| {
        selected_node_id.set(Some(node_id.clone()));

        if let Some(detail) = detail_map.read().get(&node_id) {
            selected_node_data.set(Some(detail.clone()));
        }

        if expanded_nodes.read().contains(&node_id) {
            return;
        }

        loading.set(true);
        let seed_ids = vec![node_id.clone()];
        // 修复 M11：自增 request_id，捕获当前 ID，结果到达时若不匹配则丢弃
        let my_request_id = click_request_id() + 1;
        click_request_id.set(my_request_id);
        spawn(async move {
            match search_memory_with_traversal("", &seed_ids, 1).await {
                Ok(data) => {
                    // 修复 M11：检查 request_id 是否仍然是最新的，过期则丢弃结果
                    if click_request_id() != my_request_id {
                        loading.set(false);
                        return;
                    }
                    let mut map = detail_map.read().clone();
                    for item in &data.results {
                        if item.memory_type != "relation" {
                            map.insert(item.id.clone(), item.clone());
                        }
                    }
                    // 修复 L7：限制 detail_map 大小，超过 200 时清理避免无限增长
                    if map.len() > 200 {
                        let valid_ids: HashSet<String> = nodes.read().iter().map(|n| n.id.clone()).collect();
                        map.retain(|id, _| valid_ids.contains(id));
                    }
                    detail_map.set(map);

                    let existing_ids: HashSet<String> = nodes.read().iter().map(|n| n.id.clone()).collect();
                    let (mut new_nodes, new_edges) = build_graph_from_results(&data.results);
                    new_nodes.retain(|n| !existing_ids.contains(&n.id));

                    if !new_nodes.is_empty() {
                        let current_nodes = nodes.read().clone();
                        let current_edges = edges.read().clone();
                        let updated_nodes = expand_layout(&current_nodes, &new_nodes, &seed_ids[0]);
                        let mut updated_edges = current_edges;
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
                // 修复 L5：之前 Err(_) => {} 静默吞错，改为显示 toast
                Err(e) => {
                    toast.error(&format!("加载节点关联失败: {}", e));
                }
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
            div { class: "card bg-base-100 shadow-md",
                div { class: "card-body",
                    h2 { class: "card-title mb-4", "知识图谱" }
                    div { class: "space-y-4",
                        div { class: "flex flex-col sm:flex-row gap-2",
                            input {
                                class: "input input-bordered flex-1",
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
                                    div { class: "flex flex-wrap gap-2 items-center",
                                        span { class: "text-xs text-base-content/70", "搜索历史:" }
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
                            div { class: "flex flex-wrap items-center gap-4 text-sm",
                                span { class: "text-base-content/70", "点击节点展开关联知识" }
                                div { class: "flex items-center gap-3",
                                    span { class: "flex items-center gap-1",
                                        span { class: "w-3 h-3 rounded-full", style: "background: #3b82f6;" }
                                        "知识节点"
                                    }
                                    span { class: "flex items-center gap-1",
                                        span { class: "w-3 h-3 rounded-full", style: "background: #10b981;" }
                                        "短期记忆"
                                    }
                                    span { class: "flex items-center gap-1",
                                        span { class: "w-3 h-3 rounded-full", style: "background: #f59e0b;" }
                                        "调用记录"
                                    }
                                }
                            }
                            div {
                                h4 { class: "text-sm font-semibold mb-2", "关系类型" }
                                div { class: "flex flex-wrap gap-3",
                                    span { class: "flex items-center gap-1",
                                        span { class: "w-6 h-0.5", style: "background: #ef4444;" }
                                        "属于"
                                    }
                                    span { class: "flex items-center gap-1",
                                        span { class: "w-6 h-0.5 border-dashed border-t-2", style: "border-color: #3b82f6;" }
                                        "引用"
                                    }
                                    span { class: "flex items-center gap-1",
                                        span { class: "w-6 h-0.5", style: "background: #10b981;" }
                                        "包含"
                                    }
                                    span { class: "flex items-center gap-1",
                                        span { class: "w-6 h-0.5", style: "background: #f59e0b;" }
                                        "关联"
                                    }
                                    span { class: "flex items-center gap-1",
                                        span { class: "w-6 h-0.5", style: "background: #8b5cf6;" }
                                        "派生"
                                    }
                                    span { class: "flex items-center gap-1",
                                        span { class: "w-6 h-0.5 border-dashed border-t-2", style: "border-color: #ec4899;" }
                                        "依赖"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if loading() {
                Loading {}
            } else if current_nodes.is_empty() {
                EmptyState { message: "开始搜索知识节点".to_string() }
            } else {
                div { class: "flex flex-col lg:flex-row gap-4 mt-4",
                    div { class: "flex-1 min-h-96",
                        div { class: "card bg-base-100 shadow-md h-full",
                            div { class: "card-body",
                                h3 { class: "card-title mb-4", "图谱视图 ({current_nodes.len()} 节点, {current_edges.len()} 关系)" }
                                Graph {
                                    nodes: current_nodes,
                                    edges: current_edges,
                                    selected_node_id: selected_id,
                                    highlighted_node_ids: Some(highlighted_node_ids()),
                                    on_node_click: handle_node_click,
                                }
                            }
                        }
                    }

                    if let Some(detail) = &selected_detail {
                        div { class: "w-full lg:w-96",
                            div { class: "card bg-base-100 shadow-md",
                                div { class: "card-body",
                                    div { class: "flex justify-between items-start mb-4",
                                        h3 { class: "card-title", "节点详情" }
                                        button {
                                            class: "btn btn-ghost btn-sm btn-circle",
                                            onclick: move |_| {
                                                selected_node_id.set(None);
                                                selected_node_data.set(None);
                                            },
                                            "✕"
                                        }
                                    }
                                    div { class: "space-y-4",
                                        div { class: "grid grid-cols-2 gap-4",
                                            div {
                                                label { class: "label",
                                                    span { class: "label-text font-medium", "类型" }
                                                }
                                                span { class: "{type_badge_class(&detail.memory_type)}", "{type_label(&detail.memory_type)}" }
                                            }
                                            div {
                                                label { class: "label",
                                                    span { class: "label-text font-medium", "匹配分数" }
                                                }
                                                if let Some(score) = detail.score {
                                                    span { class: "font-mono text-sm", "{score:.4}" }
                                                } else {
                                                    span { class: "text-base-content/70", "N/A" }
                                                }
                                            }
                                        }

                                        div {
                                            label { class: "label",
                                                span { class: "label-text font-medium", "内容" }
                                            }
                                            div { class: "p-3 bg-base-200 rounded-lg",
                                                p { class: "text-sm", "{detail.content}" }
                                            }
                                        }

                                        if let Some(summary) = &detail.summary {
                                            div {
                                                label { class: "label",
                                                    span { class: "label-text font-medium", "摘要" }
                                                }
                                                div { class: "p-3 bg-base-200 rounded-lg text-base-content/70",
                                                    p { class: "text-sm", "{summary}" }
                                                }
                                            }
                                        }

                                        if detail.memory_type == "relation" {
                                            div { class: "grid grid-cols-1 gap-2",
                                                if let Some(source) = &detail.source_node_id {
                                                    div {
                                                        label { class: "label",
                                                            span { class: "label-text font-medium", "源节点" }
                                                        }
                                                        span { class: "font-mono text-sm", "{source}" }
                                                    }
                                                }
                                                if let Some(target) = &detail.target_node_id {
                                                    div {
                                                        label { class: "label",
                                                            span { class: "label-text font-medium", "目标节点" }
                                                        }
                                                        span { class: "font-mono text-sm", "{target}" }
                                                    }
                                                }
                                                if let Some(rel_type) = &detail.relation_type {
                                                    div {
                                                        label { class: "label",
                                                            span { class: "label-text font-medium", "关系类型" }
                                                        }
                                                        span { class: "font-mono", "{rel_type}" }
                                                    }
                                                }
                                            }
                                        }

                                        div { class: "border-t border-base-300 pt-4",
                                            label { class: "label",
                                                span { class: "label-text font-medium", "ID" }
                                            }
                                            span { class: "font-mono text-xs text-base-content/70 break-all", "{detail.id}" }
                                        }

                                        div { class: "flex gap-2",
                                            button {
                                                class: "btn btn-outline btn-sm",
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
}
