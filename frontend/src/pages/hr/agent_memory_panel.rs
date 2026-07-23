use dioxus::prelude::{Key, *};

use crate::api::hr::{query_memory, search_memory};
use crate::api::ApiError;
use crate::components::button::Button;
use crate::components::state::{EmptyState, Loading};
use crate::store::toast::use_toast;
use common::api::MemoryResult;

#[derive(Debug, Clone, Copy, PartialEq)]
enum MemoryTab {
    ShortTerm,
    KnowledgeNode,
    Relation,
}

impl MemoryTab {
    #[allow(dead_code)]
    fn label(self) -> &'static str {
        match self {
            MemoryTab::ShortTerm => "短期记忆",
            MemoryTab::KnowledgeNode => "知识节点",
            MemoryTab::Relation => "关系",
        }
    }

    fn memory_type(self) -> &'static str {
        match self {
            MemoryTab::ShortTerm => "short_term",
            MemoryTab::KnowledgeNode => "knowledge_node",
            MemoryTab::Relation => "relation",
        }
    }

    fn badge_class(self) -> &'static str {
        match self {
            MemoryTab::ShortTerm => "badge badge-info",
            MemoryTab::KnowledgeNode => "badge badge-success",
            MemoryTab::Relation => "badge badge-accent",
        }
    }
}

fn fetch_memories(
    agent_id: Option<String>,
    tab: MemoryTab,
    kw: String,
    mut results: Signal<Vec<MemoryResult>>,
    mut loading: Signal<bool>,
    toast: crate::store::toast::ToastState,
    mut fetch_request_id: Signal<u32>,
) {
    // 修复 HIGH #13：自增 request_id，结果到达时校验是否为最新请求
    let my_id = fetch_request_id() + 1;
    fetch_request_id.set(my_id);
    loading.set(true);
    spawn(async move {
        let mem_type = Some(tab.memory_type());
        let fetch_result: Result<Vec<MemoryResult>, ApiError> = if kw.trim().is_empty() {
            query_memory(agent_id.as_deref(), mem_type)
                .await
                .map(|r| r.results)
        } else {
            search_memory(&kw, mem_type)
                .await
                .map(|r| r.results)
        };
        // 丢弃过期请求的结果
        if fetch_request_id() != my_id {
            return;
        }
        match fetch_result {
            Ok(data) => {
                results.set(data);
            }
            Err(e) => toast.error(&e),
        }
        loading.set(false);
    });
}

