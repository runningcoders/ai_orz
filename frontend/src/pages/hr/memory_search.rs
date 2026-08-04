use dioxus::prelude::{Key, *};

use crate::api::hr::search_memory;
use crate::components::button::Button;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{MemoryResult, SearchMemoryParams};

#[component]
pub fn HrMemorySearch() -> Element {
    let mut keyword = use_signal(String::new);
    let mut memory_type = use_signal(String::new);
    let mut task_id = use_signal(String::new);
    let mut results = use_signal(Vec::<MemoryResult>::new);
    let mut loading = use_signal(|| false);
    let toast = use_toast();

    let mut handle_search = move |_| {
        loading.set(true);
        let kw = keyword().clone();
        let mt = memory_type().clone();
        let tid = task_id().clone();
        spawn(async move {
            let mem_type = if mt.is_empty() {
                None
            } else {
                Some(mt.as_str())
            };
            let task_filter = if tid.trim().is_empty() {
                None
            } else {
                Some(tid.trim().to_string())
            };
            match search_memory(SearchMemoryParams {
                query: kw,
                max_results: Some(20),
                memory_type: mem_type.map(|s| s.to_string()),
                traversal_depth: None,
                traversal_breadth: None,
                traversal_strategy: None,
                seed_node_ids: None,
                tags: None,
                task_id: task_filter,
                agent_id: None,
            })
            .await
            {
                Ok(data) => {
                    let mem_results = data.results;
                    results.set(mem_results.clone());
                    if mem_results.is_empty() {
                        toast.error("未找到匹配的记忆");
                    }
                }
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    };

    rsx! {
        AppLayout {
            div { class: "card bg-base-100 shadow-md",
                div { class: "card-body",
                    h2 { class: "card-title mb-4", "记忆搜索" }
                    div { class: "filter-row",
                        div { class: "filter-item flex-[2]",
                            label { class: "form-label", "关键词" }
                            input {
                                class: "input input-bordered w-full",
                                value: "{keyword}",
                                oninput: move |e| keyword.set(e.value()),
                                placeholder: "输入关键词搜索记忆...",
                                onkeydown: move |evt| {
                                    if evt.key() == Key::Enter {
                                        handle_search(());
                                    }
                                }
                            }
                        }
                        div { class: "filter-item",
                            label { class: "form-label", "记忆类型" }
                            select {
                                class: "select select-bordered w-full",
                                value: "{memory_type}",
                                onchange: move |e| memory_type.set(e.value()),
                                option { value: "", "全部类型" }
                                option { value: "short_term", "短期记忆" }
                                option { value: "knowledge_node", "知识节点" }
                                option { value: "trace", "调用记录" }
                                option { value: "relation", "关系" }
                            }
                        }
                        div { class: "filter-item",
                            label { class: "form-label", "任务 ID 过滤" }
                            input {
                                class: "input input-bordered w-full",
                                value: "{task_id}",
                                oninput: move |e| task_id.set(e.value()),
                                placeholder: "可选，聚焦特定任务",
                                onkeydown: move |evt| {
                                    if evt.key() == Key::Enter {
                                        handle_search(());
                                    }
                                }
                            }
                        }
                        div { class: "filter-item justify-end",
                            label { class: "form-label opacity-0", "操作" }
                            Button {
                                onclick: move |_| handle_search(()),
                                "搜索"
                            }
                        }
                    }
                }
            }

            if loading() {
                Loading {}
            } else if results().is_empty() {
                EmptyState { message: "开始搜索".to_string() }
            } else {
                div { class: "card bg-base-100 shadow-md mt-4",
                    div { class: "card-body",
                        h3 { class: "card-title mb-4", "搜索结果 ({results().len()})" }
                        div { class: "space-y-2",
                            for item in &results() {
                                div { class: "p-3 border border-base-300 rounded hover:bg-base-200",
                                    div { class: "flex flex-col sm:flex-row justify-between items-start gap-2",
                                        div { class: "flex-1",
                                            span { class: "font-medium", "{item.content.chars().take(100).collect::<String>()}" }
                                            // 修复 L_NEW：使用 if let 模式替代 unwrap，避免空值 panic
                                            if let Some(summary) = &item.summary {
                                                div { class: "text-sm text-base-content/70 mt-1",
                                                    "{summary}"
                                                }
                                            }
                                            if let Some(tags) = &item.tags {
                                                if !tags.is_empty() {
                                                    div { class: "flex flex-wrap gap-1 mt-2",
                                                        for tag in tags.iter() {
                                                            span { class: "badge badge-neutral badge-xs", "{tag}" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        div { class: "flex items-center gap-2 shrink-0",
                                            span { class: "badge badge-accent text-xs", "{item.memory_type}" }
                                            if let Some(score) = item.score {
                                                span { class: "text-xs text-base-content/70", "score={score:.4}" }
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
