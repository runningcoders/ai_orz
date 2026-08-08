//! 文档中心 - 浏览仓库 docs/ 下的 Markdown 文档
//!
//! 运行时从静态资源 `/docs/index.json` 拉取目录清单（由 build.rs 复制
//! docs/ 核心文档 + wiki 到 public/docs/ 时生成），点击条目按需 fetch 对应
//! Markdown 文件，用 pulldown-cmark 渲染为 HTML 展示。
//!
//! Wiki 分区保留目录层级：扁平化为 (分组行/文档行) 序列后单层渲染，
//! 分组行点击切换展开/折叠，支持关键词过滤（过滤时自动展开命中分支）。

use std::collections::HashSet;

use dioxus::prelude::*;
use serde::Deserialize;

use crate::api::fetch_static_text;
use crate::components::markdown::render_markdown;
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::{ToastState, use_toast};

/// 文档条目（叶子节点；wiki 树中也可能出现在 children 里）
#[derive(Debug, Clone, Deserialize, PartialEq)]
struct DocEntry {
    title: String,
    path: String,
}

/// 目录分组节点（wiki 树中的目录，children 可继续嵌套分组或文档）
#[derive(Debug, Clone, Deserialize, PartialEq)]
struct DocGroup {
    label: String,
    #[serde(default)]
    children: Vec<DocGroupChild>,
}

/// 分组子节点：嵌套分组或文档条目
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(untagged)]
enum DocGroupChild {
    Group(DocGroup),
    Doc(DocEntry),
}

/// 目录清单中的一个分区（平铺 docs 与嵌套 groups 均为可选）
#[derive(Debug, Clone, Deserialize, PartialEq)]
struct DocSection {
    label: String,
    #[serde(default)]
    docs: Vec<DocEntry>,
    #[serde(default)]
    groups: Vec<DocGroupChild>,
}

/// 目录清单顶层结构（public/docs/index.json，build.rs 生成）
#[derive(Debug, Clone, Deserialize, PartialEq)]
struct DocIndex {
    sections: Vec<DocSection>,
}

/// 扁平化后的目录树行：分组行（可折叠）或文档行（可点击加载）
#[derive(Debug, Clone, PartialEq)]
enum FlatEntry {
    Group {
        key: String,
        label: String,
        depth: usize,
        expanded: bool,
    },
    Doc {
        entry: DocEntry,
        depth: usize,
    },
}

impl FlatEntry {
    /// 渲染 key（分组用树路径 key，文档用文件路径）
    fn key(&self) -> String {
        match self {
            FlatEntry::Group { key, .. } => key.clone(),
            FlatEntry::Doc { entry, .. } => entry.path.clone(),
        }
    }

    fn depth(&self) -> usize {
        match self {
            FlatEntry::Group { depth, .. } | FlatEntry::Doc { depth, .. } => *depth,
        }
    }

    /// 行展示文本（分组带 ▸/▾ 折叠指示符）
    fn text(&self) -> String {
        match self {
            FlatEntry::Group {
                label, expanded, ..
            } => {
                format!("{} {}", if *expanded { "▾" } else { "▸" }, label)
            }
            FlatEntry::Doc { entry, .. } => entry.title.clone(),
        }
    }

    /// 行样式（选中文档高亮为主色）
    fn row_class(&self, selected_path: Option<&str>) -> String {
        match self {
            FlatEntry::Group { .. } => {
                "rounded px-2 py-1 text-sm cursor-pointer font-medium hover:bg-base-200".to_string()
            }
            FlatEntry::Doc { entry, .. } => {
                if selected_path == Some(entry.path.as_str()) {
                    "rounded px-2 py-1 text-sm cursor-pointer bg-primary text-primary-content"
                        .to_string()
                } else {
                    "rounded px-2 py-1 text-sm cursor-pointer hover:bg-base-200".to_string()
                }
            }
        }
    }
}

