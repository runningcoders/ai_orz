//! 思考路径图数据映射（纯函数，零 Dioxus 依赖，可单测）
//!
//! # 为什么单独一层
//!
//! 思考轨迹的落盘数据（`ThinkTraceListItem.occurrences` / `routes`）是**结构化执行
//! 记录**：一个图节点可能在一轮 run 里被执行多次（Deliberate 多轮、回环重入），
//! 每条 occurrence 只记「第几步执行了哪个节点」，routes 只记「第几步在哪些候选里
//! 选了谁」。而图视图要的是**去重后的节点 + 聚合后的转移边**。
//!
//! 这层映射放在纯函数里，好处有三：
//! - 可单测（不依赖 wasm / DOM / 组件生命周期）；
//! - 与渲染栈解耦（同一份映射未来给 Canvas 栈复用不必重写）；
//! - 老记录全 Option 容错（空 occurrences / 空 routes / 缺字段一律降级不 panic）。
//!
//! # 映射口径（与实施单 a) 部分逐条对应）
//!
//! - 节点 = **去重后的 node 名**（一个节点一张卡，绝不按 seq 铺开）；
//!   重复出现折算成卡片摘要「×N」+ 标签（signal 集合）+ hover 明细行；
//! - 边 = `occurrences` **相邻对**派生的转移边，同 `(from, to)` 聚合成一条，
//!   标签「×N · s2,5,9」（步号超 3 个用 `…` 收尾）；
//! - 边属性由 `routes` 派生：`via` → 样式（Advisor 粗线 / Fallback 虚线 /
//!   Unique·Deterministic 实线），`reason` → 标签尾（hover 可见全量），
//!   `candidates` → 「候选 k」；
//! - `chosen` 与相邻对不一致时**以 occurrence 为准**（occurrence 是实际执行事实，
//!   route 是调度决策记录），不一致次数只做统计不打乱图结构；
//! - 回环自然涌现：同节点连续出现 = 自环；`A→B→A` = 双向弧线对（不重叠）。

use std::collections::HashMap;

use common::api::{ThinkTraceOccurrenceDto, ThinkTraceRouteDto};

use crate::components::graph::{EdgeCurve, GraphEdge, GraphNode};

/// 思考节点在图引擎里的语义 token（配色见 `graph::get_node_fill`，
/// hover 文案见 `node_card::type_label`）
pub const THINK_STEP_NODE_TYPE: &str = "think_step";

/// 边标签里最多展开的步号个数（超出用 `…` 收尾；完整步号见 hover 的标签全文）
pub const EDGE_LABEL_MAX_STEPS: usize = 3;

/// 单节点聚合结果（一节点一卡的数据来源）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThinkNodeAgg {
    /// 节点名（去重后的图节点身份）
    pub node: String,
    /// 本次 run 内执行次数（卡片「×N」）
    pub count: usize,
    /// 首次出现序号（列表序，仅作展示/排序参考）
    pub first_seq: usize,
    /// 各次出现所属图步号（按出现顺序，可重复）
    pub steps: Vec<usize>,
    /// 节点产出信号集合（去重保序，卡片标签来源）
    pub signals: Vec<String>,
    /// hover 明细行：`seq#1 @step#2 · signal`
    pub details: Vec<String>,
}

/// 单条转移边（已按 `(from, to)` 聚合）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThinkTransition {
    pub source: String,
    pub target: String,
    /// 同向转移发生次数（边标签「×N」）
    pub count: usize,
    /// 各次转移的目标步号（按出现顺序，可重复）
    pub steps: Vec<usize>,
    /// 各次转移的选路来源（可能重复，取首个非空作边样式依据）
    pub vias: Vec<String>,
    /// 各次转移的选路原因（取首个非空作标签尾）
    pub reasons: Vec<String>,
    /// 各次转移的候选数量
    pub candidate_counts: Vec<usize>,
    /// `route.chosen` 与 occurrence 相邻对不一致的次数（以 occurrence 为准）
    pub mismatched: usize,
}

