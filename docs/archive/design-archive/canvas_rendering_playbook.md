# Canvas 渲染基础设施复用指南

> 📦 归档标记（2026-08-16）：归档冻结。保留原因：canvas_rendering_playbook 设计文档归档冻结，设计决策已沉淀至 wiki 长文。生效方案：见源码和 wiki 长文。

> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 整体分层架构
> - [ui_design_system.md](./ui_design_system.md) — 前端视觉设计系统与样式规范（Canvas 渲染对齐该规范）
> - 【② Plan 落地】[统计图表Phase1基础设施与时序图展示重构.md](../plan/统计图表Phase1基础设施与时序图展示重构.md) — LineChart LTTB 降采样 + ChartScene 统一基类
> - 【② Plan 落地】[统计图表Phase2.md](../plan/统计图表Phase2.md) — DonutChart 多环 + Dashboard 指标卡片组合
> - 【② Plan 落地】[统计图表第三期.md](../plan/统计图表第三期.md) — AopGauge 双刻度仪表盘 + 种子节点推荐高亮外发光
> - 【② Plan 落地】[知识图谱推荐起点与组件复用重构.md](../plan/知识图谱推荐起点与组件复用重构.md) — GraphCanvas 组件两端复用 HR/Workspace 双页面
> - 【③ Wiki 百科】[图表组件.md](docs/wiki/zh/content/前端应用/组件系统/图表组件/图表组件.md) — LineChart/DonutChart/Gauge 三组件使用模式
> - 【③ Wiki 百科】[业务组件.md](docs/wiki/zh/content/前端应用/组件系统/业务组件.md) — GraphCanvas/RuntimePanel 等 HUD 风格组件总览
> - 【④ RAG 知识卡】[Canvas HUD 可视化](docs/wiki/knowledge/zh/Canvas%20HUD%20%E5%8F%AF%E8%A7%86%E5%8C%96%EF%BC%9AGraphCanvas%20%E7%9F%A5%E8%AF%86%E5%9B%BE%E8%B0%B1%20+%20%E5%9B%BE%E8%A1%A8%E5%9C%BA%E6%99%AFLineDonut%20+%20%E4%BB%AA%E8%A1%A8%E7%9B%98Gauge%E5%8F%8C%E7%89%88%20+%20HudPalette%E6%A9%99%E5%85%89%E5%85%89%E6%99%95/Canvas%20HUD%20%E5%8F%AF%E8%A7%86%E5%8C%96%EF%BC%9AGraphCanvas%20%E7%9F%A5%E8%AF%86%E5%9B%BE%E8%B0%B1%20+%20%E5%9B%BE%E8%A1%A8%E5%9C%BA%E6%99%AFLineDonut%20+%20%E4%BB%AA%E8%A1%A8%E7%9B%98Gauge%E5%8F%8C%E7%89%88%20+%20HudPalette%E6%A9%99%E5%85%89%E5%85%89%E6%99%95.md) — §CanvasScene Trait 4 方法 §ForceLayout α 冷却 §7 条红线

> 最后更新：2026-07-24
> 适用范围：Dioxus + web-sys Canvas 2D 渲染栈，用于指导后续页面从 SVG/DOM 迁移到 Canvas，或新建 Canvas 场景化页面。

---

## 1. 已完成的试点页面

### 1.1 `/workspace` 工作台页面

**文件**：[frontend/src/pages/workspace.rs](frontend/src/pages/workspace.rs)

**定位**：Canvas 渲染基础设施的**验证型试点页面**，不是真实业务功能页。用 6 个模拟节点（3 Agent + 3 Tool）+ 6 条连线，验证 CanvasScene 组件的渲染、事件桥接、力导向、拖拽、hover/选中、4 种粒子系统开关。

**验证结果**：
- 编译通过（`cargo check --release`）
- 后端 796 个测试全部通过
- 粒子系统开关可独立启用/禁用，效果符合预期
- 力导向布局自动分布节点，稳定后停止计算

**试点价值**：走通了"Dioxus ↔ Canvas 2D"整条桥接链路，沉淀出下面一套可复用基础设施，后续业务页面只需填充数据和渲染逻辑。

---

## 2. 可复用资产清单

所有资产位于 [frontend/src/components/](frontend/src/components/) 目录，按职责分层：

### 2.1 核心组件：`CanvasScene`

