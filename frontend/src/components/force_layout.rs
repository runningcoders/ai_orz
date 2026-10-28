//! 力导向布局算法（纯函数，可单元测试）
//!
//! 模型：
//! - 斥力：所有节点对互相排斥（库仑力，反比于距离平方）
//! - 碰撞分离：节点有实际尺寸时按等效半径保持最小中心距（见下方常量注释）
//! - 吸引力：有连线的节点对互相吸引（胡克定律，正比于距离）
//! - 质心引力：所有节点被弱弹簧拉向**全图质心**（软边界，见 `gravity`）
//! - 阻尼：每帧速度衰减，防止振荡
//! - 无硬边界墙：节点可滑出画布矩形（类无边记），但质心引力保证整体有界，
//!   离散节点不会无限漂移；「适应 / 重置」负责找回内容

use crate::components::canvas_scene::{CanvasEdge, CanvasNode};

/// 碰撞分离强度：越接近 1 推得越硬，过大会在两节点间来回震荡
const COLLISION_STRENGTH: f64 = 0.5;
/// 碰撞最小中心距系数：`(r_i + r_j) * 该值` 为期望最小中心距，1.0 = 刚好不重叠
const COLLISION_PADDING: f64 = 1.15;
/// 连线两端节点的额外留白（仅当两端都有实际尺寸时参与理想长度计算）
const SPRING_GAP: f64 = 60.0;

/// 力导向布局参数
#[derive(Debug, Clone, Copy)]
pub struct ForceLayoutConfig {
    /// 斥力强度系数
    pub repulsion: f64,
    /// 连线吸引力强度系数（弹簧刚度）
    pub attraction: f64,
    /// 理想连线长度（弹簧自然长度）
    pub ideal_length: f64,
    /// 速度阻尼系数（每帧乘以该系数，<1.0 衰减）
    pub damping: f64,
    /// 单帧最大位移（防止爆炸性跳动）
    pub max_step: f64,
    /// 分层约束力强度系数（y 方向拉向目标层的弹簧刚度）
    pub layer_attraction: f64,
    /// 分层布局的层间距（y 方向）
    pub layer_height: f64,
    /// 分层布局的顶部留白
    pub layer_top_margin: f64,
    /// 质心引力强度（软边界）：每帧把节点按 `(质心 - 位置) × gravity` 拉回
    ///
    /// 取当前全图节点均值作质心，所有引力合力恒为 0 —— 只把离散点收回、
    /// 不会让整图平移。0 表示关闭（旧的无边界行为）。
    pub gravity: f64,
}

impl Default for ForceLayoutConfig {
    fn default() -> Self {
        Self {
            repulsion: 8000.0,
            attraction: 0.05,
            ideal_length: 120.0,
            damping: 0.85,
            max_step: 10.0,
            layer_attraction: 0.15,
            layer_height: 80.0,
            layer_top_margin: 80.0,
            gravity: 0.002,
        }
    }
}

/// 节点的速度状态（位置存在 CanvasNode.x/y，速度在此结构）
#[derive(Debug, Clone, Copy, Default)]
pub struct NodeVelocity {
    pub vx: f64,
    pub vy: f64,
}

/// 力导向布局模拟器
#[derive(Debug, Clone)]
pub struct ForceLayout {
    pub config: ForceLayoutConfig,
    pub velocities: Vec<NodeVelocity>,
}

impl ForceLayout {
    /// 创建新的力导向布局模拟器
    pub fn new(config: ForceLayoutConfig) -> Self {
        Self {
            config,
            velocities: Vec::new(),
        }
    }

    /// 同步速度向量数量与节点数量一致
    pub fn sync(&mut self, node_count: usize) {
        self.velocities.resize(node_count, NodeVelocity::default());
    }