/// 由 `via` 派生的边样式（引擎侧 `width` / `dash` 通道）
#[derive(Debug, Clone, PartialEq)]
pub struct ViaStyle {
    /// 显式线宽（None = 走引擎默认基准粗细）
    pub width: Option<f64>,
    /// 显式虚线（None = 实线 + 流光动画）
    pub dash: Option<String>,
}

/// 边样式图例（画布角落 4~5 条，纯文案，顺序即渲染顺序）
pub fn edge_style_legend() -> Vec<&'static str> {
    vec![
        "→ 转移（×N = 次数）",
        "粗线 = Advisor 选路",
        "虚线 = Fallback 兜底",
        "弧线 = 回环 / 双向",
        "自环 = 同节点连续执行",
    ]
}

/// `via` → 边样式
///
/// 口径：`advisor` 加粗（人在环决策值得更显眼）、`fallback` 虚线（兜底路径弱化）、
/// `unique` / `deterministic` / 未知一律实线基准粗细（确定性主路径不需要额外标记）。
pub fn edge_style_from_via(via: Option<&str>) -> ViaStyle {
    match via.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        Some("advisor") => ViaStyle {
            width: Some(2.4),
            dash: None,
        },
        Some("fallback") => ViaStyle {
            width: None,
            dash: Some("5,4".to_string()),
        },
        _ => ViaStyle {
            width: None,
            dash: None,
        },
    }
}

/// `via` → 中文短语（边标签用；未知/空返回空串不占位）
pub fn via_phrase(via: &str) -> &'static str {
    match via.trim().to_ascii_lowercase().as_str() {
        "unique" => "唯一",
        "deterministic" => "确定性",
        "advisor" => "Advisor",
        "fallback" => "兜底",
        _ => "",
    }
}

/// 去重后的节点名（保持首次出现顺序）
pub fn dedup_nodes(occurrences: &[ThinkTraceOccurrenceDto]) -> Vec<String> {
    let mut seen: HashMap<&str, ()> = HashMap::new();
    let mut out: Vec<String> = Vec::new();
    for o in occurrences {
        let name = o.node.trim();
        if name.is_empty() {
            // 缺失节点名的脏记录：不建节点（否则会画出一张没有名字的卡）
            continue;
        }
        if seen.insert(name, ()).is_none() {
            out.push(name.to_string());
        }
    }
    out
}

/// 单节点聚合（次数 / 步号 / signal 集合 / hover 明细）
pub fn aggregate_nodes(occurrences: &[ThinkTraceOccurrenceDto]) -> Vec<ThinkNodeAgg> {
    let mut order: Vec<String> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut aggs: Vec<ThinkNodeAgg> = Vec::new();
    for o in occurrences {
        let name = o.node.trim();
        if name.is_empty() {
            continue;
        }
        let signal = o.signal.as_deref().map(str::trim).unwrap_or("");
        let detail = if signal.is_empty() {
            format!("seq#{} @step#{}", o.seq, o.step)
        } else {
            format!("seq#{} @step#{} · {}", o.seq, o.step, signal)
        };
        match index.get(name) {
            Some(&i) => {
                let agg = &mut aggs[i];
                agg.count += 1;
                agg.steps.push(o.step);
                agg.details.push(detail);
                if !signal.is_empty() && !agg.signals.iter().any(|s| s == signal) {
                    agg.signals.push(signal.to_string());
                }
            }
            None => {
                index.insert(name.to_string(), aggs.len());
                order.push(name.to_string());
                aggs.push(ThinkNodeAgg {
                    node: name.to_string(),
                    count: 1,
                    first_seq: o.seq,
                    steps: vec![o.step],
                    signals: if signal.is_empty() {
                        Vec::new()
                    } else {
                        vec![signal.to_string()]
                    },
                    details: vec![detail],
                });
            }
        }
    }
    aggs
}