#[component]
pub fn AgentMemoryPanel(agent_id: Option<String>) -> Element {
    let mut active_tab = use_signal(|| MemoryTab::ShortTerm);
    let mut keyword = use_signal(String::new);
    let results = use_signal(Vec::<MemoryResult>::new);
    let loading = use_signal(|| false);
    let toast = use_toast();
    // 修复 HIGH #13：快速切换 tab 时旧请求慢返回会覆盖新 tab 的数据，
    // 引入 fetch_request_id 机制丢弃过期请求结果
    let fetch_request_id = use_signal(|| 0u32);

    // 修复 M10：use_effect 监听 active_tab 自动 fetch，tab 按钮的 onclick 不再显式调用
    // fetch_memories（之前会触发双请求：onclick 一次 + use_effect 一次）
    use_effect({
        let agent_id = agent_id.clone();
        move || {
            let tab = active_tab();
            fetch_memories(agent_id.clone(), tab, String::new(), results, loading, toast, fetch_request_id);
        }
    });

    let results_list = results.read().clone();

    let agent_id_4 = agent_id.clone();
    let agent_id_5 = agent_id.clone();

    rsx! {
        div { class: "card bg-base-100 shadow-md",
            div { class: "card-body",
                div { class: "flex flex-col sm:flex-row justify-between items-start sm:items-center gap-4 mb-4",
                    h2 { class: "card-title", "记忆面板" }
                    div { class: "tabs tabs-boxed",
                        button {
                            class: if active_tab() == MemoryTab::ShortTerm { "tab tab-active" } else { "tab" },
                            onclick: move |_| {
                                active_tab.set(MemoryTab::ShortTerm);
                                keyword.set(String::new());
                            },
                            "短期记忆"
                        }
                        button {
                            class: if active_tab() == MemoryTab::KnowledgeNode { "tab tab-active" } else { "tab" },
                            onclick: move |_| {
                                active_tab.set(MemoryTab::KnowledgeNode);
                                keyword.set(String::new());
                            },
                            "知识节点"
                        }
                        button {
                            class: if active_tab() == MemoryTab::Relation { "tab tab-active" } else { "tab" },
                            onclick: move |_| {
                                active_tab.set(MemoryTab::Relation);
                                keyword.set(String::new());
                            },
                            "关系"
                        }
                    }
                }

                div { class: "flex gap-2 mb-4",
                    input {
                        class: "input input-bordered flex-1",
                        value: "{keyword}",
                        oninput: move |e| keyword.set(e.value()),
                        placeholder: "输入关键词搜索记忆...",
                        onkeydown: move |evt| {
                            if evt.key() == Key::Enter {
                                let kw = keyword().clone();
                                let tab = active_tab();
                                let aid = agent_id_4.clone();
                                fetch_memories(aid, tab, kw, results, loading, toast, fetch_request_id);
                            }
                        }
                    }
                    Button {
                        onclick: move |_| {
                            let kw = keyword().clone();
                            let tab = active_tab();
                            let aid = agent_id_5.clone();
                            fetch_memories(aid, tab, kw, results, loading, toast, fetch_request_id);
                        },
                        "搜索"
                    }
                }

                if loading() {
                    Loading {}
                } else if results_list.is_empty() {
                    EmptyState { message: "暂无记忆数据".to_string() }
                } else {
                    div { class: "space-y-3",
                        for item in results_list.iter() {
                            {
                                let content_preview = item.content.chars().take(120).collect::<String>();
                                let summary_text = item.summary.clone().unwrap_or_default();
                                let score_text = item.score
                                    .map(|s| format!("{:.4}", s))
                                    .unwrap_or_default();
                                let mt = item.memory_type.clone();
                                let src_node = item.source_node_id.clone().unwrap_or_default();
                                let tgt_node = item.target_node_id.clone().unwrap_or_default();
                                let rel_type = item.relation_type.clone().unwrap_or_default();
                                let has_summary = item.summary.is_some();
                                let has_score = item.score.is_some();
                                let is_relation = item.memory_type == "relation";
                                let has_src = item.source_node_id.is_some();
                                let has_tgt = item.target_node_id.is_some();
                                let has_rel = item.relation_type.is_some();

                                let active = active_tab();
                                let badge_class = active.badge_class();

                                rsx! {
                                    div { class: "p-4 border border-base-300 rounded-lg hover:bg-base-200 transition-colors",
                                        div { class: "flex justify-between items-start mb-2",
                                            div { class: "flex items-center gap-2",
                                                span { class: "{badge_class} text-xs", "{mt}" }
                                                if has_score {
                                                    span { class: "text-xs text-base-content/70", "相似度: {score_text}" }
                                                }
                                            }
                                        }
                                        div { class: "text-sm mb-2", "{content_preview}..." }
                                        if has_summary {
                                            div { class: "text-xs text-base-content/70 mb-2",
                                                "摘要: {summary_text}"
                                            }
                                        }
                                        if is_relation {
                                            div { class: "text-xs text-base-content/70 flex flex-wrap gap-2",
                                                if has_src {
                                                    span { "源: {src_node}" }
                                                }
                                                if has_rel {
                                                    span { "→ {rel_type} →" }
                                                }
                                                if has_tgt {
                                                    span { "目标: {tgt_node}" }
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
}
