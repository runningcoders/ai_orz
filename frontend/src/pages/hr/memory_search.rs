use dioxus::prelude::{Key, *};

use crate::api::hr::search_memory;
use crate::components::button::Button;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use common::api::MemoryResult;

#[component]
pub fn HrMemorySearch() -> Element {
    let mut keyword = use_signal(String::new);
    let mut memory_type = use_signal(String::new);
    let mut results = use_signal(Vec::<MemoryResult>::new);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(String::new);

    let mut handle_search = move |_| {
        loading.set(true);
        error.set(String::new());
        let kw = keyword().clone();
        let mt = memory_type().clone();
        spawn(async move {
            let mem_type = if mt.is_empty() { None } else { Some(mt.as_str()) };
            match search_memory(&kw, mem_type).await {
                Ok(data) => {
                    let mem_results = data.results;
                    results.set(mem_results.clone());
                    if mem_results.is_empty() {
                        error.set("未找到匹配的记忆".to_string());
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
                h2 { class: "card-title", "记忆搜索" }
                div { class: "space-y-4",
                    div { class: "flex gap-2",
                        input {
                            class: "form-input flex-1",
                            value: "{keyword}",
                            oninput: move |e| keyword.set(e.value()),
                            placeholder: "输入关键词搜索记忆...",
                            onkeydown: move |evt| {
                                if evt.key() == Key::Enter {
                                    spawn(async move {
                                        handle_search(());
                                    });
                                }
                            }
                        }
                        select {
                            class: "form-select",
                            value: "{memory_type}",
                            onchange: move |e| memory_type.set(e.value()),
                            option { value: "", "全部类型" }
                            option { value: "short_term", "短期记忆" }
                            option { value: "knowledge_node", "知识节点" }
                            option { value: "trace", "调用记录" }
                            option { value: "relation", "关系" }
                        }
                        Button {
                            onclick: move |_| handle_search(()),
                            "搜索"
                        }
                    }
                }
            }

            if loading() {
                Loading {}
            } else if !error().is_empty() {
                EmptyState { message: "{error()}" }
            } else if results().is_empty() {
                EmptyState { message: "开始搜索".to_string() }
            } else {
                div { class: "card",
                    h3 { class: "card-title", "搜索结果 ({results().len()})" }
                    div { class: "space-y-2",
                        for item in &results() {
                            div { class: "p-3 border rounded hover:bg-muted",
                                div { class: "flex justify-between items-start",
                                    div {
                                        span { class: "font-medium", "{item.content.chars().take(100).collect::<String>()}" }
                                        if item.summary.is_some() {
                                            div { class: "text-sm text-muted mt-1",
                                                "{item.summary.clone().unwrap()}"
                                            }
                                        }
                                    }
                                    div {
                                        span { class: "badge badge-accent text-xs", "{item.memory_type}" }
                                        if item.score.is_some() {
                                            span { class: "text-xs text-muted ml-2", "score={item.score.unwrap():.4}" }
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