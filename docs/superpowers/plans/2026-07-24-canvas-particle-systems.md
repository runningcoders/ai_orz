# Canvas 粒子系统与视觉增强实施计划（阶段 3）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Canvas 2D 渲染层上实现 4 种粒子效果（数据流粒子、节点辉光粒子、背景粒子、节点诞生/消亡粒子）+ 节点辉光着色器效果，提升视觉表现力到游戏级，不引入任何重依赖（无 wgpu/rapier）。

**Architecture:** 新增独立的 `particles.rs` 模块，定义 `ParticleSystem` trait + 4 个具体实现（`DataFlowParticles` / `GlowParticles` / `BackgroundParticles` / `BirthDeathParticles`）。每个粒子系统持有自己的粒子状态（位置、速度、生命周期），提供 `update(dt)` + `draw(ctx)` 方法。CanvasScene 在渲染循环中依次调用各粒子系统的 update + draw（先画背景，再画连线+粒子流，再画节点辉光粒子，最后画节点）。粒子系统配置通过 `CanvasSceneProps` 传入，可独立开关。

**Tech Stack:** Dioxus 0.7.9, web-sys 0.3.103 Canvas 2D, 纯 Rust 粒子算法（无外部依赖）

---

## 文件结构

| 文件 | 责任 | 操作 |
|------|------|------|
| `frontend/src/components/particles.rs` | 粒子系统 trait + 4 个实现 + Particle 辅助结构 | 新建 |
| `frontend/src/components/mod.rs` | 注册 particles 模块 | 修改 |
| `frontend/src/components/canvas_scene.rs` | CanvasScene 集成粒子系统（渲染循环中调用） | 修改 |
| `frontend/src/pages/workspace.rs` | 增加粒子效果开关验证 | 修改 |

---

### Task 1: 粒子系统基础结构 + 粒子辅助函数（含单元测试）

**Files:**
- Create: `frontend/src/components/particles.rs`
- Modify: `frontend/src/components/mod.rs`

- [ ] **Step 1: 在 components/mod.rs 注册 particles 模块**

在 `frontend/src/components/mod.rs` 的 `pub mod force_layout;` 之后追加：

```rust
pub mod particles;
```

- [ ] **Step 2: 创建 particles.rs，定义 Particle 结构 + trait + 辅助函数**

在 `frontend/src/components/particles.rs` 中写入：

```rust
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
#[derive(Debug, Clone, Copy)]
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
        // 透明度叠加到颜色（假设颜色是 #rrggbb 格式，转 rgba）
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
    // 已经是 rgba 格式，直接返回
    if hex.starts_with("rgba") || hex.starts_with("rgb(") {
        return hex.to_string();
    }
    // 解析 #rrggbb
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
    // 解析失败，返回默认白色
    format!("rgba(255, 255, 255, {:.2})", alpha)
}

/// 生成指定范围内的随机浮点数
pub fn random_range(min: f64, max: f64) -> f64 {
    use std::cell::Cell;
    thread_local! {
        static SEED: Cell<u64> = Cell::new(12345);
    }
    SEED.with(|s| {
        let mut x = s.get();
        // xorshift64
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        s.set(x);
        let normalized = (x as f64) / (u64::MAX as f64);
        min + normalized * (max - min)
    }
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
```

- [ ] **Step 3: 在 particles.rs 末尾追加单元测试**

在 `frontend/src/components/particles.rs` 末尾追加：

```rust
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
        // 3 位 hex 不支持，应返回默认白色
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
}
```

- [ ] **Step 4: 运行单元测试验证**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo test --bin frontend particles`
Expected: 8 个测试全部 PASS

- [ ] **Step 5: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/components/particles.rs frontend/src/components/mod.rs
git commit -m "feat(particles): 粒子系统基础结构 + 8 个单元测试"
```

---

### Task 2: 数据流粒子系统（连线上能量流动）

**Files:**
- Modify: `frontend/src/components/particles.rs`

- [ ] **Step 1: 在 particles.rs 追加 DataFlowParticles 实现**

在 `frontend/src/components/particles.rs` 文件末尾（单元测试之前）追加：

```rust
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
                    // 计算方向
                    let dx = to.x - from.x;
                    let dy = to.y - from.y;
                    let dist = (dx * dx + dy * dy).sqrt().max(0.01);
                    let vx = dx / dist * self.speed;
                    let vy = dy / dist * self.speed;
                    // 生命周期 = 距离 / 速度
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
        // 移除死亡粒子
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
```

