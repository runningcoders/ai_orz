use dioxus::prelude::{Key, *};
use std::collections::HashSet;

use crate::api::hr::{search_memory, search_memory_with_traversal};
use crate::components::button::Button;
use crate::components::graph::{calculate_layout, Graph, GraphEdge, GraphNode};
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use common::api::MemoryResult;

#[component]
pub fn HrKnowledgeGraph() -> Element {
    let mut keyword = use_signal(String::new);
    let mut nodes = use_signal(Vec::<GraphNode>::new);
    let mut edges = use_signal(Vec::<GraphEdge>::new);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut expanded_nodes = use_signal(HashSet::<String>::new);

    let mut handle_search = move |_| {
        loading.set(true);
        error.set(String::new());
        expanded_nodes.set(HashSet::new());
        let kw = keyword().clone();
        spawn(async move {
            match search_memory(&kw, None).await {
                Ok(data) => {
                    let new_nodes = data.results.iter().enumerate().map(|(i, item)| GraphNode {
                        id: item.id.clone(),
                        label: item.content.chars().take(20).collect::<String>(),
                        node_type: item.memory_type.clone(),
                        x: 400.0 + (i as f64 - data.results.len() as f64 / 2.0) * 80.0,
                        y: 300.0,
                    }).collect::<Vec<_>>();
                    nodes.set(calculate_layout(&new_nodes));
                    edges.set(Vec::new());
                    if data.results.is_empty() {
                        error.set("未找到匹配的知识节点".to_string());
                    }
                }
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    };



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
                                    spawn(async move {
                                        handle_search(());
                                    });
                                }
                            }
                        }
                        Button {
                            onclick: move |_| handle_search(()),
                            "搜索"
                        }
                    }
                    div { class: "text-sm text-muted",
                        "提示：搜索找到初始节点后，点击节点可展开关联知识"
                    }
                }
            }

            if loading() {
                Loading {}
            } else if !error().is_empty() {
                EmptyState { message: "{error()}" }
            } else if nodes().is_empty() {
                EmptyState { message: "开始搜索知识节点".to_string() }
            } else {
                div { class: "card",
                    h3 { class: "card-title", "图谱视图 ({nodes().len()} 节点, {edges().len()} 关系)" }
                    div { class: "flex justify-center",
                        Graph {
                            nodes: nodes().clone(),
                            edges: edges().clone(),
                            on_node_click: None
                        }
                    }
                }
            }
        }
    }
}