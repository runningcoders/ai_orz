use crate::components::hud::{HudPanel, HudSection};
use dioxus::prelude::*;
use std::collections::{HashMap, HashSet};

use crate::api::hr::{recommend_seed_nodes, search_agents, search_memory_with_traversal};
use crate::components::SearchableSelect;
use crate::components::button::Button;
use crate::components::graph::{
    Graph, GraphEdge, GraphNode, calculate_layout, expand_layout, type_label,
};
use crate::components::graph_canvas::KnowledgeGraphCanvas;
use crate::components::markdown::MarkdownRenderer;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use crate::utils::number::format_relevance;
use common::api::{
    AgentListItem, MemoryResult, RecommendSeedNodesParams, SearchAgentsRequest, SearchMemoryParams,
    SeedNodeRecommendation,
};
use common::enums::KnowledgeRelationType;

/// 匹配方式中文名。
///
/// 后端 `search_match.match_type` 有三种取值；详情面板必须把它和「相关度」
/// 一起展示——单看一个百分比，用户无法分辨这是语义相似度还是关键词命中。
fn match_type_label(match_type: &str) -> &str {
    match match_type {
        "hybrid" => "语义 + 关键词",
        "vector" => "语义匹配",
        "keyword" => "关键词命中",
        other => other,
    }
}

/// 渲染风格：svg（兜底）或 canvas（HUD 驾驶舱风格）
#[derive(Clone, Copy, PartialEq)]
enum GraphStyle {
    Svg,
    Canvas,
}

/// 节点展示名：**名称 → 摘要首行 → 正文首行 → 「未命名节点」**
///
/// 「未命名节点」只兜底「实体节点本身字段全空」这一种情况；**不给关系端点造占位**
/// （端点缺失的边整条不画，见 `build_graph_from_results`）。
/// ID 不进卡片，统一留给 hover 详情（见 `graph::node_hover_lines`）。
///
/// ⚠️ 名称保持**完整不截断**：截断只属于「节点卡片标题」这一处渲染
/// （`node_card::title` 单行收窄）；label 若在数据构建层被截断，hover 详情卡、
/// SVG 兜底与边端点名将拿到残缺文本（体验问题排查专项·第二批·问题一）。
fn node_display_name(item: &MemoryResult) -> String {
    let head = |s: &str| s.trim().lines().next().unwrap_or("").trim().to_string();
    if let Some(name) = item
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return name.to_string();
    }
    if let Some(summary) = item
        .summary
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return head(summary);
    }
    let h = head(&item.content);
    if h.is_empty() {
        "未命名节点".to_string()
    } else {
        h
    }
}