/// `occurrences` 相邻对 → 逐条转移（未聚合，count 恒 1）
///
/// 回环口径：同一节点连续出现 ⇒ `source == target`（自环），由调用方按
/// `EdgeCurve::Loop` 绘制；`A→B→A` 天然产生两条方向相反的边。
pub fn raw_transitions(
    occurrences: &[ThinkTraceOccurrenceDto],
    routes: &[ThinkTraceRouteDto],
) -> Vec<ThinkTransition> {
    let mut out: Vec<ThinkTransition> = Vec::new();
    for pair in occurrences.windows(2) {
        let from = pair[0].node.trim();
        let to = pair[1].node.trim();
        if from.is_empty() || to.is_empty() {
            continue;
        }
        // 目标步号：转移落点在「执行到 to 的那一步」，故取 pair[1].step
        let step = pair[1].step;
        let route = routes
            .iter()
            .find(|r| r.step == step && r.chosen.trim() == to);
        let chosen_mismatch = routes
            .iter()
            .find(|r| r.step == step)
            .is_some_and(|r| r.chosen.trim() != to);
        out.push(ThinkTransition {
            source: from.to_string(),
            target: to.to_string(),
            count: 1,
            steps: vec![step],
            vias: route.map(|r| r.via.clone()).into_iter().collect(),
            reasons: route
                .and_then(|r| r.reason.clone())
                .filter(|r| !r.trim().is_empty())
                .into_iter()
                .collect(),
            candidate_counts: route.map(|r| r.candidates.len()).into_iter().collect(),
            mismatched: usize::from(chosen_mismatch),
        });
    }
    out
}

/// 同 `(from, to)` 聚合（保持首次出现顺序）
pub fn merge_parallel_edges(raw: Vec<ThinkTransition>) -> Vec<ThinkTransition> {
    let mut order: Vec<(String, String)> = Vec::new();
    let mut index: HashMap<(String, String), usize> = HashMap::new();
    let mut out: Vec<ThinkTransition> = Vec::new();
    for t in raw {
        let key = (t.source.clone(), t.target.clone());
        match index.get(&key) {
            Some(&i) => {
                let agg = &mut out[i];
                agg.count += t.count;
                agg.steps.extend(t.steps);
                agg.vias.extend(t.vias);
                agg.reasons.extend(t.reasons);
                agg.candidate_counts.extend(t.candidate_counts);
                agg.mismatched += t.mismatched;
            }
            None => {
                index.insert(key.clone(), out.len());
                order.push(key);
                out.push(t);
            }
        }
    }
    out
}

/// `occurrences` + `routes` → 聚合后的转移边
pub fn derive_transitions(
    occurrences: &[ThinkTraceOccurrenceDto],
    routes: &[ThinkTraceRouteDto],
) -> Vec<ThinkTransition> {
    merge_parallel_edges(raw_transitions(occurrences, routes))
}

/// 步号列表 → 紧凑文案（最多 `max` 个，超出 `…` 收尾）
pub fn join_steps(steps: &[usize], max: usize) -> String {
    let mut out = String::new();
    for (i, s) in steps.iter().take(max).enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&s.to_string());
    }
    if steps.len() > max {
        out.push('…');
    }
    out
}

/// 转移边标签（画布上会被截断到 10 字符，hover 里可读全文）
///
/// 形如 `×2 · s3,7 · Advisor · 候选3 · 因:...`
pub fn edge_label(t: &ThinkTransition) -> String {
    let mut s = format!(
        "×{} · s{}",
        t.count,
        join_steps(&t.steps, EDGE_LABEL_MAX_STEPS)
    );
    let via = t
        .vias
        .iter()
        .find(|v| !v.trim().is_empty())
        .map(|v| v.as_str());
    if let Some(v) = via {
        let phrase = via_phrase(v);
        if !phrase.is_empty() {
            s.push_str(" · ");
            s.push_str(phrase);
        }
    }
    if let Some(&c) = t.candidate_counts.iter().max()
        && c > 1
    {
        s.push_str(&format!(" · 候选{c}"));
    }
    if let Some(r) = t.reasons.iter().find(|r| !r.trim().is_empty()) {
        let short: String = r.trim().chars().take(24).collect();
        if !short.is_empty() {
            s.push_str(&format!(" · 因:{short}"));
        }
    }
    if t.mismatched > 0 {
        s.push_str(&format!(" · 记录不一致×{}", t.mismatched));
    }
    s
}

