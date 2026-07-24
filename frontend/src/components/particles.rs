//! 粒子系统模块
//!
//! 提供 4 种粒子效果：
//! - 数据流粒子（DataFlowParticles）：连线上从 source 流向 target 的能量粒子
//! - 节点辉光粒子（GlowParticles）：节点被 hover/选中时向外扩散的粒子
//! - 背景粒子（BackgroundParticles）：场景中漂浮的环境粒子，营造氛围
//! - 节点诞生/消亡粒子（BirthDeathParticles）：节点新增时的爆发、删除时的消散
//!
//! 所有粒子系统实现 ParticleSystem trait，在 CanvasScene 渲染循环中被调用。

use web_sys::CanvasRenderingContext2d;

use crate::components::canvas_scene::{CanvasEdge, CanvasNode};

/// 单个粒子状态
#[derive(Debug, Clone)]
pub struct Particle {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    /// 剩余生命周期（秒），<=0 表示已死亡
    pub life: f64,
    /// 最大生命周期（秒），用于计算透明度
    pub max_life: f64,
    /// 粒子半径
    pub radius: f64,
    /// 粒子颜色（rgba 字符串）
    pub color: String,
}

impl Particle {
    /// 创建新粒子
    pub fn new(x: f64, y: f64, vx: f64, vy: f64, life: f64, radius: f64, color: String) -> Self {
        Self {
            x,
            y,
            vx,
            vy,
            life,
            max_life: life,
            radius,
            color,
        }
    }

    /// 粒子是否已死亡
    pub fn is_dead(&self) -> bool {
        self.life <= 0.0
    }

    /// 透明度（基于剩余生命占最大生命的比例）
    pub fn alpha(&self) -> f64 {
        if self.max_life > 0.0 {
            (self.life / self.max_life).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// 更新粒子位置和生命周期
    pub fn update(&mut self, dt: f64) {
        self.x += self.vx * dt;
        self.y += self.vy * dt;
        self.life -= dt;
    }

    /// 绘制单个粒子（带透明度渐变）
    pub fn draw(&self, ctx: &CanvasRenderingContext2d) {
        let alpha = self.alpha();
        if alpha <= 0.0 {
            return;
        }
        let rgba = color_with_alpha(&self.color, alpha);
        ctx.set_fill_style_str(&rgba);
        ctx.begin_path();
        let _ = ctx.arc(self.x, self.y, self.radius, 0.0, std::f64::consts::TAU);
        ctx.fill();
    }
}

/// 粒子系统 trait：所有粒子效果实现此接口
pub trait ParticleSystem {
    /// 每帧更新粒子状态（位置、生命周期）
    fn update(&mut self, dt: f64);

    /// 绘制所有活跃粒子
    fn draw(&self, ctx: &CanvasRenderingContext2d);

    /// 粒子数量
    fn count(&self) -> usize;
}

/// 将 #rrggbb 颜色转为带透明度的 rgba(r, g, b, a) 字符串
///
/// 若输入已是 rgba 格式则直接返回（忽略 alpha 参数）
pub fn color_with_alpha(hex: &str, alpha: f64) -> String {
    if hex.starts_with("rgba") || hex.starts_with("rgb(") {
        return hex.to_string();
    }
    if let Some(hex_str) = hex.strip_prefix('#') {
        if hex_str.len() == 6 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&hex_str[0..2], 16),
                u8::from_str_radix(&hex_str[2..4], 16),
                u8::from_str_radix(&hex_str[4..6], 16),
            ) {
                return format!("rgba({}, {}, {}, {:.2})", r, g, b, alpha);
            }
        }
    }
    format!("rgba(255, 255, 255, {:.2})", alpha)
}

/// 生成指定范围内的随机浮点数（xorshift64 伪随机，WASM 无 std::time）
pub fn random_range(min: f64, max: f64) -> f64 {
    use std::cell::Cell;
    thread_local! {
        static SEED: Cell<u64> = Cell::new(12345);
    }
    SEED.with(|s| {
        let mut x = s.get();
        if x == 0 {
            x = 12345;
        }
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        s.set(x);
        let normalized = (x as f64) / (u64::MAX as f64);
        min + normalized * (max - min)
    })
}

