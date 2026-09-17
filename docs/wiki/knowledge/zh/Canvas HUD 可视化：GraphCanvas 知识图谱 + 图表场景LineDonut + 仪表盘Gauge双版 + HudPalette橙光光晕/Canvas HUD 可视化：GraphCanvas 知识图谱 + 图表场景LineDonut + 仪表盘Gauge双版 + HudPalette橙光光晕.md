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
  - "frontend/src/components/ring_progress.rs"
  - "frontend/src/components/time_range_picker.rs"
  - "frontend/src/components/chat/chat_side_panel.rs"
  - "frontend/src/components/node_card.rs"
  - "frontend/src/pages/message/chat.rs"
source_files:
  - 'frontend/src/components/graph_canvas.rs#L1-L80 (GraphCanvas 组件：dioxus_canvas::Canvas 节点 + 2D Context 渲染；属性 knowledge_graph: KnowledgeGraphDto + 交互：拖拽节点 + 滚轮缩放 + hover 显示摘要 tooltip)'
  - frontend/src/components/canvas_scene.rs (CanvasScene Trait：统一 Scene 生命周期 fn setup(ctx) / fn update(dt_secs) / fn draw(&2DContext) / fn handle_event(event)；GraphScene / ChartScene / WorkspaceScene 三实现)
  - 'frontend/src/components/graph.rs (Graph 数据结构：节点 nodes: Vec<Node{id,label,x,y,attr}> + 边 edges: Vec<Edge{src,dst,weight}>；Vec<id> 索引而非 HashMap，查邻接用 edges 遍历)'
  - frontend/src/components/force_layout.rs#L1-L100 (ForceLayout 力导向算法：每 tick 算斥力（所有节点对库仑力）+ 引力（边胡克力）+ 中心拉力；alpha 冷却 0.99^tick；300 帧后停止节省 CPU)
  - frontend/src/components/layered_layout.rs (LayeredLayout 分层布局：按 knowledge node depth 或 category 分层；Sugiyama 四阶段简易版，去除交叉最小化，用在 Agent 依赖树和项目任务 DAG)
  - frontend/src/components/chart_scene.rs#L1-L60 (ChartScene 统一图表场景：折线 LineChart 数据点 + 时间轴 + 坐标轴 + 鼠标 hover 十字准星 + tooltip；Donut 饼图多环)
  - frontend/src/components/charts/line_chart.rs (LineChart 组件：内部用 ChartScene；props: points: Vec<TimeSeriesPoint{ts, value}> + series: String + color；点数据 > 500 自动降采样 200 点防渲染卡顿；轴刻度两处自适应：Y 轴数值走 utils/number.rs::format_compact_axis（K/M/B 三级进位、≤5 字符），X 轴 x_axis_time_format() 按**桶宽**（min_step_ms() 取相邻 interval_start 最小正间隔）选 TimestampFormat：桶宽 ≥1 天→Date、细于一天且同自然日→TimeOfDay、细于一天但跨天→DateTime，单点/空数组→Date；标签个数按画布可用宽度反推 sample_indices() 并贴边夹取；12 单元测试覆盖)
  - frontend/src/components/gauge.rs (Gauge 仪表盘：240° 圆弧刻度 + 指针 + 0-100 值映射；HUD 风格橙光描边；AopGauge 同组件 + 双刻度（队列长度 + 延迟毫秒）)'
  - frontend/src/components/hud_palette.rs (HudPalette 调色板：HUD_ORANGE #FF8C00 / HUD_BLUE #00BFFF / HUD_GREEN #32CD32 / HUD_RED #FF4444；draw_glow_stroke(ctx, color, line_width) 加 box-shadow 光晕 blur 8px 渲染橙光条)
  - frontend/src/components/canvas_scene.rs#L112-L117 (measure_text_width：web-sys TextMetrics 精确测量，极端异常回退到字符数×字号×0.6 估算)
  - frontend/src/components/graph_canvas.rs#L21+ (measure_text_width 调用点：节点 label、tooltip、边标签)
  - docs/archive/design-archive/canvas_rendering_playbook.md（§CanvasScene 统一渲染管线 §力导向参数 α 冷却规则 §橙光光晕 blur 值调优 §5 层 Canvas 节点叠放顺序）
  - docs/design/ui_design_system.md（§HUD 驾驶舱风格视觉规范 §DaisyUI 基础组件 + 自定义 HUD 组件融合方式 §30+ 主题的配色适配策略）
  - docs/archive/plan-archive/统计图表Phase1基础设施与时序图展示重构.md（§LineChart 降采样算法 §时间轴月份刻度 §ChartScene 统一基类抽取）
  - docs/archive/plan-archive/统计图表Phase2.md（§DonutChart 类目分环 §多系列折线叠加 §Dashboard 5指标卡片 + 2仪表盘组合页）
  - docs/archive/plan-archive/统计图表第三期.md（§AopGauge 双刻度仪表盘 §KnowledgeGraph 种子节点推荐高亮 §recommend_seed_nodes 三因子分数映射到节点颜色）
  - docs/archive/plan-archive/知识图谱推荐起点与组件复用重构.md（§GraphCanvas 组件两端复用：HR记忆搜索页 + Workspace工作台页 §种子节点推荐圆圈外发光）
  - docs/wiki/zh/content/前端应用/页面模块/HR 管理页面/知识图谱可视化.md（HR 知识图谱页面：GraphCanvas + ForceLayout + 节点点击跳转 /hr/memory-search?id=）
  - docs/wiki/zh/content/前端应用/组件系统/图表组件/图表组件.md（图表组件总览：LineChart/DonutChart/Gauge 三组件 + 使用模式 + 降采样与性能建议）
  - docs/wiki/zh/content/前端应用/组件系统/业务组件.md（业务组件：GraphCanvas/RuntimePanel/ChatSidePanel/MessageBubble 四件套 + HUD 风格示例）
  - 【平行卡 1】docs/wiki/knowledge/zh/DuckDB 多维统计双层互补：record_event! 宏自动表推断 + RuntimeStatsCollector 内存滑动窗口 + 5 维度开箱即用表/DuckDB 多维统计双层互补：record_event! 宏自动表推断 + RuntimeStatsCollector 内存滑动窗口 + 5 维度开箱即用表.md（统计数据来源：stats_query API 返回 TimeSeriesPoint[] → LineChart 组件渲染）'
  - 【平行卡 2】docs/wiki/knowledge/zh/知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出/知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出.md（KnowledgeGraphDto 数据来源：traverse_knowledge_graph API → GraphCanvas 渲染的 nodes/edges）
  - frontend/src/components/chat/chat_side_panel.rs
  - frontend/src/components/stats.rs

  - frontend/src/components/ring_progress.rs (2026-09-11 新增：通用环形进度组件，Canvas 2D 画圆环 + 填充弧；Agent context 占比专用)
  - frontend/src/components/time_range_picker.rs (2026-09-11 新增：通用时间区间筛选组件，start/end + 预设快捷按钮)
  - frontend/src/components/chat/chat_side_panel.rs (2026-09-11 增量：引用块即时显示 + 宽度以本条消息为上限)
  - frontend/src/pages/message/chat.rs (2026-09-11 增量：引用块即时显示)
  - frontend/src/components/node_card.rs (2026-09-17 新增：矩形信息卡的几何/文案/配色 SSOT，中立无业务依赖 —— NODE_BOX_W=168 + box_width/box_height/wrap_text/tag_chips/hover_lines/type_label；graph.rs（SVG）与 canvas_scene.rs（Canvas）双端共用，此前几何散在 graph.rs 导致基础设施反向依赖业务组件)
  - frontend/src/components/graph.rs (2026-09-17 增量：GraphNode 新增 description；卡片几何改为 node_card 的薄包装；SVG 卡片宽度改走 node_box_width（内容驱动收窄）)
  - frontend/src/components/graph_canvas.rs (2026-09-17 增量：**删除 KnowledgeGraphRenderer 死代码**（565→145 行），降为纯适配层；关系类型进 CanvasEdge.tag；启用 ForceLayout + 四类粒子，对齐 Agent 关系图)
  - frontend/src/components/canvas_scene.rs (2026-09-17 增量：props.edges 同步进 edges_state signal（修复「展开节点后新边不渲染」）；DefaultRenderer 升级双形态（圆形 / 矩形信息卡，由 CanvasNode::is_card 判定）+ 边 hover 橙色加粗 + hover 卡画布内避让；新增 selected_node_id 受控 prop；边 hover 阈值 6→10px)
  - frontend/src/components/force_layout.rs (2026-09-17 增量：按等效半径的碰撞分离力 + 弹簧自然长度按两端半径放宽 + 边界留白取最大半径 —— 修复「卡片挤成一坨」)
  - common/src/api/neural_tools.rs (2026-09-17 增量：MemoryResult 新增 name 字段，知识节点 = node_name；content 仍是 node_description；同日再增 weight 字段 = 关系强度 0~1，仅 relation 类型有值，`None` = 未标注)
  - frontend/src/components/edge_style.rs (2026-09-17 新增：边权重的视觉映射 SSOT —— weight_style(强度) → (线宽 1.1~3.8, 不透明度系数 0.45~1.0)、weight_label → hover 读数；未标注走基准线宽 1.5，与「强度 0」区分；Canvas 与 SVG 双路径共用，禁止各写一套系数)
  - frontend/src/components/canvas_scene.rs (2026-09-17 增量：CanvasEdge 增 weight；draw_edges 的色相/基色不透明度与强度系数拆开算（颜色只表达语义，粗细+浓淡表达强度）；hover 高亮线宽按被 hover 边的实际线宽 + 1.5 计算，避免固定 3.0 盖不住粗线)
  - migrations/20260917000001_add_weight_to_knowledge_relation.sql (2026-09-17 新增：knowledge_node_relation 加 weight REAL 可空无默认值，存量行 = NULL = 未标注)
  - src/handlers/hr/agent/save_long_term_memory.rs (2026-09-17 增量：relations[].weight → normalized_weight() 归一化后落库；工具描述与沉淀提示词同步说明「拿不准就省略」)

  - frontend/src/components/chat/chat_side_panel.rs (2026-09-18 增量：任务依赖图前端自绘 build_task_graph_data + 缩略图 300x200 点击弹 Modal 放大 920x620 + 悬挂依赖边过滤)
  - frontend/src/components/graph.rs (2026-09-18 增量：svg_width/svg_height 可配 props（clamp 最小 160/120）+ task_status_node_type 任务状态语义化 token + NEUTRAL_NODE_FILL/NEUTRAL_EDGE_COLOR 兜底色 + 4 条守卫测试)
  - frontend/src/components/layered_layout.rs (2026-09-18 增量：y 自适应——层距压缩 min(layer_height, usable_height/max_layer) + 深链垂直居中，修复深链 DAG 纵向溢出)
  - frontend/src/components/node_card.rs (2026-09-18 增量：type_label 补 5 个任务状态中文文案)

  - 【平行卡 3】docs/wiki/knowledge/zh/统计查询 API 与前端仪表盘：DuckDB 5 维表查询 + RuntimeStats 内存滑动聚合 + StatsHandler REST API + 前端 Line/Donut/Gauge 展示/统计查询 API 与前端仪表盘：DuckDB 5 维表查询 + RuntimeStats 内存滑动聚合 + StatsHandler REST API + 前端 Line/Donut/Gauge 展示.md（TimeRangePicker 消费方：统计看板时间筛选）
  - 【平行卡 4】docs/wiki/knowledge/zh/思考运行时前端观测：runtime-status cancel-thinking runtime-list 接口与 runtime_panel 组件/思考运行时前端观测：runtime-status cancel-thinking runtime-list 接口与 runtime_panel 组件.md（RingProgress 消费方：Agent 上下文 Token 占比展示）
---

## §1 概述

**本卡角色**：前端 HUD 驾驶舱风格 Canvas 可视化体系知识卡。覆盖 GraphCanvas（知识图谱 Canvas 渲染 + 力导向/分层两布局）、ChartScene 统一图表场景（LineChart 时序折线/DonutChart 甜甜圈）、仪表盘 Gauge/AopGauge 双刻度、HudPalette 橙光调色板 + draw_glow_stroke 光晕工具。**定位：新增图表类型、调整图谱布局卡顿、排查 HUD 橙光效果被主题色覆盖、调力导向 alpha 冷却参数时读。**

- **CanvasScene Trait 统一渲染管线（所有可视化共享）**（canvas_scene.rs）：① setup() 创建时初始化数据 + 分配缓存顶点数组；② update(dt: f32) 每帧 tick 传 dt 秒（ForceLayout alpha 冷却/Particles 位移用）；③ draw(&ctx: &CanvasRenderingContext2d) 纯绘制；④ handle_event(event: CanvasEvent) 处理鼠标拖拽/滚轮缩放。GraphScene/ChartScene/GaugeScene 都 impl CanvasScene，统一 dioxus::use_effect 注册 requestAnimationFrame 循环。帧率策略：后台 tab（document.hidden=true）→ 自动降到 2fps（不退出循环），前台 tab 60fps；Graph 节点数 < 100 用 60fps，>500 用 20fps（自动 clamp）。
- **GraphCanvas 图谱双布局 + 两端复用**（graph_canvas.rs + HR 知识图谱页 + Workspace 工作台页）：ForceLayout 力导向用于自由探索（知识图谱 HR 页）：所有节点对算 1/r² 斥力 + 胡克力边引力 + center(0,0) 中心拉力；alpha 冷却系数 α_t = 0.99^t，300 帧后 α<0.01 → stop。LayeredLayout 分层用于结构化视图（任务 DAG/Agent 工具依赖）：先算 depth 层号（BFS）→ 层内等分 x → 层间按 y 等分；不做连线交叉最小化（性能优先，仅按 edge weight 重排）。两端复用：HR 页和 Workspace 页都用同一 GraphCanvas 组件，仅 props 的 layout_mode="force" | "layered" + 数据来源不同；种子节点推荐 recommend_seed_nodes 返回的 node.score → 映射到节点颜色（HUD_ORANGE 高分→HUD_BLUE 低分）+ 外发光 draw_glow_stroke。
- **HUD 风格橙光调色板（HudPalette）+ 仪表盘 Gauge**（hud_palette.rs + gauge.rs）：4 主色 HUD_ORANGE/HUD_BLUE/HUD_GREEN/HUD_RED；draw_glow_stroke 实现：先 `ctx.shadow_blur = 8.0` + `ctx.shadow_color = HUD_ORANGE` → 画一次描边（光晕）→ reset shadow → 画第二次正常描边（实线）；这样 CSS 不会被 DaisyUI 主题覆盖（是 Canvas 2D API，不是 DOM）。仪表盘 Gauge：value 0-100 → 映射到 240° 圆弧起点角度 150° 到终点 390°；指针三角箭头 + 刻度 20 条（每 20 一条长刻度）；AopGauge（aop_gauge.rs）同 Gauge 组件 + 上半圆环 AOP 队列延迟毫秒 + 下半圆环消费者阻塞数 双刻度，System AOP 页用。HudPalette 新增 HUD_SECONDARY(#22d3ee 青蓝) + HUD_TERTIARY(#a78bfa 紫) 两色，支撑 LineChart 同轴三条曲线（输入/输出/total）冷暖和对比色区分。
- **聊天侧栏 Agent Tab 消费图表组件**（chat_side_panel.rs + stats.rs）：LineChart 组件从「主要在统计仪表盘」扩展到聊天侧栏 Agent 运行统计 Tab；AgentStatsPanelCompact 紧凑面板（320x180 原生渲染），展示唤醒次数 + Token 消耗（input/output/total）三线趋势；HudPalette 橙光光晕风格 + 次色/第三色区分曲线；聊天侧栏 Tab 按需加载（共享轮询高频链路零额外开销，统计仅在 Tab 挂载时触发）。

**RingProgress 通用环形进度组件**（2026-09-11 新增）：`frontend/src/components/ring_progress.rs` 纯 Canvas 2D 渲染（非 DOM），props: `ratio: f32`（0.0-1.0 进度比例）+ `color: String`；内环半径 + 外环 strokeWidth + 橙色光晕（HudPalette.draw_glow_stroke 复用）；Agent 上下文 Token 占比专用——AgentRuntimeState.tokens_used_ratio → RingProgress 渲染 + context_threshold 为 None 时显示配置入口提示。

**TimeRangePicker 通用时间区间筛选组件**（2026-09-11 新增）：`frontend/src/components/time_range_picker.rs` props: `start/end` DateTime + 预设快捷按钮（近 1h / 6h / 24h / 7d / 30d）+ 自定义日期选择器（dioxus-datepicker 集成）。所有统计看板统一引入——AOP 系统页、ModelProvider Token 时序、用户页统计。

**聊天引用块即时显示 + 宽度以本条消息为上限**（2026-09-11 增量）：chat.rs 消息列表中带引用（reference_id）的消息，即时渲染引用块（气泡上方显示被引用的消息摘要），无需 hover 才弹出；引用块宽度严格限制为「本条消息气泡宽度」，不再溢出撑破消息列表布局。

**任务依赖图前端自绘 + 放大弹窗（2026-09-18 增量，commit a7134bb7）**：聊天侧栏任务列表新增 DAG 依赖图——`chat_side_panel.rs::build_task_graph_data(tasks, w, h)` 纯前端从任务列表构建 GraphNode/GraphEdge（前置任务不在列表内的**悬挂依赖直接丢弃**，不产生缺失端点的边；`with_task_graph` 后端产物图改为不传，依赖图不再依赖后端生成），复用 graph.rs 的 LayeredLayout 分层渲染 SVG。缩略图 300x200，点击弹 Modal 放大 920x620（`graph_zoom_open` signal 状态放主组件，弹窗 top layer 渲染不受侧栏 overflow 裁剪），两个视图共用同一构建函数仅尺寸不同。graph.rs 配套扩展：① `svg_width`/`svg_height` 可配置 props（默认 800/600，clamp 最小 160/120）；② `task_status_node_type(status)` 把任务状态数值转语义化 token（cancelled/pending/in_progress/completed/archived），`get_node_fill` 配 5 个语义色（cancelled 红 / pending 蓝 / in_progress 琥珀 / completed 绿 / archived 灰）+ `NEUTRAL_NODE_FILL`/`NEUTRAL_EDGE_COLOR` 兜底色；③ 4 条守卫测试（relation 标签 / 任务状态 / memory 类型全覆盖，未知标签回落中性色）。配套 **layered_layout.rs y 自适应**：深链 DAG 层距压缩为 `min(config.layer_height, usable_height/max_layer)` + 垂直居中（`v_offset`），修复深链任务图纵向溢出画布把最后几层裁掉；环上节点沉底（无入度为 0 节点时 layer=0、bottom=1）。node_card.rs 同步补 5 个任务状态中文 type_label（如 task_in_progress → "任务（进行中）"）。

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

**知识图谱节点/边语义化展示 + hover 详情卡片（d248829a）**：节点标签改用"名称 + 类型徽标（type_label 中文化）"，边标签全部中文化，id 不再上图。**hover 详情卡片统一规格**：Canvas（graph_canvas.rs）与 SVG（graph.rs）双路径复用同一绘制规格——节点卡含名称/类型徽标/摘要/标签，边卡含关系类型 + 端点。draw_hover_card（canvas_scene.rs#L184）纯渲染函数，hover_card_size（#L170）精确测量 → 双路径各自 build_hover_card 组装 lines 后走同一 draw 入口。**画布边界避让**：卡片超右缘自动翻左侧、纵向 clamp 在画布内，不再超出边界被截断。**渲染顺序修正**：Canvas 渲染时边先于节点，边 hover 卡片暂存 pending_edge_card，全部节点绘制后补绘防遮挡（边在下层，节点卡在上层不被边覆盖）。**hover 互斥防闪烁**：节点 hover 与边 hover 事件层互斥，只保留最后一个 hover 对象的卡片，避免重叠闪烁。删除 `common MemoryType::zh_label` 死代码方法（返回临时值引用 E0515），type_label 改为纯静态字符串映射无临时值问题。

**边权重表达关联强度（2026-09-17）**：关系边此前只有 `relation_type` 一列，图上所有连线长得一模一样，「A 依赖 B」与「A 顺带提到 B」无法区分。加 `knowledge_node_relation.weight`（REAL 可空，`NULL` = 未标注）后，`edge_style::weight_style` 把它映射成**线宽（1.1~3.8）+ 不透明度系数（0.45~1.0）**，未标注走基准线宽 1.5；hover 边卡在标注过时多一行 `强度: 80%`。强度**不占用颜色通道**（颜色已被关系类型哈希色与 ready/not_ready 语义色占用）。写入侧由 `save_long_term_memory` 的 `relations[].weight` 声明并经 `normalized_weight()` 归一化，**缺省不落默认值**。可选的替代方案「派生权重」被否决：候选信号（共现证据数）依赖 `knowledge_reference`，而节点写入路径恒传 `references: vec![]`，该表为空 → 派生值恒 0。
```

---

## §4 硬约束与回归红线（28 条）

1. **Canvas 2D 绘制不能依赖 DaisyUI CSS 变量**：HUD 色必须硬编码 HudPalette 的 const，不要从 window.getComputedStyle 读 --p（DaisyUI 主色），否则 WASM 里 DOM API 跨线程调用 + 切换主题 30+ 每换一次重绘所有 Canvas，性能炸。例外：Canvas 周围 DOM 外壳 card 样式可用 class="bg-base-200"。
2. **ForceLayout 斥力 O(n²) 必须节点数 ≥1000 时降采样**：nodes.len() > 800 自动从 O(n²) 切换到 Barnes-Hut O(n log n) 近似（四叉树空间分块近似斥力）；测试 1500 节点渲染时 dt 单帧 > 32ms（< 30fps）→ 必须启用近似模式；默认模式 O(n²) 够用，代码不预实现 Barnes-Hut（YAGNI）。
3. **LineChart 数据点 >500 必须降采样（LTTB 算法）**：直接把 2000 点画到 400px 宽 Canvas = 每条线叠 5 个点 waste CPU；强制用 LTTB Largest-Triangle-Three-Buckets 降到 ≤200；降采样不改变统计结果（LTTB 保留极值和突变形状）；测试断言降采样后 min/max 值不差过原数组。
4. **draw_glow_stroke 双遍顺序不能反**：shadow 描边（shader 模糊）→ reset shadow → 正常实线描边；顺序反过来 = 实线也被 blur，结果整个 UI 一片虚糊；Grep 所有调用必须是 save → 设置 shadow → stroke(ctx) → restore → 第二次 stroke(ctx)。
5. **后台 tab 帧率自动降到 2fps（不停止 RAF）**：用 `document.visibility_state == "hidden"` 判断；完全停止 RAF 会导致用户切回来图谱位置重新计算跳一下；2fps 够维持 AOP 仪表盘数据不脏；切换前台时 requestAnimationFrame 恢复到 60fps。
6. **GraphCanvas 的 on_node_click 事件必须是 dioxus EventHandler，不闭包 capture ctx 引用**：move || { write(ctx...) } 导致 Component rerender 时 EventHandler clone 成本爆炸（每 click clone 整个 signal）；正确模式：EventHandler<NodeId> 用 dioxus 自带通道，回调里只用局部变量 id（不从外层 move 大对象）。
7. **AopGauge 的双刻度上下环颜色必须对应当前状态（不是固定）**：队列延迟 < 50ms HUD_GREEN 正常；50-200ms HUD_ORANGE 警告；> 200ms HUD_RED 严重；不要固定 HUD_BLUE 显示（误导运维）；统计图表页每 5s 轮询 stats_query 接口后，自动按延迟值 set_color。
8. **TextMetrics 精确测量替代字符数估算**：所有 Canvas 文本布局（节点 label、tooltip、边标签）统一走 `canvas_scene.rs:measure_text_width`（web-sys TextMetrics `ctx.measure_text(text)`），不准再用 `text.chars().count() * font_size` 估算（比例字体 i18n 误差大）；节点文字宽度 > 节点半径的 80% 时自动截断加 `…`；force_layout 布局每帧 dt 必须 clamp ≥ 1ms 避免 RAF 极短间隔抖动
9. **RingProgress 必须纯 Canvas 2D 不依赖 DOM**（2026-09-11 新增）：RingProgress 是 HUD 体系的 Canvas 原生组件，**禁止**改成 DOM + CSS 实现（失去 draw_glow_stroke 光晕效果 + 被 DaisyUI 主题覆盖）；ratio 参数必须 clamp 在 0.0~1.0 之间再绘制，负数值或 >1.0 都截断
10. **TimeRangePicker 预设按钮时间必须后端可用**（2026-09-11 新增）：前端预设快捷按钮（1h/6h/24h/7d/30d）发出的时间区间必须能被后端 Stats 接口接受（ISO 8601 RFC3339 格式）；禁止前端用"秒级时间戳"或自定义格式；所有消费方（AOP 系统页/ModelProvider Token 时序/用户页统计）统一用同一个组件，不各自造时间筛选
11. **聊天引用块宽度以本条消息为上限**（2026-09-11 新增）：引用块 CSS `max-width: 100%` + `overflow: hidden` + `text-overflow: ellipsis`，禁止溢出撑破消息列表；长引用内容截断显示 + tooltip hover 显示完整内容；引用块在消息气泡上方即时渲染（非 hover 弹出），宽度严格 ≤ 本条消息气泡宽度
12. **LineChart X 轴格式按桶宽判定，单点必须显示真实日期**（2026-09-11 回归红线，2026-09-15 判据由「总跨度」改为「桶宽」）：单点时首尾跨度为 0，不能靠间隔推断格式。后端日桶 `interval_start` 由 `timestamp - (timestamp % 86400000)` 生成（UTC 零点对齐），东八区渲染出来恰好是 08:00——一个并不存在的"时刻"。`x_axis_time_format()` 的判据是**桶宽**（`min_step_ms()` 取相邻 `interval_start` 的**最小正间隔**，不用平均/中位数——后端只对有数据的桶出点，空桶被跳过会把平均间隔拉大而误判粒度）：桶宽 ≥ 1 天 → `Date`；细于一天且落在同一自然日 → `TimeOfDay`；细于一天但跨天 → `DateTime`；`data.len() < 2` 一律退化为 `Date`，不准走桶宽判定分支。⚠️ 旧实现按总跨度（<2h → TimeOfDay）判定，会把「最近 1 天」的 24 个小时桶画出一排完全相同的 `9-15`。
13. **统计图坐标轴刻度必须「宽度自证」**（2026-09-15 新增）：①Y 轴数值一律走 `utils/number.rs::format_compact_axis`（f64 入参 + K/M/B 大写后缀 + 字符数 ≤ 5）——`pad_left = 40`、刻度右对齐画在 `x = 34`，10px 字 5 字符 ≈ 27px，**超 5 字符即顶破画布左缘**（旧 `format_axis_value` 只进位到 K，百万级画出 `1234.6K` = 7 字符）；②X 轴标签**个数**按画布可用宽度反推（槽位宽 = 最宽标签 + 最小间隙 → `plot_w / 槽位宽`，再 `sample_indices()` 均匀取样），首末标签按 `text_width/2` 贴边夹取，**禁止写死个数**（窄容器 320px 配右轴后绘图区只剩 ~234px，5 个标签必压字）。
14. **整数读数的量级格式只有 `format_compact_count` 一个入口**（2026-09-15 新增）：卡片 / 徽标 / 顶栏 / 表格单元格里的所有整数读数（Token 数、调用次数、上下文长度…）一律走 `utils/number.rs::format_compact_count`（u64 入参 + `K`/`M`/`B` 大写三级 + 有效位随量级收敛 + 去尾随 0），**禁止就地写 `{:.1}K` 这类局部实现**——存量三份曾各自为政：`stats.rs::format_token_count` 只到 `M`、固定一位小数，`runtime_panel.rs::format_token` 只到 `k` 且小写，工作台顶栏用 `format_compact_count`（小写 `k`），十亿级读数退化成 `1234.6M`，同屏还并存 `1.2k` / `1.2K` 两种写法。后缀大小写与进位档位必须与坐标轴刻度 `format_compact_axis` 共用一套（`number.rs` 单测 `compact_count_and_axis_share_unit_spelling` 断言两者输出相等）；轴刻度专用差异仅剩「f64 入参 + 小数值/轴底 0」（见第 13 条①）。
15. **hover 详情卡片 Canvas/SVG 双路径必须复用同一规格**（d248829a 新增）：`hover_card_size` + `draw_hover_card` 从 Canvas 路径抽到 `canvas_scene.rs#L170-L233` 作为纯渲染函数，SVG 路径（`graph.rs#L62-L88` 的 `hover_card_box` + `build_hover_card`）必须用同样的行文本规格、同样的 padding、同样的字体大小——禁止 Canvas 路径和 SVG 路径各自画一套"hover 卡片"（一个对齐卡片宽高、一个对不齐，用户同一页面切换布局时卡片跳变）。
16. **hover 卡片必须做画布边界避让**（d248829a 新增）：hover 卡片绘制位置 `(x, y)` 必须 clamp：①卡片右缘 `x + card_w` 超画布宽 → 翻转到鼠标左侧 `x - card_w - 24`；②卡片底部 `y + card_h/2` 超画布底 → `y = card_h/2`；③卡片顶部 `< 0` → `y = card_h/2`。**禁止直接按鼠标位置画不避让**——图谱节点靠近右边界时卡片会被截掉一半。
17. **Canvas 渲染顺序必须：边 → 节点 → 边 hover 卡片 → 节点 hover 卡片**（d248829a 新增）：Canvas 2D 无 z-index，最后画的在上层。边 hover 卡片若在边绘制完立即画，会被后续节点覆盖——必须用 `pending_edge_card` 暂存，全部节点画完后补绘边卡，节点 hover 卡最后画。违反此条 = 边 hover 卡片被节点遮挡看不见。
18. **hover 事件层节点/边互斥防闪烁**（d248829a 新增）：同一帧内命中边后又命中节点（或反之）时，**只保留最后一个 hover 对象**的卡片——禁止两张卡片同时显示，也禁止用"取消第一张卡片 → 立即画第二张"这种逐帧重建。实现：`hover_card_data` 一个 Option，每次 hover 事件覆盖更新，渲染循环统一按当前值画一张。
19. **type_label 必须是纯静态字符串映射，禁止返回临时值引用**（d248829a 新增）：`common::enums::memory::MemoryType::zh_label()` 曾返回 `format!("…")` 的临时 String 被 & 引用——Rust E0515。正确做法：`type_label(t: &str) -> &'static str`（graph.rs#L171）用 `match t { "fact" => "事实", ... }` 直接返回字符串字面量。**所有枚举"中文化 label"函数必须是纯映射、返回 `&'static str`**，禁止临时 String 引用。
20. **矩形信息卡的几何/文案 SSOT 在 `node_card.rs`，不在 `graph.rs`**（2026-09-17 修订）：`graph.rs` 是业务组件，而 `canvas_scene.rs`（基础设施）也要用同一套几何 —— 基础设施反向依赖业务组件是错的，故下沉到中立的 `node_card.rs`（纯几何、零业务依赖、可单测）。卡片宽 `NODE_BOX_W=168`（`box_width` 按内容在 104~168 收窄）、高度 `box_height`（按实际正文行数）、折行 `wrap_text`、标签 `tag_chips`、hover 行 `hover_lines` **只允许从 node_card.rs 引用**——任何一边自算尺寸必然漂移（改了一边宽度忘改另一边折行 = 文字溢出卡片或命中区错位）。注意 `wrap_text` 用估算宽度（CJK=字号、ASCII=0.55×字号）而非 TextMetrics，因为 SVG 无法精确测量；两边必须同口径。
21. **CanvasScene 的边列表必须走 signal，禁止在 RAF 闭包里捕获 props.edges 快照**（2026-09-17 新增）：渲染循环闭包捕获的是「effect 当次运行时」的 edges——展开节点后新增的边不参与绘制，症状是「点击节点展开后新出来的节点没有连线」（节点走 signal 每帧刷新、边却停在挂载时，不报错、纯视觉缺失）。边与节点同构：`use_effect(use_reactive(&props.edges, …))` 同步进 `edges_state` signal，RAF 每帧 `read().clone()`。边 hover 命中阈值用 10px（1.5~2px 的细线配 6px 几乎点不中，用户会以为「连线不能 hover」）。
22. **不要为图谱另写渲染器：`CanvasScene` 只认内置 `DefaultRenderer`**（2026-09-17 新增）：`graph_canvas.rs` 曾有 400+ 行 `KnowledgeGraphRenderer`，因 `canvas_scene.rs` 硬编码 `let renderer = DefaultRenderer` 而**从未执行**——注释写着「用自定义 HUD 效果避免视觉过载」，实际跑出来是默认圆圈 + 圆下完整 ID，用户看到的就是「只显示『知识节点』和一个 id」。节点形态差异一律用数据表达（`CanvasNode::is_card()`：有 `summary` / `description` / `tags` 即矩形卡片，否则圆形）。若将来真要支持多渲染器，必须先把注入通道做进 props 并有测试覆盖，否则等价于死代码。
23. **力导向必须按节点等效半径做碰撞避让，卡片节点的 `radius` 要填外接圆半径**（2026-09-17 新增）：`1/d²` 点斥力在近距离压不住 168px 宽的卡片，结果是「节点挤成一坨、连线糊在底下」；`ForceLayout::step` 已加碰撞分离力（最小中心距 `(r_i + r_j) * 1.15`）、弹簧自然长度按两端半径 + 60px 放宽、边界留白取最大半径。⚠️ 适配层若把卡片节点的 `radius` 填成小圆半径，卡片照样互压（该字段同时是无正文端点节点圆形形态的绘制半径）。
24. **边强度只准走「粗细 + 不透明度」，颜色留给语义；未标注（`None`）≠ 强度 0**（2026-09-17 新增）：强度映射 SSOT 在 `edge_style.rs`（`weight_style` → `(线宽, 不透明度系数)`、`weight_label` → hover 读数），Canvas 与 SVG 两条路径只许引用、禁止各写一套系数（同一个强度在两个视图里粗细不同 = 用户以为数据变了）。三条红线：①**颜色通道已被关系类型/状态语义占用**（`tag_color` 哈希取色、关系图 ready/not_ready 语义色），拿颜色表达强度会和语义色打架；②`None`（未标注）渲染**基准线宽**，只有 `Some(0.0)` 才是最细最淡——把未标注当 0 会让存量关系整片塌到最细，看起来像「所有关联都很弱」，那是伪造语义；③hover 只在**标注过**时渲染 `强度: N%`，未标注整行不渲染（写「强度 0%」会被读成「明确很弱」），共享的 `canvas_scene` 因此不会给 Agent 关系图平添噪音。配套：写入侧缺省**不落默认值**（补 0.5 会把「没标」变成「标了中等强度」），归一化规则 `KnowledgeRelationParam::normalized_weight`（越界夹紧、NaN/Inf 丢弃）必须与前端 `edge_style::normalize` 同口径。反向教训：想让图谱边先「有差异」再谈准确，去派生权重（共现证据数）是走不通的——`knowledge_reference` 恒为空表（两条节点写入路径都传 `references: vec![]`）。
25. **任务状态新增取值必须同步 `task_status_node_type` + `get_node_fill`，守卫测试兜底**（2026-09-18 新增）：`graph.rs` 的 `task_status_node_types_all_have_semantic_colors` 守卫测试枚举全部状态值断言非 `NEUTRAL_NODE_FILL`——新增 TaskStatus 枚举项若漏改颜色映射表直接测试失败，而不是静默画成中性灰被当成「未知类型」；同理 relation 标签（`vocabulary_relation_labels_all_have_semantic_colors`）与 memory 类型（`memory_type_values_all_have_node_colors`）各有守卫，未知值统一回落中性色常量。
26. **SVG 渲染尺寸必须走 `svg_width`/`svg_height` props，禁止 fork 组件改常量**（2026-09-18 新增）：同一 Graph 渲染组件被缩略图（300x200）与放大弹窗（920x620）复用，尺寸 clamp 最小 160/120 防负值/过小；新增消费方传 props 即可，严禁复制组件副本改写死尺寸。
27. **layered_layout 深链必须压缩层距适配画布高度**（2026-09-18 新增）：层距 = `min(config.layer_height, usable_height/max_layer)` + 垂直居中偏移 `v_offset`，禁止固定 layer_height 直排——深链任务图会纵向溢出画布把最后几层裁掉；环（无入度为 0 节点）整体沉到最底层（layer=0、bottom=1）。守卫测试：深链分层各层 y 不超画布 + 浅链垂直居中。
28. **依赖图构建必须过滤悬挂依赖边**（2026-09-18 新增）：前置任务不在当前任务列表内（已删除/跨项目）时直接丢弃该边，禁止产出缺失端点的 GraphEdge——SVG/Canvas 对未知节点索引会 panic 或画出飞线；任务依赖图缩略图与放大弹窗共用 `build_task_graph_data`，过滤逻辑只写一处。