/// 节点卡片摘要：`×N · step 2,5,9`（单次出现不写 ×1，保持卡片干净）
pub fn node_summary(agg: &ThinkNodeAgg) -> String {
    if agg.count <= 1 {
        format!("step {}", join_steps(&agg.steps, 4))
    } else {
        format!("×{} · step {}", agg.count, join_steps(&agg.steps, 4))
    }
}

/// 节点卡片 hover 明细（多次出现逐条列出）
pub fn node_detail(agg: &ThinkNodeAgg) -> String {
    if agg.count <= 1 {
        agg.details.first().cloned().unwrap_or_default()
    } else {
        format!("出现 {} 次：{}", agg.count, agg.details.join(" / "))
    }
}

/// 分层布局配置（比例与任务依赖图同源：`chat_side_panel::build_task_graph_data`）
pub fn layout_config(
    width: f64,
    height: f64,
) -> crate::components::layered_layout::LayeredLayoutConfig {
    crate::components::layered_layout::LayeredLayoutConfig {
        width,
        height,
        top_margin: height * 0.11,
        layer_height: height * 0.22,
        side_margin: width * 0.09,
    }
}

/// 思考路径图构建结果
#[derive(Debug, Clone, PartialEq)]
pub struct ThinkPathGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    /// 节点聚合（列表降级视图复用；与 `nodes` 一一对应）
    pub aggs: Vec<ThinkNodeAgg>,
    /// 转移聚合（列表降级视图复用；与 `edges` 一一对应）
    pub transitions: Vec<ThinkTransition>,
}

/// 思考路径图构建入口（纯函数：给定数据与画布尺寸，返回可直接喂给 `Graph` 的数据）
///
/// 容错：空 `occurrences` → 空图（调用方降级为徽章列表）；空 `routes` → 只画转移边；
/// 缺字段 / 空节点名一律跳过，全程零 panic。
pub fn build_think_path_graph(
    occurrences: &[ThinkTraceOccurrenceDto],
    routes: &[ThinkTraceRouteDto],
    width: f64,
    height: f64,
) -> ThinkPathGraph {
    let aggs = aggregate_nodes(occurrences);
    let transitions = derive_transitions(occurrences, routes);
    // 节点身份 = 去重后的 node 名（一节点一卡，绝不按 seq 铺开）
    let node_ids = dedup_nodes(occurrences);
    if node_ids.is_empty() {
        return ThinkPathGraph {
            nodes: Vec::new(),
            edges: Vec::new(),
            aggs,
            transitions,
        };
    }

    // 布局依赖方向：转移 A→B ⇒ B 依赖 A（前置在上、后继在下，与任务依赖图同款）
    let mut deps_map: HashMap<String, Vec<String>> = HashMap::new();
    for id in &node_ids {
        deps_map.insert(id.clone(), Vec::new());
    }
    for t in &transitions {
        if let Some(preds) = deps_map.get_mut(&t.target)
            && t.source != t.target
            && !preds.iter().any(|p| p == &t.source)
        {
            preds.push(t.source.clone());
        }
    }
    let config = layout_config(width, height);
    let positions =
        crate::components::layered_layout::compute_layered_layout(&node_ids, &deps_map, &config);

    let nodes: Vec<GraphNode> = aggs
        .iter()
        .map(|a| {
            let (_, x, y) =
                positions
                    .get(&a.node)
                    .copied()
                    .unwrap_or((0, width / 2.0, height / 2.0));
            GraphNode {
                id: a.node.clone(),
                label: a.node.clone(),
                description: node_detail(a),
                node_type: THINK_STEP_NODE_TYPE.to_string(),
                x,
                y,
                tags: a.signals.clone(),
                summary: Some(node_summary(a)),
            }
        })
        .collect();

    let mut edges: Vec<GraphEdge> = transitions
        .iter()
        .map(|t| {
            let via = t
                .vias
                .iter()
                .find(|v| !v.trim().is_empty())
                .map(|v| v.as_str());
            let style = edge_style_from_via(via);
            GraphEdge {
                source: t.source.clone(),
                target: t.target.clone(),
                label: edge_label(t),
                weight: None,
                direction: "directed".to_string(),
                curve: Some(EdgeCurve::Straight),
                dash: style.dash,
                width: style.width,
            }
        })
        .collect();

    // 曲率：回边 / 双向成对边走弧线，自环绕行；上限 0.3×层间距
    let layer_gap = config.layer_height;
    assign_edge_curves(&nodes, &mut edges, layer_gap);

    ThinkPathGraph {
        nodes,
        edges,
        aggs,
        transitions,
    }
}