/// 计算两点之间的距离
pub fn distance(x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    let dx = x2 - x1;
    let dy = y2 - y1;
    (dx * dx + dy * dy).sqrt()
}

/// 线性插值
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

// ==================== 数据流粒子系统 ====================

/// 数据流粒子系统：在连线上生成从 source 流向 target 的能量粒子
///
/// 每条连线维护自己的粒子列表。粒子从 from 节点出发，沿连线方向移动到 to 节点，
/// 到达后消失并在起点重新生成。粒子的颜色继承 from 节点的颜色。
pub struct DataFlowParticles {
    /// 所有活跃粒子
    particles: Vec<Particle>,
    /// 粒子速度（像素/秒）
    pub speed: f64,
    /// 每条连线的粒子生成间隔（秒）
    pub spawn_interval: f64,
    /// 距离上次生成的时间累计（按 edge 索引）
    spawn_timers: std::collections::HashMap<String, f64>,
    /// 粒子半径
    pub particle_radius: f64,
}

impl DataFlowParticles {
    pub fn new() -> Self {
        Self {
            particles: Vec::new(),
            speed: 80.0,
            spawn_interval: 0.8,
            spawn_timers: std::collections::HashMap::new(),
            particle_radius: 2.5,
        }
    }

    /// 根据当前节点和连线状态生成新粒子
    pub fn spawn(&mut self, edges: &[CanvasEdge], nodes: &[CanvasNode], dt: f64) {
        for edge in edges {
            let from = nodes.iter().find(|n| n.id == edge.from_id);
            let to = nodes.iter().find(|n| n.id == edge.to_id);
            if let (Some(from), Some(to)) = (from, to) {
                let key = format!("{}->{}", edge.from_id, edge.to_id);
                let timer = self.spawn_timers.entry(key).or_insert(0.0);
                *timer += dt;
                if *timer >= self.spawn_interval {
                    *timer = 0.0;
                    let dx = to.x - from.x;
                    let dy = to.y - from.y;
                    let dist = (dx * dx + dy * dy).sqrt().max(0.01);
                    let vx = dx / dist * self.speed;
                    let vy = dy / dist * self.speed;
                    let life = dist / self.speed;
                    self.particles.push(Particle::new(
                        from.x,
                        from.y,
                        vx,
                        vy,
                        life,
                        self.particle_radius,
                        from.color.clone(),
                    ));
                }
            }
        }
    }
}

impl ParticleSystem for DataFlowParticles {
    fn update(&mut self, dt: f64) {
        for p in &mut self.particles {
            p.update(dt);
        }
        self.particles.retain(|p| !p.is_dead());
    }

    fn draw(&self, ctx: &CanvasRenderingContext2d) {
        for p in &self.particles {
            p.draw(ctx);
        }
    }

    fn count(&self) -> usize {
        self.particles.len()
    }
}

// ==================== 节点辉光粒子系统 ====================

/// 节点辉光粒子系统：节点被 hover/选中时向外扩散的粒子
///
/// 当节点状态变为 hovered 或 selected 时，从节点中心向外生成粒子，
/// 粒子带随机方向和速度，逐渐淡出消失。
pub struct GlowParticles {
    particles: Vec<Particle>,
    /// 每次生成的粒子数量
    pub burst_count: usize,
    /// 粒子初始速度范围（像素/秒）
    pub min_speed: f64,
    pub max_speed: f64,
    /// 粒子生命周期（秒）
    pub life: f64,
    /// 粒子半径
    pub particle_radius: f64,
    /// 上次触发的节点 ID（避免每帧都触发）
    last_triggered: Option<String>,
}

impl GlowParticles {
    pub fn new() -> Self {
        Self {
            particles: Vec::new(),
            burst_count: 8,
            min_speed: 30.0,
            max_speed: 60.0,
            life: 0.8,
            particle_radius: 2.0,
            last_triggered: None,
        }
    }

