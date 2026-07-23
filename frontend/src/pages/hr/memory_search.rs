use dioxus::prelude::{Key, *};

use crate::api::hr::search_memory;
use crate::components::button::Button;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::MemoryResult;

#[component]
pub fn HrMemorySearch() -> Element {
    let mut keyword = use_signal(String::new);
    let mut memory_type = use_signal(String::new);
    let mut results = use_signal(Vec::<MemoryResult>::new);
    let mut loading = use_signal(|| false);
    let toast = use_toast();

    let mut handle_search = move |_| {
        loading.set(true);
        let kw = keyword().clone();
        let mt = memory_type().clone();
        spawn(async move {
            let mem_type = if mt.is_empty() { None } else { Some(mt.as_str()) };
            match search_memory(&kw, mem_type).await {
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
                    div { class: "flex flex-col sm:flex-row gap-2",
                        input {
                            class: "input input-bordered flex-1",
                            value: "{keyword}",
                            oninput: move |e| keyword.set(e.value()),
                            placeholder: "输入关键词搜索记忆...",
                            onkeydown: move |evt| {
                                if evt.key() == Key::Enter {
                                    // 修复 L8：handle_search 内部已 spawn，外层 spawn 多余
                                    handle_search(());
                                }
                            }
                        }
                        select {
                            class: "select select-bordered w-full sm:w-auto",
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
