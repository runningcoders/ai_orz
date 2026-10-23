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
/// - `mini`：`true` = 点线形态（缩略图，节点圆点半径由 [`crate::components::graph::dot_node_radius`] 按画布与节点数算），`false` = 卡片形态（放大弹窗）。
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
    /// 点线形态占位下限（实际点径取 `graph::dot_node_radius` × 2，随画布/节点数变）
    const MINI_DOT: f64 = 18.0;
    /// 点与点之间的最小间隙（下限；实际取点径的一半）
    const MINI_H_GAP: f64 = 12.0;

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
        // 点线形态：点径与渲染侧同源（`graph::dot_node_radius`），画布越大 / 节点越少
        // 点越大；占位 = 点径（直径），间隙 = 半径。用常量 18 会让「点画 46px、位留 18px」
        // 直接互压，用同一函数推间距才能永远配套。
        let dot = (crate::components::graph::dot_node_radius(width, height, nodes.len()) * 2.0)
            .max(MINI_DOT);
        let gap = (dot * 0.5).max(MINI_H_GAP);
        (dot, dot, gap, gap)
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
    // 0.88 → 0.95：留白收到 2.5%，节点能真正摊到画布边缘（AMan 第九轮反馈④
    //「能不能让这几个节点在布局上尽量散开一点」）。再往外就会贴边/被箭头压住。
    let usable_w = (width * 0.95).max(1.0);
    let usable_h = (height * 0.95).max(1.0);
    let max_cols = (((usable_w + h_gap) / (node_w + h_gap)).floor() as usize).max(1);
    // 槽宽上限：防止两三个节点被拉到画布两端
    let col_cap = node_w + usable_w * 0.5;

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
            .min(height * 0.34) // 上限：再多就是空耗，不如让整体居中
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