**文件**：[canvas_scene.rs](frontend/src/components/canvas_scene.rs)

**职责**：封装 `<canvas>` 元素 + 2D Context 初始化 + 持续渲染循环 + 力导向 + 拖拽 + hover/选中 + 粒子集成。业务页面只需传入 `CanvasSceneProps`，无需关心底层细节。

**Props 清单**：

| 字段 | 类型 | 默认 | 说明 |
|------|------|------|------|
| `width` / `height` | `f64` | 800×600 | Canvas 尺寸（CSS 像素） |
| `nodes` | `Vec<CanvasNode>` | 空 | 节点列表（位置 0,0 会触发圆形布局） |
| `edges` | `Vec<CanvasEdge>` | 空 | 连线列表 |
| `on_node_click` | `Option<EventHandler<String>>` | None | 节点点击回调 |
| `enable_force_layout` | `bool` | true | 力导向布局开关 |
| `enable_data_flow_particles` | `bool` | true | 数据流粒子开关 |
| `enable_glow_particles` | `bool` | true | 节点辉光粒子开关 |
| `enable_background_particles` | `bool` | true | 背景粒子开关 |
| `enable_birth_death_particles` | `bool` | true | 诞生/消亡粒子开关 |

### 2.2 通用数据结构

**`CanvasNode`**：
```rust
pub struct CanvasNode {
    pub id: String,
    pub x: f64,        // 0,0 表示未初始化，会触发圆形布局
    pub y: f64,
    pub radius: f64,
    pub label: String,
    pub color: String,  // #rrggbb 格式
}
```

**`CanvasEdge`**：
```rust
pub struct CanvasEdge {
    pub from_id: String,
    pub to_id: String,
}
```

> **扩展提示**：业务场景若需要更多字段（如 `node_type`、`tags`、`weight`、`summary`），可在业务侧用 `struct BusinessNode { base: CanvasNode, tags: Vec<String>, ... }` 包装，或直接 fork 一份扩展字段。当前 `CanvasNode` 保持最小化以降低复杂度。

### 2.3 渲染器抽象：`CanvasRenderer` trait