/// 递归扁平化分组树（子节点可混合文档与目录）：
/// - 过滤激活时只保留命中分支，且全部展开
/// - 未过滤时分组默认折叠，仅展开 `expanded` 集合中的分组
fn flatten_children(
    children: &[DocGroupChild],
    depth: usize,
    parent_key: &str,
    expanded: &HashSet<String>,
    filter: &str,
    out: &mut Vec<FlatEntry>,
) {
    let filtering = !filter.is_empty();
    for (i, child) in children.iter().enumerate() {
        match child {
            DocGroupChild::Doc(doc) => {
                if !filtering || title_matches(&doc.title, filter) {
                    out.push(FlatEntry::Doc {
                        entry: doc.clone(),
                        depth,
                    });
                }
            }
            DocGroupChild::Group(group) => {
                if filtering && !group_matches(group, filter) {
                    continue;
                }
                let key = format!("{}-{}", parent_key, i);
                let is_expanded = expanded.contains(&key);
                out.push(FlatEntry::Group {
                    key: key.clone(),
                    label: group.label.clone(),
                    depth,
                    expanded: is_expanded,
                });
                // 折叠且未过滤时不展开子节点
                if !filtering && !is_expanded {
                    continue;
                }
                let label_hit = filtering && title_matches(&group.label, filter);
                let sub_children: Vec<DocGroupChild> = group
                    .children
                    .iter()
                    .filter(|c| {
                        !filtering
                            || label_hit
                            || match c {
                                DocGroupChild::Group(g) => group_matches(g, filter),
                                DocGroupChild::Doc(d) => title_matches(&d.title, filter),
                            }
                    })
                    .cloned()
                    .collect();
                flatten_children(&sub_children, depth + 1, &key, expanded, filter, out);
            }
        }
    }
}

/// 关键词是否命中标题（不区分大小写）
fn title_matches(title: &str, filter: &str) -> bool {
    title.to_lowercase().contains(filter)
}

/// 分组自身或任意后代文档是否命中过滤词
fn group_matches(group: &DocGroup, filter: &str) -> bool {
    if title_matches(&group.label, filter) {
        return true;
    }
    group.children.iter().any(|child| match child {
        DocGroupChild::Group(g) => group_matches(g, filter),
        DocGroupChild::Doc(d) => title_matches(&d.title, filter),
    })
}

/// 文档路径 URL 编码：wiki 文件名含中文/空格，逐段 encodeURIComponent 后拼回
fn encode_doc_path(path: &str) -> String {
    path.split('/')
        .map(|seg| String::from(js_sys::encode_uri_component(seg)))
        .collect::<Vec<_>>()
        .join("/")
}

/// 加载单篇文档：fetch 原始 Markdown 并渲染为 HTML
async fn load_doc(
    path: String,
    mut selected: Signal<Option<String>>,
    mut html: Signal<Option<String>>,
    mut loading: Signal<bool>,
    toast: ToastState,
) {
    selected.set(Some(path.clone()));
    loading.set(true);
    match fetch_static_text(&format!("/docs/{}", encode_doc_path(&path))).await {
        Ok(md) => html.set(Some(render_markdown(&md))),
        Err(e) => {
            html.set(None);
            toast.error(format!("加载文档失败: {}", e));
        }
    }
    loading.set(false);
}

