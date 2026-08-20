---
kind: RAG 原子知识卡
name: Canvas HUD 可视化：GraphCanvas 知识图谱 + 图表场景 Line/Donut + 仪表盘 Gauge 双版 + HudPalette 橙光光晕
category: 前端应用 / HUD 可视化
scope:
  - "frontend/src/components/graph_canvas.rs"
  - "frontend/src/components/canvas_scene.rs"
  - "frontend/src/components/graph.rs"
  - "frontend/src/components/force_layout.rs"
  - "frontend/src/components/layered_layout.rs"
  - "frontend/src/components/relation_graph.rs"
  - "frontend/src/components/workspace_graph.rs"
  - "frontend/src/components/chart_scene.rs"
  - "frontend/src/components/charts/**"
  - "frontend/src/components/gauge.rs"
  - "frontend/src/components/aop_gauge.rs"
  - "frontend/src/components/hud_palette.rs"
  - "frontend/src/components/particles.rs"
  - "frontend/src/components/kanban_canvas.rs"
source_files:
  - 'frontend/src/components/graph_canvas.rs#L1-L80 (GraphCanvas 组件：dioxus_canvas::Canvas 节点 + 2D Context 渲染；属性 knowledge_graph: KnowledgeGraphDto + 交互：拖拽节点 + 滚轮缩放 + hover 显示摘要 tooltip)'
  - frontend/src/components/canvas_scene.rs (CanvasScene Trait：统一 Scene 生命周期 fn setup(ctx) / fn update(dt_secs) / fn draw(&2DContext) / fn handle_event(event)；GraphScene / ChartScene / WorkspaceScene 三实现)
  - 'frontend/src/components/graph.rs (Graph 数据结构：节点 nodes: Vec<Node{id,label,x,y,attr}> + 边 edges: Vec<Edge{src,dst,weight}>；Vec<id> 索引而非 HashMap，查邻接用 edges 遍历)'
  - frontend/src/components/force_layout.rs#L1-L100 (ForceLayout 力导向算法：每 tick 算斥力（所有节点对库仑力）+ 引力（边胡克力）+ 中心拉力；alpha 冷却 0.99^tick；300 帧后停止节省 CPU)
  - frontend/src/components/layered_layout.rs (LayeredLayout 分层布局：按 knowledge node depth 或 category 分层；Sugiyama 四阶段简易版，去除交叉最小化，用在 Agent 依赖树和项目任务 DAG)
  - frontend/src/components/chart_scene.rs#L1-L60 (ChartScene 统一图表场景：折线 LineChart 数据点 + 时间轴 + 坐标轴 + 鼠标 hover 十字准星 + tooltip；Donut 饼图多环)
  - 'frontend/src/components/charts/line_chart.rs (LineChart 组件：内部用 ChartScene；props: points: Vec<TimeSeriesPoint{ts, value}> + series: String + color；点数据 > 500 自动降采样 200 点防渲染卡顿)'
  - frontend/src/components/gauge.rs (Gauge 仪表盘：240° 圆弧刻度 + 指针 + 0-100 值映射；HUD 风格橙光描边；AopGauge 同组件 + 双刻度（队列长度 + 延迟毫秒）)
  - frontend/src/components/hud_palette.rs (HudPalette 调色板：HUD_ORANGE #FF8C00 / HUD_BLUE #00BFFF / HUD_GREEN #32CD32 / HUD_RED #FF4444；draw_glow_stroke(ctx, color, line_width) 加 box-shadow 光晕 blur 8px 渲染橙光条)
  - docs/archive/design-archive/canvas_rendering_playbook.md（§CanvasScene 统一渲染管线 §力导向参数 α 冷却规则 §橙光光晕 blur 值调优 §5 层 Canvas 节点叠放顺序）
  - docs/design/ui_design_system.md（§HUD 驾驶舱风格视觉规范 §DaisyUI 基础组件 + 自定义 HUD 组件融合方式 §30+ 主题的配色适配策略）
  - docs/archive/plan-archive/统计图表Phase1基础设施与时序图展示重构.md（§LineChart 降采样算法 §时间轴月份刻度 §ChartScene 统一基类抽取）
  - docs/archive/plan-archive/统计图表Phase2.md（§DonutChart 类目分环 §多系列折线叠加 §Dashboard 5指标卡片 + 2仪表盘组合页）
  - docs/archive/plan-archive/统计图表第三期.md（§AopGauge 双刻度仪表盘 §KnowledgeGraph 种子节点推荐高亮 §recommend_seed_nodes 三因子分数映射到节点颜色）
  - docs/archive/plan-archive/知识图谱推荐起点与组件复用重构.md（§GraphCanvas 组件两端复用：HR记忆搜索页 + Workspace工作台页 §种子节点推荐圆圈外发光）
  - docs/wiki/zh/content/前端应用/页面模块/HR 管理页面/知识图谱可视化.md（HR 知识图谱页面：GraphCanvas + ForceLayout + 节点点击跳转 /hr/memory-search?id=）
  - docs/wiki/zh/content/前端应用/组件系统/图表组件/图表组件.md（图表组件总览：LineChart/DonutChart/Gauge 三组件 + 使用模式 + 降采样与性能建议）
  - docs/wiki/zh/content/前端应用/组件系统/业务组件.md（业务组件：GraphCanvas/RuntimePanel/ChatSidePanel/MessageBubble 四件套 + HUD 风格示例）
  - '【平行卡 1】docs/wiki/knowledge/zh/DuckDB 多维统计双层互补：record_event! 宏自动表推断 + RuntimeStatsCollector 内存滑动窗口 + 5 维度开箱即用表/DuckDB 多维统计双层互补：record_event! 宏自动表推断 + RuntimeStatsCollector 内存滑动窗口 + 5 维度开箱即用表.md（统计数据来源：stats_query API 返回 TimeSeriesPoint[] → LineChart 组件渲染）'
  - 【平行卡 2】docs/wiki/knowledge/zh/知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出/知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出.md（KnowledgeGraphDto 数据来源：traverse_knowledge_graph API → GraphCanvas 渲染的 nodes/edges）
---

## §1 概述

**本卡角色**：前端 HUD 驾驶舱风格 Canvas 可视化体系知识卡。覆盖 GraphCanvas（知识图谱 Canvas 渲染 + 力导向/分层两布局）、ChartScene 统一图表场景（LineChart 时序折线/DonutChart 甜甜圈）、仪表盘 Gauge/AopGauge 双刻度、HudPalette 橙光调色板 + draw_glow_stroke 光晕工具。**定位：新增图表类型、调整图谱布局卡顿、排查 HUD 橙光效果被主题色覆盖、调力导向 alpha 冷却参数时读。**

- **CanvasScene Trait 统一渲染管线（所有可视化共享）**（canvas_scene.rs）：① setup() 创建时初始化数据 + 分配缓存顶点数组；② update(dt: f32) 每帧 tick 传 dt 秒（ForceLayout alpha 冷却/Particles 位移用）；③ draw(&ctx: &CanvasRenderingContext2d) 纯绘制；④ handle_event(event: CanvasEvent) 处理鼠标拖拽/滚轮缩放。GraphScene/ChartScene/GaugeScene 都 impl CanvasScene，统一 dioxus::use_effect 注册 requestAnimationFrame 循环。帧率策略：后台 tab（document.hidden=true）→ 自动降到 2fps（不退出循环），前台 tab 60fps；Graph 节点数 < 100 用 60fps，>500 用 20fps（自动 clamp）。
- **GraphCanvas 图谱双布局 + 两端复用**（graph_canvas.rs + HR 知识图谱页 + Workspace 工作台页）：ForceLayout 力导向用于自由探索（知识图谱 HR 页）：所有节点对算 1/r² 斥力 + 胡克力边引力 + center(0,0) 中心拉力；alpha 冷却系数 α_t = 0.99^t，300 帧后 α<0.01 → stop。LayeredLayout 分层用于结构化视图（任务 DAG/Agent 工具依赖）：先算 depth 层号（BFS）→ 层内等分 x → 层间按 y 等分；不做连线交叉最小化（性能优先，仅按 edge weight 重排）。两端复用：HR 页和 Workspace 页都用同一 GraphCanvas 组件，仅 props 的 layout_mode="force" | "layered" + 数据来源不同；种子节点推荐 recommend_seed_nodes 返回的 node.score → 映射到节点颜色（HUD_ORANGE 高分→HUD_BLUE 低分）+ 外发光 draw_glow_stroke。
- **HUD 风格橙光调色板（HudPalette）+ 仪表盘 Gauge**（hud_palette.rs + gauge.rs）：4 主色 HUD_ORANGE/HUD_BLUE/HUD_GREEN/HUD_RED；draw_glow_stroke 实现：先 `ctx.shadow_blur = 8.0` + `ctx.shadow_color = HUD_ORANGE` → 画一次描边（光晕）→ reset shadow → 画第二次正常描边（实线）；这样 CSS 不会被 DaisyUI 主题覆盖（是 Canvas 2D API，不是 DOM）。仪表盘 Gauge：value 0-100 → 映射到 240° 圆弧起点角度 150° 到终点 390°；指针三角箭头 + 刻度 20 条（每 20 一条长刻度）；AopGauge（aop_gauge.rs）同 Gauge 组件 + 上半圆环 AOP 队列延迟毫秒 + 下半圆环消费者阻塞数 双刻度，System AOP 页用。

---

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 源码锚点 |
|------|------|---------|---------|
| canvas_scene.rs CanvasScene Trait | 统一渲染管线 | setup/update/draw/handle_event 四方法抽象；use_canvas_animation Hook 注册 RAF 循环；dt 自动算 RAF delta | 见 Trait 定义 |
| graph_canvas.rs GraphCanvas 组件 | 图谱渲染入口 | dioxus props { graph: KnowledgeGraphDto, layout_mode: Force/Layered, on_node_click: EventHandler<NodeId> }；内部 GraphScene impl CanvasScene；鼠标拖拽节点修改 x/y（mutable） | `:L1-L80` |
| force_layout.rs ForceLayout | 力导向算法 | pub fn tick(&mut self, graph: &mut Graph, dt: f32)；斥力 O(n²) 每对节点；引力 O(e) 每条边；α_t+1 = α_t × 0.99；α < 0.01 直接 return skip | `:L1-L100` |
| layered_layout.rs LayeredLayout | 分层布局 | 拓扑排序算 depth；层内按节点 weight 降序排 x 等分；层间 y = depth × row_height（120px 默认）；跨层边用三次贝塞尔曲线 | 见 layout fn |
| chart_scene.rs ChartScene | 图表统一基类 | 算 data_bounds(min/max) → draw_axes(x轴/y轴刻度 + 网格线) → draw_tooltip_hover（鼠标位置 x 映射数据点）；LineChartScene/DonutChartScene 继承数据结构 | `:L1-L60` |
| charts/line_chart.rs LineChart | 折线组件 | props.points.len() > 500 → LTTB（Largest-Triangle-Three-Buckets）降采样到 200 点；两条线画不同色 series；hover 十字准星 + tooltip | 见 LineChart impl |
| gauge.rs Gauge 仪表盘 | HUD 风格仪表盘 | 240° 圆弧 + 指针三角 + 刻度；draw_glow_stroke 双遍描边（shadow blur=8 → 橙光晕）；DaisyUI 主题切换不影响 Canvas 色值 | 见 draw fn |
| hud_palette.rs HudPalette | 橙光调色板 | const 4 主色 + draw_glow_stroke(&ctx, color, lw, blur: f32) 工具函数（先 shadow→描边→reset shadow→实线第二遍，重复代码 3 行通用） | 见 palette impl |

**章节来源**
- [canvas_rendering_playbook.md:L20-L80](docs/archive/design-archive/canvas_rendering_playbook.md#L20-L80)
- [graph_canvas.rs:L1-L80](frontend/src/components/graph_canvas.rs#L1-L80)
- [hud_palette.rs](frontend/src/components/hud_palette.rs)

---

## §3 知识图谱渲染全链路

```
HR 知识图谱页面加载：
1. GET /api/v1/memory/traverse?start_node=seed_a&levels=3
   → MemoryDomain.traverse_knowledge_graph → BFS levels 3 层
   → 返回 KnowledgeGraphDto { nodes: 87, edges: 214 }
2. 并行请求 GET /api/v1/memory/recommend_seed_nodes?agent_id=ag_123
   → recommend_seed_nodes 返回 top-10 节点 id[] + score[]
   → score 0-1 映射到节点颜色 HUD_ORANGE(1.0) → HUD_BLUE(0.0) 插值
3. props 传入 GraphCanvas:
   GraphCanvas {
     graph: dto,
     layout_mode: Force,
     seed_node_ids: top_10_ids,
     on_node_click: move |id| navigator().push(Route::HrMemorySearch { node_id: id })
   }
4. GraphScene.setup()：
   初始化 ForceLayout（random_positions 中心附近，±screen_w/4）
   + 标记 seed 节点 glow=true（外发光 16px HUD_ORANGE）
5. RAF 循环：
   [tick 0-300] ForceLayout.tick() 冷却到 α=0.01 停止计算
   → 每帧 update(dt)：节点 < 100 → 60fps；节点 500+ → 20fps clamp
   → draw()：先画边（灰线）→ 再画节点（圆形 + seed 有 shadow blur 外发光）
6. 用户交互：
   - 拖拽节点：mousedown 命中（距离 < 12px）→ 记录拖动，mousemove 改 node.x/y → ForceLayout 恢复 α=0.3 再冷却
   - 滚轮：scale ×= 1.1 / 0.9（clamp 0.2 ~ 4.0）→ translate 偏移
   - hover：命中节点 → draw tooltip（node.label + degree + score）
```

---

## §4 硬约束与回归红线（7 条）

1. **Canvas 2D 绘制不能依赖 DaisyUI CSS 变量**：HUD 色必须硬编码 HudPalette 的 const，不要从 window.getComputedStyle 读 --p（DaisyUI 主色），否则 WASM 里 DOM API 跨线程调用 + 切换主题 30+ 每换一次重绘所有 Canvas，性能炸。例外：Canvas 周围 DOM 外壳 card 样式可用 class="bg-base-200"。
2. **ForceLayout 斥力 O(n²) 必须节点数 ≥1000 时降采样**：nodes.len() > 800 自动从 O(n²) 切换到 Barnes-Hut O(n log n) 近似（四叉树空间分块近似斥力）；测试 1500 节点渲染时 dt 单帧 > 32ms（< 30fps）→ 必须启用近似模式；默认模式 O(n²) 够用，代码不预实现 Barnes-Hut（YAGNI）。
3. **LineChart 数据点 >500 必须降采样（LTTB 算法）**：直接把 2000 点画到 400px 宽 Canvas = 每条线叠 5 个点 waste CPU；强制用 LTTB Largest-Triangle-Three-Buckets 降到 ≤200；降采样不改变统计结果（LTTB 保留极值和突变形状）；测试断言降采样后 min/max 值不差过原数组。
4. **draw_glow_stroke 双遍顺序不能反**：shadow 描边（shader 模糊）→ reset shadow → 正常实线描边；顺序反过来 = 实线也被 blur，结果整个 UI 一片虚糊；Grep 所有调用必须是 save → 设置 shadow → stroke(ctx) → restore → 第二次 stroke(ctx)。
5. **后台 tab 帧率自动降到 2fps（不停止 RAF）**：用 `document.visibility_state == "hidden"` 判断；完全停止 RAF 会导致用户切回来图谱位置重新计算跳一下；2fps 够维持 AOP 仪表盘数据不脏；切换前台时 requestAnimationFrame 恢复到 60fps。
6. **GraphCanvas 的 on_node_click 事件必须是 dioxus EventHandler，不闭包 capture ctx 引用**：move || { write(ctx...) } 导致 Component rerender 时 EventHandler clone 成本爆炸（每 click clone 整个 signal）；正确模式：EventHandler<NodeId> 用 dioxus 自带通道，回调里只用局部变量 id（不从外层 move 大对象）。
7. **AopGauge 的双刻度上下环颜色必须对应当前状态（不是固定）**：队列延迟 < 50ms HUD_GREEN 正常；50-200ms HUD_ORANGE 警告；> 200ms HUD_RED 严重；不要固定 HUD_BLUE 显示（误导运维）；统计图表页每 5s 轮询 stats_query 接口后，自动按延迟值 set_color。