/// 力导向松弛：在 `spread_layout` 的分层结果上跑**力传导布局**，让节点上下错落
///
/// # 为什么直接复用现成算法（AMan 第六轮反馈②「别自己写了，直接用人家的」）
///
/// `spread_layout` 只解决「不重叠」：它把同层节点摊成一条**规整的一横排**（y 完全相同、
/// 横向等距）。规整反而难看 —— 所有线从同一水平线落到同一水平线，缺少错落感。
/// 力传导布局（`components/force_layout.rs`，知识图谱 / 工作台的 Canvas 栈一直在用）
/// 本来就是为这件事写的：斥力 + 碰撞分离 + 连线弹簧 + 分层约束力，天然产出「大致在
/// 同一层、但不齐平」的有机分布。这里不新写算法，直接把节点转成 `CanvasNode` 跑它的
/// `ForceLayout::step`，跑完把坐标搬回来。
///
/// # 安全兜底（只保证「不重叠 + 层序可读」，不改变力传导给出的位置）
///
/// - **y 夹在所属层带内**（抖动上限 `layer_gap × 0.18`，再夹到 12px）：层序仍是可读的
///   拓扑顺序，只是同层节点高低错开；上限同时保证相邻层带不会因为抖动贴到一起。
/// - **x 写回后按卡片宽做一次最小间距校正 + 整层居中**：纯「不许重叠」的硬保证。
/// - 层带内若力传导给出完全相同的 y（对称场景），补一个由节点 id 派生的**确定性**微抖，
///   避免又退回「排成一条线」（同一份数据每次渲染位置一致，不会跳）。
///
/// # 返回
///
/// 原样返回 `layer_gap`：层带中心距仍由 `spread_layout` 决定，松弛只改层内分布。
pub fn relax_layout(
    nodes: &mut [GraphNode],
    edges: &[GraphEdge],
    width: f64,
    height: f64,
    mini: bool,
    layer_gap: f64,
) -> f64 {
    use crate::components::canvas_scene::{CanvasEdge, CanvasNode};
    use crate::components::force_layout::{ForceLayout, ForceLayoutConfig};

    if nodes.len() < 2 {
        return layer_gap;
    }

    // 1) 恢复行：`spread_layout` 的输出按 y 聚成若干行（容差 1px，与 `spread_layout`
    //    同一口径）。**不能**用 `layer_gap × 0.5` 粗聚：折行后的内层行间距只有
    //    「卡片高 + 行间隙」，会被误并成同一行，抖动上限就退化成整层共享的那个大值，
    //    内层行反而永远齐平。
    let band_tol = 1.0;
    let mut band_centers: Vec<f64> = Vec::new();
    let mut band_of: Vec<usize> = vec![0; nodes.len()];
    {
        let mut order: Vec<usize> = (0..nodes.len()).collect();
        order.sort_by(|&a, &b| {
            nodes[a]
                .y
                .partial_cmp(&nodes[b].y)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for i in order {
            let y = nodes[i].y;
            let idx = match band_centers.iter().position(|c| (y - *c).abs() <= band_tol) {
                Some(p) => p,
                None => {
                    band_centers.push(y);
                    band_centers.len() - 1
                }
            };
            band_of[i] = idx;
        }
    }
    if band_centers.is_empty() {
        return layer_gap;
    }

    // 2) 转成 CanvasNode / CanvasEdge，交给力传导
    let mut c_nodes: Vec<CanvasNode> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let radius = if mini {
                // 与渲染侧 / spread_layout 同源：点半径随画布与节点数自适应，
                // 力传导的斥力与碰撞分离按这个半径算，才不会把点挤到一起
                crate::components::graph::dot_node_radius(width, height, nodes.len())
            } else {
                let w = crate::components::graph::node_box_width(n);
                let h = crate::components::graph::node_box_height(n);
                w.max(h) / 2.0
            };
            CanvasNode {
                id: n.id.clone(),
                x: n.x,
                y: n.y,
                radius,
                label: n.label.clone(),
                color: String::new(),
                node_type: None,
                layer: Some(band_of[i] as i32),
                summary: None,
                description: String::new(),
                tags: Vec::new(),
                highlighted: false,
            }
        })
        .collect();
    let c_edges: Vec<CanvasEdge> = edges
        .iter()
        .filter(|e| e.source != e.target)
        .map(|e| CanvasEdge {
            from_id: e.source.clone(),
            to_id: e.target.clone(),
            ..Default::default()
        })
        .collect();

    // 分层约束的「层距」必须与实际行距对齐：折行场景下行距比 `layer_gap` 小，
    // 用 `layer_gap` 会把内层行硬生生往远处拉（偏离真实行位 → 写回夹紧后又齐平）。
    let layer_step = if band_centers.len() > 1 {
        (band_centers[band_centers.len() - 1] - band_centers[0]) / (band_centers.len() - 1) as f64
    } else {
        layer_gap
    };
    let cfg = ForceLayoutConfig {
        repulsion: if mini { 1500.0 } else { 6000.0 },
        attraction: 0.06,
        ideal_length: if mini { 70.0 } else { 150.0 },
        damping: 0.85,
        max_step: 6.0,
        // 层约束从 0.30 降到 0.12：0.30 时 y 一直被死死拉回层中心，力传导给出的
        // 错落一写回就被抹平（这正是「看不出力导向」的成因）。层序由下面的
        // y 夹紧（不越行）保证，不靠这个系数。
        layer_attraction: 0.12,
        layer_height: layer_step.max(1.0),
        layer_top_margin: band_centers.first().copied().unwrap_or(0.0),
    };
    let mut layout = ForceLayout::new(cfg);
    for _ in 0..300 {
        let d = layout.step(&mut c_nodes, &c_edges, width, height);
        if layout.is_stable(d, 0.8) {
            break;
        }
    }

    // 3) 每行允许的垂直抖动上限：**由相邻行的实际空隙决定**，不再一律 ±12px
    //
    // 旧实现 `jitter_cap = (layer_gap × 0.18).clamp(4, 12)`：不管层间还剩多少空白，
    // y 最多只动 12px —— 力传导跑完再一夹，视觉上仍然是「分行 + 齐平」。这就是
    // AMan 第九轮反馈②「你确定那个思维轨迹用的是力导向布局吗」的答案：调用了，
    // 但结果被兜底覆盖掉了大半。
    //
    // 现在按行算：`cap = (行间空隙 × 0.45).clamp(4, 56)`，再夹 `layer_gap × 0.24`
    // 保证同一行的 y 抖动极差（2 × cap）不会大到让 `assign_edge_curves` 把「向下边」
    // 误判成「同层回边」（那儿的判定阈值是 `layer_gap × 0.5`）。
    let band_half: Vec<f64> = (0..band_centers.len())
        .map(|band| {
            nodes
                .iter()
                .enumerate()
                .filter(|(i, _)| band_of[*i] == band)
                .map(|(_, n)| {
                    if mini {
                        crate::components::graph::dot_node_radius(width, height, nodes.len())
                    } else {
                        crate::components::graph::node_box_height(n) / 2.0
                    }
                })
                .fold(0.0_f64, f64::max)
        })
        .collect();
    let jitter_caps: Vec<f64> = (0..band_centers.len())
        .map(|band| {
            let mut room = layer_gap;
            if band > 0 {
                room = room.min(
                    band_centers[band]
                        - band_centers[band - 1]
                        - band_half[band]
                        - band_half[band - 1],
                );
            }
            if band + 1 < band_centers.len() {
                room = room.min(
                    band_centers[band + 1]
                        - band_centers[band]
                        - band_half[band]
                        - band_half[band + 1],
                );
            }
            // ×0.45 留视觉呼吸；clamp 4~56 给足错落空间；再夹 0.24×层距守住层序可读；
            // 最后一行是硬保险：行贴得极近（room 很小）时宁可抖 0，也不许压进邻行
            (room * 0.45)
                .clamp(4.0, 56.0)
                .min(layer_gap * 0.24)
                .min(room.max(0.0) * 0.5)
                .max(0.0)
        })
        .collect();

    // 4) 写回：y 夹在**本行**的抖动上限内（错落但不越行、不压邻行），x 用力传导结果
    let jitter_of = |id: &str| -> f64 {
        // FNV-1a：确定性哈希 → 同一份数据每次渲染的错落位置一致
        let mut h: u64 = 1469598103934665603;
        for b in id.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(1099511628211);
        }
        (h % 1000) as f64 / 1000.0 - 0.5
    };
    for (i, n) in nodes.iter_mut().enumerate() {
        let band = band_of[i];
        let base = band_centers[band];
        let cap = jitter_caps[band];
        let dy = (c_nodes[i].y - base) + jitter_of(&n.id) * cap * 0.6;
        n.y = base + dy.clamp(-cap, cap);
        n.x = c_nodes[i].x;
    }

    // 5) 安全兜底：层内按卡片宽做最小间距校正，再整层横向居中
    let mini_dot =
        (crate::components::graph::dot_node_radius(width, height, nodes.len()) * 2.0).max(18.0);
    let (node_w, h_gap) = if mini {
        // 与渲染 / `spread_layout` 同源：占位 = 点径，最小间隙 = 半个点径。
        // 旧实现硬编码 (18, 18)，画布变大、点画到 34px 直径时就会互压。
        (mini_dot, (mini_dot * 0.5).max(12.0))
    } else {
        (
            nodes
                .iter()
                .map(crate::components::graph::node_box_width)
                .fold(0.0_f64, f64::max),
            26.0,
        )
    };
    let need = node_w + h_gap;
    for band in 0..band_centers.len() {
        let mut idxs: Vec<usize> = (0..nodes.len()).filter(|&i| band_of[i] == band).collect();
        if idxs.is_empty() {
            continue;
        }
        idxs.sort_by(|&a, &b| {
            nodes[a]
                .x
                .partial_cmp(&nodes[b].x)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for k in 1..idxs.len() {
            let prev = nodes[idxs[k - 1]].x;
            if nodes[idxs[k]].x - prev < need {
                nodes[idxs[k]].x = prev + need;
            }
        }
        let first = nodes[*idxs.first().unwrap()].x;
        let last = nodes[*idxs.last().unwrap()].x;
        let shift = width / 2.0 - (first + last) / 2.0;
        for &i in &idxs {
            nodes[i].x += shift;
        }
    }
    layer_gap
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
    // 错落：复用知识图谱同款力传导布局做层内松弛（不再手写「一横排」）
    let layer_gap = relax_layout(&mut nodes, &edges, width, height, mini, layer_gap);
    // 曲率：回边 / 双向成对边走弧线，自环绕行；同层多条回边按组内序分档（详见函数文档）
    assign_edge_curves(&nodes, &mut edges, width, height, layer_gap, mini);

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
/// # 弯向选择：先躲人，再往外弯（AMan 第五/六轮反馈「不要往别人的点上弯，往外弯」）
///
/// 每条弧线都有两个可选的弯向（`+bulge` / `-bulge`），弧顶位置 = 两端中点 + bulge·N。
/// 对两个方向各算一次「弧顶到最近第三方卡片矩形外沿的余量」（负值=已经压进卡片），
/// 取余量大的那一侧；两侧余量之差小于 1px（等价于都空、或都被压）时，改用
/// **远离全图重心**的一侧 —— 也就是画面上「往外弯」。这样大片空白会被用上，
/// 而不是把弧顶塞进节点堆里。
///
/// 仍是 O(边数 × 节点数) 的**局部**启发式，**不是全局路由/正交布线**：多条线在密集
/// 区间同时交错时仍可能有个别穿卡，属已知边界（要彻底解决需引入真正的图布局库）。
///
/// - 自环（`source == target`）→ `EdgeCurve::Loop`；
/// - 双向对（同一无序对存在两条边）→ 组内**同号**弧线，分居参考线两侧；
/// - 同层 / 向上回边 → 组内均匀分档弧线；
/// - 正常向下 → 直线；直线若从第三方节点身上穿过，改成绕行弧线。
///
/// # 幅值也参与避障（AMan 第九轮反馈③：弧线贴着节点边过去）
///
/// 旧实现幅值固定（`base` / `max_bulge` 分档），`pick_sign` 只决定弯向 —— 中间夹了
/// 节点时幅值不够也加不上去，弧线只能擦着节点边过。现在 `clearance` 沿曲线
/// t = 0.25 / 0.5 / 0.75 三点取样，并在「两侧 × 幅值 ×1.0 / ×1.35 / ×1.75」里选
/// 余量最大的一组；只有明显更空（> 1px）才换档，避免无谓加弯。
/// 双向对仍「组内同号同幅」（符号与幅值都是组级一次决策），同层回边组内同乘一个
/// 公共放大因子以保住「弧度两两不同」。
///
/// 曲率上限：基础档 `layer_gap * 0.55`，放大档硬顶 `layer_gap * 0.9`（再大就顶到
/// 相邻层的卡片）。如实说明：这仍是**局部启发式**，不是全局路由 —— 多条线在密集
/// 区间同时交错时仍可能有个别穿卡。
pub fn assign_edge_curves(
    nodes: &[GraphNode],
    edges: &mut [GraphEdge],
    width: f64,
    height: f64,
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
    // 幅值放大上限：再大就顶到相邻层去了
    let factor_cap = (layer_gap * 0.9).max(base);

    #[derive(Clone, Copy)]
    enum Kind {
        Loop,
        Pair,
        /// 同层 / 向上回边，携带源层的行键（y 聚簇后分组）
        Back(i64),
        Straight,
    }

    let x_of = |id: &str| pos.get(id).map(|p| p.0).unwrap_or(0.0);
    let y_of = |id: &str| pos.get(id).map(|p| p.1).unwrap_or(0.0);

    // 行带：把节点按 y 聚成若干行后编组。
    // 为什么不用 `y.round()`：力传导松弛后同行的 y 已错开几十像素，取整会把同一条
    // 行拆成多个组，组内每条边各占一个弧度、反而退化成同值。
    let band_tol = (layer_gap * 0.5).max(1.0);
    let mut band_id: HashMap<&str, usize> = HashMap::new();
    {
        let mut centers: Vec<f64> = Vec::new();
        let mut order: Vec<usize> = (0..nodes.len()).collect();
        order.sort_by(|&a, &b| {
            nodes[a]
                .y
                .partial_cmp(&nodes[b].y)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for i in order {
            let y = nodes[i].y;
            let idx = match centers.iter().position(|c| (y - *c).abs() <= band_tol) {
                Some(p) => p,
                None => {
                    centers.push(y);
                    centers.len() - 1
                }
            };
            band_id.insert(nodes[i].id.as_str(), idx);
        }
    }
    let band_of = |id: &str| band_id.get(id).copied().unwrap_or(0);

    // 节点半尺寸：点线态必须与渲染同源（`dot_node_radius`），否则「画的点」比
    //「避障用的点」大，弧线就会擦着点的边缘过去（AMan 第九轮反馈③）。
    let dot_half = crate::components::graph::dot_node_radius(width, height, nodes.len()) + 3.0;
    let half_of = |n: &GraphNode| -> (f64, f64) {
        if mini {
            (dot_half, dot_half)
        } else {
            (
                crate::components::graph::node_box_width(n) / 2.0 + 4.0,
                crate::components::graph::node_box_height(n) / 2.0 + 4.0,
            )
        }
    };
    // 曲线到最近**第三方**节点的余量（负值 = 已经压进节点本体）。
    // 沿曲线 t = 0.25 / 0.5 / 0.75 三点采样取最小值 —— 只在 t = 0.5 采样会漏掉
    //「弧顶两侧擦着别的节点过去」，那正是「中间有个节点，线就贴着边过去」。
    let clearance = |from: (f64, f64), to: (f64, f64), bulge: f64, skip: (&str, &str)| -> f64 {
        let p1 = crate::components::graph::quadratic_control(from.0, from.1, to.0, to.1, bulge);
        [0.25_f64, 0.5, 0.75]
            .iter()
            .map(|t| {
                let u = 1.0 - t;
                let px = u * u * from.0 + 2.0 * u * t * p1.0 + t * t * to.0;
                let py = u * u * from.1 + 2.0 * u * t * p1.1 + t * t * to.1;
                nodes
                    .iter()
                    .filter(|n| n.id.as_str() != skip.0 && n.id.as_str() != skip.1)
                    .map(|n| {
                        let (hw, hh) = half_of(n);
                        let dx = (px - n.x).abs() - hw;
                        let dy = (py - n.y).abs() - hh;
                        dx.max(dy)
                    })
                    .fold(f64::INFINITY, f64::min)
            })
            .fold(f64::INFINITY, f64::min)
    };
    // 全图重心：两侧余量相当时用它定「往外弯」的方向
    let (cx, cy) = {
        let n = nodes.len().max(1) as f64;
        (
            nodes.iter().map(|x| x.x).sum::<f64>() / n,
            nodes.iter().map(|x| x.y).sum::<f64>() / n,
        )
    };
    let outward_of = |from: (f64, f64), to: (f64, f64), bulge: f64| -> f64 {
        let p1 = crate::components::graph::quadratic_control(from.0, from.1, to.0, to.1, bulge);
        (p1.0 - cx).powi(2) + (p1.1 - cy).powi(2)
    };
    // 只选弯向、不改幅值（幅值承载「组内两两不同」这条不变式）
    let choose_sign = |from: (f64, f64), to: (f64, f64), v: f64, skip: (&str, &str)| -> f64 {
        let cp = clearance(from, to, v, skip);
        let cn = clearance(from, to, -v, skip);
        if (cp - cn).abs() > 1.0 {
            return if cp > cn { 1.0 } else { -1.0 };
        }
        if outward_of(from, to, v) >= outward_of(from, to, -v) {
            1.0
        } else {
            -1.0
        }
    };
    // 幅值与弯向一起选：基础幅值不够躲开时按 ×1.35 / ×1.75 往外撑（上限 factor_cap）。
    // 只有「明显更空」才换档，否则保持基础幅值 —— 不无谓加弯。
    let best_curve = |from: (f64, f64), to: (f64, f64), v: f64, skip: (&str, &str)| -> (f64, f64) {
        let mut pick = (choose_sign(from, to, v, skip), v);
        let mut best = clearance(from, to, pick.0 * pick.1, skip);
        for f in [1.0_f64, 1.35, 1.75] {
            let mag = (v * f).min(factor_cap);
            for sign in [1.0_f64, -1.0] {
                let cl = clearance(from, to, sign * mag, skip);
                if cl > best + 1.0 {
                    best = cl;
                    pick = (sign, mag);
                }
            }
        }
        pick
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
            // 同层判定用「层距的一半」作阈值：同一行的 y 抖动极差 ≤ 0.48 × 层距
            //（见 `relax_layout` 的 jitter 上限），相邻行的最小差必过大半。
            if y_of(&e.target) - y_of(&e.source) < layer_gap * 0.5 {
                Kind::Back(band_of(&e.source) as i64)
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
        let ranked: Vec<(usize, f64)> = idxs
            .iter()
            .enumerate()
            .map(|(rank, &i)| {
                // n 条边 → 弧度取 max_bulge 的 1/(n+1) … n/(n+1)，两两必不相等
                (i, max_bulge * (rank as f64 + 1.0) / (n as f64 + 1.0))
            })
            .collect();
        // 组内公共放大因子：整组同乘才保住「两两不同」，取「最挤那条余量最大」的档
        let mut best_f = 1.0;
        let mut best_worst = f64::NEG_INFINITY;
        for f in [1.0_f64, 1.35, 1.75] {
            let mut worst = f64::INFINITY;
            for &(i, v) in &ranked {
                let (s, t) = (&edges[i].source, &edges[i].target);
                let from = (x_of(s), y_of(s));
                let to = (x_of(t), y_of(t));
                let mag = (v * f).min(factor_cap);
                let cl = clearance(from, to, mag, (s.as_str(), t.as_str())).max(clearance(
                    from,
                    to,
                    -mag,
                    (s.as_str(), t.as_str()),
                ));
                worst = worst.min(cl);
            }
            if worst > best_worst + 1e-9 {
                best_worst = worst;
                best_f = f;
            }
        }
        for &(i, v) in &ranked {
            back_bulge.insert(i, (v * best_f).min(factor_cap));
        }
    }

    // 3) 双向对：组级一次「弯向 + 幅值」决策（组内同号同幅，分居参考线两侧）
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
    let mut pair_curve: HashMap<(String, String), (f64, f64)> = HashMap::new();
    for (key, idxs) in &pair_members {
        let rep = *idxs.iter().min().unwrap();
        let s = edges[rep].source.clone();
        let t = edges[rep].target.clone();
        let (Some(from), Some(to)) = (pos.get(s.as_str()).copied(), pos.get(t.as_str()).copied())
        else {
            continue;
        };
        pair_curve.insert(
            key.clone(),
            best_curve(from, to, base, (s.as_str(), t.as_str())),
        );
    }

    // 4) 写回
    for (i, e) in edges.iter_mut().enumerate() {
        let (s, t) = (e.source.clone(), e.target.clone());
        let from = (x_of(&s), y_of(&s));
        let to = (x_of(&t), y_of(&t));
        let skip = (s.as_str(), t.as_str());
        e.curve = Some(match kinds[i] {
            Kind::Loop => EdgeCurve::Loop,
            Kind::Pair => {
                // 组内同号同幅：A→B 与 B→A 各自按 left-hand 弯，自然分居参考线两侧
                let key = if s <= t {
                    (s.clone(), t.clone())
                } else {
                    (t.clone(), s.clone())
                };
                let (sign, mag) = pair_curve.get(&key).copied().unwrap_or((1.0, base));
                EdgeCurve::Arc(sign * mag)
            }
            Kind::Back(_) => {
                let v = back_bulge.get(&i).copied().unwrap_or(base * 0.6);
                EdgeCurve::Arc(choose_sign(from, to, v, skip) * v)
            }
            // 正常向下：直线优先；但直线若从第三方节点身上穿过，改成绕行弧线
            //（中间夹了节点时直线会「贴着边过去」）。
            Kind::Straight => {
                if clearance(from, to, 0.0, skip) < 0.0 {
                    let (sign, mag) = best_curve(from, to, base * 0.8, skip);
                    EdgeCurve::Arc(sign * mag)
                } else {
                    EdgeCurve::Straight
                }
            }
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

    /// 卡片绝不重叠：力传导松弛后同层有最小水平间距、层间有最小垂直间隙
    #[test]
    fn relaxed_layout_keeps_every_card_apart() {
        let occs = vec![
            occ("Triage", 1, 1, None),
            occ("Prepare", 1, 2, None),
            occ("Deliberate", 1, 3, None),
            occ("Act", 1, 4, None),
            occ("Evaluate", 1, 5, None),
            occ("Triage", 2, 6, None),
        ];
        let g = build_think_path_graph(&occs, &[], 920.0, 620.0, false);
        assert!(g.nodes.len() >= 5, "至少 5 个节点: {}", g.nodes.len());
        for i in 0..g.nodes.len() {
            for j in (i + 1)..g.nodes.len() {
                let a = &g.nodes[i];
                let b = &g.nodes[j];
                let need_x = (crate::components::graph::node_box_width(a)
                    + crate::components::graph::node_box_width(b))
                    / 2.0;
                let need_y = (crate::components::graph::node_box_height(a)
                    + crate::components::graph::node_box_height(b))
                    / 2.0;
                let dx = (a.x - b.x).abs();
                let dy = (a.y - b.y).abs();
                assert!(
                    dx >= need_x - 0.5 || dy >= need_y - 0.5,
                    "卡片 {} 与 {} 重叠: dx={dx:.1} (需 ≥{need_x:.1}) dy={dy:.1} (需 ≥{need_y:.1})",
                    a.id,
                    b.id
                );
            }
        }
    }

    /// 同一层的节点必须错落（力传导松弛的 y 抖动），不能又退回一条水平线
    #[test]
    fn same_layer_nodes_are_staggered_not_collinear() {
        // A→B→C→A 成环 ⇒ 三节点同层
        let occs = vec![
            occ("A", 1, 1, None),
            occ("B", 1, 2, None),
            occ("C", 1, 3, None),
            occ("A", 2, 4, None),
        ];
        let g = build_think_path_graph(&occs, &[], 920.0, 620.0, false);
        let mut ys: Vec<f64> = g.nodes.iter().map(|n| n.y).collect();
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let span = *ys.last().unwrap() - *ys.first().unwrap();
        assert!(span > 0.5, "同层节点应上下错落，实际共线: {ys:?}");
        assert!(span < 60.0, "错落幅度不应大到看不出同层: {ys:?}");
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
        assert!(
            span < 120.0,
            "环上节点仍应在同一层带内（错落幅度有限）: span={span}"
        );
        assert!(
            span > 0.5,
            "同层节点应上下错落而不是排成一条线: span={span}"
        );

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

    /// 缩略图点线形态：圆点必须「看得见 + 不互压」（AMan 第八轮反馈②）
    ///
    /// 旧实现点位按常量 18px 留、渲染按常量 7px 画；现在渲染与布局都走
    /// `dot_node_radius`，本测试钉住「留的位 ≥ 画的点径」这条不变式。
    #[test]
    fn mini_points_are_readable_and_never_overlap() {
        let mut nodes: Vec<GraphNode> = (0..4)
            .map(|i| GraphNode {
                id: format!("n{i}"),
                label: format!("Step{i}"),
                description: String::new(),
                node_type: THINK_STEP_NODE_TYPE.to_string(),
                x: 100.0 + i as f64 * 5.0,
                y: 100.0,
                tags: Vec::new(),
                summary: None,
            })
            .collect();
        spread_layout(&mut nodes, 300.0, 200.0, true);
        let r = crate::components::graph::dot_node_radius(300.0, 200.0, nodes.len());
        assert!(r >= 12.0, "缩略图圆点半径过小（{r}px），会看不清");
        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                let dx = (nodes[i].x - nodes[j].x).abs();
                let dy = (nodes[i].y - nodes[j].y).abs();
                assert!(
                    dx >= 2.0 * r - 1e-6 || dy >= 2.0 * r - 1e-6,
                    "圆点重叠: {i} vs {j} dx={dx} dy={dy} 需 ≥ 直径 {}",
                    2.0 * r
                );
            }
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

    /// 松弛后相邻行不得互相压住：抖动上限必须由「行间实际空隙」决定
    /// （旧实现一律 ±12px；第九轮起按行间空隙放开，这条钉子保证放开后不越界）
    #[test]
    fn relax_never_pushes_rows_into_each_other() {
        // 3 行 × 3 个卡片：先 spread 定行，再 relax 让行内上下错落
        let mut nodes: Vec<GraphNode> = (0..9)
            .map(|i| GraphNode {
                id: format!("n{i}"),
                label: format!("Step{}", i % 3),
                description: String::new(),
                node_type: THINK_STEP_NODE_TYPE.to_string(),
                x: 460.0,
                y: 100.0 + (i / 3) as f64 * 150.0,
                tags: Vec::new(),
                summary: None,
            })
            .collect();
        let layer_gap = spread_layout(&mut nodes, 920.0, 620.0, false);
        relax_layout(&mut nodes, &[], 920.0, 620.0, false, layer_gap);
        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                let hi = crate::components::graph::node_box_height(&nodes[i]) / 2.0;
                let hj = crate::components::graph::node_box_height(&nodes[j]) / 2.0;
                let wi = crate::components::graph::node_box_width(&nodes[i]) / 2.0;
                let wj = crate::components::graph::node_box_width(&nodes[j]) / 2.0;
                let dx = (nodes[i].x - nodes[j].x).abs();
                let dy = (nodes[i].y - nodes[j].y).abs();
                assert!(
                    dy >= hi + hj - 1e-6 || dx >= wi + wj - 1e-6,
                    "卡片重叠: {i} vs {j} dy={dy} 需 ≥ {} 或 dx={dx} 需 ≥ {}",
                    hi + hj,
                    wi + wj
                );
            }
        }
    }
}