    /// 触发节点辉光（从节点中心向外爆发粒子）
    pub fn trigger(&mut self, node: &CanvasNode) {
        if self.last_triggered.as_deref() == Some(node.id.as_str()) {
            return;
        }
        self.last_triggered = Some(node.id.clone());
        for _ in 0..self.burst_count {
            let angle = random_range(0.0, std::f64::consts::TAU);
            let speed = random_range(self.min_speed, self.max_speed);
            let vx = angle.cos() * speed;
            let vy = angle.sin() * speed;
            self.particles.push(Particle::new(
                node.x,
                node.y,
                vx,
                vy,
                self.life,
                self.particle_radius,
                node.color.clone(),
            ));
        }
    }

    /// 清除触发状态（节点取消 hover/选中时调用，允许下次重新触发）
    pub fn reset_trigger(&mut self) {
        self.last_triggered = None;
    }
}

impl ParticleSystem for GlowParticles {
    fn update(&mut self, dt: f64) {
        for p in &mut self.particles {
            p.update(dt);
            p.vx *= 0.95;
            p.vy *= 0.95;
        }
        self.particles.retain(|p| !p.is_dead());
    }

    fn draw(&self, ctx: &CanvasRenderingContext2d) {
        for p in &self.particles {
            p.draw(ctx);
        }
    }

    fn count(&self) -> usize {
        self.particles.len()
    }
}

// ==================== 背景粒子系统 ====================

/// 背景粒子系统：场景中漂浮的环境粒子，营造氛围
///
/// 粒子在画布范围内随机分布，缓慢移动，碰到边界后反弹。
/// 透明度较低，不干扰主内容。
pub struct BackgroundParticles {
    particles: Vec<Particle>,
    /// 目标粒子数量
    pub target_count: usize,
    /// 画布宽度
    pub width: f64,
    /// 画布高度
    pub height: f64,
}

impl BackgroundParticles {
    pub fn new(width: f64, height: f64, count: usize) -> Self {
        let mut system = Self {
            particles: Vec::new(),
            target_count: count,
            width,
            height,
        };
        system.init();
        system
    }

    /// 初始化粒子（在画布范围内随机分布）
    fn init(&mut self) {
        for _ in 0..self.target_count {
            let x = random_range(0.0, self.width);
            let y = random_range(0.0, self.height);
            let vx = random_range(-10.0, 10.0);
            let vy = random_range(-10.0, 10.0);
            let radius = random_range(0.5, 1.5);
            let alpha = random_range(0.1, 0.3);
            let color = format!("rgba(156, 163, 175, {:.2})", alpha);
            self.particles.push(Particle::new(x, y, vx, vy, f64::MAX, radius, color));
        }
    }

    /// 调整画布尺寸（重新分布超出边界的粒子）
    pub fn resize(&mut self, width: f64, height: f64) {
        self.width = width;
        self.height = height;
        for p in &mut self.particles {
            if p.x > width {
                p.x = random_range(0.0, width);
            }
            if p.y > height {
                p.y = random_range(0.0, height);
            }
        }
    }
}

impl ParticleSystem for BackgroundParticles {
    fn update(&mut self, dt: f64) {
        for p in &mut self.particles {
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            if p.x < 0.0 || p.x > self.width {
                p.vx = -p.vx;
                p.x = p.x.clamp(0.0, self.width);
            }
            if p.y < 0.0 || p.y > self.height {
                p.vy = -p.vy;
                p.y = p.y.clamp(0.0, self.height);
            }
        }
    }

    fn draw(&self, ctx: &CanvasRenderingContext2d) {
        for p in &self.particles {
            ctx.set_fill_style_str(&p.color);
            ctx.begin_path();
            let _ = ctx.arc(p.x, p.y, p.radius, 0.0, std::f64::consts::TAU);
            ctx.fill();
        }
    }

    fn count(&self) -> usize {
        self.particles.len()
    }
}

// ==================== 节点诞生/消亡粒子系统 ====================

