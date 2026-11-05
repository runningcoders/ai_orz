//! 文档中心 - 浏览仓库 docs/ 下的 Markdown 文档
//!
//! 运行时从静态资源 `/docs/index.json` 拉取目录清单（由 build.rs 打包
//! docs/ 下全部文档到 public/docs/ 时生成），点击条目按需 fetch 对应
//! Markdown 文件，用 MarkdownRenderer 组件渲染（自动触发 ```mermaid 图的 JS 替换）。
//!
//! 分区顺序（由 build.rs 保证）：总览 → 架构图 → 能力域知识库 → 模块索引
//! 图表类条目（index.json 中 `type: "html"`）不走 Markdown 渲染：
//! 右侧内容区交给共享组件 `HtmlPreview`（`components/html_preview.rs`）以静态 URL 模式
//! 内嵌——与产物 / 附件详情页的 HTML 预览是同一条渲染路径（sandbox 隔离图表自带的
//! 脚本 / 样式）；
//! 标题栏另提供「新标签打开」直链 /docs/diagrams/xxx.html。
//! 各分区默认展开、可整体折叠；能力域知识库为「模块 → A/B/C 层 → 文档」三级折叠树。

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::components::html_preview::{HtmlPreview, HtmlPreviewSandbox};
use crate::components::hud::HudPanel;
use crate::components::hud::PageHeader;
use dioxus::html::events::MouseEvent;
use dioxus::prelude::*;
use serde::Deserialize;
use wasm_bindgen::{JsCast, JsValue};

use crate::api::fetch_static_text;
use crate::components::markdown::MarkdownRenderer;
use crate::components::state::Loading;
use crate::hooks::use_theme;
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::{ToastState, use_toast};
use crate::utils::doc_link::DocRenderContext;

/// 文档条目（叶子节点；wiki 树中也可能出现在 children 里）
#[derive(Debug, Clone, Deserialize, PartialEq)]
struct DocEntry {
    title: String,
    path: String,
    /// 原始仓库路径（如 `docs/AGENT_GUIDE.md`），渲染期归一化 `./` `../` 链接的基准
    #[serde(default)]
    repo_path: String,
    /// 条目类型：缺省为 Markdown；`html` 表示自包含 HTML 资源（如架构图），
    /// 由 build.rs 生成 index.json 时标注，前端以新标签页直链打开。
    #[serde(default, rename = "type")]
    kind: String,
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

    /// 是否为当前选中的文档行（分组行恒为 false）
    fn is_active(&self, selected_path: Option<&str>) -> bool {
        matches!(self, FlatEntry::Doc { entry, .. } if selected_path == Some(entry.path.as_str()))
    }

    fn row_class(&self, selected_path: Option<&str>) -> String {
        match self {
            FlatEntry::Group { .. } => {
                "rounded px-2 py-1 text-sm cursor-pointer font-medium truncate hover:bg-base-200"
                    .to_string()
            }
            FlatEntry::Doc { entry, .. } => {
                if selected_path == Some(entry.path.as_str()) {
                    "rounded px-2 py-1 text-sm cursor-pointer truncate bg-primary text-primary-content"
                        .to_string()
                } else {
                    "rounded px-2 py-1 text-sm cursor-pointer truncate hover:bg-base-200"
                        .to_string()
                }
            }
        }
    }
}

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

fn title_matches(title: &str, filter: &str) -> bool {
    title.to_lowercase().contains(filter)
}

fn group_matches(group: &DocGroup, filter: &str) -> bool {
    if title_matches(&group.label, filter) {
        return true;
    }
    group.children.iter().any(|child| match child {
        DocGroupChild::Group(g) => group_matches(g, filter),
        DocGroupChild::Doc(d) => title_matches(&d.title, filter),
    })
}

/// 递归收集「仓库路径 → 站内副本」映射（渲染期站内链接分发用）
fn collect_repo_pages(children: &[DocGroupChild], out: &mut HashMap<String, String>) {
    for child in children {
        match child {
            DocGroupChild::Doc(doc) => {
                if !doc.repo_path.is_empty() {
                    out.insert(doc.repo_path.clone(), doc.path.clone());
                }
            }
            DocGroupChild::Group(group) => collect_repo_pages(&group.children, out),
        }
    }
}

/// 从目录清单构建全量映射（总览 + 能力域知识库）
fn build_repo_pages(index: &DocIndex) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for section in &index.sections {
        for doc in &section.docs {
            if !doc.repo_path.is_empty() {
                map.insert(doc.repo_path.clone(), doc.path.clone());
            }
        }
        collect_repo_pages(&section.groups, &mut map);
    }
    map
}