- [ ] **Step 2: 追加数据流粒子测试**

在 `frontend/src/components/particles.rs` 的 `mod tests` 块中追加：

```rust
    #[test]
    fn test_data_flow_particles_spawn_and_update() {
        use crate::components::canvas_scene::{CanvasEdge, CanvasNode};
        let mut system = DataFlowParticles::new();
        let nodes = vec![
            CanvasNode { id: "a".to_string(), x: 0.0, y: 0.0, radius: 10.0, label: "A".to_string(), color: "#3b82f6".to_string() },
            CanvasNode { id: "b".to_string(), x: 100.0, y: 0.0, radius: 10.0, label: "B".to_string(), color: "#10b981".to_string() },
        ];
        let edges = vec![CanvasEdge { from_id: "a".to_string(), to_id: "b".to_string() }];

        // 初始无粒子
        assert_eq!(system.count(), 0);

        // 第一次 spawn，timer 未到 spawn_interval，不生成
        system.spawn(&edges, &nodes, 0.1);
        system.update(0.1);
        assert_eq!(system.count(), 0);

        // 累计时间到 spawn_interval，生成粒子
        system.spawn(&edges, &nodes, 0.8);
        system.update(0.0);
        assert_eq!(system.count(), 1);

        // 粒子沿 x 轴移动
        let p = &system.particles[0];
        assert!(p.vx > 0.0, "粒子 x 速度应为正（流向 target）");
        assert!((p.vy - 0.0).abs() < 0.01, "粒子 y 速度应为 0");
    }

    #[test]
    fn test_data_flow_particles_reach_target_and_die() {
        use crate::components::canvas_scene::{CanvasEdge, CanvasNode};
        let mut system = DataFlowParticles::new();
        system.spawn_interval = 0.0; // 立即生成

        let nodes = vec![
            CanvasNode { id: "a".to_string(), x: 0.0, y: 0.0, radius: 10.0, label: "A".to_string(), color: "#3b82f6".to_string() },
            CanvasNode { id: "b".to_string(), x: 80.0, y: 0.0, radius: 10.0, label: "B".to_string(), color: "#10b981".to_string() },
        ];
        let edges = vec![CanvasEdge { from_id: "a".to_string(), to_id: "b".to_string() }];

        system.spawn(&edges, &nodes, 1.0);
        assert_eq!(system.count(), 1);

        // 距离 80，速度 80，生命周期 1.0 秒
        let life = system.particles[0].life;
        assert!((life - 1.0).abs() < 0.01, "生命周期应为 1.0 秒，实际 {}", life);

        // 更新 1.0 秒，粒子到达 target 并死亡
        system.update(1.0);
        assert_eq!(system.count(), 0, "粒子应已死亡被移除");
    }
```

- [ ] **Step 3: 运行测试**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo test --bin frontend particles`
Expected: 10 个测试全部 PASS（原 8 + 新增 2）

- [ ] **Step 4: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/components/particles.rs
git commit -m "feat(particles): 数据流粒子系统（连线上能量流动）"
```

---

### Task 3: 节点辉光粒子（hover/选中时扩散）

**Files:**
- Modify: `frontend/src/components/particles.rs`

- [ ] **Step 1: 在 particles.rs 追加 GlowParticles 实现**

在 `frontend/src/components/particles.rs` 的 `DataFlowParticles` 实现之后（单元测试之前）追加：

```rust
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
        // 同一节点避免连续触发（防抖）
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
            // 粒子减速（阻尼效果）
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
```

- [ ] **Step 2: 追加辉光粒子测试**

在 `mod tests` 块中追加：

```rust
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

        // 同一节点再次触发应被防抖
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

        // 更新超过生命周期
        system.update(0.6);
        assert_eq!(system.count(), 0, "粒子应在生命周期结束后死亡");
    }
```

- [ ] **Step 3: 运行测试**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo test --bin frontend particles`
Expected: 13 个测试全部 PASS

- [ ] **Step 4: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/components/particles.rs
git commit -m "feat(particles): 节点辉光粒子系统（hover/选中时扩散）"
```

---

### Task 4: 背景粒子系统（环境氛围）

**Files:**
- Modify: `frontend/src/components/particles.rs`