**文件**：[canvas_scene.rs:40-65](frontend/src/components/canvas_scene.rs#L40-L65)

**职责**：抽象渲染逻辑，业务场景实现此 trait 定义自己的渲染样式。

```rust
pub trait CanvasRenderer {
    fn clear(&self, ctx: &CanvasRenderingContext2d, width: f64, height: f64);
    fn draw_nodes(&self, ctx: &CanvasRenderingContext2d, nodes: &[CanvasNode]);
    fn draw_edges(&self, ctx: &CanvasRenderingContext2d, edges: &[CanvasEdge], nodes: &[CanvasNode]);
    fn hit_test(&self, nodes: &[CanvasNode], x: f64, y: f64) -> Option<String>;
    fn draw_nodes_with_state(/* hover/selected/dragging */) { /* 默认委托 draw_nodes */ }
}
```

**内置实现**：`DefaultRenderer` — 基础圆形节点 + 直线连线 + hover 放大 + 选中蓝色边框 + 拖拽橙色光晕。

**扩展方式**：
- 简单场景：直接用 `DefaultRenderer`
- 自定义样式：实现 `CanvasRenderer` trait，如 `KnowledgeGraphRenderer`（节点按 tags 多色边框）、`AgentStatusRenderer`（节点按状态变色）
- CanvasScene 内部通过 `let renderer = DefaultRenderer;` 硬编码使用，若需注入自定义渲染器，可将 `CanvasSceneProps` 扩展 `renderer: Box<dyn CanvasRenderer>` 字段

### 2.4 力导向布局：`ForceLayout`

**文件**：[force_layout.rs](frontend/src/components/force_layout.rs)

**职责**：纯函数式力导向布局算法，模拟斥力（库仑）+ 吸引力（胡克）+ 阻尼 + 边界约束。

**配置项**（`ForceLayoutConfig`）：

| 参数 | 默认 | 说明 |
|------|------|------|
| `repulsion` | 8000.0 | 斥力强度 |
| `attraction` | 0.05 | 弹簧刚度 |
| `ideal_length` | 120.0 | 理想连线长度 |
| `damping` | 0.85 | 速度衰减 |
| `max_step` | 10.0 | 单帧最大位移 |

**辅助函数**：`circle_initial_layout(n, cx, cy, r)` — 圆形初始布局，用于新节点的初始位置。

**稳定判定**：位移小于 0.5 时标记为稳定，停止计算以节省 CPU。

### 2.5 粒子系统：4 种 + `ParticleSystem` trait

**文件**：[particles.rs](frontend/src/components/particles.rs)

| 粒子系统 | 用途 | 触发方式 |
|---------|------|---------|
| `DataFlowParticles` | 连线上从 source 流向 target 的能量粒子 | 自动（按 `spawn_interval` 生成） |
| `GlowParticles` | 节点被 hover/选中时向外扩散 | `trigger(&node)` 手动触发 |
| `BackgroundParticles` | 场景中漂浮的环境粒子 | 初始化即自动运行 |
| `BirthDeathParticles` | 节点新增时爆发、删除时消散 | `trigger_birth(&node)` / `trigger_death(...)` |

**通用 trait**：
```rust
pub trait ParticleSystem {
    fn update(&mut self, dt: f64);
    fn draw(&self, ctx: &CanvasRenderingContext2d);
    fn count(&self) -> usize;
}
```

**工具函数**：`color_with_alpha(hex, alpha)` — `#rrggbb` 转 `rgba(r,g,b,a)`；`random_range(min, max)` — WASM 无 `std::time` 的 xorshift64 伪随机。

### 2.6 渲染层级（draw order）

CanvasScene 内部渲染顺序（从底到顶）：
1. `clear` 清屏
2. `BackgroundParticles` 背景
3. `draw_edges` 连线
4. `DataFlowParticles` 连线粒子
5. `GlowParticles` 节点辉光
6. `draw_nodes_with_state` 节点（含 hover/选中/拖拽状态）
7. `BirthDeathParticles` 诞生/消亡

---

## 3. 关键技术模式（踩坑经验）

### 3.1 `Rc<RefCell<Option<Closure>>>` 自引用递归 rAF

**问题**：`request_animation_frame` 需要递归调用自身形成渲染循环，但 Rust 闭包默认不能捕获自己。

**解法**：
```rust
let callback_ref: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
let cb_ref_inner = callback_ref.clone();
let closure = Closure::<dyn FnMut()>::new(move || {
    // 渲染逻辑...
    if let Some(cb) = cb_ref_inner.borrow().as_ref() {
        let _ = window.request_animation_frame(cb.as_ref().unchecked_ref());
    }
});
*callback_ref.borrow_mut() = Some(closure);
```

**卸载清理**：`use_drop` 中设 `running=false` + `*callback_ref.borrow_mut() = None` 打破 `Rc` 循环引用，避免内存泄漏。

### 3.2 `use_effect` 是 `FnMut`，闭包内不能 move 捕获变量

**问题**：`use_effect` 可被多次调用，若内部 `Closure::new` 直接 move 了外层变量，第二次调用时变量已失效。

**解法**：在 `use_effect` 内部、`Closure::new` 之前 clone：
```rust
use_effect(move || {
    let edges_inner = render_edges.clone();  // ← 关键：在 effect 内 clone
    let nodes_state_c = nodes_state.clone();
    Closure::<dyn FnMut()>::new(move || {
        // 使用 edges_inner、nodes_state_c
    })
});
```

### 3.3 props 同步 effect：保留已有位置 + 新节点圆形布局

**问题**：props.nodes 变化时，若直接覆盖 nodes_state，已有节点的力导向位置会丢失，画面跳变。

**解法**：合并 props 节点与已有节点，保留 `x/y`，只更新外观字段；新增节点（位置 0,0）用 `circle_initial_layout` 初始化。

### 3.4 高清屏适配

```rust
let dpr = web_sys::window().map(|w| w.device_pixel_ratio()).unwrap_or(1.0);
canvas.set_width((width * dpr) as u32);   // 物理像素
canvas.set_height((height * dpr) as u32);
let _ = ctx.scale(dpr, dpr);               // 绘制坐标用 CSS 像素
```

### 3.5 事件坐标转换

```rust
let rect = canvas.get_bounding_client_rect();
let x = e.client_coordinates().x - rect.left();
let y = e.client_coordinates().y - rect.top();
```

### 3.6 web-sys API 名称

- ❌ `request_animation_frame_with_callback(cb)` — 不存在
- ✅ `window.request_animation_frame(cb.as_ref().unchecked_ref())`

### 3.7 Signal 的 mut 关键字

只有需要 `.write()` 的 Signal 才加 `mut`。仅 `.read()` 或 `.clone()` 使用的 Signal 不要加 `mut`，否则触发 `unused_mut` 警告。

### 3.8 WASM 无 `std::time` 的伪随机

粒子系统需要随机数，但 WASM 无 `std::time::Instant`。用 `thread_local!` + `Cell<u64>` + xorshift64 实现确定性伪随机（种子固定 12345）。

---

## 4. 复用清单：重构其他页面时怎么做

### 4.1 适用场景

| 场景 | 是否适合 Canvas | 理由 |
|------|--------------|------|
| 知识图谱（节点 > 100） | ✅ 强烈推荐 | SVG 性能瓶颈，Canvas 可支撑千级节点 |
| Agent 状态实时可视化 | ✅ 推荐 | 粒子动效 + 力导向 + 实时数据流 |
| 任务依赖图 / 流程图 | ✅ 推荐 | 节点连线模型天然适配 |
| 数据表格 / 表单 | ❌ 不适合 | DOM 原生交互更好，Canvas 失去可访问性 |
| 富文本对话气泡 | ❌ 不适合 | 浏览器原生排版引擎更适合 |
| Modal / Toast / 菜单 | ❌ 不适合 | DOM 事件 + ARIA 更合适 |

### 4.2 重构步骤模板

以"知识图谱从 SVG 迁移到 Canvas"为例：

1. **数据层**：将业务实体（如 `MemoryResult`）转换为 `CanvasNode`/`CanvasEdge`
   ```rust
   fn memory_to_canvas_node(m: &MemoryResult) -> CanvasNode {
       CanvasNode {
           id: m.id.clone(),
           x: 0.0, y: 0.0,  // 触发圆形布局
           radius: 30.0,
           label: m.title.chars().take(8).collect(),
           color: memory_type_color(&m.memory_type),
       }
   }
   ```

2. **渲染器**：实现 `CanvasRenderer` trait，自定义节点样式（如 tags 多色边框）
   ```rust
   struct KnowledgeGraphRenderer;
   impl CanvasRenderer for KnowledgeGraphRenderer { /* ... */ }
   ```

3. **页面**：用 `CanvasScene` 替换原 SVG 组件
   ```rust
   rsx! {
       CanvasScene {
           width: 800.0, height: 500.0,
           nodes: canvas_nodes,
           edges: canvas_edges,
           on_node_click: move |id| { /* 展示详情侧边栏 */ },
       }
   }
   ```

4. **开关**：按需关闭不需要的粒子系统（如知识图谱可能不需要背景粒子）

5. **详情面板**：用 DOM 覆盖层（绝对定位 div）而非 Canvas 内渲染富文本

### 4.3 注意事项

- **文本排版退化**：Canvas 原生 `fill_text` 无换行、无富文本。若需要复杂文本，要么用 DOM 覆盖层，要么引入 pretext（见 [frontend_roadmap.md 6.1](docs/frontend_roadmap.md)）
- **可访问性**：Canvas 内元素无 ARIA、无键盘导航。重要交互控件仍用 DOM
- **事件精度**：命中检测靠 `hit_test` 几何计算，复杂形状需自己实现
- **包体**：web-sys 的 Canvas features 已在 Cargo.toml 配置，无额外依赖

---

## 5. Cargo.toml 依赖配置

已在 [frontend/Cargo.toml](frontend/Cargo.toml#L16) 配置 web-sys 的 Canvas features：

```toml
web-sys = { version = "0.3.103", features = [
    "HtmlCanvasElement", "CanvasRenderingContext2d",
    # ... 其他 features
]}
```

无需额外依赖即可使用 Canvas 2D API。

---

## 6. 相关文档

- [frontend_roadmap.md](docs/frontend_roadmap.md) — 前端整体路线图，含游戏化重构五阶段路径
- [frontend_architecture.md](docs/design/frontend_architecture.md) — 前端架构设计
- 实施计划：
  - [2026-07-24-canvas-rendering-infrastructure.md](docs/superpowers/plans/2026-07-24-canvas-rendering-infrastructure.md)
  - [2026-07-24-canvas-dynamic-rendering.md](docs/superpowers/plans/2026-07-24-canvas-dynamic-rendering.md)
  - [2026-07-24-canvas-particle-systems.md](docs/superpowers/plans/2026-07-24-canvas-particle-systems.md)