    /// 执行一帧力学步进，更新节点位置，返回本帧总位移（用于稳定检测）
    ///
    /// 返回值：所有节点位移之和。当该值趋近于 0 时，布局已稳定。
    ///
    /// `width` / `height` 仅保留为 API 形参（调用方按画布显示尺寸传入）：
    /// 布局本身不感知画布边界，节点坐标是无限世界坐标，画布只是取景窗口。
    /// 整体有界由质心引力保证（见 `gravity`），不靠画布边界 clamp。
    pub fn step(
        &mut self,
        nodes: &mut [CanvasNode],
        edges: &[CanvasEdge],
        _width: f64,
        _height: f64,
    ) -> f64 {
        self.sync(nodes.len());
        let n = nodes.len();
        if n == 0 {
            return 0.0;
        }

        let cfg = self.config;
        let mut forces: Vec<(f64, f64)> = vec![(0.0, 0.0); n];

        // 1. 斥力：所有节点对互相排斥
        for i in 0..n {
            for j in (i + 1)..n {
                let mut dx = nodes[i].x - nodes[j].x;
                let mut dy = nodes[i].y - nodes[j].y;
                // 完全重合时方向未定义：加微小偏移打破对称（沿 x 轴分开）
                if dx.abs() < 1e-9 && dy.abs() < 1e-9 {
                    dx = 1.0;
                    dy = 0.0;
                }
                // 避免除零：距离极小时加微小偏移
                let dist_sq = dx * dx + dy * dy + 0.01;
                let dist = dist_sq.sqrt();
                let force = cfg.repulsion / dist_sq;
                let fx = force * dx / dist;
                let fy = force * dy / dist;
                forces[i].0 += fx;
                forces[i].1 += fy;
                forces[j].0 -= fx;
                forces[j].1 -= fy;

                // 1.1 碰撞分离：节点有实际尺寸（矩形信息卡）时点斥力压不住 —— 1/d² 在
                // 距离很近时才够强，结果是卡片叠成一坨、连线糊在底下。按各自等效半径
                // 算最小中心距，破了就补一记线性强推。
                let min_sep = (nodes[i].radius + nodes[j].radius) * COLLISION_PADDING;
                if dist < min_sep {
                    let push = (min_sep - dist) * COLLISION_STRENGTH;
                    let px = push * dx / dist;
                    let py = push * dy / dist;
                    forces[i].0 += px;
                    forces[i].1 += py;
                    forces[j].0 -= px;
                    forces[j].1 -= py;
                }
            }
        }

        // 2. 吸引力：有连线的节点对互相吸引（胡克定律）
        for edge in edges {
            let i = nodes.iter().position(|node| node.id == edge.from_id);
            let j = nodes.iter().position(|node| node.id == edge.to_id);
            if let (Some(i), Some(j)) = (i, j) {
                if i == j {
                    continue;
                }
                let dx = nodes[j].x - nodes[i].x;
                let dy = nodes[j].y - nodes[i].y;
                let dist = (dx * dx + dy * dy).sqrt().max(0.01);
                // 理想长度按两端实际尺寸放宽：写死 120px 时，168px 宽的卡片连起来
                // 必然互压（120 < 168）；只有带尺寸的节点参与放宽，圆点场景保持原值
                let ideal = cfg
                    .ideal_length
                    .max(nodes[i].radius + nodes[j].radius + SPRING_GAP);
                let displacement = dist - ideal;
                let force = cfg.attraction * displacement;
                let fx = force * dx / dist;
                let fy = force * dy / dist;
                forces[i].0 += fx;
                forces[i].1 += fy;
                forces[j].0 -= fx;
                forces[j].1 -= fy;
            }
        }

        // 2.5 分层约束力：对有 layer 字段的节点，施加 y 方向拉力到目标层
        for i in 0..n {
            if let Some(layer) = nodes[i].layer {
                let target_y = cfg.layer_top_margin + (layer as f64) * cfg.layer_height;
                let dy = target_y - nodes[i].y;
                forces[i].1 += cfg.layer_attraction * dy;
            }
        }

        // 2.6 质心引力（软边界）：向全图质心施加弱弹簧。质心取当前节点均值，
        // 引力合力恒为 0（不会让整图平移），只把离散点 / 离散组件收回来 ——
        // 斥力按 1/d² 衰减、引力按 d 线性增长，二者必有平衡距离，
        // 因此「无边界墙」下整体仍然有界，孤立节点不会无限漂移。
        if cfg.gravity > 0.0 {
            let (mut cx, mut cy) = (0.0, 0.0);
            for node in nodes.iter() {
                cx += node.x;
                cy += node.y;
            }
            let (cx, cy) = (cx / n as f64, cy / n as f64);
            for i in 0..n {
                forces[i].0 += (cx - nodes[i].x) * cfg.gravity;
                forces[i].1 += (cy - nodes[i].y) * cfg.gravity;
            }
        }

        // 3. 应用力到速度，再应用速度到位置（带阻尼和限幅）
        // 不做画布边界 clamp：边界墙会让「拖动节点」撞墙、元素多时挤成一团；
        // 无边界下斥力随距离平方衰减 + is_stable 阈值兜底，散开有限不会失控
        let mut total_displacement = 0.0;
        for i in 0..n {
            self.velocities[i].vx = (self.velocities[i].vx + forces[i].0) * cfg.damping;
            self.velocities[i].vy = (self.velocities[i].vy + forces[i].1) * cfg.damping;

            // 限幅：单帧位移不超过 max_step
            let vx = self.velocities[i].vx.clamp(-cfg.max_step, cfg.max_step);
            let vy = self.velocities[i].vy.clamp(-cfg.max_step, cfg.max_step);
            self.velocities[i].vx = vx;
            self.velocities[i].vy = vy;

            nodes[i].x += vx;
            nodes[i].y += vy;

            total_displacement += vx.abs() + vy.abs();
        }

        total_displacement
    }