/// 节点诞生/消亡粒子系统
///
/// - 节点新增时：从中心向外爆发粒子（诞生效果）
/// - 节点删除时：从原位置向内收缩粒子（消亡效果）
pub struct BirthDeathParticles {
    particles: Vec<Particle>,
    /// 诞生爆发粒子数
    pub birth_count: usize,
    /// 消亡粒子数
    pub death_count: usize,
    /// 诞生粒子初始速度范围
    pub birth_speed: f64,
    /// 消亡粒子初始速度（向内收缩）
    pub death_speed: f64,
    /// 粒子生命周期
    pub life: f64,
    /// 粒子半径
    pub particle_radius: f64,
}

impl BirthDeathParticles {
    pub fn new() -> Self {
        Self {
            particles: Vec::new(),
            birth_count: 16,
            death_count: 12,
            birth_speed: 80.0,
            death_speed: 40.0,
            life: 1.0,
            particle_radius: 3.0,
        }
    }

    /// 触发节点诞生效果（从中心向外爆发）
    pub fn trigger_birth(&mut self, node: &CanvasNode) {
        for _ in 0..self.birth_count {
            let angle = random_range(0.0, std::f64::consts::TAU);
            let speed = random_range(self.birth_speed * 0.5, self.birth_speed);
            let vx = angle.cos() * speed;
            let vy = angle.sin() * speed;
            self.particles.push(Particle::new(
                node.x,
                node.y,
                vx,
                vy,
                self.life,
                self.particle_radius,
                node.color.clone(),
            ));
        }
    }

    /// 触发节点消亡效果（从原位置向内收缩）
    pub fn trigger_death(&mut self, x: f64, y: f64, color: &str, outer_radius: f64) {
        for _ in 0..self.death_count {
            let angle = random_range(0.0, std::f64::consts::TAU);
            let dist = random_range(outer_radius * 0.5, outer_radius);
            let start_x = x + angle.cos() * dist;
            let start_y = y + angle.sin() * dist;
            let dx = x - start_x;
            let dy = y - start_y;
            let d = (dx * dx + dy * dy).sqrt().max(0.01);
            let vx = dx / d * self.death_speed;
            let vy = dy / d * self.death_speed;
            self.particles.push(Particle::new(
                start_x,
                start_y,
                vx,
                vy,
                self.life,
                self.particle_radius,
                color.to_string(),
            ));
        }
    }
}

impl ParticleSystem for BirthDeathParticles {
    fn update(&mut self, dt: f64) {
        for p in &mut self.particles {
            p.update(dt);
            p.vx *= 0.92;
            p.vy *= 0.92;
        }
        self.particles.retain(|p| !p.is_dead());
    }

    fn draw(&self, ctx: &CanvasRenderingContext2d) {
        for p in &self.particles {
            p.draw(ctx);
        }
    }