/// 曲率分派（回环 / 双向 / 同层回边 → 弧线，其余直线）
///
/// - 自环（`source == target`）→ `EdgeCurve::Loop`；
/// - 同一对节点存在双向边 → 两条都走弧线且**符号相反**（分居连线两侧，不重叠）；
/// - 其余：目标层不低于源层（同层或向上回边）→ 弧线绕行；正常向下 → 直线。
///
/// 曲率绝对值 clamp 到 `layer_gap * 0.3`（避免弧线跨层挤压相邻节点）。
pub fn assign_edge_curves(nodes: &[GraphNode], edges: &mut [GraphEdge], layer_gap: f64) {
    let pos: HashMap<&str, (f64, f64)> =
        nodes.iter().map(|n| (n.id.as_str(), (n.x, n.y))).collect();
    // 无序对 → 方向数
    let mut pair_dirs: HashMap<(String, String), usize> = HashMap::new();
    for e in edges.iter() {
        if e.source == e.target {
            continue;
        }
        let key = if e.source <= e.target {
            (e.source.clone(), e.target.clone())
        } else {
            (e.target.clone(), e.source.clone())
        };
        *pair_dirs.entry(key).or_insert(0) += 1;
    }
    let max_bulge = (layer_gap * 0.3).max(6.0);
    let base = (layer_gap * 0.45).max(10.0);
    let clamp = |v: f64| v.clamp(-max_bulge, max_bulge);

    for e in edges.iter_mut() {
        if e.source == e.target {
            e.curve = Some(EdgeCurve::Loop);
            continue;
        }
        let key = if e.source <= e.target {
            (e.source.clone(), e.target.clone())
        } else {
            (e.target.clone(), e.source.clone())
        };
        if pair_dirs.get(&key).copied().unwrap_or(0) > 1 {
            // 双向边：按节点名定号，保证两个方向落在不同侧
            let sign = if e.source <= e.target { 1.0 } else { -1.0 };
            e.curve = Some(EdgeCurve::Arc(clamp(sign * base)));
            continue;
        }
        let (sy, ty) = match (pos.get(e.source.as_str()), pos.get(e.target.as_str())) {
            (Some(s), Some(t)) => (s.1, t.1),
            _ => (0.0, 1.0),
        };
        if ty <= sy + 1.0 {
            // 同层或向上回边：弧线绕行（直线会压在其他节点上）
            e.curve = Some(EdgeCurve::Arc(clamp(base * 0.6)));
        } else {
            e.curve = Some(EdgeCurve::Straight);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn occ(node: &str, seq: usize, step: usize, signal: Option<&str>) -> ThinkTraceOccurrenceDto {
        ThinkTraceOccurrenceDto {
            node: node.to_string(),
            seq,
            step,
            signal: signal.map(|s| s.to_string()),
        }
    }

    fn route(
        step: usize,
        candidates: &[&str],
        chosen: &str,
        via: &str,
        reason: Option<&str>,
    ) -> ThinkTraceRouteDto {
        ThinkTraceRouteDto {
            step,
            candidates: candidates.iter().map(|c| c.to_string()).collect(),
            chosen: chosen.to_string(),
            via: via.to_string(),
            reason: reason.map(|r| r.to_string()),
        }
    }

    #[test]
    fn dedup_keeps_first_seen_order() {
        let occs = vec![
            occ("Triage", 1, 1, None),
            occ("Deliberate", 1, 2, None),
            occ("Triage", 2, 3, None),
            occ("", 0, 4, None),
        ];
        assert_eq!(dedup_nodes(&occs), vec!["Triage", "Deliberate"]);
    }

    #[test]
    fn aggregate_counts_repeats_and_collects_signals() {
        let occs = vec![
            occ("Deliberate", 1, 2, Some("plan")),
            occ("Deliberate", 2, 3, Some("plan")),
            occ("Deliberate", 3, 4, Some("act")),
        ];
        let aggs = aggregate_nodes(&occs);
        assert_eq!(aggs.len(), 1);
        let a = &aggs[0];
        assert_eq!(a.count, 3, "同名节点必须聚合成一卡，用 ×N 表达次数");
        assert_eq!(a.steps, vec![2, 3, 4]);
        assert_eq!(a.signals, vec!["plan", "act"], "signal 去重保序");
        assert_eq!(a.details.len(), 3);
        assert!(
            a.details[0].contains("seq#1 @step#2 · plan"),
            "{:?}",
            a.details
        );
    }

    #[test]
    fn transitions_derive_from_adjacent_pairs_and_merge() {
        let occs = vec![
            occ("Triage", 1, 1, None),
            occ("Deliberate", 1, 2, None),
            occ("Triage", 2, 3, None),
            occ("Deliberate", 2, 4, None),
        ];
        let routes = vec![
            route(
                2,
                &["Triage", "Deliberate"],
                "Deliberate",
                "advisor",
                Some("需要多轮推理"),
            ),
            route(3, &["Triage"], "Triage", "unique", None),
            route(
                4,
                &["Triage", "Deliberate"],
                "Deliberate",
                "deterministic",
                None,
            ),
        ];
        let ts = derive_transitions(&occs, &routes);
        // 三条相邻对 → 两个方向各聚合一条
        let forward = ts
            .iter()
            .find(|t| t.source == "Triage" && t.target == "Deliberate")
            .unwrap();
        assert_eq!(forward.count, 2);
        assert_eq!(forward.steps, vec![2, 4]);
        assert_eq!(forward.mismatched, 0);
        let back = ts
            .iter()
            .find(|t| t.source == "Deliberate" && t.target == "Triage")
            .unwrap();
        assert_eq!(back.count, 1);
        assert_eq!(back.vias, vec!["unique"]);
    }

    #[test]
    fn self_loop_emerges_from_consecutive_same_node() {
        let occs = vec![occ("Deliberate", 1, 2, None), occ("Deliberate", 2, 3, None)];
        let ts = derive_transitions(&occs, &[]);
        assert_eq!(ts.len(), 1);
        assert_eq!(ts[0].source, ts[0].target);
    }

    #[test]
    fn chosen_mismatch_defers_to_occurrence() {
        let occs = vec![occ("A", 1, 1, None), occ("B", 1, 2, None)];
        // 记录说选了 C，但实际执行到了 B ⇒ 图按 occurrence 画 A→B，并标记不一致
        let routes = vec![route(2, &["B", "C"], "C", "advisor", None)];
        let ts = derive_transitions(&occs, &routes);
        assert_eq!(ts.len(), 1);
        assert_eq!((ts[0].source.as_str(), ts[0].target.as_str()), ("A", "B"));
        assert_eq!(ts[0].mismatched, 1);
    }

    #[test]
    fn empty_occurrences_yield_empty_graph() {
        let g = build_think_path_graph(&[], &[], 300.0, 200.0);
        assert!(g.nodes.is_empty());
        assert!(g.edges.is_empty());
        // 有 routes 但无 occurrences：仍不建图（无节点可画），不 panic
        let g2 =
            build_think_path_graph(&[], &[route(2, &["A"], "A", "unique", None)], 300.0, 200.0);
        assert!(g2.nodes.is_empty());
    }

    #[test]
    fn edge_label_collapses_steps_and_carries_via() {
        let t = ThinkTransition {
            source: "A".into(),
            target: "B".into(),
            count: 4,
            steps: vec![2, 5, 9, 11],
            vias: vec!["advisor".into()],
            reasons: vec!["多轮推理未收敛".into()],
            candidate_counts: vec![3],
            mismatched: 0,
        };
        let label = edge_label(&t);
        assert!(label.starts_with("×4 · s2,5,9…"), "{label}");
        assert!(label.contains("Advisor"), "{label}");
        assert!(label.contains("候选3"), "{label}");
        assert!(label.contains("因:多轮推理未收敛"), "{label}");
    }

    #[test]
    fn via_style_marks_advisor_thick_and_fallback_dashed() {
        assert_eq!(edge_style_from_via(Some("advisor")).width, Some(2.4));
        assert_eq!(
            edge_style_from_via(Some("fallback")).dash.as_deref(),
            Some("5,4")
        );
        assert!(edge_style_from_via(Some("unique")).dash.is_none());
        assert!(edge_style_from_via(Some("unique")).width.is_none());
        assert!(edge_style_from_via(None).dash.is_none());
        assert_eq!(via_phrase("ADVISOR"), "Advisor", "大小写不敏感");
        assert_eq!(via_phrase("other"), "");
    }

    #[test]
    fn bidir_edges_get_opposite_arc_signs() {
        let occs = vec![
            occ("A", 1, 1, None),
            occ("B", 1, 2, None),
            occ("A", 2, 3, None),
        ];
        let g = build_think_path_graph(&occs, &[], 300.0, 200.0);
        let ab = g
            .edges
            .iter()
            .find(|e| e.source == "A" && e.target == "B")
            .unwrap();
        let ba = g
            .edges
            .iter()
            .find(|e| e.source == "B" && e.target == "A")
            .unwrap();
        match (ab.curve, ba.curve) {
            (Some(EdgeCurve::Arc(x)), Some(EdgeCurve::Arc(y))) => {
                assert!(x * y < 0.0, "双向边弧线必须分居两侧: {x} / {y}");
                let layer_gap = layout_config(300.0, 200.0).layer_height;
                assert!(
                    x.abs() <= layer_gap * 0.3 + 1e-9,
                    "曲率须 clamp 到 0.3×层间距"
                );
            }
            other => panic!("双向边应为相反符号弧线: {other:?}"),
        }
    }

    #[test]
    fn self_loop_gets_loop_curve() {
        let occs = vec![occ("Deliberate", 1, 1, None), occ("Deliberate", 2, 2, None)];
        let g = build_think_path_graph(&occs, &[], 300.0, 200.0);
        assert_eq!(g.edges.len(), 1);
        assert_eq!(g.edges[0].curve, Some(EdgeCurve::Loop));
    }

    #[test]
    fn nodes_carry_repeat_count_and_signal_tags() {
        let occs = vec![
            occ("Plan", 1, 1, Some("ready")),
            occ("Plan", 2, 2, Some("done")),
        ];
        let g = build_think_path_graph(&occs, &[], 300.0, 200.0);
        assert_eq!(g.nodes.len(), 1, "同名节点只出现一次");
        assert_eq!(g.nodes[0].summary.as_deref(), Some("×2 · step 1,2"));
        assert_eq!(g.nodes[0].tags, vec!["ready", "done"]);
        assert_eq!(g.nodes[0].node_type, THINK_STEP_NODE_TYPE);
    }
}