    /// 判断布局是否已稳定（总位移小于阈值）
    pub fn is_stable(&self, total_displacement: f64, threshold: f64) -> bool {
        total_displacement < threshold
    }
}

/// 尺寸感知的圆形初始布局：按各节点等效半径分配弧长槽位
///
/// `circle_initial_layout` 按**个数**均分角度，不看节点大小——168px 宽的卡片
/// 在 200px 半径的圆上（槽距 ~125px）头几帧必然互压，要靠随后的力学模拟慢慢
/// 推开（表现为「开图闪一下挤成一坨」）。这里按 `Σ(2r + gap)` 定圆周长度，
/// 大卡片占大弧槽，初始态即不重叠；半径下限仍由调用方给（`min_radius`）。
///
/// 角度均分改弧长正比分：`slot_i = (2·r_i + gap) / R`，相邻槽心的弧距即
/// `r_i + r_{i+1} + gap`，与两圆相切公式一致。
pub fn circle_initial_layout_sized(
    radii: &[f64],
    center_x: f64,
    center_y: f64,
    min_radius: f64,
    gap: f64,
) -> Vec<(f64, f64)> {
    if radii.is_empty() {
        return Vec::new();
    }
    let total_arc: f64 = radii.iter().map(|r| 2.0 * r.max(0.0) + gap).sum();
    // 半径 = 总弧长 / 圆周；下限兜底小图（几个小圆点不必撑成大圆）
    let radius = (total_arc / std::f64::consts::TAU).max(min_radius).max(1.0);
    let mut positions = Vec::with_capacity(radii.len());
    let mut acc = 0.0;
    for r in radii {
        let slot = (2.0 * r.max(0.0) + gap) / radius;
        let angle = acc + slot / 2.0;
        acc += slot;
        positions.push((
            center_x + radius * angle.cos(),
            center_y + radius * angle.sin(),
        ));
    }
    positions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::canvas_scene::{CanvasEdge, CanvasNode};

    fn make_node(id: &str, x: f64, y: f64) -> CanvasNode {
        CanvasNode {
            id: id.to_string(),
            x,
            y,
            radius: 20.0,
            label: id.to_string(),
            color: "#3b82f6".to_string(),
            node_type: None,
            layer: None,
            ..Default::default()
        }
    }

    #[test]
    fn test_repulsion_pushes_nodes_apart() {
        let mut nodes = vec![make_node("a", 100.0, 100.0), make_node("b", 100.0, 100.0)];
        let edges: Vec<CanvasEdge> = vec![];
        let mut layout = ForceLayout::new(ForceLayoutConfig::default());

        let before_dist =
            ((nodes[0].x - nodes[1].x).powi(2) + (nodes[0].y - nodes[1].y).powi(2)).sqrt();
        layout.step(&mut nodes, &edges, 800.0, 600.0);
        let after_dist =
            ((nodes[0].x - nodes[1].x).powi(2) + (nodes[0].y - nodes[1].y).powi(2)).sqrt();

        assert!(
            after_dist > before_dist,
            "斥力应将重合节点推开: before={}, after={}",
            before_dist,
            after_dist
        );
    }

    #[test]
    fn test_attraction_pulls_connected_nodes_closer() {
        let mut nodes = vec![make_node("a", 50.0, 300.0), make_node("b", 750.0, 300.0)];
        let edges = vec![CanvasEdge {
            from_id: "a".to_string(),
            to_id: "b".to_string(),
            ..Default::default()
        }];
        let config = ForceLayoutConfig {
            repulsion: 100.0,
            attraction: 0.2,
            ideal_length: 120.0,
            damping: 0.9,
            max_step: 50.0,
            layer_attraction: 0.15,
            layer_height: 80.0,
            layer_top_margin: 80.0,
            // 关掉软边界：本测试只验证胡克吸引力，质心引力会让两端同时内收，
            // 干扰「距离是否真的缩短」的判定基准
            gravity: 0.0,
        };
        let mut layout = ForceLayout::new(config);

        let before_dist =
            ((nodes[0].x - nodes[1].x).powi(2) + (nodes[0].y - nodes[1].y).powi(2)).sqrt();
        for _ in 0..10 {
            layout.step(&mut nodes, &edges, 800.0, 600.0);
        }
        let after_dist =
            ((nodes[0].x - nodes[1].x).powi(2) + (nodes[0].y - nodes[1].y).powi(2)).sqrt();

        assert!(
            after_dist < before_dist,
            "吸引力应拉近连线节点: before={}, after={}",
            before_dist,
            after_dist
        );
    }

    /// 无边界画布：节点受力后应能滑出「画布矩形」而不是被按回可视区。
    ///
    /// 画布只是取景窗口（类无边记），拖动不撞墙；找回内容靠「适应 / 重置」。
    #[test]
    fn test_nodes_free_to_leave_canvas_bounds() {
        // 两个节点叠在画布左上角附近：斥力沿连线向外推，a 应滑出左边界
        let mut nodes = vec![make_node("a", 5.0, 5.0), make_node("b", 12.0, 5.0)];
        let edges: Vec<CanvasEdge> = vec![];
        let mut layout = ForceLayout::new(ForceLayoutConfig::default());

        layout.step(&mut nodes, &edges, 800.0, 600.0);

        assert!(
            nodes[0].x < 5.0,
            "节点应能滑出画布边界（无边界墙）: x={}",
            nodes[0].x
        );
    }

    /// 软边界（质心引力）：离群很远的孤立节点必须被拉回，不能无限扩散。
    ///
    /// 回归「力导向布局没有边界，一旦产生离散的点就会无限扩散，不好找到」：
    /// 斥力按 1/d² 衰减、质心引力按 d 线性增长，二者必有平衡距离 ——
    /// 不需要画布边界墙，整体也是有界的。
    #[test]
    fn test_gravity_pulls_stray_node_back() {
        let mut nodes = vec![
            make_node("a", 0.0, 0.0),
            make_node("b", 100.0, 0.0),
            make_node("c", 0.0, 100.0),
            make_node("stray", 1500.0, 1500.0),
        ];
        let edges: Vec<CanvasEdge> = vec![];
        let mut layout = ForceLayout::new(ForceLayoutConfig::default());

        let dist = |nodes: &[CanvasNode]| {
            ((nodes[3].x - nodes[0].x).powi(2) + (nodes[3].y - nodes[0].y).powi(2)).sqrt()
        };
        let before = dist(&nodes);
        for _ in 0..600 {
            layout.step(&mut nodes, &edges, 800.0, 600.0);
        }
        let after = dist(&nodes);

        assert!(
            after < before * 0.5,
            "离散节点应被质心引力拉回: before={before:.1}, after={after:.1}"
        );
    }

    /// 质心引力只收缩、不整体平移：对称布局下引力合力为 0，质心保持不动。
    #[test]
    fn test_gravity_keeps_centroid_fixed() {
        // 只留质心引力（其余力全关），验证它收缩而非搬运整图
        let config = ForceLayoutConfig {
            repulsion: 0.0,
            attraction: 0.0,
            ideal_length: 0.0,
            damping: 0.9,
            max_step: 50.0,
            layer_attraction: 0.0,
            layer_height: 0.0,
            layer_top_margin: 0.0,
            gravity: 0.05,
        };
        let mut nodes = vec![make_node("a", -400.0, 0.0), make_node("b", 400.0, 0.0)];
        let edges: Vec<CanvasEdge> = vec![];
        let mut layout = ForceLayout::new(config);

        for _ in 0..40 {
            layout.step(&mut nodes, &edges, 800.0, 600.0);
        }
        let gap = (nodes[1].x - nodes[0].x).abs();
        assert!(gap < 800.0, "质心引力应把两端内收: gap={gap:.1}");
        assert!(
            (nodes[0].x + nodes[1].x).abs() < 1e-6,
            "对称布局的质心不应平移: sum={:.3}",
            nodes[0].x + nodes[1].x
        );
    }

    #[test]
    fn test_stable_detection() {
        let layout = ForceLayout::new(ForceLayoutConfig::default());
        assert!(layout.is_stable(0.1, 1.0), "小位移应判为稳定");
        assert!(!layout.is_stable(100.0, 1.0), "大位移应判为不稳定");
    }

    /// 尺寸感知初始布局：大卡片必须占大弧槽，相邻中心距 ≥ 两等效半径和
    ///（回归「开图头几帧卡片互压」：按个数均分角度时 168px 宽卡在 200px 半径
    /// 圆上槽距仅 ~125px）
    #[test]
    fn test_circle_initial_layout_sized_keeps_cards_apart() {
        // 3 张 168×74 卡片（等效半径 84）+ 2 个小圆点（半径 12）
        let radii = [84.0, 84.0, 84.0, 12.0, 12.0];
        let positions = circle_initial_layout_sized(&radii, 400.0, 300.0, 100.0, 24.0);
        assert_eq!(positions.len(), radii.len());
        // 相邻（含首尾环绕）中心距 ≥ r_i + r_j（相切），gap 只增加余量
        for i in 0..positions.len() {
            let j = (i + 1) % positions.len();
            let d = ((positions[i].0 - positions[j].0).powi(2)
                + (positions[i].1 - positions[j].1).powi(2))
            .sqrt();
            let need = radii[i] + radii[j];
            assert!(
                d >= need - 1.0,
                "相邻节点应至少相切: pair=({i},{j}) dist={d:.1} need={need:.1}"
            );
        }
    }

    #[test]
    fn test_circle_initial_layout_sized_edge_cases() {
        assert!(circle_initial_layout_sized(&[], 0.0, 0.0, 100.0, 24.0).is_empty());
        // 单个节点：半径下限兜底，不除零
        let one = circle_initial_layout_sized(&[7.0], 400.0, 300.0, 100.0, 24.0);
        assert_eq!(one.len(), 1);
        // 半径不得小于下限（几个小圆点不该被撑成占满画布的大圆）
        let dots = circle_initial_layout_sized(&[7.0; 4], 400.0, 300.0, 100.0, 24.0);
        let r = ((dots[0].0 - 400.0).powi(2) + (dots[0].1 - 300.0).powi(2)).sqrt();
        assert!((r - 100.0).abs() < 0.01, "小图应保留下限半径: {r}");
    }

    #[test]
    fn test_empty_nodes_step() {
        let mut layout = ForceLayout::new(ForceLayoutConfig::default());
        let mut nodes: Vec<CanvasNode> = vec![];
        let edges: Vec<CanvasEdge> = vec![];
        let displacement = layout.step(&mut nodes, &edges, 800.0, 600.0);
        assert_eq!(displacement, 0.0);
    }

    #[test]
    fn test_self_loop_edge_ignored() {
        let mut nodes = vec![make_node("a", 100.0, 100.0)];
        let edges = vec![CanvasEdge {
            from_id: "a".to_string(),
            to_id: "a".to_string(),
            ..Default::default()
        }];
        let mut layout = ForceLayout::new(ForceLayoutConfig::default());
        let displacement = layout.step(&mut nodes, &edges, 800.0, 600.0);
        assert!(
            displacement < 1.0,
            "单节点自环不应产生位移: {}",
            displacement
        );
    }

    /// 大尺寸节点（矩形信息卡）必须被碰撞分离推开。
    ///
    /// 回归「知识图谱挤成一坨」：只靠 1/d² 点斥力时，168px 宽的卡片在近距离
    /// 压不住，会叠在同一点上；有了按等效半径的碰撞分离，中心距应达到
    /// `(r1 + r2) * 1.15` 量级。
    #[test]
    fn test_collision_separates_large_cards() {
        let mut a = make_node("a", 400.0, 300.0);
        let mut b = make_node("b", 404.0, 300.0);
        // 84 = 168px 宽卡片的半宽，即卡片节点的等效半径
        a.radius = 84.0;
        b.radius = 84.0;
        let mut nodes = vec![a, b];
        let edges: Vec<CanvasEdge> = vec![];
        let mut layout = ForceLayout::new(ForceLayoutConfig::default());

        for _ in 0..240 {
            layout.step(&mut nodes, &edges, 900.0, 700.0);
        }
        let dist = ((nodes[0].x - nodes[1].x).powi(2) + (nodes[0].y - nodes[1].y).powi(2)).sqrt();
        assert!(
            dist > 170.0,
            "168px 宽的卡片应被碰撞分离推开，实际中心距仅 {dist:.1}px"
        );
    }
}