/// 从搜索结果构建图谱节点和边。
///
/// ⚠️ **图批次不变式：边只画两端节点都在本批结果里的那些**。层级（`traversal_depth`）
/// 是**节点维度**的概念 —— 边不是层级实体，只是连接两个已在结果里的节点的线；
/// 端点缺失的边没有意义（旧实现会给它造个占位节点，于是整屏「未命名节点」：
/// 没有名称、没有正文、hover 也没内容）。后端 `drop_dangling_relations` 是第一道闸，
/// 这里是第二道。
fn build_graph_from_results(results: &[MemoryResult]) -> (Vec<GraphNode>, Vec<GraphEdge>) {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut seen_node_ids = HashSet::new();

    // 第一遍：收集实体节点的展示名映射（id → 名称），
    // 供关系边的端点节点复用真实名称，避免图上出现 UUID 或英文类型词
    let mut name_by_id: HashMap<&str, String> = HashMap::new();
    for item in results {
        if matches!(
            item.memory_type.as_str(),
            "knowledge_node" | "short_term" | "trace"
        ) {
            name_by_id
                .entry(item.id.as_str())
                .or_insert_with(|| node_display_name(item));
        }
    }

    for item in results {
        match item.memory_type.as_str() {
            "knowledge_node" | "short_term" | "trace" => {
                if seen_node_ids.insert(item.id.clone()) {
                    nodes.push(GraphNode {
                        id: item.id.clone(),
                        label: node_display_name(item),
                        // 正文完整保留：卡片上只放前两行，hover 详情展示全文
                        description: item.content.clone(),
                        node_type: item.memory_type.clone(),
                        x: 0.0,
                        y: 0.0,
                        tags: item.tags.clone().unwrap_or_default(),
                        summary: item.summary.clone(),
                    });
                }
            }
            "relation" => {
                if let (Some(src), Some(tgt)) = (&item.source_node_id, &item.target_node_id) {
                    // 图批次不变式：两端节点都在本批结果里才画这条边。
                    // 缺失端点**不造占位节点**，整条边直接不画。
                    if !name_by_id.contains_key(src.as_str())
                        || !name_by_id.contains_key(tgt.as_str())
                    {
                        continue;
                    }
                    // 边标签：关系类型中文化（related → 相关），无类型回退「关联」
                    let label = item
                        .relation_type
                        .as_deref()
                        .map(KnowledgeRelationType::zh_label_from_display)
                        .unwrap_or("关联")
                        .to_string();
                    edges.push(GraphEdge {
                        source: src.clone(),
                        target: tgt.clone(),
                        label,
                        // 关系强度：图谱按它调线宽与浓淡，未标注（存量边）走基准
                        weight: item.weight,
                    });
                }
            }
            _ => {}
        }
    }

    (nodes, edges)
}

fn type_badge_class(t: &str) -> &'static str {
    // 记忆节点类型（knowledge_node/short_term/trace/relation）是「类别标签」，
    // 统一走中性 orz-tag chip（与状态徽章区分）
    let _ = t;
    "badge orz-tag badge-sm"
}