- [ ] **Step 1: 在 particles.rs 追加 BackgroundParticles 实现**

在 `GlowParticles` 实现之后追加：

```rust
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
            // 背景粒子生命周期无限（life 设大值）
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
            // 边界反弹
            if p.x < 0.0 || p.x > self.width {
                p.vx = -p.vx;
                p.x = p.x.clamp(0.0, self.width);
            }
            if p.y < 0.0 || p.y > self.height {
                p.vy = -p.vy;
                p.y = p.y.clamp(0.0, self.height);
            }
            // 背景粒子不衰减生命周期（life = MAX）
        }
    }

    fn draw(&self, ctx: &CanvasRenderingContext2d) {
        for p in &self.particles {
            // 背景粒子直接用颜色绘制（颜色已含 alpha）
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
```

- [ ] **Step 2: 追加背景粒子测试**

```rust
    #[test]
    fn test_background_particles_init() {
        let system = BackgroundParticles::new(800.0, 600.0, 30);
        assert_eq!(system.count(), 30);
    }

    #[test]
    fn test_background_particles_boundary_bounce() {
        let mut system = BackgroundParticles::new(100.0, 100.0, 5);
        // 把粒子推到左边界外
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
        // 把粒子推到超出新画布的位置
        for p in &mut system.particles {
            p.x = 900.0;
        }
        system.resize(400.0, 300.0);
        assert_eq!(system.width, 400.0);
        for p in &system.particles {
            assert!(p.x <= 400.0, "粒子应在 resize 后回到画布内");
        }
    }
```

- [ ] **Step 3: 运行测试**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo test --bin frontend particles`
Expected: 16 个测试全部 PASS

- [ ] **Step 4: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/components/particles.rs
git commit -m "feat(particles): 背景粒子系统（环境氛围）"
```

---

### Task 5: 节点诞生/消亡粒子系统

**Files:**
- Modify: `frontend/src/components/particles.rs`

- [ ] **Step 1: 在 particles.rs 追加 BirthDeathParticles 实现**

在 `BackgroundParticles` 实现之后追加：

```rust
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
    /// outer_radius 为粒子初始分布的半径范围
    pub fn trigger_death(&mut self, x: f64, y: f64, color: &str, outer_radius: f64) {
        for _ in 0..self.death_count {
            // 粒子从外围向中心收缩
            let angle = random_range(0.0, std::f64::consts::TAU);
            let dist = random_range(outer_radius * 0.5, outer_radius);
            let start_x = x + angle.cos() * dist;
            let start_y = y + angle.sin() * dist;
            // 速度方向：指向中心
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
            // 诞生粒子减速
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
```

- [ ] **Step 2: 追加诞生/消亡粒子测试**

```rust
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

        // 所有粒子应从节点中心出发
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

        // 所有粒子速度应指向中心（100, 100）
        for p in &system.particles {
            // 如果粒子在中心右侧，速度应为负 x
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
```

- [ ] **Step 3: 运行测试**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo test --bin frontend particles`
Expected: 19 个测试全部 PASS

- [ ] **Step 4: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/components/particles.rs
git commit -m "feat(particles): 节点诞生/消亡粒子系统"
```

---

### Task 6: CanvasScene 集成粒子系统

**Files:**
- Modify: `frontend/src/components/canvas_scene.rs`

这是核心集成任务。将 4 个粒子系统接入 CanvasScene 渲染循环。

- [ ] **Step 1: 在 canvas_scene.rs 顶部追加 use 语句**

在 `use crate::components::force_layout::{circle_initial_layout, ForceLayout, ForceLayoutConfig};` 之后追加：

```rust
use crate::components::particles::{
    BackgroundParticles, BirthDeathParticles, DataFlowParticles, GlowParticles, ParticleSystem,
};
```

- [ ] **Step 2: 扩展 CanvasSceneProps，添加粒子开关字段**

将 `CanvasSceneProps` 改为：