/// 空 repo_path（历史 index.json）按「无上下文」处理
fn repo_opt(repo_path: &str) -> Option<String> {
    (!repo_path.is_empty()).then(|| repo_path.to_string())
}

/// 读取 `#doc=<id>` 深链参数（id 已百分号编码；优先原始仓库路径，历史条目用站内短路径）
fn hash_doc_id() -> Option<String> {
    let hash = web_sys::window()?.location().hash().ok()?;
    let encoded = hash.strip_prefix('#')?.strip_prefix("doc=")?;
    js_sys::decode_uri_component(encoded)
        .ok()?
        .as_string()
        .filter(|s| !s.is_empty())
}

/// 把当前文档写回 `#doc=<id>`：用 replaceState，不新增历史、不触发路由重算
fn set_hash_doc(id: &str) {
    let Some(win) = web_sys::window() else {
        return;
    };
    let Ok(history) = win.history() else {
        return;
    };
    let encoded = js_sys::encode_uri_component(id)
        .as_string()
        .unwrap_or_default();
    let url = format!("#doc={encoded}");
    let _ = history.replace_state_with_url(&JsValue::NULL, "", Some(&url));
}

/// 按深链 id（repo_path 或站内 path）查找文档条目
fn find_doc_by_key(index: &DocIndex, key: &str) -> Option<DocEntry> {
    fn walk(children: &[DocGroupChild], key: &str) -> Option<DocEntry> {
        for child in children {
            match child {
                DocGroupChild::Doc(doc) if doc.repo_path == key || doc.path == key => {
                    return Some(doc.clone());
                }
                DocGroupChild::Doc(_) => {}
                DocGroupChild::Group(group) => {
                    if let Some(doc) = walk(&group.children, key) {
                        return Some(doc);
                    }
                }
            }
        }
        None
    }
    for section in &index.sections {
        if let Some(doc) = section
            .docs
            .iter()
            .find(|d| d.repo_path == key || d.path == key)
        {
            return Some(doc.clone());
        }
        if let Some(doc) = walk(&section.groups, key) {
            return Some(doc);
        }
    }
    None
}

/// 文档在导航树中的位置：所属 section 标签 + 沿途分组 key（用于自动展开并滚动定位）
fn find_doc_location(index: &DocIndex, key: &str) -> Option<(DocEntry, String, Vec<String>)> {
    fn dir_contains(group: &DocGroup, key: &str) -> bool {
        group.children.iter().any(|child| match child {
            DocGroupChild::Doc(doc) => doc.repo_path == key || doc.path == key,
            DocGroupChild::Group(g) => dir_contains(g, key),
        })
    }
    fn walk(
        children: &[DocGroupChild],
        parent_key: &str,
        key: &str,
        group_keys: &mut Vec<String>,
    ) -> Option<DocEntry> {
        for (i, child) in children.iter().enumerate() {
            match child {
                DocGroupChild::Doc(doc) if doc.repo_path == key || doc.path == key => {
                    return Some(doc.clone());
                }
                DocGroupChild::Doc(_) => {}
                DocGroupChild::Group(group) => {
                    if !dir_contains(group, key) {
                        continue;
                    }
                    let group_key = format!("{parent_key}-{i}");
                    group_keys.push(group_key.clone());
                    if let Some(doc) = walk(&group.children, &group_key, key, group_keys) {
                        return Some(doc);
                    }
                    group_keys.pop();
                }
            }
        }
        None
    }
    for section in &index.sections {
        if let Some(doc) = section
            .docs
            .iter()
            .find(|d| d.repo_path == key || d.path == key)
        {
            return Some((doc.clone(), section.label.clone(), Vec::new()));
        }
        let mut group_keys = Vec::new();
        if let Some(doc) = walk(&section.groups, &section.label, key, &mut group_keys) {
            return Some((doc, section.label.clone(), group_keys));
        }
    }
    None
}

fn encode_doc_path(path: &str) -> String {
    path.split('/')
        .map(|seg| String::from(js_sys::encode_uri_component(seg)))
        .collect::<Vec<_>>()
        .join("/")
}

/// 是否为静态 HTML 资源（不走 Markdown 渲染）
fn is_html_path(path: &str) -> bool {
    path.ends_with(".html")
}

fn is_html_entry(entry: &DocEntry) -> bool {
    entry.kind == "html" || is_html_path(&entry.path)
}

