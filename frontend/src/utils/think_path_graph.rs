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

/// 分层布局初始配置（只负责拓扑分层与初始 x 排序，最终间距由 `spread_layout` 按节点尺寸重排）
///
/// 与任务依赖图同源的比例，但留白更省（`top_margin` 0.11→0.06、`side_margin` 0.09→0.06）：
/// 这些留白原本直接吃掉可用宽高，而可用宽高正是层距/同层间距的唯一来源。
pub fn layout_config(
    width: f64,
    height: f64,
) -> crate::components::layered_layout::LayeredLayoutConfig {
    crate::components::layered_layout::LayeredLayoutConfig {
        width,
        height,
        top_margin: (height * 0.06).max(10.0),
        layer_height: height * 0.30,
        side_margin: (width * 0.06).max(10.0),
    }
}

/// 布局松散化：按节点**视觉尺寸**重排层内水平间距与层间距，返回实际层中心距
///
/// # 为什么必须在 `compute_layered_layout` 之后再排一次
///
/// `compute_layered_layout` 只按画布比例均分（`side_margin + (i+0.5)*usable/count`），
/// 完全不知道节点本身多大，于是两种重叠都会发生：
///
/// - **横向**：920px 画布上 5 个同层节点槽宽 151px，而长名卡片宽 168px —— 卡片直接互相压住；
///   缩略图 300px 画布上 3 个同层节点槽宽 82px，卡片更是一团。
/// - **纵向**：620px 画布上 6 层时层距被 `usable_height/max_layer` 压到 81px，而卡片高 74px
///   —— 只剩 7px 垂直间隙；层数再多就直接压住。
///
/// 卡片一重叠，后绘制的卡片就会盖住边与箭头，看起来正是「挤成一团、线叠在一起」。
/// 这里以节点视觉尺寸为下限重排：宁可内容略微超出画布、交给 `compute_fit_view` 等比缩小，
/// 也不让卡片互相压住（等比缩小时「间距/尺寸」的比值不变，重叠却是硬伤）。
///
/// # 层内折行（AMan 第五轮反馈④「不要排成一条线，尽量利用上下宽度」）
///
/// 旧实现每层只做**一维横排**：同层 8 个节点会被摊成一条长横线——横向挤到槽宽下限、
/// 纵向却大片空白。现在先按可用宽算出「一横排最多几列」，超出的折成第二行、第三行……
/// 层高随之变成 `单卡高 + (行数-1)·(卡高 + 行间隙)`，上下宽度被真正用起来。
///
/// # 参数
///
/// - `mini`：`true` = 点线形态（缩略图，节点是半径 7px 的点），`false` = 卡片形态（放大弹窗）。
///   两种形态的节点视觉尺寸差一个量级，用同一套间距会让点线图被拉得过散。
///
/// # 返回
///
/// 相邻单行层的**层中心距**（层间隙 + 卡片高，上限画布高 0.32），供 `assign_edge_curves`
/// 定弧线曲率分档。实际层中心距在被上限截断时可能更大，故消费方视为「不小于」。
pub fn spread_layout(nodes: &mut [GraphNode], width: f64, height: f64, mini: bool) -> f64 {
    /// 卡片形态：同层水平最小间隙 / 相邻层垂直最小间隙（px）
    const MIN_H_GAP: f64 = 26.0;
    const MIN_V_GAP: f64 = 30.0;
    /// 点线形态：点直径 `global_node_radius(0)=7.0` × 2 = 14，取 18 留余量
    const MINI_DOT: f64 = 18.0;
    const MINI_H_GAP: f64 = 18.0;
    const MINI_V_GAP: f64 = 18.0;

    if nodes.is_empty() {
        return 0.0;
    }

    // 1) 按 y 分行（同层 y 相同；容差 1px 吸收浮点误差）——沿用拓扑分层结果，本函数只
    //    决定「层内怎么摆、层间多高」
    let mut rows: Vec<(f64, Vec<usize>)> = Vec::new();
    let mut order: Vec<usize> = (0..nodes.len()).collect();
    order.sort_by(|&a, &b| {
        nodes[a]
            .y
            .partial_cmp(&nodes[b].y)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for i in order {
        let y = nodes[i].y;
        match rows.last_mut() {
            Some((row_y, idxs)) if (y - *row_y).abs() <= 1.0 => idxs.push(i),
            _ => rows.push((y, vec![i])),
        }
    }

    // 2) 节点视觉尺寸：卡片态取全图最宽/最高的那个做基准，保证任何一行都不重叠
    let (node_w, node_h, h_gap, v_gap) = if mini {
        (MINI_DOT, MINI_DOT, MINI_H_GAP, MINI_V_GAP)
    } else {
        let w = nodes
            .iter()
            .map(crate::components::graph::node_box_width)
            .fold(0.0_f64, f64::max);
        let h = nodes
            .iter()
            .map(crate::components::graph::node_box_height)
            .fold(0.0_f64, f64::max);
        (w, h, MIN_H_GAP, MIN_V_GAP)
    };

    // 3) 层内折行：一横排最多几列 = floor((可用宽 + 间隙) / (卡宽 + 间隙))
    let usable_w = (width * 0.88).max(1.0);
    let usable_h = (height * 0.88).max(1.0);
    let max_cols = (((usable_w + h_gap) / (node_w + h_gap)).floor() as usize).max(1);
    // 槽宽上限：防止两三个节点被拉到画布两端
    let col_cap = node_w + usable_w * 0.35;

    let mut layers: Vec<(Vec<usize>, f64)> = Vec::with_capacity(rows.len());
    for (_, mut idxs) in rows {
        idxs.sort_by(|&a, &b| {
            nodes[a]
                .x
                .partial_cmp(&nodes[b].x)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let cols = idxs.len().min(max_cols).max(1);
        let inner_rows = idxs.len().div_ceil(cols).max(1);
        let layer_h = node_h + (inner_rows as f64 - 1.0) * (node_h + v_gap);
        layers.push((idxs, layer_h));
    }

    // 4) 垂直：可用高先喂给「层内容高」，剩余空白均分给层间隙；内容超出画布时允许溢出
    //    （交给 `compute_fit_view` 等比缩小），但绝不把卡片压到一起。
    let content_h: f64 = layers.iter().map(|(_, h)| *h).sum();
    let gaps = layers.len().saturating_sub(1) as f64;
    let gap = if gaps > 0.0 {
        ((usable_h - content_h) / gaps)
            .max(v_gap)
            .min(height * 0.30) // 上限：再多就是空耗，不如让整体居中
    } else {
        0.0
    };
    let total_h = content_h + gap * gaps;
    let mut cursor = height / 2.0 - total_h / 2.0;

    // 5) 写回坐标：层内逐行居中，行内按槽宽展开
    for (idxs, layer_h) in layers {
        let count = idxs.len();
        let cols = count.min(max_cols).max(1);
        let spacing = (usable_w / cols as f64).max(node_w + h_gap).min(col_cap);
        let inner_rows = count.div_ceil(cols);
        for r in 0..inner_rows {
            let begin = r * cols;
            let end = (begin + cols).min(count);
            if begin >= end {
                continue;
            }
            let y = cursor + node_h / 2.0 + r as f64 * (node_h + v_gap);
            let total_w = spacing * ((end - begin) as f64 - 1.0);
            let start_x = width / 2.0 - total_w / 2.0;
            for (k, &idx) in idxs[begin..end].iter().enumerate() {
                nodes[idx].x = start_x + k as f64 * spacing;
                nodes[idx].y = y;
            }
        }
        cursor += layer_h + gap;
    }

    ((gap + node_h).min(height * 0.32)).max(node_h)
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
    mini: bool,
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

    let mut nodes: Vec<GraphNode> = aggs
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

    // 布局松散化：按节点实际视觉尺寸重排（详见 `spread_layout` 文档），
    // 返回的层距随重排结果变，供弧线曲率分档使用
    let layer_gap = spread_layout(&mut nodes, width, height, mini);
    // 曲率：回边 / 双向成对边走弧线，自环绕行；同层多条回边按组内序分档（详见函数文档）
    assign_edge_curves(&nodes, &mut edges, layer_gap, mini);

    ThinkPathGraph {
        nodes,
        edges,
        aggs,
        transitions,
    }
}

/// 曲率分派：自环 / 双向对 / 同层回边 → 弧线，其余直线
///
/// # 双向边为什么必须**同号**（AMan 第五轮反馈②「双向线用了完全一样的弧度」）
///
/// `quadratic_control` 的法线是相对于**有向线段**的 left-hand normal —— 反向边的法线
/// 本身也反向（`N_BA = -N_AB`）。旧实现按 `source <= target` 取相反符号，负负得正，
/// 两条曲线落在**同一侧**、几何完全重合。组内统一取同一符号即可让它们各自按自己的
/// left-hand 分居参考线两侧，等幅且对称。
///
/// # 为什么要按「组内序」分档，而不是给同一类边同一个弧度
///
/// 同一层上常有多条回边（环上的 A→B、B→C、C→A 会全部沉到同一层）。旧实现给它们统一的
/// `base * 0.6`，于是这些曲线几何完全重合——视觉上就是「线叠在一起、分不清谁连谁」。
/// 这里把同层回边按源节点 x 排序成一组，弧度在 `(0, max_bulge]` 内均匀铺开：
/// 组内任意两条弧度必不相同，曲线随节点横向位置呈扇形展开，各自可辨。
///
/// # 弧向避障（启发式，AMan 第五轮反馈③「尽量不穿过卡片」）
///
/// 每条弧线都有两个可选的弯向（`+bulge` / `-bulge`）。取曲线中点（二次贝塞尔 `t=0.5`，
/// 即「两端中点 + bulge·N」）检测是否落进任何**第三方**卡片矩形（半宽/半高 + 4px 余量）：
/// 当前弯向撞卡、反向不撞，就整条换到反向。这是 O(边数 × 节点数) 的**局部**启发式，
/// **不是全局路由** —— 多条线在密集区间同时交错时仍可能有个别穿卡，属已知边界。
///
/// - 自环（`source == target`）→ `EdgeCurve::Loop`；
/// - 双向对（同一无序对存在两条边）→ 组内**同号**弧线，分居参考线两侧；
/// - 同层 / 向上回边 → 组内均匀分档弧线；
/// - 正常向下 → 直线。
///
/// 曲率上限 `layer_gap * 0.55`：再大就会顶到相邻层的卡片。
pub fn assign_edge_curves(
    nodes: &[GraphNode],
    edges: &mut [GraphEdge],
    layer_gap: f64,
    mini: bool,
) {
    let pos: HashMap<&str, (f64, f64)> =
        nodes.iter().map(|n| (n.id.as_str(), (n.x, n.y))).collect();
    // 无序对 → 方向数（>1 说明双向）
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
    let base = (layer_gap * 0.45).max(12.0);
    let max_bulge = (layer_gap * 0.55).max(base);

    #[derive(Clone, Copy)]
    enum Kind {
        Loop,
        Pair,
        /// 同层 / 向上回边，携带源层的行键（y 取整后分组）
        Back(i64),
        Straight,
    }

    let x_of = |id: &str| pos.get(id).map(|p| p.0).unwrap_or(0.0);
    let y_of = |id: &str| pos.get(id).map(|p| p.1).unwrap_or(0.0);

    // 卡片矩形（含 4px 余量）：弧线中点落进去就要换侧
    let half_of = |n: &GraphNode| -> (f64, f64) {
        if mini {
            (11.0, 11.0)
        } else {
            (
                crate::components::graph::node_box_width(n) / 2.0 + 4.0,
                crate::components::graph::node_box_height(n) / 2.0 + 4.0,
            )
        }
    };
    // 弧线中点是否压到第三方卡片（排除边自身的两端）
    let blocked = |from: (f64, f64), to: (f64, f64), bulge: f64, skip: (&str, &str)| -> bool {
        let p1 = crate::components::graph::quadratic_control(from.0, from.1, to.0, to.1, bulge);
        let mid = (
            (from.0 + 2.0 * p1.0 + to.0) / 4.0,
            (from.1 + 2.0 * p1.1 + to.1) / 4.0,
        );
        nodes.iter().any(|n| {
            if n.id.as_str() == skip.0 || n.id.as_str() == skip.1 {
                return false;
            }
            let (hw, hh) = half_of(n);
            (mid.0 - n.x).abs() < hw && (mid.1 - n.y).abs() < hh
        })
    };

    // 1) 分类
    let kinds: Vec<Kind> = edges
        .iter()
        .map(|e| {
            if e.source == e.target {
                return Kind::Loop;
            }
            let key = if e.source <= e.target {
                (e.source.clone(), e.target.clone())
            } else {
                (e.target.clone(), e.source.clone())
            };
            if pair_dirs.get(&key).copied().unwrap_or(0) > 1 {
                return Kind::Pair;
            }
            if y_of(&e.target) <= y_of(&e.source) + 1.0 {
                Kind::Back(y_of(&e.source).round() as i64)
            } else {
                Kind::Straight
            }
        })
        .collect();

    // 2) 同层回边分组：同一行内按源 x（次级：目标 x）排序，弧度均匀铺开
    let mut groups: HashMap<i64, Vec<usize>> = HashMap::new();
    for (i, k) in kinds.iter().enumerate() {
        if let Kind::Back(row) = k {
            groups.entry(*row).or_default().push(i);
        }
    }
    let mut back_bulge: HashMap<usize, f64> = HashMap::new();
    for (_, mut idxs) in groups {
        idxs.sort_by(|&a, &b| {
            x_of(&edges[a].source)
                .partial_cmp(&x_of(&edges[b].source))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    x_of(&edges[a].target)
                        .partial_cmp(&x_of(&edges[b].target))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        let n = idxs.len();
        for (rank, &i) in idxs.iter().enumerate() {
            // n 条边 → 弧度取 max_bulge 的 1/(n+1) … n/(n+1)，两两必不相等
            let v = max_bulge * (rank as f64 + 1.0) / (n as f64 + 1.0);
            back_bulge.insert(i, v);
        }
    }

    // 3) 双向对：组级一次符号决策（组内同号），顺带做弧向避障
    let mut pair_members: HashMap<(String, String), Vec<usize>> = HashMap::new();
    for (i, e) in edges.iter().enumerate() {
        if e.source == e.target {
            continue;
        }
        let key = if e.source <= e.target {
            (e.source.clone(), e.target.clone())
        } else {
            (e.target.clone(), e.source.clone())
        };
        if pair_dirs.get(&key).copied().unwrap_or(0) > 1 {
            pair_members.entry(key).or_default().push(i);
        }
    }
    let mut pair_sign: HashMap<(String, String), f64> = HashMap::new();
    for (key, idxs) in &pair_members {
        let rep = *idxs.iter().min().unwrap();
        let s = edges[rep].source.clone();
        let t = edges[rep].target.clone();
        let (Some(from), Some(to)) = (pos.get(s.as_str()).copied(), pos.get(t.as_str()).copied())
        else {
            continue;
        };
        let skip = (s.as_str(), t.as_str());
        let sign = if blocked(from, to, base, skip) && !blocked(from, to, -base, skip) {
            -1.0
        } else {
            1.0
        };
        pair_sign.insert(key.clone(), sign);
    }

    // 4) 写回
    for (i, e) in edges.iter_mut().enumerate() {
        let (s, t) = (e.source.clone(), e.target.clone());
        e.curve = Some(match kinds[i] {
            Kind::Loop => EdgeCurve::Loop,
            Kind::Pair => {
                // 组内同号：A→B 与 B→A 各自按 left-hand 弯，自然分居参考线两侧
                let key = if s <= t {
                    (s.clone(), t.clone())
                } else {
                    (t.clone(), s.clone())
                };
                EdgeCurve::Arc(pair_sign.get(&key).copied().unwrap_or(1.0) * base)
            }
            Kind::Back(_) => {
                let v = back_bulge.get(&i).copied().unwrap_or(base * 0.6);
                // 弧顶撞到第三方卡片、反向不撞时整条翻向（绝对值不变，组内仍两两不等）
                let from = (x_of(&s), y_of(&s));
                let to = (x_of(&t), y_of(&t));
                let flip = blocked(from, to, v, (s.as_str(), t.as_str()))
                    && !blocked(from, to, -v, (s.as_str(), t.as_str()));
                EdgeCurve::Arc(if flip { -v } else { v })
            }
            Kind::Straight => EdgeCurve::Straight,
        });
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
        let g = build_think_path_graph(&[], &[], 300.0, 200.0, false);
        assert!(g.nodes.is_empty());
        assert!(g.edges.is_empty());
        // 有 routes 但无 occurrences：仍不建图（无节点可画），不 panic
        let g2 = build_think_path_graph(
            &[],
            &[route(2, &["A"], "A", "unique", None)],
            300.0,
            200.0,
            false,
        );
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

    /// 双向边必须**几何分居参考线两侧**（旧实现按 source<=target 取反符号，
    /// 与 left-hand normal 的方向反转两次抵消 → 两条曲线完全重合）
    #[test]
    fn bidirectional_pair_arcs_land_on_opposite_sides() {
        let occs = vec![
            occ("A", 1, 1, None),
            occ("B", 1, 2, None),
            occ("A", 2, 3, None),
        ];
        let g = build_think_path_graph(&occs, &[], 920.0, 620.0, false);
        let pos = |id: &str| {
            let n = g.nodes.iter().find(|n| n.id == id).unwrap();
            (n.x, n.y)
        };
        let (ax, ay) = pos("A");
        let (bx, by) = pos("B");
        let lens = ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt();
        assert!(lens > 1.0, "两个节点不应重合");
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
        let (x, y) = match (ab.curve, ba.curve) {
            (Some(EdgeCurve::Arc(x)), Some(EdgeCurve::Arc(y))) => (x, y),
            other => panic!("双向边应为弧线: {other:?}"),
        };
        // 两条边的控制点相对参考线 A→B 的法向投影：left-hand normal 对反向边自带负号，
        // 故 proj(A→B) = 2x、proj(B→A) = -2y。分居两侧 ⇔ 两者异号。
        let proj_ab = 2.0 * x;
        let proj_ba = -2.0 * y;
        assert!(
            proj_ab * proj_ba < 0.0,
            "双向边必须分居参考线两侧（不能重合）: {proj_ab} / {proj_ba}"
        );
        assert!(
            (proj_ab.abs() - proj_ba.abs()).abs() < 1e-9,
            "双向边弧度应等幅对称: {proj_ab} / {proj_ba}"
        );
        assert!(x.abs() > 0.0, "双向边必须真的弯出去: {x}");
    }

    #[test]
    fn self_loop_gets_loop_curve() {
        let occs = vec![occ("Deliberate", 1, 1, None), occ("Deliberate", 2, 2, None)];
        let g = build_think_path_graph(&occs, &[], 300.0, 200.0, false);
        assert_eq!(g.edges.len(), 1);
        assert_eq!(g.edges[0].curve, Some(EdgeCurve::Loop));
    }

    #[test]
    fn nodes_carry_repeat_count_and_signal_tags() {
        let occs = vec![
            occ("Plan", 1, 1, Some("ready")),
            occ("Plan", 2, 2, Some("done")),
        ];
        let g = build_think_path_graph(&occs, &[], 300.0, 200.0, false);
        assert_eq!(g.nodes.len(), 1, "同名节点只出现一次");
        assert_eq!(g.nodes[0].summary.as_deref(), Some("×2 · step 1,2"));
        assert_eq!(g.nodes[0].tags, vec!["ready", "done"]);
        assert_eq!(g.nodes[0].node_type, THINK_STEP_NODE_TYPE);
    }

    /// 同层多条回边（环沉底场景）：弧度必须两两不同，否则曲线完全重合
    #[test]
    fn same_layer_back_edges_get_distinct_arc_bulges() {
        // A→B→C→A 成环 ⇒ 三个节点全部沉到同一层，三条边都在层内
        let occs = vec![
            occ("A", 1, 1, None),
            occ("B", 1, 2, None),
            occ("C", 1, 3, None),
            occ("A", 2, 4, None),
        ];
        let g = build_think_path_graph(&occs, &[], 920.0, 620.0, false);
        assert_eq!(g.edges.len(), 3, "环上三条边");
        let ys: Vec<f64> = g.nodes.iter().map(|n| n.y).collect();
        let span = ys.iter().copied().fold(f64::MIN, f64::max)
            - ys.iter().copied().fold(f64::MAX, f64::min);
        assert!(span < 1.0, "环上节点应同层（y 相同）: span={span}");

        let mut bulges: Vec<f64> = Vec::new();
        for e in &g.edges {
            match e.curve {
                Some(EdgeCurve::Arc(v)) => bulges.push(v.abs()),
                other => panic!("同层回边应为弧线: {other:?}"),
            }
        }
        assert_eq!(bulges.len(), 3);
        let mut uniq = bulges.clone();
        uniq.sort_by(|a, b| a.partial_cmp(b).unwrap());
        uniq.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        assert_eq!(uniq.len(), 3, "三条同层回边弧度必须两两不同: {bulges:?}");
        assert!(bulges.iter().all(|v| *v > 0.0 && *v <= 200.0));
    }

    /// 同层卡片不得重叠：**同一行内**相邻中心距 ≥ 最宽卡片宽；一排放不下必须折行
    #[test]
    fn spread_layout_separates_same_layer_cards() {
        let mut nodes: Vec<GraphNode> = (0..8)
            .map(|i| GraphNode {
                id: format!("n{i}"),
                label: "Deliberate".to_string(), // 长名 → 卡片接近最大宽度
                description: String::new(),
                node_type: THINK_STEP_NODE_TYPE.to_string(),
                x: 100.0 + i as f64 * 10.0, // 原始布局：挤在 100..150
                y: 300.0,
                tags: Vec::new(),
                summary: None,
            })
            .collect();
        let max_w = nodes
            .iter()
            .map(crate::components::graph::node_box_width)
            .fold(0.0_f64, f64::max);
        spread_layout(&mut nodes, 920.0, 620.0, false);
        // 8 个卡（每卡 ~104px + 26 间隙）在 920px 画布上一排只放得下 6 个 → 必须折行
        let mut ys: Vec<f64> = nodes.iter().map(|n| n.y).collect();
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let min_y = *ys.first().unwrap();
        let max_y = *ys.last().unwrap();
        ys.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        assert!(ys.len() >= 2, "同层卡片一排放不下时必须折行: {ys:?}");
        // 行内不重叠：同一 y 的卡片按 x 排序后中心距 ≥ 卡片宽
        for row_y in &ys {
            let mut xs: Vec<f64> = nodes
                .iter()
                .filter(|n| (n.y - *row_y).abs() < 1e-9)
                .map(|n| n.x)
                .collect();
            xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
            for w in xs.windows(2) {
                assert!(
                    w[1] - w[0] >= max_w,
                    "同行相邻卡片中心距 {} 小于卡片宽 {}（会重叠）",
                    w[1] - w[0],
                    max_w
                );
            }
        }
        // 单层内容整体垂直居中于画布（620/2 = 310）
        let mid = (min_y + max_y) / 2.0;
        assert!((mid - 310.0).abs() < 1e-9, "单层内容应垂直居中: {mid}");
    }

    /// 层距不得小于卡片高（旧实现会把层距压到 81px，而卡片高 74px）
    #[test]
    fn spread_layout_keeps_vertical_gap_above_card_height() {
        let mut nodes: Vec<GraphNode> = (0..7)
            .map(|i| GraphNode {
                id: format!("n{i}"),
                label: format!("Step{i}"),
                description: "描述文本".to_string(),
                node_type: THINK_STEP_NODE_TYPE.to_string(),
                x: 460.0,
                y: 100.0 + i as f64 * 40.0,
                tags: vec!["plan".to_string()],
                summary: Some("×2".to_string()),
            })
            .collect();
        let max_h = nodes
            .iter()
            .map(crate::components::graph::node_box_height)
            .fold(0.0_f64, f64::max);
        let gap = spread_layout(&mut nodes, 920.0, 620.0, false);
        assert!(gap >= max_h, "层距 {gap} 小于卡片高 {max_h}（会重叠）");
        let mut ys: Vec<f64> = nodes.iter().map(|n| n.y).collect();
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for w in ys.windows(2) {
            assert!(w[1] - w[0] >= max_h, "相邻层中心距不足卡片高: {w:?}");
        }
    }

    /// 多层时必须把上下宽度用起来：层中心距 ≥ 卡片高 + 最小垂直间隙
    #[test]
    fn spread_layout_uses_vertical_space() {
        let mut nodes: Vec<GraphNode> = (0..3)
            .map(|i| GraphNode {
                id: format!("n{i}"),
                label: format!("Step{i}"),
                description: String::new(),
                node_type: THINK_STEP_NODE_TYPE.to_string(),
                x: 460.0,
                y: 120.0 + i as f64 * 40.0,
                tags: Vec::new(),
                summary: None,
            })
            .collect();
        let max_h = nodes
            .iter()
            .map(crate::components::graph::node_box_height)
            .fold(0.0_f64, f64::max);
        let layer_gap = spread_layout(&mut nodes, 920.0, 620.0, false);
        assert!(
            layer_gap >= max_h + 29.0,
            "层中心距 {layer_gap} 应 ≥ 卡片高 {max_h} + 最小间隙 30"
        );
        // 实际层中心距不小于返回值（返回值只做上限截断，用于弧线曲率分档）
        let mut ys: Vec<f64> = nodes.iter().map(|n| n.y).collect();
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(
            ys[1] - ys[0] >= layer_gap - 1e-9,
            "实际层距 {} 不得小于返回的 layer_gap {layer_gap}",
            ys[1] - ys[0]
        );
    }

    /// 同层节点多时折行：8 个长名卡在 920px 画布上不能摊成一条横线
    #[test]
    fn spread_layout_wraps_many_nodes_into_rows() {
        let mut nodes: Vec<GraphNode> = (0..8)
            .map(|i| GraphNode {
                id: format!("n{i}"),
                label: "Deliberate".to_string(),
                description: String::new(),
                node_type: THINK_STEP_NODE_TYPE.to_string(),
                x: 100.0 + i as f64 * 8.0,
                y: 310.0,
                tags: Vec::new(),
                summary: None,
            })
            .collect();
        let max_w = nodes
            .iter()
            .map(crate::components::graph::node_box_width)
            .fold(0.0_f64, f64::max);
        spread_layout(&mut nodes, 920.0, 620.0, false);
        let mut ys: Vec<f64> = nodes.iter().map(|n| n.y).collect();
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        ys.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        assert!(ys.len() >= 2, "8 个同层卡必须折行: {ys:?}");
        for row_y in &ys {
            let mut xs: Vec<f64> = nodes
                .iter()
                .filter(|n| (n.y - *row_y).abs() < 1e-9)
                .map(|n| n.x)
                .collect();
            xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
            for w in xs.windows(2) {
                assert!(w[1] - w[0] >= max_w, "同行重叠: {w:?} < {max_w}");
            }
        }
        // 折行后整层仍横向居中（不偏向一侧）
        let cx = nodes.iter().map(|n| n.x).sum::<f64>() / nodes.len() as f64;
        assert!((cx - 460.0).abs() < 1e-9, "折行后应整体居中: {cx}");
    }
}
