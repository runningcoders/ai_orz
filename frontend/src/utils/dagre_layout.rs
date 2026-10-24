//! dagre 分层布局适配层（任务依赖图 / 思考轨迹图 / 工作台 ProjectDetail 共用，纯函数可单测）
//!
//! # 为什么换掉自研四步管线
//!
//! 旧管线（Kahn 分层 → spread → 力导向松弛 → 弧线避障）有三个结构性问题：
//!
//! 1. `compute_layered_layout` 同层均分**不知卡片宽度**——920px 画布上 5 个同层
//!    节点槽宽 151px，而长名卡片宽 168px，卡片直接互压；`spread_layout` 只是补救，
//!    密度高时仍靠力导向软推。
//! 2. 弧线避障是 O(边×节点) 局部启发式——多条线密集交错时仍可能穿卡（旧代码注释
//!    自认「要彻底解决需引入真正的图布局库」）。
//! 3. 思考轨迹几乎必然成环（`think → dispatch → evaluate → think`，本机 116 条
//!    轨迹 95 条如此）——旧管线靠自研破环规则（first_seq 早于才参与分层）绕开
//!    Kahn 的环沉底，是特例补丁。
//!
//! dagre（Sugiyama 框架工业实现）一次做完四件事：**acyclic 破环**（原生，回边
//! 反转后布局、渲染时绕行）、**network simplex 分层**、**Brandes-Köpf 坐标**
//! （节点尺寸感知，同层不重叠由坐标层面保证）、**dummy node 边路由**（输出
//! waypoints 折线，取代避障启发式）。
//!
//! # 坐标映射
//!
//! dagre 输出自己的坐标系（从 0 起算）。这里按包围盒**等比缩放 + 居中**映射进
//! 画布（四周留 margin）：任何规模的图都完整落进画布，「节点飞出 / 贴边」从根上
//! 消失。缩放上限 **2.5**：小图也会放大铺满画布（此前上限 1.0，导致 4 节点图缩在
//! 中间一小簇、点挤成一团、连线缝隙仅十几 px 看不见）。放大**只作用于坐标**，节点
//! 按原尺寸绘制，所以间距被拉开而点不会跟着变大。

use std::collections::{HashMap, HashSet};

use dagre_dgl_rs::{EdgeLabel, Graph, GraphLabel, NodeLabel, layout};

/// 同组平行边（含双向对）的错开间距（px）
///
/// dagre 把同一对节点的多条边路由到**同一条**路径（双向对更是逐点重合），不挪开就会
/// 边叠边、标签叠标签。24px 略大于边标签衬底高度，保证两条标签不互相压住。
const PARALLEL_EDGE_GAP: f64 = 24.0;

/// 喂给 dagre 的节点（含视觉尺寸：卡片态逐节点算宽高，点线态统一点径）
#[derive(Debug, Clone)]
pub struct DagreNode {
    pub id: String,
    pub width: f64,
    pub height: f64,
}

/// 喂给 dagre 的边（source → target；自环不在此列，见 [`DagreLayout::self_loops`]）
#[derive(Debug, Clone)]
pub struct DagreEdge {
    pub source: String,
    pub target: String,
}

/// dagre 布局结果（全部为画布坐标系）
#[derive(Debug, Clone, Default)]
pub struct DagreLayout {
    /// 节点中心坐标
    pub positions: HashMap<String, (f64, f64)>,
    /// (source, target) → 折线点序列（不含两端节点中心；方向已校验为 source→target）
    pub routes: HashMap<(String, String), Vec<(f64, f64)>>,
    /// 自环边集合（dagre 不布局自环，调用方走 `EdgeCurve::Loop`）
    pub self_loops: HashSet<(String, String)>,
}

/// 卡片态 / 点线态的间距与留白参数
struct Spacing {
    /// dagre 同层节点最小间距（不含节点宽）
    nodesep: f64,
    /// dagre 层间最小间距（不含节点高）
    ranksep: f64,
    /// 映射进画布时的四周留白（px，给箭头 / 边标签留空间）
    margin: f64,
}