async fn load_doc(
    path: String,
    repo_path: Option<String>,
    mut selected: Signal<Option<String>>,
    mut current_repo: Signal<Option<String>>,
    mut markdown: Signal<Option<String>>,
    mut loading: Signal<bool>,
    toast: ToastState,
) {
    // 图表 HTML 无法按 Markdown 渲染：进入选中态，由右侧内容区 iframe 内嵌展示
    if is_html_path(&path) {
        selected.set(Some(path.clone()));
        current_repo.set(repo_path);
        markdown.set(None);
        loading.set(false);
        return;
    }
    selected.set(Some(path.clone()));
    current_repo.set(repo_path);
    loading.set(true);
    match fetch_static_text(&format!("/docs/{}", encode_doc_path(&path))).await {
        Ok(md) => markdown.set(Some(md)),
        Err(e) => {
            markdown.set(None);
            toast.error(format!("加载文档失败: {}", e));
        }
    }
    loading.set(false);
}

#[component]
pub fn SystemDocs() -> Element {
    let toast = use_toast();
    // 架构图 iframe 自带 dark 主题：按应用当前主题传 `?theme=` 保持一致
    let theme = use_theme();
    let mut index: Signal<Option<DocIndex>> = use_signal(|| None);
    let selected: Signal<Option<String>> = use_signal(|| None);
    // 当前文档的原始仓库路径 + 全量「仓库路径 → 站内副本」映射（渲染期链接分发用）
    let current_repo: Signal<Option<String>> = use_signal(|| None);
    let mut repo_pages: Signal<Option<Rc<HashMap<String, String>>>> = use_signal(|| None);
    let markdown: Signal<Option<String>> = use_signal(|| None);
    let loading_doc = use_signal(|| false);
    let mut filter: Signal<String> = use_signal(String::new);
    // 知识库三级树的分组展开集合
    let mut expanded: Signal<HashSet<String>> = use_signal(HashSet::new);
    // Section 级折叠状态（label 在集合内 = 已收起）；默认全部展开
    let mut collapsed_sections: Signal<HashSet<String>> = use_signal(HashSet::new);

    use_effect(move || {
        spawn(async move {
            match fetch_static_text("/docs/index.json").await {
                Ok(text) => match serde_json::from_str::<DocIndex>(&text) {
                    Ok(idx) => {
                        let pages = Rc::new(build_repo_pages(&idx));
                        // 深链优先：`#doc=<repo_path|path>` 命中则直接打开该文档（含架构图）
                        let deep_linked = hash_doc_id().and_then(|key| find_doc_by_key(&idx, &key));
                        let first = deep_linked
                            .or_else(|| {
                                idx.sections.iter().find_map(|s| {
                                    s.docs.iter().find(|d| !is_html_entry(d)).cloned()
                                })
                            })
                            .map(|d| (d.path.clone(), repo_opt(&d.repo_path)));
                        index.set(Some(idx));
                        repo_pages.set(Some(pages));
                        if let Some((path, repo)) = first {
                            load_doc(
                                path,
                                repo,
                                selected,
                                current_repo,
                                markdown,
                                loading_doc,
                                toast,
                            )
                            .await;
                        }
                    }
                    Err(e) => toast.error(format!("解析文档目录失败: {}", e)),
                },
                Err(e) => toast.error(format!("加载文档目录失败（请确认前端已重新构建）: {}", e)),
            }
        });
    });

    // 选中文档 → 同步 `#doc=` 深链 + 自动展开左侧导航所在的 section / 分组
    use_effect(move || {
        let Some(path) = selected.read().clone() else {
            return;
        };
        let key = current_repo.read().clone().unwrap_or_else(|| path.clone());
        set_hash_doc(&key);
        let idx_guard = index.read();
        let Some(idx) = idx_guard.as_ref() else {
            return;
        };
        if let Some((_, section, group_keys)) = find_doc_location(idx, &key) {
            collapsed_sections.write().remove(&section);
            if !group_keys.is_empty() {
                expanded.write().extend(group_keys);
            }
        }
    });

    // 高亮行滚动到可视区：依赖展开状态，确保长目录里的深链目标定位到左侧导航
    use_effect(move || {
        let _ = selected.read();
        let _ = expanded.read();
        let _ = collapsed_sections.read();
        let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
            return;
        };
        if let Ok(Some(el)) = doc.query_selector("[data-doc-active=\"true\"]") {
            // 只保证高亮行可见：默认 block=start 会把整页（含窗口）滚到该行，
            // 表现为点击左侧条目时页面闪一下、被点条目顶到最上方。
            let opts = web_sys::ScrollIntoViewOptions::new();
            web_sys::ScrollIntoViewOptions::set_block(
                &opts,
                web_sys::ScrollLogicalPosition::Nearest,
            );
            el.scroll_into_view_with_scroll_into_view_options(&opts);
        }
    });

    let index_opt = index.read().clone();
    let selected_opt = selected.read().clone();
    let markdown_opt = markdown.read().clone();
    let filter_text = filter.read().clone();
    let filter_key = filter_text.trim().to_lowercase();
    let filtering = !filter_key.is_empty();
    let expanded_set = expanded.read().clone();
    let collapsed_set = collapsed_sections.read().clone();

    let section_views: Vec<(DocSection, Vec<FlatEntry>, bool)> = index_opt
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
                        let section_collapsed = !filtering && collapsed_set.contains(&s.label);
                        Some((s, flat, section_collapsed))
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    // 当前文档上下文：原文位置 + 收录映射；未命中映射的链接在渲染期退化 GitHub
    let doc_ctx = current_repo
        .read()
        .clone()
        .zip(repo_pages.read().clone())
        .map(|(doc_repo_path, repo_to_page)| DocRenderContext {
            doc_repo_path,
            repo_to_page,
        });

    // 架构图（HTML）内嵌：标题用于 iframe 头部/无障碍，主题与应用主题保持一致
    let html_title = selected_opt
        .as_deref()
        .filter(|p| is_html_path(p))
        .and_then(|p| index_opt.as_ref().and_then(|idx| find_doc_by_key(idx, p)))
        .map(|d| d.title)
        .unwrap_or_else(|| "架构图".to_string());
    let iframe_theme = if theme.current() == "orz-light" {
        "light"
    } else {
        "dark"
    };

    rsx! {
        AppLayout {
            div { class: "space-y-4",
                PageHeader {
                    eyebrow: Some("SYSTEM".to_string()),
                    title: "文档中心".to_string(),
                    actions: Some(rsx!{
                    span { class: "text-sm opacity-60", "仓库 docs/ 核心文档，随前端构建更新" }
                    }),
                },

                if index_opt.is_none() {
                    div { class: "flex justify-center py-12",
                        Loading { size: "lg" }
                    }
                } else {
                    div { class: "flex flex-col lg:flex-row gap-4 items-start",
                        // 左侧目录
                        HudPanel { signal: Some(true), extra_class: Some("w-full lg:w-80 shrink-0".to_string()),
                            div { class: "card-body p-3 lg:max-h-[75vh] overflow-y-auto",
                                input {
                                    class: "input input-sm input-bordered w-full mb-2",
                                    placeholder: "过滤文档标题...",
                                    value: "{filter_text}",
                                    oninput: move |e| filter.set(e.value()),
                                }
                                if section_views.is_empty() {
                                    div { class: "text-center opacity-60 py-6 text-sm",
                                        if filtering { "无匹配文档" } else { "目录为空" }
                                    }
                                }
                                for (section, flat_entries, collapsed) in section_views.iter() {
                                    div { key: "{section.label}", class: "mb-1",
                                        // Section 标题行：支持点击整体收起/展开
                                        div {
                                            class: "flex items-center justify-between px-2 py-1.5 mt-2 rounded cursor-pointer hover:bg-base-200",
                                            onclick: {
                                                let label = section.label.clone();
                                                move |_| {
                                                    let mut set = collapsed_sections.write();
                                                    if !set.remove(&label) {
                                                        set.insert(label.clone());
                                                    }
                                                }
                                            },
                                            div { class: "text-xs font-semibold opacity-70 uppercase tracking-wide",
                                                "{section.label}"
                                            }
                                            span { class: "text-xs opacity-60",
                                                if *collapsed { "▸" } else { "▾" }
                                            }
                                        }

                                        if !collapsed {
                                            // Section 内部：直接 docs 列表 + Wiki 扁平树
                                            if !section.docs.is_empty() {
                                                ul { class: "menu menu-sm p-0 mt-1",
                                                    for doc in section.docs.iter() {
                                                        li { key: "{doc.path}",
                                                            if is_html_entry(doc) {
                                                                // 架构图：点击后右侧 iframe 内嵌；href 保留直开降级
                                                                a {
                                                                    class: if selected_opt.as_deref() == Some(doc.path.as_str()) {
                                                                        "active truncate"
                                                                    } else {
                                                                        "truncate"
                                                                    },
                                                                    "data-doc-active": if selected_opt.as_deref() == Some(doc.path.as_str()) {
                                                                        "true"
                                                                    } else {
                                                                        "false"
                                                                    },
                                                                    href: "/docs/{encode_doc_path(&doc.path)}",
                                                                    onclick: {
                                                                        let path = doc.path.clone();
                                                                        let repo = repo_opt(&doc.repo_path);
                                                                        move |evt: MouseEvent| {
                                                                            evt.prevent_default();
                                                                            spawn(load_doc(
                                                                                path.clone(),
                                                                                repo.clone(),
                                                                                selected,
                                                                                current_repo,
                                                                                markdown,
                                                                                loading_doc,
                                                                                toast,
                                                                            ));
                                                                        }
                                                                    },
                                                                    "{doc.title}"
                                                                }
                                                            } else {
                                                                a {
                                                                    class: if selected_opt.as_deref() == Some(doc.path.as_str()) {
                                                                        "active truncate"
                                                                    } else {
                                                                        "truncate"
                                                                    },
                                                                    "data-doc-active": if selected_opt.as_deref() == Some(doc.path.as_str()) {
                                                                        "true"
                                                                    } else {
                                                                        "false"
                                                                    },
                                                                    onclick: {
                                                                        let path = doc.path.clone();
                                                                        let repo = repo_opt(&doc.repo_path);
                                                                        move |_| {
                                                                            spawn(load_doc(
                                                                                path.clone(),
                                                                                repo.clone(),
                                                                                selected,
                                                                                current_repo,
                                                                                markdown,
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
                                            }
                                            for entry in flat_entries.iter() {
                                                div {
                                                    key: "{entry.key()}",
                                                    class: "{entry.row_class(selected_opt.as_deref())}",
                                                    "data-doc-active": if entry.is_active(selected_opt.as_deref()) { "true" } else { "false" },
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
                                                                    repo_opt(&doc.repo_path),
                                                                    selected,
                                                                    current_repo,
                                                                    markdown,
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
                        }

                        // 右侧文档内容：Markdown 走 MarkdownRenderer；架构图 HTML 走 iframe 隔离内嵌
                        HudPanel { signal: Some(true), extra_class: Some("flex-1 w-full min-w-0".to_string()),
                            div { class: "card-body",
                                if loading_doc() {
                                    div { class: "flex justify-center py-12",
                                        Loading { size: "md" }
                                    }
                                } else if let Some(path) = selected_opt.as_deref().filter(|p| is_html_path(p)) {
                                    div { class: "space-y-2",
                                        div { class: "flex items-center justify-between gap-2",
                                            div { class: "text-sm font-medium truncate", "{html_title}" }
                                            a {
                                                class: "btn btn-ghost btn-xs shrink-0",
                                                href: "/docs/{encode_doc_path(path)}",
                                                target: "_blank",
                                                rel: "noopener noreferrer",
                                                "↗ 新标签打开"
                                            }
                                        }
                                        // 渲染统一收敛到共享组件 components/html_preview.rs：
                                        // 与产物 / 附件预览同一套 iframe 属性与隔离口径，差别只在来源形态
                                        // （这里是同源静态 URL，产物是 API 取回的源码）。
                                        // WithPopups = 架构图既有的开窗能力逐字保留；sandbox / referrerpolicy
                                        // 由组件统一给出，此处不再拼字符串。
                                        // archify 的 localStorage/clipboard 调用均有 try/catch 兜底
                                        HtmlPreview {
                                            src: Some(format!("/docs/{}?theme={}", encode_doc_path(path), iframe_theme)),
                                            title: html_title.clone(),
                                            sandbox: HtmlPreviewSandbox::WithPopups,
                                            lazy: false,
                                        }
                                    }
                                } else if let Some(md) = markdown_opt.as_ref() {
                                    div {
                                        class: "markdown-body max-w-none overflow-x-auto",
                                        // 文档互链：渲染期已标 data-doc-path，这里拦截后原地切换
                                        onclick: move |evt: MouseEvent| {
                                            let data = evt.data();
                                            let Some(web_evt) =
                                                data.downcast::<web_sys::MouseEvent>()
                                            else {
                                                return;
                                            };
                                            let Some(target) = web_evt.target() else {
                                                return;
                                            };
                                            let Ok(target) = target.dyn_into::<web_sys::Element>()
                                            else {
                                                return;
                                            };
                                            let Ok(Some(anchor)) =
                                                target.closest("a.doc-link-internal")
                                            else {
                                                return;
                                            };
                                            let Some(page) =
                                                anchor.get_attribute("data-doc-path")
                                            else {
                                                return;
                                            };
                                            evt.prevent_default();
                                            spawn(load_doc(
                                                page,
                                                anchor.get_attribute("data-doc-repo"),
                                                selected,
                                                current_repo,
                                                markdown,
                                                loading_doc,
                                                toast,
                                            ));
                                        },
                                        MarkdownRenderer {
                                            content: md.clone(),
                                            doc_ctx: doc_ctx.clone()
                                        }
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