```rust
#[derive(Props, Clone, PartialEq)]
pub struct CanvasSceneProps {
    pub width: f64,
    pub height: f64,
    pub nodes: Vec<CanvasNode>,
    pub edges: Vec<CanvasEdge>,
    pub on_node_click: Option<EventHandler<String>>,
    #[props(default = true)]
    pub enable_force_layout: bool,
    /// 是否启用数据流粒子（连线能量流动）
    #[props(default = true)]
    pub enable_data_flow_particles: bool,
    /// 是否启用节点辉光粒子（hover/选中扩散）
    #[props(default = true)]
    pub enable_glow_particles: bool,
    /// 是否启用背景粒子（环境氛围）
    #[props(default = true)]
    pub enable_background_particles: bool,
    /// 是否启用节点诞生/消亡粒子
    #[props(default = true)]
    pub enable_birth_death_particles: bool,
}
```

并更新 `Default` 实现：

```rust
impl Default for CanvasSceneProps {
    fn default() -> Self {
        Self {
            width: 800.0,
            height: 600.0,
            nodes: Vec::new(),
            edges: Vec::new(),
            on_node_click: None,
            enable_force_layout: true,
            enable_data_flow_particles: true,
            enable_glow_particles: true,
            enable_background_particles: true,
            enable_birth_death_particles: true,
        }
    }
}
```

- [ ] **Step 3: 在 CanvasScene 组件中初始化粒子系统 Signal**

在 `let mut selected_id: Signal<Option<String>> = use_signal(|| None);` 之后追加：

```rust
    // 粒子系统状态
    let mut data_flow: Signal<DataFlowParticles> = use_signal(|| DataFlowParticles::new());
    let mut glow: Signal<GlowParticles> = use_signal(|| GlowParticles::new());
    let mut background: Signal<BackgroundParticles> = use_signal(|| {
        BackgroundParticles::new(props.width, props.height, 40)
    });
    let mut birth_death: Signal<BirthDeathParticles> = use_signal(|| BirthDeathParticles::new());
```

- [ ] **Step 4: 在 props 同步 effect 中触发诞生效果**

在 props 同步 effect 的 `nodes_state_sync.set(merged);` 之前，检测新增节点并触发诞生效果。将 props 同步 effect 中的循环改为：

```rust
        // 检测新增节点（props 有但 current 没有）
        let new_node_ids: Vec<String> = props_nodes
            .iter()
            .filter(|n| !current.iter().any(|c| c.id == n.id))
            .map(|n| n.id.clone())
            .collect::<Vec<_>>();
```

然后在 `nodes_state_sync.set(merged);` 之前追加：

```rust
        // 触发新增节点的诞生效果
        if !new_node_ids.is_empty() {
            let mut bd = birth_death_sync.write();
            for id in &new_node_ids {
                if let Some(node) = merged.iter().find(|n| &n.id == id) {
                    bd.trigger_birth(node);
                }
            }
        }
```

并在 effect 顶部捕获 birth_death 的 clone（在 `let mut is_stable_sync = is_stable.clone();` 之后追加）：

```rust
    let mut birth_death_sync = birth_death.clone();
```

- [ ] **Step 5: 在渲染循环中集成粒子系统**

在渲染循环 effect 中（`let closure = Closure::<dyn FnMut()>::new(move || {` 内部），将渲染部分替换为：

