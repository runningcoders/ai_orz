use dioxus::prelude::{Key, *};

use crate::api::ApiError;
use crate::api::hr::{query_memory, search_memory};
use crate::components::button::Button;
use crate::components::markdown::MarkdownRenderer;
use crate::components::state::{EmptyState, Loading};
use crate::store::toast::use_toast;
use common::api::{MemoryResult, QueryMemoryParams, SearchMemoryParams};

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

#[allow(clippy::too_many_arguments)]
fn fetch_memories(
    agent_id: Option<String>,
    tab: MemoryTab,
    kw: String,
    task_filter: Option<String>,
    mut results: Signal<Vec<MemoryResult>>,
    mut loading: Signal<bool>,
    toast: crate::store::toast::ToastState,
    mut fetch_request_id: Signal<u32>,
) {
    // 修复 HIGH #13：自增 request_id，结果到达时校验是否为最新请求。
    // 用 peek() 读取，避免 use_effect 订阅 fetch_request_id：
    // 否则每次 set 都会触发 use_effect 重跑，形成无限循环卡死页面。
    let my_id = *fetch_request_id.peek() + 1;
    fetch_request_id.set(my_id);
    loading.set(true);
    spawn(async move {
        let mem_type = Some(tab.memory_type());
        let fetch_result: Result<Vec<MemoryResult>, ApiError> = if kw.trim().is_empty() {
            query_memory(QueryMemoryParams {
                agent_id,
                memory_type: mem_type.map(|s| s.to_string()),
                limit: Some(20),
                tags: None,
                task_id: task_filter.clone(),
                status: None,
            })
            .await
            .map(|r| r.results)
        } else {
            search_memory(SearchMemoryParams {
                query: kw,
                max_results: Some(20),
                memory_type: mem_type.map(|s| s.to_string()),
                traversal_depth: None,
                traversal_breadth: None,
                traversal_strategy: None,
                seed_node_ids: None,
                tags: None,
                task_id: task_filter.clone(),
                agent_id: agent_id.clone(),
            })
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
    let mut task_id = use_signal(String::new);
    let results = use_signal(Vec::<MemoryResult>::new);
    let loading = use_signal(|| false);
    let toast = use_toast();
    // 修复 HIGH #13：快速切换 tab 时旧请求慢返回会覆盖新 tab 的数据，
    // 引入 fetch_request_id 机制丢弃过期请求结果
    let fetch_request_id = use_signal(|| 0u32);
    // 展开态记忆 ID（展开后用 Markdown 渲染完整内容）
    let mut expanded_id = use_signal(|| None::<String>);

    // 修复 M10：use_effect 监听 active_tab 自动 fetch，tab 按钮的 onclick 不再显式调用
    // fetch_memories（之前会触发双请求：onclick 一次 + use_effect 一次）
    use_effect({
        let agent_id = agent_id.clone();
        move || {
            let tab = active_tab();
            fetch_memories(
                agent_id.clone(),
                tab,
                String::new(),
                None,
                results,
                loading,
                toast,
                fetch_request_id,
            );
        }
    });

    let results_list = results.read().clone();

    let agent_id_4 = agent_id.clone();
    let agent_id_5 = agent_id.clone();
    let agent_id_6 = agent_id.clone();

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

                div { class: "flex flex-col sm:flex-row gap-2 mb-4",
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
                                let tid = task_id().clone();
                                let task_filter = if tid.trim().is_empty() { None } else { Some(tid.trim().to_string()) };
                                fetch_memories(aid, tab, kw, task_filter, results, loading, toast, fetch_request_id);
                            }
                        }
                    }
                    input {
                        class: "input input-bordered sm:w-64",
                        value: "{task_id}",
                        oninput: move |e| task_id.set(e.value()),
                        placeholder: "任务 ID 过滤（可选）",
                        onkeydown: move |evt| {
                            if evt.key() == Key::Enter {
                                let kw = keyword().clone();
                                let tab = active_tab();
                                let aid = agent_id_6.clone();
                                let tid = task_id().clone();
                                let task_filter = if tid.trim().is_empty() { None } else { Some(tid.trim().to_string()) };
                                fetch_memories(aid, tab, kw, task_filter, results, loading, toast, fetch_request_id);
                            }
                        }
                    }
                    Button {
                        onclick: move |_| {
                            let kw = keyword().clone();
                            let tab = active_tab();
                            let aid = agent_id_5.clone();
                            let tid = task_id().clone();
                            let task_filter = if tid.trim().is_empty() { None } else { Some(tid.trim().to_string()) };
                            fetch_memories(aid, tab, kw, task_filter, results, loading, toast, fetch_request_id);
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
                                let item_id = item.id.clone();
                                let is_expanded = expanded_id() == Some(item.id.clone());
                                let toggle_id = item.id.clone();
                                let full_content = item.content.clone();
                                let full_summary = item.summary.clone();
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
                                let tags = item.tags.clone();
                                let has_tags = tags.as_ref().is_some_and(|t| !t.is_empty());

                                let active = active_tab();
                                let badge_class = active.badge_class();

                                rsx! {
                                    div {
                                        key: "{item_id}",
                                        class: "p-4 border border-base-300 rounded-lg hover:bg-base-200 transition-colors cursor-pointer",
                                        onclick: move |_| {
                                            let next = if expanded_id() == Some(toggle_id.clone()) {
                                                None
                                            } else {
                                                Some(toggle_id.clone())
                                            };
                                            expanded_id.set(next);
                                        },
                                        div { class: "flex justify-between items-start mb-2",
                                            div { class: "flex items-center gap-2",
                                                span { class: "{badge_class} text-xs", "{mt}" }
                                                if has_score {
                                                    span { class: "text-xs text-base-content/70", "相似度: {score_text}" }
                                                }
                                            }
                                        }
                                        if is_expanded {
                                            // 展开态：Markdown 渲染完整内容与摘要
                                            div { class: "text-sm mb-2",
                                                MarkdownRenderer { content: full_content.clone(), compact: true }
                                            }
                                            if has_summary {
                                                if let Some(summary) = full_summary.clone() {
                                                    div { class: "text-xs text-base-content/70 mb-2",
                                                        MarkdownRenderer { content: summary, compact: true }
                                                    }
                                                }
                                            }
                                        } else {
                                            div { class: "text-sm mb-2", "{content_preview}..." }
                                            if has_summary {
                                                div { class: "text-xs text-base-content/70 mb-2",
                                                    "摘要: {summary_text}"
                                                }
                                            }
                                        }
                                        if has_tags {
                                            if let Some(tags_list) = &tags {
                                                div { class: "flex flex-wrap gap-1 mb-2",
                                                    for tag in tags_list.iter() {
                                                        span { class: "badge badge-neutral badge-xs", "{tag}" }
                                                    }
                                                }
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