fn spacing_for(mini: bool) -> Spacing {
    if mini {
        // 点线态：点径本身小，间距沿用旧 spread_layout 的 MINI_H_GAP 口径
        Spacing {
            nodesep: 12.0,
            ranksep: 14.0,
            margin: 10.0,
        }
    } else {
        // 卡片态：沿用旧 spread_layout 的 MIN_H_GAP / MIN_V_GAP
        Spacing {
            nodesep: 26.0,
            ranksep: 30.0,
            margin: 24.0,
        }
    }
}

/// 边路由原始输出（画布映射前）：(source, target) → 折线点序列
type RawRoute = ((String, String), Vec<(f64, f64)>);

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// 对图跑 dagre 分层布局并映射进画布
///
/// # 参数
///
/// - `nodes`：节点（含视觉尺寸）。卡片态逐节点传 `node_box_width/height`，
///   点线态统一点径（`dot_node_radius × 2`）。
/// - `edges`：有向边（source → target）。自环不喂给 dagre（dagre 对 `source==target`
///   不产出可用 waypoints），记录进 `self_loops` 由调用方走 `EdgeCurve::Loop`。
/// - `width` / `height`：画布尺寸。`mini`：`true` = 缩略图点线态，`false` = 卡片态。
///
/// # 容错
///
/// 空节点 → 空结果（调用方降级徽章列表）；单节点 / 全自环图 → dagre 孤立节点
/// 布局 + 缩放居中，不 panic。
pub fn dagre_layout(
    nodes: &[DagreNode],
    edges: &[DagreEdge],
    width: f64,
    height: f64,
    mini: bool,
) -> DagreLayout {
    let mut out = DagreLayout::default();
    if nodes.is_empty() {
        return out;
    }

    // 自环分离 + 防御性去重（dagre 简单图模式下同 (source,target) 只应有一条）
    let mut self_loops = HashSet::new();
    let mut dagre_edges: Vec<&DagreEdge> = Vec::new();
    let mut seen: HashSet<(&str, &str)> = HashSet::new();
    for e in edges {
        if e.source == e.target {
            self_loops.insert((e.source.clone(), e.target.clone()));
            continue;
        }
        if seen.insert((e.source.as_str(), e.target.as_str())) {
            dagre_edges.push(e);
        }
    }
    out.self_loops = self_loops;

    let spacing = spacing_for(mini);
    // with_options(multigraph=false, directed=true, compound=false)：有向简单图，
    // 与旧 Kahn 分层同款口径
    let mut g = Graph::with_options(false, true, false);
    g.set_graph(GraphLabel {
        rankdir: Some("TB".to_string()),
        nodesep: Some(spacing.nodesep),
        ranksep: Some(spacing.ranksep),
        ..Default::default()
    });
    for n in nodes {
        g.set_node(
            &n.id,
            NodeLabel {
                width: n.width,
                height: n.height,
                ..Default::default()
            },
        );
    }
    for e in &dagre_edges {
        g.set_edge(&e.source, &e.target, EdgeLabel::default(), None);
    }
    layout(&mut g);

    // ---- 读回节点中心 ----
    let raw_nodes: Vec<(String, (f64, f64), f64, f64)> = nodes
        .iter()
        .map(|n| {
            let label = g.node(&n.id);
            (
                n.id.clone(),
                (label.x.unwrap_or(0.0), label.y.unwrap_or(0.0)),
                n.width,
                n.height,
            )
        })
        .collect();
    let center_of = |id: &str| -> (f64, f64) {
        raw_nodes
            .iter()
            .find(|(nid, ..)| nid == id)
            .map(|(_, p, ..)| *p)
            .unwrap_or((0.0, 0.0))
    };

    // ---- 读回边 waypoints（拉平 dummy node 分段）+ 方向校验 ----
    //
    // dagre 的 acyclic 阶段会反转回边（如 evaluate→think 反转成 think→evaluate
    // 做布局），拉平后的点序列方向**不保证**仍是 source→target。按「首点离谁更
    // 近」校验：反向代价更小就 reverse，保证箭头永远落在 target 端。
    let mut raw_routes: Vec<RawRoute> = Vec::new();
    for e in &dagre_edges {
        let Some(label) = g.edge_vw(&e.source, &e.target) else {
            continue;
        };
        let mut pts: Vec<(f64, f64)> = label
            .points
            .iter()
            .flat_map(|seg| seg.iter().map(|p| (p.x, p.y)))
            .collect();
        if pts.len() < 2 {
            continue;
        }
        let src = center_of(&e.source);
        let tgt = center_of(&e.target);
        let forward = dist(pts[0], src) + dist(pts[pts.len() - 1], tgt);
        let backward = dist(pts[0], tgt) + dist(pts[pts.len() - 1], src);
        if backward < forward {
            pts.reverse();
        }
        raw_routes.push(((e.source.clone(), e.target.clone()), pts));
    }

    // ---- 同组平行边（含双向对）横向错开 ----
    //
    // dagre 对同一对节点的多条边给出**完全重合**的折线：正向边 A→B 与被 acyclic
    // 反转的 B→A 走同一条路径（只是点的顺序反过来）。于是两条边叠在一起、两个边
    // 标签也压在**同一处**（用户截图：两条标签糊成一坨，箭头叠着两个方向）。
    // 这里按「垂直于首条边走向」把同组边逐条错开：两个方向各偏一侧，线与标签
    // 自然分开。错开量对全组统一，整体平移不改变折线形状。
    {
        let mut groups: HashMap<(String, String), Vec<usize>> = HashMap::new();
        for (i, ((s, t), _)) in raw_routes.iter().enumerate() {
            let key = if s <= t {
                (s.clone(), t.clone())
            } else {
                (t.clone(), s.clone())
            };
            groups.entry(key).or_default().push(i);
        }
        for (_, idxs) in groups {
            if idxs.len() < 2 {
                continue;
            }
            let (ref_s, ref_t) = raw_routes[idxs[0]].0.clone();
            let a = center_of(&ref_s);
            let b = center_of(&ref_t);
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len = (dx * dx + dy * dy).sqrt();
            if len < f64::EPSILON {
                continue;
            }
            // 垂直于首条边走向的单位向量
            let perp = (-dy / len, dx / len);
            let n = idxs.len() as f64;
            for (k, &i) in idxs.iter().enumerate() {
                let off = (k as f64 - (n - 1.0) / 2.0) * PARALLEL_EDGE_GAP;
                if off.abs() < f64::EPSILON {
                    continue;
                }
                for p in raw_routes[i].1.iter_mut() {
                    p.0 += perp.0 * off;
                    p.1 += perp.1 * off;
                }
            }
        }
    }

    // ---- 包围盒（节点矩形 + 所有 waypoint）→ 等比缩放 + 居中 ----
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (_, (x, y), w, h) in &raw_nodes {
        min_x = min_x.min(x - w / 2.0);
        max_x = max_x.max(x + w / 2.0);
        min_y = min_y.min(y - h / 2.0);
        max_y = max_y.max(y + h / 2.0);
    }
    for (_, pts) in &raw_routes {
        for (x, y) in pts {
            min_x = min_x.min(*x);
            max_x = max_x.max(*x);
            min_y = min_y.min(*y);
            max_y = max_y.max(*y);
        }
    }
    if min_x > max_x || min_y > max_y {
        // 理论不可达（nodes 非空即有矩形）；防御性兜底不 panic
        for (id, (_, y), ..) in &raw_nodes {
            out.positions
                .insert(id.clone(), (width / 2.0, (height / 2.0).max(*y)));
        }
        return out;
    }
    let bbox_w = (max_x - min_x).max(1.0);
    let bbox_h = (max_y - min_y).max(1.0);
    let avail_w = (width - 2.0 * spacing.margin).max(1.0);
    let avail_h = (height - 2.0 * spacing.margin).max(1.0);
    // 上限 2.5：小图**放大**铺满画布（此前锁 1.0，4 节点图缩在中间一小簇、
    // 点挤成一团、连线缝隙只剩十几 px 看不见）；下限 0.05：极端密集图保底可读。
    // 放大只作用于坐标（节点本身按原尺寸绘制），所以点不会跟着变大，间距被拉开。
    // 2.5 而非 2.0：点径按「直径缩短 1/3」下调后 bbox 更小，4 节点缩略图 fit 比约
    // 2.31，卡在 2.0 会填不满画布。
    let scale = (avail_w / bbox_w).min(avail_h / bbox_h).clamp(0.05, 2.5);
    let offset_x = (width - bbox_w * scale) / 2.0 - min_x * scale;
    let offset_y = (height - bbox_h * scale) / 2.0 - min_y * scale;
    let map = |p: (f64, f64)| (p.0 * scale + offset_x, p.1 * scale + offset_y);

    for (id, p, ..) in &raw_nodes {
        out.positions.insert(id.clone(), map(*p));
    }
    for (key, pts) in raw_routes {
        out.routes
            .insert(key, pts.iter().map(|p| map(*p)).collect());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str) -> DagreNode {
        DagreNode {
            id: id.to_string(),
            width: 168.0,
            height: 74.0,
        }
    }

    fn edge(from: &str, to: &str) -> DagreEdge {
        DagreEdge {
            source: from.to_string(),
            target: to.to_string(),
        }
    }

    /// 三节点环 + 分叉（思维轨迹真实结构）
    fn cyclic_graph() -> (Vec<DagreNode>, Vec<DagreEdge>) {
        (
            vec![node("think"), node("dispatch"), node("evaluate")],
            vec![
                edge("think", "dispatch"),
                edge("dispatch", "evaluate"),
                edge("evaluate", "think"),
            ],
        )
    }

    #[test]
    fn empty_nodes_yields_empty_layout() {
        let out = dagre_layout(&[], &[], 920.0, 620.0, false);
        assert!(out.positions.is_empty());
        assert!(out.routes.is_empty());
    }

    #[test]
    fn cyclic_graph_keeps_back_edge_as_route() {
        let (nodes, edges) = cyclic_graph();
        let out = dagre_layout(&nodes, &edges, 920.0, 620.0, false);
        assert_eq!(out.positions.len(), 3);
        // 环边仍作为折线路由产出（不丢边）
        assert!(
            out.routes
                .contains_key(&("evaluate".to_string(), "think".to_string())),
            "back edge must still be routed, got {:?}",
            out.routes.keys().collect::<Vec<_>>()
        );
        // 所有坐标落进画布（含 margin 口径：不贴边）
        for (id, (x, y)) in &out.positions {
            assert!(
                *x >= 0.0 && *x <= 920.0 && *y >= 0.0 && *y <= 620.0,
                "node {id} escaped canvas: ({x}, {y})"
            );
        }
    }

    #[test]
    fn same_layer_nodes_do_not_overlap() {
        // 5 个同层宽卡节点（旧管线的重灾区：槽宽 151 < 卡宽 168）
        let nodes: Vec<DagreNode> = (0..5).map(|i| node(&format!("leaf{i}"))).collect();
        let mut edges = Vec::new();
        for i in 0..5 {
            edges.push(edge("root", &format!("leaf{i}")));
        }
        let mut all = vec![node("root")];
        all.extend(nodes);
        let out = dagre_layout(&all, &edges, 920.0, 620.0, false);
        let leaves: Vec<(f64, f64)> = (0..5).map(|i| out.positions[&format!("leaf{i}")]).collect();
        // 同层节点按 x 排序后，相邻中心距必须大于卡宽（不重叠由坐标层面保证）
        let mut xs: Vec<f64> = leaves.iter().map(|(x, _)| *x).collect();
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for pair in xs.windows(2) {
            assert!(
                pair[1] - pair[0] >= 168.0,
                "same-layer cards overlap: gap {} < 168",
                pair[1] - pair[0]
            );
        }
    }

    #[test]
    fn self_loops_are_separated_not_routed() {
        let (nodes, mut edges) = cyclic_graph();
        edges.push(edge("dispatch", "dispatch"));
        let out = dagre_layout(&nodes, &edges, 920.0, 620.0, false);
        assert!(
            out.self_loops
                .contains(&("dispatch".to_string(), "dispatch".to_string())),
            "self loop must be separated"
        );
        assert!(
            !out.routes
                .contains_key(&("dispatch".to_string(), "dispatch".to_string())),
            "self loop must not go through dagre routing"
        );
    }

    #[test]
    fn single_node_layout_centers_in_canvas() {
        let out = dagre_layout(&[node("solo")], &[], 920.0, 620.0, false);
        let (x, y) = out.positions["solo"];
        assert!((x - 460.0).abs() < 1.0, "x={x}");
        assert!((y - 310.0).abs() < 1.0, "y={y}");
    }

    #[test]
    fn route_direction_is_source_to_target() {
        // 环边（acyclic 会反转的那条）：折线首点必须离 source 更近
        let (nodes, edges) = cyclic_graph();
        let out = dagre_layout(&nodes, &edges, 920.0, 620.0, false);
        let pts = &out.routes[&("evaluate".to_string(), "think".to_string())];
        let src = out.positions["evaluate"];
        let tgt = out.positions["think"];
        let forward = dist(pts[0], src) + dist(pts[pts.len() - 1], tgt);
        let backward = dist(pts[0], tgt) + dist(pts[pts.len() - 1], src);
        assert!(
            forward <= backward,
            "route direction wrong: first point closer to target"
        );
    }

    #[test]
    fn mini_and_card_modes_both_fit_canvas() {
        let (nodes, edges) = cyclic_graph();
        for (w, h, mini) in [(300.0, 200.0, true), (920.0, 620.0, false)] {
            let out = dagre_layout(&nodes, &edges, w, h, mini);
            for (id, (x, y)) in &out.positions {
                assert!(
                    *x >= 0.0 && *x <= w && *y >= 0.0 && *y <= h,
                    "mini={mini} node {id} escaped canvas ({w}x{h}): ({x}, {y})"
                );
            }
        }
    }

    /// 小图必须放大铺满画布（此前 scale 锁 1.0，4 节点缩在中间一小簇）
    #[test]
    fn small_graph_scales_up_to_fill_canvas() {
        let dot = 40.0;
        let mk = |id: &str| DagreNode {
            id: id.to_string(),
            width: dot,
            height: dot,
        };
        let nodes = vec![mk("a"), mk("b"), mk("c"), mk("d")];
        let edges = vec![edge("a", "d"), edge("b", "d"), edge("c", "d")];
        let out = dagre_layout(&nodes, &edges, 300.0, 200.0, true);
        let xs: Vec<f64> = ["a", "b", "c"]
            .iter()
            .map(|id| out.positions[*id].0)
            .collect();
        let span = xs.iter().cloned().fold(f64::MIN, f64::max)
            - xs.iter().cloned().fold(f64::MAX, f64::min);
        // 同层三点中心跨度：scale=1.0 时约 104（挤成一簇），放大后应 > 150
        assert!(
            span > 150.0,
            "small graph not spread to fill canvas: span={span}"
        );
    }

    /// 折线弧长中点（用于比较两条折线是否重合）
    fn polyline_mid(pts: &[(f64, f64)]) -> (f64, f64) {
        let total: f64 = pts.windows(2).map(|w| dist(w[0], w[1])).sum();
        if total < f64::EPSILON {
            return pts[0];
        }
        let mut acc = 0.0;
        for w in pts.windows(2) {
            let seg = dist(w[0], w[1]);
            if acc + seg >= total / 2.0 {
                let t = if seg < f64::EPSILON {
                    0.0
                } else {
                    (total / 2.0 - acc) / seg
                };
                return (
                    w[0].0 + (w[1].0 - w[0].0) * t,
                    w[0].1 + (w[1].1 - w[0].1) * t,
                );
            }
            acc += seg;
        }
        pts[pts.len() - 1]
    }

    /// 双向对（A→B + B→A）：dagre 把两条边路由到**同一条**路径，必须错开，
    /// 否则两条边与两个边标签完全重叠（用户截图：标签糊成一坨）
    #[test]
    fn bidirectional_pair_routes_are_separated() {
        let nodes = vec![node("a"), node("b")];
        let mut edges = vec![edge("a", "b")];
        edges.push(edge("b", "a"));
        let out = dagre_layout(&nodes, &edges, 920.0, 620.0, false);
        let ab = &out.routes[&("a".to_string(), "b".to_string())];
        let ba = &out.routes[&("b".to_string(), "a".to_string())];
        let sep = dist(polyline_mid(ab), polyline_mid(ba));
        assert!(
            sep >= PARALLEL_EDGE_GAP * 0.9,
            "双向边折线仍重合: 中点间距 {sep}"
        );
    }
}