```rust
        let mut data_flow_c = data_flow.clone();
        let mut glow_c = glow.clone();
        let mut background_c = background.clone();
        let mut birth_death_c = birth_death.clone();
        let enable_data_flow = props.enable_data_flow_particles;
        let enable_glow = props.enable_glow_particles;
        let enable_bg = props.enable_background_particles;
        let enable_bd = props.enable_birth_death_particles;

        let closure = Closure::<dyn FnMut()>::new(move || {
            // 力导向步进
            if enable_force && !*is_stable_c.read() {
                let mut nodes = nodes_state_c.read().clone();
                let mut layout = force_layout_c.write();
                let displacement = layout.step(&mut nodes, &edges_inner, width, height);
                nodes_state_c.set(nodes);
                if layout.is_stable(displacement, 0.5) {
                    is_stable_c.set(true);
                }
            }

            // 粒子更新（dt 约为 1/60 秒，简化为固定值）
            let dt = 1.0 / 60.0;
            if enable_bg {
                background_c.write().update(dt);
            }
            if enable_data_flow {
                let nodes = nodes_state_c.read().clone();
                data_flow_c.write().spawn(&edges_inner, &nodes, dt);
                data_flow_c.write().update(dt);
            }
            if enable_glow {
                glow_c.write().update(dt);
            }
            if enable_bd {
                birth_death_c.write().update(dt);
            }

            // 渲染（顺序：背景 → 连线 → 数据流粒子 → 节点辉光粒子 → 节点 → 诞生/消亡粒子）
            let nodes = nodes_state_c.read().clone();
            let hovered = hovered_id_c.read().clone();
            let selected = selected_id_c.read().clone();
            let dragging = dragging_id_c.read().clone();

            renderer_c.clear(&ctx, width, height);

            // 1. 背景粒子（最底层）
            if enable_bg {
                background_c.read().draw(&ctx);
            }

            // 2. 连线
            renderer_c.draw_edges(&ctx, &edges_inner, &nodes);

            // 3. 数据流粒子（在连线上方，节点下方）
            if enable_data_flow {
                data_flow_c.read().draw(&ctx);
            }

            // 4. 节点辉光粒子（节点周围扩散）
            if enable_glow {
                glow_c.read().draw(&ctx);
            }

            // 5. 节点
            renderer_c.draw_nodes_with_state(&ctx, &nodes, &hovered, &selected, &dragging);

            // 6. 诞生/消亡粒子（最上层，醒目）
            if enable_bd {
                birth_death_c.read().draw(&ctx);
            }

            // 递归注册下一帧
            if running_clone.load(std::sync::atomic::Ordering::SeqCst) {
                if let Some(cb) = cb_ref_inner.borrow().as_ref() {
                    if let Some(window) = web_sys::window() {
                        let _ = window.request_animation_frame(cb.as_ref().unchecked_ref());
                    }
                }
            }
        });
```

注意：在渲染循环 effect 中，需要将粒子系统的 Signal clone 到闭包外层变量（类似 `let mut data_flow_c = data_flow.clone();`）。

- [ ] **Step 6: 在鼠标事件中触发辉光效果**

在 `onmousedown` 事件中，命中节点后追加辉光触发：

```rust
                if let Some(node_id) = renderer.hit_test(&nodes, x, y) {
                    dragging_id.set(Some(node_id.clone()));
                    is_stable.set(false);
                    if let Some(node) = nodes.iter().find(|n| n.id == node_id) {
                        drag_offset.set((x - node.x, y - node.y));
                        // 触发拖拽开始时的辉光
                        glow.write().trigger(node);
                    }
                }
```

在 `onclick` 事件中，命中节点后追加辉光触发：

```rust
                if let Some(node_id) = renderer.hit_test(&nodes, x, y) {
                    selected_id.set(Some(node_id.clone()));
                    // 触发选中时的辉光
                    if let Some(node) = nodes.iter().find(|n| n.id == node_id) {
                        glow.write().reset_trigger();
                        glow.write().trigger(node);
                    }
                    on_click.call(node_id);
                }
```

- [ ] **Step 7: 验证编译**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check`
Expected: 编译通过

如果遇到错误，重点检查：
- Signal 的 clone 和 write 操作（`data_flow.clone()` 在 effect 外层，闭包内用 `.write()`）
- 粒子系统的所有权（use_effect 是 FnMut，需要 clone Signal 而非 move）
- 渲染顺序中的借用冲突（`read()` 返回的 guard 生命周期）

修复编译错误直到通过。

- [ ] **Step 8: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/components/canvas_scene.rs
git commit -m "feat(canvas): CanvasScene 集成 4 种粒子系统"
```

---

### Task 7: workspace 页面增加粒子效果开关

**Files:**
- Modify: `frontend/src/pages/workspace.rs`

- [ ] **Step 1: 在 workspace.rs 增加粒子开关状态和 UI**

在 `frontend/src/pages/workspace.rs` 的 `Workspace` 组件中，在 `let toast = use_toast();` 之后追加：

```rust
    let mut enable_data_flow = use_signal(|| true);
    let mut enable_glow = use_signal(|| true);
    let mut enable_background = use_signal(|| true);
    let mut enable_birth_death = use_signal(|| true);
```

- [ ] **Step 2: 在 rsx 中 CanvasScene 之前追加粒子开关 UI**

在 `// Canvas 场景` 注释之前追加：