    fn count(&self) -> usize {
        self.particles.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_particle_life_cycle() {
        let mut p = Particle::new(0.0, 0.0, 1.0, 0.0, 2.0, 3.0, "#3b82f6".to_string());
        assert!(!p.is_dead());
        assert!((p.alpha() - 1.0).abs() < 0.01);

        p.update(1.0);
        assert!(!p.is_dead());
        assert!((p.x - 1.0).abs() < 0.01);
        assert!((p.alpha() - 0.5).abs() < 0.01);

        p.update(1.0);
        assert!(p.is_dead());
        assert!(p.alpha() <= 0.0);
    }

    #[test]
    fn test_color_with_alpha_hex() {
        let rgba = color_with_alpha("#3b82f6", 0.5);
        assert_eq!(rgba, "rgba(59, 130, 246, 0.50)");
    }

    #[test]
    fn test_color_with_alpha_rgba_passthrough() {
        let input = "rgba(100, 200, 50, 0.3)";
        let result = color_with_alpha(input, 0.8);
        assert_eq!(result, input);
    }

    #[test]
    fn test_color_with_alpha_invalid() {
        let result = color_with_alpha("not-a-color", 0.5);
        assert_eq!(result, "rgba(255, 255, 255, 0.50)");
    }

    #[test]
    fn test_color_with_alpha_short_hex() {
        let result = color_with_alpha("#abc", 0.5);
        assert_eq!(result, "rgba(255, 255, 255, 0.50)");
    }

    #[test]
    fn test_distance() {
        assert!((distance(0.0, 0.0, 3.0, 4.0) - 5.0).abs() < 0.01);
        assert!((distance(1.0, 1.0, 1.0, 1.0) - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_lerp() {
        assert!((lerp(0.0, 10.0, 0.5) - 5.0).abs() < 0.01);
        assert!((lerp(0.0, 10.0, 0.0) - 0.0).abs() < 0.01);
        assert!((lerp(0.0, 10.0, 1.0) - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_random_range() {
        for _ in 0..100 {
            let v = random_range(5.0, 10.0);
            assert!(v >= 5.0 && v <= 10.0, "random value out of range: {}", v);
        }
    }

    #[test]
    fn test_particle_update_velocity() {
        let mut p = Particle::new(0.0, 0.0, 2.0, -3.0, 5.0, 1.0, "#fff".to_string());
        p.update(2.0);
        assert!((p.x - 4.0).abs() < 0.01, "x should be 4.0, got {}", p.x);
        assert!((p.y - (-6.0)).abs() < 0.01, "y should be -6.0, got {}", p.y);
        assert!((p.life - 3.0).abs() < 0.01, "life should be 3.0, got {}", p.life);
    }

    #[test]
    fn test_data_flow_particles_spawn_and_update() {
        use crate::components::canvas_scene::{CanvasEdge, CanvasNode};
        let mut system = DataFlowParticles::new();
        let nodes = vec![
            CanvasNode { id: "a".to_string(), x: 0.0, y: 0.0, radius: 10.0, label: "A".to_string(), color: "#3b82f6".to_string() },
            CanvasNode { id: "b".to_string(), x: 100.0, y: 0.0, radius: 10.0, label: "B".to_string(), color: "#10b981".to_string() },
        ];
        let edges = vec![CanvasEdge { from_id: "a".to_string(), to_id: "b".to_string() }];

        assert_eq!(system.count(), 0);

        system.spawn(&edges, &nodes, 0.1);
        system.update(0.1);
        assert_eq!(system.count(), 0);

        system.spawn(&edges, &nodes, 0.8);
        system.update(0.0);
        assert_eq!(system.count(), 1);

        let p = &system.particles[0];
        assert!(p.vx > 0.0, "粒子 x 速度应为正（流向 target）");
        assert!((p.vy - 0.0).abs() < 0.01, "粒子 y 速度应为 0");
    }

    #[test]
    fn test_data_flow_particles_reach_target_and_die() {
        use crate::components::canvas_scene::{CanvasEdge, CanvasNode};
        let mut system = DataFlowParticles::new();
        system.spawn_interval = 0.0;

        let nodes = vec![
            CanvasNode { id: "a".to_string(), x: 0.0, y: 0.0, radius: 10.0, label: "A".to_string(), color: "#3b82f6".to_string() },
            CanvasNode { id: "b".to_string(), x: 80.0, y: 0.0, radius: 10.0, label: "B".to_string(), color: "#10b981".to_string() },
        ];
        let edges = vec![CanvasEdge { from_id: "a".to_string(), to_id: "b".to_string() }];

        system.spawn(&edges, &nodes, 1.0);
        assert_eq!(system.count(), 1);

        let life = system.particles[0].life;
        assert!((life - 1.0).abs() < 0.01, "生命周期应为 1.0 秒，实际 {}", life);

        system.update(1.0);
        assert_eq!(system.count(), 0, "粒子应已死亡被移除");
    }

    #[test]
    fn test_glow_particles_trigger_burst() {
        use crate::components::canvas_scene::CanvasNode;
        let mut system = GlowParticles::new();
        let node = CanvasNode {
            id: "a".to_string(),
            x: 100.0,
            y: 100.0,
            radius: 20.0,
            label: "A".to_string(),
            color: "#3b82f6".to_string(),
        };

        assert_eq!(system.count(), 0);
        system.trigger(&node);
        assert_eq!(system.count(), system.burst_count);

        system.trigger(&node);
        assert_eq!(system.count(), system.burst_count, "同节点重复触发应被防抖");
    }

    #[test]
    fn test_glow_particles_reset_and_retrigger() {
        use crate::components::canvas_scene::CanvasNode;
        let mut system = GlowParticles::new();
        let node = CanvasNode {
            id: "a".to_string(),
            x: 0.0,
            y: 0.0,
            radius: 10.0,
            label: "A".to_string(),
            color: "#fff".to_string(),
        };

        system.trigger(&node);
        let count1 = system.count();
        system.reset_trigger();
        system.trigger(&node);
        let count2 = system.count();
        assert_eq!(count2, count1 * 2, "reset 后应能再次触发");
    }

    #[test]
    fn test_glow_particles_die_over_time() {
        use crate::components::canvas_scene::CanvasNode;
        let mut system = GlowParticles::new();
        system.life = 0.5;
        let node = CanvasNode {
            id: "a".to_string(),
            x: 0.0,
            y: 0.0,
            radius: 10.0,
            label: "A".to_string(),
            color: "#fff".to_string(),
        };
        system.trigger(&node);
        assert!(system.count() > 0);

        system.update(0.6);
        assert_eq!(system.count(), 0, "粒子应在生命周期结束后死亡");
    }

    #[test]
    fn test_background_particles_init() {
        let system = BackgroundParticles::new(800.0, 600.0, 30);
        assert_eq!(system.count(), 30);
    }

    #[test]
    fn test_background_particles_boundary_bounce() {
        let mut system = BackgroundParticles::new(100.0, 100.0, 5);
        for p in &mut system.particles {
            p.x = -10.0;
            p.vx = -5.0;
        }
        system.update(1.0);
        for p in &system.particles {
            assert!(p.x >= 0.0, "粒子应在左边界反弹，x={}", p.x);
            assert!(p.vx > 0.0, "反弹后 x 速度应为正");
        }
    }

    #[test]
    fn test_background_particles_resize() {
        let mut system = BackgroundParticles::new(800.0, 600.0, 10);
        for p in &mut system.particles {
            p.x = 900.0;
        }
        system.resize(400.0, 300.0);
        assert_eq!(system.width, 400.0);
        for p in &system.particles {
            assert!(p.x <= 400.0, "粒子应在 resize 后回到画布内");
        }
    }

    #[test]
    fn test_birth_particles_burst() {
        use crate::components::canvas_scene::CanvasNode;
        let mut system = BirthDeathParticles::new();
        let node = CanvasNode {
            id: "a".to_string(),
            x: 100.0,
            y: 100.0,
            radius: 20.0,
            label: "A".to_string(),
            color: "#3b82f6".to_string(),
        };
        system.trigger_birth(&node);
        assert_eq!(system.count(), system.birth_count);

        for p in &system.particles {
            assert!((p.x - 100.0).abs() < 0.01);
            assert!((p.y - 100.0).abs() < 0.01);
        }
    }

    #[test]
    fn test_death_particles_converge_to_center() {
        let mut system = BirthDeathParticles::new();
        system.trigger_death(100.0, 100.0, "#ef4444", 40.0);
        assert_eq!(system.count(), system.death_count);

        for p in &system.particles {
            if p.x > 100.0 {
                assert!(p.vx < 0.0, "中心右侧粒子速度 x 应为负");
            } else if p.x < 100.0 {
                assert!(p.vx > 0.0, "中心左侧粒子速度 x 应为正");
            }
        }
    }

    #[test]
    fn test_birth_death_particles_die_over_time() {
        use crate::components::canvas_scene::CanvasNode;
        let mut system = BirthDeathParticles::new();
        system.life = 0.3;
        let node = CanvasNode {
            id: "a".to_string(),
            x: 0.0,
            y: 0.0,
            radius: 10.0,
            label: "A".to_string(),
            color: "#fff".to_string(),
        };
        system.trigger_birth(&node);
        assert!(system.count() > 0);
        system.update(0.4);
        assert_eq!(system.count(), 0);
    }
}