/// 可复用知识图谱子组件。
/// agent_id = Some(id)：锁定 Agent（Agent 详情页场景）
/// agent_id = None：全局知识图谱（独立页面场景，由父组件提供 Agent 选择器）
#[component]
pub fn KnowledgeGraph(agent_id: Option<String>) -> Element {
    let mut keyword = use_signal(String::new);
    // 关联展开层数（后端 traversal_depth）：搜索与「点击节点展开」共用
    let mut traversal_depth = use_signal(|| 1i32);
    let mut tags_input = use_signal(String::new);
    let mut nodes = use_signal(Vec::<GraphNode>::new);
    let mut edges = use_signal(Vec::<GraphEdge>::new);
    let mut loading = use_signal(|| false);
    let toast = use_toast();
    let mut expanded_nodes = use_signal(HashSet::<String>::new);
    let mut selected_node_id = use_signal(|| None::<String>);
    let mut selected_node_data = use_signal(|| None::<MemoryResult>);
    let mut search_history = use_signal(Vec::<String>::new);
    let mut highlighted_node_ids = use_signal(Vec::<String>::new);
    let mut detail_map = use_signal(std::collections::HashMap::<String, MemoryResult>::new);
    // 修复 M11：节点点击请求 ID，用于取消过期的并发请求结果（用户快速点击多个节点时）
    let mut click_request_id = use_signal(|| 0u32);
    // 渲染风格切换：默认 Canvas（HUD 驾驶舱风格），可切回 SVG 作为兜底
    let mut graph_style = use_signal(|| GraphStyle::Canvas);

    // 推荐起点状态
    let mut recommendations = use_signal(Vec::<SeedNodeRecommendation>::new);
    let mut rec_loading = use_signal(|| false);

    // 将 agent_id 存入 Signal（Copy 类型），使闭包捕获后仍为 Copy，可在 for 循环中复用。
    // 修复 E2E 发现的渲染器崩溃：Signal::set 无条件标脏，渲染期 set 会触发
    // 「重渲染 → 再 set」死循环，主线程无限自旋直到 Chromium 杀掉渲染器（页面白屏）。
    // 等值守卫打断循环；用 peek 避免渲染体订阅自身。
    let aid_init = agent_id.clone();
    let mut agent_id_signal = use_signal(move || aid_init);
    if *agent_id_signal.peek() != agent_id {
        agent_id_signal.set(agent_id);
    }

    // 加载推荐起点
    let mut load_recommendations = move || {
        let aid = agent_id_signal();
        rec_loading.set(true);
        spawn(async move {
            let params = RecommendSeedNodesParams {
                agent_id: aid,
                limit: Some(5),
            };
            match recommend_seed_nodes(&params).await {
                Ok(resp) => recommendations.set(resp.recommendations),
                Err(e) => toast.error(format!("加载推荐起点失败: {}", e)),
            }
            rec_loading.set(false);
        });
    };

    // 首次渲染加载推荐起点（agent_id 变化时也会重新触发）
    use_effect(move || {
        load_recommendations();
    });

    let mut handle_search = move |_| {
        let kw = keyword().clone();
        if kw.is_empty() {
            return;
        }
        let tags_raw = tags_input().clone();
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

        let aid = agent_id_signal();
        spawn(async move {
            let tags_vec: Vec<String> = if tags_raw.trim().is_empty() {
                Vec::new()
            } else {
                tags_raw
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            };
            let tags_field: Option<Vec<String>> = if tags_vec.is_empty() {
                None
            } else {
                Some(tags_vec)
            };
            let params = SearchMemoryParams {
                query: kw,
                max_results: Some(50),
                memory_type: None,
                traversal_depth: Some(traversal_depth()),
                traversal_breadth: Some(10),
                traversal_strategy: Some("breadth_first".to_string()),
                seed_node_ids: Some(Vec::new()),
                tags: tags_field,
                task_id: None,
                agent_id: aid,
            };
            match search_memory_with_traversal(params).await {
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

    let mut handle_node_click = move |node_id: String| {
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
        let aid = agent_id_signal();
        spawn(async move {
            let params = SearchMemoryParams {
                query: "".to_string(),
                max_results: Some(50),
                memory_type: None,
                traversal_depth: Some(traversal_depth()),
                traversal_breadth: Some(10),
                traversal_strategy: Some("breadth_first".to_string()),
                seed_node_ids: Some(seed_ids.clone()),
                tags: None,
                task_id: None,
                agent_id: aid,
            };
            match search_memory_with_traversal(params).await {
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
                        let valid_ids: HashSet<String> =
                            nodes.read().iter().map(|n| n.id.clone()).collect();
                        map.retain(|id, _| valid_ids.contains(id));
                    }
                    // 展开完成后回填选中节点详情：点击时该节点可能尚未入 detail_map
                    //（如被下方 200 条清理淘汰）；仅当用户仍选中该节点时回填，
                    // 避免覆盖用户已关闭/切换的选择
                    if selected_node_id.read().as_deref() == Some(seed_ids[0].as_str())
                        && let Some(detail) = map.get(&seed_ids[0])
                    {
                        selected_node_data.set(Some(detail.clone()));
                    }
                    detail_map.set(map);

                    let existing_ids: HashSet<String> =
                        nodes.read().iter().map(|n| n.id.clone()).collect();
                    let (mut new_nodes, new_edges) = build_graph_from_results(&data.results);
                    new_nodes.retain(|n| !existing_ids.contains(&n.id));

                    if !new_nodes.is_empty() {
                        let current_nodes = nodes.read().clone();
                        let current_edges = edges.read().clone();
                        let updated_nodes = expand_layout(&current_nodes, &new_nodes, &seed_ids[0]);
                        let mut updated_edges = current_edges;
                        let existing_edge_keys: HashSet<(String, String)> = updated_edges
                            .iter()
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
                    toast.error(format!("加载节点关联失败: {}", e));
                }
            }
            loading.set(false);
        });
    };

    let current_nodes = nodes.read().clone();
    let current_edges = edges.read().clone();
    let selected_id = selected_node_id.read().clone();
    let selected_detail = selected_node_data.read().clone();
    // 只在摘要**独立于正文**时才单独渲染：写入侧此前会把摘要缺省落成正文
    // （或正文前 100 字），详情面板「内容」「摘要」两栏显示同一段话 ——
    // 这正是「字段没有信息量」的直接观感来源。
    let independent_summary = selected_detail.as_ref().and_then(|d| {
        let summary = d
            .summary
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())?;
        (!d.content.trim().starts_with(summary)).then(|| summary.to_string())
    });
    // 关系边详情：两端显示节点名而不是 UUID（端点通常就在图上，查得到）。
    // 裸 ID 对用户没有意义，节点 ID 只在下方「节点 ID」栏出现一次。
    let relation_endpoints = selected_detail
        .as_ref()
        .filter(|d| d.memory_type == "relation")
        .map(|d| {
            let label_of = |id: &str| {
                current_nodes
                    .iter()
                    .find(|n| n.id == id)
                    .map(|n| n.label.clone())
                    .unwrap_or_else(|| id.to_string())
            };
            (
                d.source_node_id.as_deref().map(label_of),
                d.target_node_id.as_deref().map(label_of),
            )
        });

    rsx! {
        div { class: "space-y-4",
            // 推荐起点区域
            {if rec_loading() {
                Some(rsx! {
                    HudPanel { signal: Some(true),
                        div { class: "card-body py-3",
                            span { class: "text-sm text-base-content/70", "正在计算推荐起点..." }
                        }
                    }
                })
            } else if !recommendations().is_empty() {
                {
                    let recs = recommendations();
                    let rec_count = recs.len();
                    Some(rsx! {
                        HudPanel { signal: Some(true),
                            div { class: "card-body py-3",
                                h4 { class: "text-sm font-semibold mb-2", "🎯 推荐起点（按关联度数 Top {rec_count}）" }
                                div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-2",
                                    for rec in recs.iter().cloned() {
                                        {
                                            let nid = rec.node_id.clone();
                                            let name = rec.node_name.clone();
                                            let desc = rec.node_description.clone();
                                            let degree = rec.degree;
                                            let incoming = rec.incoming_count;
                                            let outgoing = rec.outgoing_count;
                                            let tags = rec.tags.clone();
                                            rsx! {
                                                button {
                                                    class: "hud-panel hover:bg-base-300 transition-colors text-left p-2 rounded-lg cursor-pointer",
                                                    onclick: move |_| handle_node_click(nid.clone()),
                                                    div { class: "flex flex-col gap-1",
                                                        span { class: "font-medium text-sm truncate", "{name}" }
                                                        span { class: "text-xs text-base-content/70 line-clamp-2", "{desc}" }
                                                        div { class: "flex flex-wrap gap-1 mt-1",
                                                            span { class: "badge orz-tag badge-sm", "度数 {degree}" }
                                                            span { class: "badge orz-tag badge-sm", "入 {incoming}" }
                                                            span { class: "badge orz-tag badge-sm", "出 {outgoing}" }
                                                            for tag in tags.iter().take(3) {
                                                                span { class: "badge orz-tag badge-sm", "{tag}" }
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
                    })
                }
            } else {
                None
            }}

            // 搜索框区域
            HudPanel { signal: Some(true),
                title: Some("知识图谱".to_string()),
                div { class: "card-body",
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
                            input {
                                class: "input input-bordered sm:w-56",
                                value: "{tags_input}",
                                oninput: move |e| tags_input.set(e.value()),
                                placeholder: "标签过滤（逗号分隔）...",
                                onkeydown: move |evt| {
                                    if evt.key() == Key::Enter {
                                        handle_search(());
                                    }
                                }
                            }
                            // 关联展开层数：搜索命中的节点与「点击节点」共用同一个
                            // traversal_depth，改这里两边同时生效
                            div { class: "join self-center",
                                for d in [1i32, 2, 3] {
                                    button {
                                        class: if traversal_depth() == d {
                                            "btn hud-btn btn-sm join-item btn-primary"
                                        } else {
                                            "btn hud-btn btn-sm join-item btn-ghost"
                                        },
                                        title: "沿关系展开的层数（搜索命中节点 / 点击节点时生效）",
                                        onclick: move |_| traversal_depth.set(d),
                                        "{d} 跳"
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
                                                class: "btn hud-btn btn-xs btn-ghost",
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
                                span { class: "text-base-content/70", "点击节点展开关联知识 · 选中节点显示扫描环 · 未选中节点呼吸光晕" }
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
                                p { class: "text-xs text-base-content/60 mt-2",
                                    "实线边流光表示活跃数据流向 · 虚线边表示引用/依赖关系 · 选中节点的关联边流光加速" }
                            }
                        }
                    }
                }
            }

            // 图谱视图 + 节点详情
            // 加载态不再卸载图谱：首次搜索（尚无节点）时显示 Loading；
            // 已有节点时保持图谱挂载，仅叠加遮罩 spinner，避免 CanvasScene
            // 随整树卸载重挂载引发 RAF Closure 竞态与页面闪现
            if current_nodes.is_empty() {
                if loading() {
                    Loading {}
                } else {
                    EmptyState { message: "开始搜索知识节点或点击推荐起点".to_string() }
                }
            } else {
                div { class: "relative flex flex-col lg:flex-row gap-4",
                    // 展开节点期间的加载遮罩（图谱保持挂载，不中断 RAF 渲染循环）
                    {if loading() {
                        Some(rsx! {
                            div { class: "absolute inset-0 z-30 flex flex-col items-center justify-center gap-2 bg-base-100/60 backdrop-blur-sm rounded-box",
                                span { class: "loading loading-spinner loading-md text-primary" }
                                span { class: "text-sm text-base-content/70", "正在加载关联数据..." }
                            }
                        })
                    } else {
                        None
                    }}
                    div { class: "flex-1 min-h-[600px]",
                    HudPanel { signal: Some(true), extra_class: Some("h-full".to_string()),
                        div { class: "card-body",
                            {
                                let canvas_btn_class = if graph_style() == GraphStyle::Canvas { "btn btn-xs join-item btn-primary" } else { "btn btn-xs join-item btn-ghost" };
                                let svg_btn_class = if graph_style() == GraphStyle::Svg { "btn btn-xs join-item btn-primary" } else { "btn btn-xs join-item btn-ghost" };
                                rsx! {
                                    HudSection { title: format!("图谱视图 ({} 节点, {} 关系)", current_nodes.len(), current_edges.len()),
                                        actions: Some(rsx!{
                                            // 视口操作提示：滚轮/拖拽平移没有天然的视觉线索
                                            // （右下角只显示缩放百分比），一句话说明最省事；
                                            // 线粗这层编码也要点一句，否则「有的线更粗」会被
                                            // 当成渲染抖动 —— 数值本身留给 hover
                                            span { class: "text-xs text-base-content/50 whitespace-nowrap hidden sm:inline",
                                                "Ctrl/⌘+滚轮缩放 · 拖拽空白平移 · 线越粗关联越强"
                                            }
                                            // 风格切换按钮：Canvas（HUD）/ SVG（兜底）
                                            div { class: "join",
                                                button {
                                                    class: "{canvas_btn_class}",
                                                    onclick: move |_| graph_style.set(GraphStyle::Canvas),
                                                    title: "Canvas HUD 风格（高级渲染，适合大规模节点）",
                                                    "Canvas"
                                                }
                                                button {
                                                    class: "{svg_btn_class}",
                                                    onclick: move |_| graph_style.set(GraphStyle::Svg),
                                                    title: "SVG 风格（兜底方案，适合少量节点）",
                                                    "SVG"
                                                }
                                            }
                                        }),
                                    }
                                }
                            }
                            {match graph_style() {
                                GraphStyle::Canvas => rsx! {
                                    KnowledgeGraphCanvas {
                                        nodes: current_nodes,
                                        edges: current_edges,
                                        selected_node_id: selected_id,
                                        highlighted_node_ids: Some(highlighted_node_ids()),
                                        on_node_click: handle_node_click,
                                    }
                                },
                                GraphStyle::Svg => rsx! {
                                    Graph {
                                        nodes: current_nodes,
                                        edges: current_edges,
                                        selected_node_id: selected_id,
                                        highlighted_node_ids: Some(highlighted_node_ids()),
                                        on_node_click: handle_node_click,
                                    }
                                },
                            }}
                        }
                    }
                }

                    if let Some(detail) = &selected_detail {
                        // 桌面端浮动覆盖（类无边记）：absolute 不占 flex 布局位，点击节点
                        // 展开详情时左侧画布宽度不变，节点不再被挤压形变；内容超长时面板
                        // 内部滚动。移动端维持原有的上下堆叠流式布局
                        div { class: "w-full lg:absolute lg:inset-y-0 lg:right-0 lg:z-20 lg:w-96 lg:shadow-2xl",
                            HudPanel { signal: Some(true), extra_class: Some("h-full overflow-y-auto".to_string()),
                                div { class: "card-body",
                                    HudSection { title: "节点详情".to_string(),
                                        actions: Some(rsx!{
                                            button {
                                                class: "btn hud-btn btn-ghost btn-sm btn-circle",
                                                onclick: move |_| {
                                                    selected_node_id.set(None);
                                                    selected_node_data.set(None);
                                                },
                                                "✕"
                                            }
                                        }),
                                    }
                                    div { class: "space-y-4",
                                        // 知识节点的真实名称（node_name）：图谱卡片第一行用的就是它，
                                        // 详情面板必须能对上；无名称的类型（短期记忆/调用记录）不显示
                                        if let Some(name) = detail.name.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                                            div {
                                                label { class: "label",
                                                    span { class: "label-text font-medium", "名称" }
                                                }
                                                div { class: "p-2 bg-base-200 rounded-lg",
                                                    span { class: "text-sm font-medium break-words", "{name}" }
                                                }
                                            }
                                        }
                                        div { class: "grid grid-cols-2 gap-4",
                                            div {
                                                label { class: "label",
                                                    span { class: "label-text font-medium", "类型" }
                                                }
                                                span { class: "{type_badge_class(&detail.memory_type)}", "{type_label(&detail.memory_type)}" }
                                            }
                                            div {
                                                label { class: "label",
                                                    span { class: "label-text font-medium", "相关度" }
                                                }
                                                div { class: "flex items-center gap-2",
                                                    span { class: "font-mono text-sm",
                                                        // 遍历展开出来的邻居/关系边没有匹配过程，留空比写
                                                        // 「N/A」诚实 —— 后者会被读成「匹配度 0」
                                                        match detail.score {
                                                            Some(score) => format_relevance(score),
                                                            None => "—".to_string(),
                                                        }
                                                    }
                                                    if let Some(m) = detail.search_match.as_ref() {
                                                        span { class: "badge orz-tag badge-sm",
                                                            "{match_type_label(&m.match_type)}"
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        // 关系边没有独立正文，DB 里只有 relation_type：
                                        // 用「关系类型 + 两端节点名」把这条边讲清楚
                                        if let Some((src, tgt)) = relation_endpoints.as_ref() {
                                            div {
                                                label { class: "label",
                                                    span { class: "label-text font-medium", "关系" }
                                                }
                                                div { class: "p-2 bg-base-200 rounded-lg text-sm",
                                                    span { class: "font-medium", "{detail.name.clone().unwrap_or_default()}" }
                                                    if let (Some(src), Some(tgt)) = (src, tgt) {
                                                        span { class: "text-base-content/70",
                                                            "  {src} → {tgt}"
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        // id 不上图：点击节点后在详情面板查看
                                        div {
                                            label { class: "label",
                                                span { class: "label-text font-medium", "节点 ID" }
                                            }
                                            div { class: "p-2 bg-base-200 rounded-lg font-mono text-xs break-all text-base-content/70",
                                                "{detail.id}"
                                            }
                                        }

                                        // 关系边的「内容」就是关系类型本身（上面「关系」栏已展示），
                                        // 再渲染一遍只是同一行字重复出现
                                        if detail.memory_type != "relation" {
                                            div {
                                                label { class: "label",
                                                    span { class: "label-text font-medium", "内容" }
                                                }
                                                div { class: "p-3 bg-base-200 rounded-lg",
                                                    MarkdownRenderer { content: detail.content.clone(), compact: true }
                                                }
                                            }
                                        }

                                        if let Some(summary) = &independent_summary {
                                            div {
                                                label { class: "label",
                                                    span { class: "label-text font-medium", "摘要" }
                                                }
                                                div { class: "p-3 bg-base-200 rounded-lg text-base-content/70",
                                                    MarkdownRenderer { content: summary.clone(), compact: true }
                                                }
                                            }
                                        }

                                        if let Some(tags) = &detail.tags {
                                            if !tags.is_empty() {
                                                div {
                                                    label { class: "label",
                                                        span { class: "label-text font-medium", "标签" }
                                                    }
                                                    div { class: "flex flex-wrap gap-2",
                                                        for tag in tags.iter() {
                                                            span { class: "badge orz-tag badge-sm", "{tag}" }
                                                        }
                                                    }
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
                                                class: "btn hud-btn btn-outline btn-sm",
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

/// 知识图谱路由入口：AppLayout + Agent 选择器 + KnowledgeGraph
#[component]
pub fn HrKnowledgeGraph() -> Element {
    let mut selected_agent_id = use_signal(|| None::<String>);
    let mut agent_search_results = use_signal(Vec::<AgentListItem>::new);
    let mut agent_search_loading = use_signal(|| false);
    let toast = use_toast();

    let mut handle_agent_search = move |keyword: String| {
        if keyword.trim().is_empty() {
            agent_search_results.set(Vec::new());
            return;
        }
        agent_search_loading.set(true);
        spawn(async move {
            let req = SearchAgentsRequest {
                keyword: Some(keyword),
                ..Default::default()
            };
            match search_agents(&req).await {
                Ok(resp) => agent_search_results.set(resp.items),
                Err(e) => toast.error(format!("搜索 Agent 失败: {}", e)),
            }
            agent_search_loading.set(false);
        });
    };

    let mut handle_agent_select = move |selection: String| {
        let agent_id = if let Some(id_start) = selection.rfind('(') {
            selection[id_start + 1..selection.len() - 1].to_string()
        } else {
            selection
        };
        selected_agent_id.set(Some(agent_id));
    };

    let agent_options = agent_search_results
        .read()
        .iter()
        .map(|a| format!("{} ({})", a.name, a.id))
        .collect::<Vec<String>>();

    rsx! {
        AppLayout {
            div { class: "space-y-4",
                HudPanel { signal: Some(true),
                    div { class: "card-body py-3",
                        div { class: "flex gap-2 items-center",
                            span { class: "text-sm font-medium whitespace-nowrap", "Agent:" }
                            div { class: "flex-1 max-w-md",
                                SearchableSelect {
                                    placeholder: "选择 Agent（留空=全蜂巢知识图谱）...".to_string(),
                                    selected: None,
                                    options: agent_options,
                                    on_select: move |selection: String| handle_agent_select(selection),
                                    on_search: Some(EventHandler::new(move |kw: String| handle_agent_search(kw))),
                                    loading: *agent_search_loading.read(),
                                }
                            }
                            if selected_agent_id().is_some() {
                                button {
                                    class: "btn hud-btn btn-ghost btn-sm",
                                    onclick: move |_| {
                                        selected_agent_id.set(None);
                                    },
                                    "✕ 清除"
                                }
                            }
                        }
                        // 语义说明：知识节点是蜂巢共享资产（所有 Agent 都能看到全部节点），
                        // Agent 选择器只收窄「起点」，不改变可见性 —— 不写清楚的话，
                        // 用户会以为选 Agent 等于「只看它的节点」而错过别的 Agent 的知识。
                        p { class: "text-xs text-base-content/50 mt-1.5",
                            "知识节点在蜂巢内全局共享（任何 Agent 都能检索到全部节点）；选择 Agent 只把关键词搜索的起点收窄到它，沿图展开不受限制。"
                        }
                    }
                }

                KnowledgeGraph { agent_id: selected_agent_id() }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, name: &str) -> MemoryResult {
        MemoryResult {
            id: id.to_string(),
            name: Some(name.to_string()),
            content: format!("{name}的正文"),
            memory_type: "knowledge_node".to_string(),
            score: None,
            summary: None,
            source_node_id: None,
            target_node_id: None,
            relation_type: None,
            weight: None,
            tags: None,
            search_match: None,
        }
    }

    fn edge(id: &str, src: &str, tgt: &str) -> MemoryResult {
        MemoryResult {
            id: id.to_string(),
            name: Some("导致".to_string()),
            content: "导致".to_string(),
            memory_type: "relation".to_string(),
            score: None,
            summary: None,
            source_node_id: Some(src.to_string()),
            target_node_id: Some(tgt.to_string()),
            relation_type: Some("causes".to_string()),
            weight: Some(0.8),
            tags: None,
            search_match: None,
        }
    }

    /// 图批次不变式（第二道防线）：端点缺失的边**整条不画**，也不为端点造占位节点。
    ///
    /// 旧实现会给缺失端点造「未命名节点」占位卡（没有名称 / 正文 / hover 内容），
    /// 于是整屏「未命名节点」—— 这正是知识图谱页最初的问题形态。
    #[test]
    fn dangling_edges_are_dropped_without_placeholder_nodes() {
        let results = vec![
            node("kn_a", "订单状态机"),
            edge("kr_1", "kn_a", "kn_b"), // kn_b 不在本批结果里
        ];
        let (nodes, edges) = build_graph_from_results(&results);
        assert_eq!(nodes.len(), 1, "不该为缺失端点造占位节点: {nodes:?}");
        assert_eq!(nodes[0].id, "kn_a");
        assert!(edges.is_empty(), "端点缺失的边必须整条丢弃: {edges:?}");

        // 两端都在 → 正常出边，且不重复造节点
        let results = vec![
            node("kn_a", "订单状态机"),
            node("kn_b", "订单超时补偿"),
            edge("kr_1", "kn_a", "kn_b"),
        ];
        let (nodes, edges) = build_graph_from_results(&results);
        assert_eq!(nodes.len(), 2, "端点已是实体节点，不该再造占位: {nodes:?}");
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].source, "kn_a");
        assert_eq!(edges[0].target, "kn_b");
        assert_eq!(edges[0].weight, Some(0.8));
    }

    /// 名称在数据层保持完整：`node_display_name` 不再 14 字符截断，
    /// hover 详情卡 / SVG 兜底 / 边端点名才能拿到完整文本（截断只留在渲染处 title()）。
    #[test]
    fn display_name_keeps_full_name_without_truncation() {
        let long_name = "超长知识节点名称用于验证hover弹窗标题完整显示";
        let item = node("kn_long", long_name);
        assert_eq!(node_display_name(&item), long_name);

        let (nodes, _) = build_graph_from_results(&[node("kn_long", long_name)]);
        assert_eq!(
            nodes[0].label, long_name,
            "GraphNode.label 不应在数据构建层被截断"
        );

        // 兜底名（正文首行）同样完整保留，不再截断
        let first_line = format!("{}{}", "第一行正文", "很长".repeat(9));
        let mut by_content = node("kn_c", "");
        by_content.name = None;
        by_content.content = format!("{first_line}\n第二行");
        assert_eq!(node_display_name(&by_content), first_line);
    }

    /// 「未命名节点」只兜底「实体节点字段全空」，不是端点占位的产物。
    #[test]
    fn empty_entity_falls_back_to_unnamed_label() {
        let mut blank = node("kn_z", "");
        blank.name = None;
        blank.content = String::new();
        let (nodes, _) = build_graph_from_results(&[blank]);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].label, "未命名节点");
    }
}