```rust
                    // 粒子效果开关
                    div { class: "flex flex-wrap gap-2 mb-4",
                        label { class: "label cursor-pointer flex items-center gap-2",
                            input {
                                r#type: "checkbox",
                                class: "toggle toggle-sm toggle-primary",
                                checked: "{enable_data_flow}",
                                onchange: move |e| enable_data_flow.set(e.checked()),
                            }
                            span { class: "label-text text-sm", "数据流粒子" }
                        }
                        label { class: "label cursor-pointer flex items-center gap-2",
                            input {
                                r#type: "checkbox",
                                class: "toggle toggle-sm toggle-secondary",
                                checked: "{enable_glow}",
                                onchange: move |e| enable_glow.set(e.checked()),
                            }
                            span { class: "label-text text-sm", "辉光粒子" }
                        }
                        label { class: "label cursor-pointer flex items-center gap-2",
                            input {
                                r#type: "checkbox",
                                class: "toggle toggle-sm toggle-accent",
                                checked: "{enable_background}",
                                onchange: move |e| enable_background.set(e.checked()),
                            }
                            span { class: "label-text text-sm", "背景粒子" }
                        }
                        label { class: "label cursor-pointer flex items-center gap-2",
                            input {
                                r#type: "checkbox",
                                class: "toggle toggle-sm toggle-neutral",
                                checked: "{enable_birth_death}",
                                onchange: move |e| enable_birth_death.set(e.checked()),
                            }
                            span { class: "label-text text-sm", "诞生/消亡" }
                        }
                    }
```

- [ ] **Step 3: 更新 CanvasScene 调用，传入粒子开关**

将 CanvasScene 调用改为：

```rust
                        CanvasScene {
                            width: 800.0,
                            height: 500.0,
                            nodes: nodes.clone(),
                            edges: edges.clone(),
                            enable_data_flow_particles: *enable_data_flow.read(),
                            enable_glow_particles: *enable_glow.read(),
                            enable_background_particles: *enable_background.read(),
                            enable_birth_death_particles: *enable_birth_death.read(),
                            on_node_click: move |id: String| {
                                selected_id.set(Some(id.clone()));
                                toast.info(&format!("点击节点: {id}"));
                            }
                        }
```

- [ ] **Step 4: 验证编译**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check`
Expected: 编译通过

- [ ] **Step 5: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/pages/workspace.rs
git commit -m "feat(workspace): 增加粒子效果开关验证 4 种粒子"
```

---

### Task 8: 完整验证 + Release 构建

**Files:**
- 无修改，仅验证

- [ ] **Step 1: 运行单元测试**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo test --bin frontend`
Expected: 所有测试 PASS（力导向 7 + 粒子 19 = 26 个测试）

- [ ] **Step 2: Release 构建**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo build --release`
Expected: 构建成功，warning 数量不超过基线（3 个）

- [ ] **Step 3: 后端测试回归**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo test --workspace 2>&1 | grep "test result" | awk '{sum+=$4} END {print "总计通过: " sum}'`
Expected: 746 passed（无回归）

- [ ] **Step 4: 推送到远程**

```bash
cd /Users/aman/Technology/rust/ai_orz
git push origin main
```

---

## 验证清单

完成所有 Task 后，在浏览器中打开 /workspace 验证：

- [ ] 背景有缓慢漂浮的小粒子（环境氛围）
- [ ] 连线上有粒子从 source 流向 target（数据流方向）
- [ ] hover 节点时粒子向外扩散（辉光）
- [ ] 点击节点时粒子爆发（选中辉光）
- [ ] 页面加载时节点有诞生爆发效果
- [ ] 4 个粒子开关可独立控制效果
- [ ] 力导向布局仍正常工作
- [ ] 拖拽功能正常
- [ ] 高清屏下粒子清晰
- [ ] 现有页面功能无回归

## 阶段 3 完成标志

1. 4 种粒子系统全部实现（19 个单元测试覆盖核心逻辑）
2. CanvasScene 渲染循环集成粒子（背景→连线→数据流→辉光→节点→诞生/消亡）
3. 粒子效果可通过 props 独立开关
4. workspace 页面提供 4 个开关验证效果
5. 无重依赖引入（纯 Canvas 2D + Rust 算法）
6. 现有功能零回归

## 阶段 4 预告（不在本计划范围）

- 场景化工作空间（全屏 Canvas + 空间布局）
- Agent 状态实时可视化（唤醒/思考动画）
- 缩放/平移交互
- DOM 覆盖层（Canvas 上的 HTML 控件）
- 知识图谱从 SVG 迁移到 Canvas（结合阶段 2-3 能力）