#[component]
pub fn SystemDocs() -> Element {
    let toast = use_toast();
    let mut index: Signal<Option<DocIndex>> = use_signal(|| None);
    let selected: Signal<Option<String>> = use_signal(|| None);
    let html: Signal<Option<String>> = use_signal(|| None);
    let loading_doc = use_signal(|| false);
    let mut filter: Signal<String> = use_signal(String::new);
    // 当前展开的分组 key 集合（wiki 目录树折叠状态）
    let mut expanded: Signal<HashSet<String>> = use_signal(HashSet::new);

    // 初始加载目录清单，并默认打开第一篇文档
    use_effect(move || {
        spawn(async move {
            match fetch_static_text("/docs/index.json").await {
                Ok(text) => match serde_json::from_str::<DocIndex>(&text) {
                    Ok(idx) => {
                        let first = idx
                            .sections
                            .first()
                            .and_then(|s| s.docs.first())
                            .map(|d| d.path.clone());
                        index.set(Some(idx));
                        if let Some(path) = first {
                            load_doc(path, selected, html, loading_doc, toast).await;
                        }
                    }
                    Err(e) => toast.error(format!("解析文档目录失败: {}", e)),
                },
                Err(e) => toast.error(format!("加载文档目录失败（请确认前端已重新构建）: {}", e)),
            }
        });
    });

    let index_opt = index.read().clone();
    let html_opt = html.read().clone();
    let selected_opt = selected.read().clone();
    let filter_text = filter.read().clone();
    let filter_key = filter_text.trim().to_lowercase();
    let filtering = !filter_key.is_empty();
    let expanded_set = expanded.read().clone();

    // 应用过滤并扁平化分组树：得到 (分区, 平铺文档, 扁平树行) 渲染序列
    let section_views: Vec<(DocSection, Vec<FlatEntry>)> = index_opt
        .clone()
        .map(|idx| {
            idx.sections
                .into_iter()
                .filter_map(|mut s| {
                    if filtering {
                        s.docs.retain(|d| title_matches(&d.title, &filter_key));
                        s.groups.retain(|g| match g {
                            DocGroupChild::Group(gg) => group_matches(gg, &filter_key),
                            DocGroupChild::Doc(d) => title_matches(&d.title, &filter_key),
                        });
                    }
                    let mut flat = Vec::new();
                    flatten_children(
                        &s.groups,
                        0,
                        &s.label,
                        &expanded_set,
                        &filter_key,
                        &mut flat,
                    );
                    if s.docs.is_empty() && flat.is_empty() {
                        None
                    } else {
                        Some((s, flat))
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    rsx! {
        AppLayout {
            div { class: "space-y-4",
                div { class: "flex justify-between items-center",
                    h2 { class: "card-title", "文档中心" }
                    span { class: "text-sm opacity-60", "仓库 docs/ 核心文档，随前端构建更新" }
                }

                if index_opt.is_none() {
                    div { class: "flex justify-center py-12",
                        span { class: "loading loading-spinner loading-lg" }
                    }
                } else {
                    div { class: "flex flex-col lg:flex-row gap-4 items-start",
                        // 左侧目录
                        div { class: "card bg-base-100 shadow w-full lg:w-72 shrink-0",
                            div { class: "card-body p-3 lg:max-h-[75vh] overflow-y-auto",
                                input {
                                    class: "input input-sm input-bordered w-full mb-1",
                                    placeholder: "过滤文档标题...",
                                    value: "{filter_text}",
                                    oninput: move |e| filter.set(e.value()),
                                }
                                if section_views.is_empty() {
                                    div { class: "text-center opacity-60 py-6 text-sm",
                                        if filtering { "无匹配文档" } else { "目录为空" }
                                    }
                                }
                                for (section, flat_entries) in section_views.iter() {
                                    div { key: "{section.label}",
                                        div {
                                            class: "text-xs font-semibold opacity-60 uppercase tracking-wide mt-2 mb-1 px-2",
                                            "{section.label}"
                                        }
                                        if !section.docs.is_empty() {
                                            ul { class: "menu menu-sm p-0",
                                                for doc in section.docs.iter() {
                                                    li { key: "{doc.path}",
                                                        a {
                                                            class: if selected_opt.as_deref() == Some(doc.path.as_str()) {
                                                                "active"
                                                            } else {
                                                                ""
                                                            },
                                                            onclick: {
                                                                let path = doc.path.clone();
                                                                move |_| {
                                                                    spawn(load_doc(
                                                                        path.clone(),
                                                                        selected,
                                                                        html,
                                                                        loading_doc,
                                                                        toast,
                                                                    ));
                                                                }
                                                            },
                                                            "{doc.title}"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        // wiki 扁平树：分组行点击折叠/展开，文档行点击加载
                                        for entry in flat_entries.iter() {
                                            div {
                                                key: "{entry.key()}",
                                                class: "{entry.row_class(selected_opt.as_deref())}",
                                                style: "padding-left: {8 + entry.depth() * 12}px",
                                                onclick: {
                                                    let entry = entry.clone();
                                                    move |_| match &entry {
                                                        FlatEntry::Group { key, .. } => {
                                                            let mut set = expanded.write();
                                                            if !set.remove(key) {
                                                                set.insert(key.clone());
                                                            }
                                                        }
                                                        FlatEntry::Doc { entry: doc, .. } => {
                                                            spawn(load_doc(
                                                                doc.path.clone(),
                                                                selected,
                                                                html,
                                                                loading_doc,
                                                                toast,
                                                            ));
                                                        }
                                                    }
                                                },
                                                "{entry.text()}"
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // 右侧文档内容
                        div { class: "card bg-base-100 shadow flex-1 w-full min-w-0",
                            div { class: "card-body",
                                if loading_doc() {
                                    div { class: "flex justify-center py-12",
                                        span { class: "loading loading-spinner loading-md" }
                                    }
                                } else if let Some(h) = html_opt.as_ref() {
                                    div {
                                        class: "markdown-body max-w-none overflow-x-auto",
                                        dangerous_inner_html: "{h}"
                                    }
                                } else {
                                    div { class: "text-center opacity-60 py-12",
                                        "请从左侧目录选择文档"
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
