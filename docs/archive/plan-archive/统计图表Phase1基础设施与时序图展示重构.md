# 统计图表Phase1基础设施与时序图展示重构

> 📦 归档标记（2026-08-16）：归档冻结。保留原因：统计图表Phase1基础设施与时序图展示重构 功能已完成并通过验收，文档转为历史快照。生效方案：见源码和 wiki 长文。

> 文档角色：plan（要去哪 + 完成状态快照），归档后查阅意图：
> - 新增图表组件（环形/柱状）时，回看"ChartRenderer trait 扩展"与"hud_palette 视觉统一"两处
> - 若需在 StatsPanel 增加更多时序图或页面级 Dashboard，直接跳转对应组件文件（见 §涉及文件）
> 关联文档：
> - 对应 design 文档：[stats_module_design.md](../archive/design-archive/stats_module_design.md) — DuckDB 持久化 + RuntimeStats 内存双层统计框架设计
> - 姊妹 Plan 关联：
>   - [统计图表Phase2.md](统计图表Phase2.md) — 五维度统计面板（Agent/Project/Task/ModelProvider/Tool）+ TokenSumResult 接口
>   - [统计图表第三期.md](统计图表第三期.md) — AOP 事件统计面板集成 RuntimeStatsCollector 内存滑动窗口
>   - [通用后台任务模块与Seed异步化重构.md](通用后台任务模块与Seed异步化重构.md) — 姊妹：任务进度百分比可视化面板
>   - [HNSW持久化与索引重建异步化重构.md](HNSW持久化与索引重建异步化重构.md) — 向量重建进度后续可接同款折线图组件
> - Wiki 长文真实路径：[docs/wiki/zh/content/项目概述/核心功能特性/多维统计系统/多维统计系统.md](docs/wiki/zh/content/项目概述/核心功能特性/多维统计系统/多维统计系统.md) — 五维度总览卡片入口
> - Wiki 长文真实路径：[docs/wiki/zh/content/前端应用/组件系统/可视化组件/HUD Canvas 图表组件族.md](docs/wiki/zh/content/前端应用/组件系统/可视化组件/HUD%20Canvas%20图表组件族.md) — ChartRenderer trait + hud_palette + LineChart/Dount/Gauge 复用
> - RAG 卡真实路径 1：[docs/wiki/knowledge/zh/统计查询 API 与前端仪表盘：DuckDB 5 维表查询 + RuntimeStats 内存滑动聚合 + StatsHandler REST API + 前端 Line/Donut/Gauge 展示/统计查询 API 与前端仪表盘：DuckDB 5 维表查询 + RuntimeStats 内存滑动聚合 + StatsHandler REST API + 前端 Line/Donut/Gauge 展示.md](docs/wiki/knowledge/zh/统计查询%20API%20与前端仪表盘：DuckDB%205%20维表查询%20+%20RuntimeStats%20内存滑动聚合%20+%20StatsHandler%20REST%20API%20+%20前端%20Line/Donut/Gauge%20展示/统计查询%20API%20与前端仪表盘：DuckDB%205%20维表查询%20+%20RuntimeStats%20内存滑动聚合%20+%20StatsHandler%20REST%20API%20+%20前端%20Line/Donut/Gauge%20展示.md)
> - RAG 卡真实路径 2：[docs/wiki/knowledge/zh/Canvas HUD 可视化：GraphCanvas 知识图谱 + 图表场景LineDonut + 仪表盘Gauge双版 + HudPalette橙光光晕/Canvas HUD 可视化：GraphCanvas 知识图谱 + 图表场景LineDonut + 仪表盘Gauge双版 + HudPalette橙光光晕.md](docs/wiki/knowledge/zh/Canvas%20HUD%20可视化：GraphCanvas%20知识图谱%20+%20图表场景LineDonut%20+%20仪表盘Gauge双版%20+%20HudPalette橙光光晕/Canvas%20HUD%20可视化：GraphCanvas%20知识图谱%20+%20图表场景LineDonut%20+%20仪表盘Gauge双版%20+%20HudPalette橙光光晕.md)

---

## 一、重构目标（为什么做）

统计数据后端有 model_call_time_series 时序数据，但前端从未消费过该字段，4 个实体详情页（Agent/Project/Task/ModelProvider）只有数字卡片无趋势可视化；知识图谱 Canvas HUD 背景绘制代码在 graph_canvas.rs 内私有，想写图表还得再复制一份径向渐变+网格+四角，未来视觉改动会两处不同步。

| 问题维度 | 解决方式 |
|---------|---------|
| (a) 4 详情页有时序数据但前端零展示 | HUD 风格 LineChart 组件消费 Vec<TimeSeriesPoint>；4 StatsPanel 数字卡片下方渲染折线图展示模型调用趋势 |
| (b) HUD 背景绘制代码重复，视觉难统一 | 抽取 hud_palette.rs 公共工具（draw_hud_background + hex_to_rgba + 4 细分函数）；知识图谱 clear 改用调用；图表 clear 直接复用 |
| (c) 图表基础设施空白，后续加图得重复写 Canvas rAF 循环 | ChartRenderer trait（clear/draw 接口契约）+ LineChart 组件内自包含 rAF+DPR+use_drop 模式，未来加环形图/柱状图复用同模式 |
| (d) 详情页请求统计时可能缺 stats_interval 参数导致时序字段为空 | 4 详情页 StatsOptions 确认（缺失则补）stats_interval；后端 Domain with_model_call_stats=true 时自动 with_time_series=true |

**收敛后效果**：4 详情页 StatsPanel 下方出现 HUD 风格（深色径向渐变+橙色折线+发光呼吸光晕+流光）折线图，后端时序数据真正前端可视化；新增图表组件未来只需实现 ChartRenderer trait 或复制 LineChart 的 rAF 模式。

---

## 二、架构思路（怎么做的）

```
视觉统一层（零重复）
└─ hud_palette.rs
   ├─ 常量 HUD_PRIMARY / HUD_BASE_BG
   ├─ hex_to_rgba(hex, alpha) 工具函数
   └─ draw_hud_background(ctx, w, h)
      ├─ draw_hud_base  深色基底
      ├─ draw_hud_radial_glow  径向渐变光晕
      ├─ draw_hud_grid  40px 橙色网格
      └─ draw_hud_corners  四角装饰刻度
   知识图谱 CanvasRenderer.clear  调用 draw_hud_background
   图表 LineChart draw_chart  调用 draw_hud_background

渲染基础设施层
└─ chart_scene.rs
   └─ ChartRenderer trait { clear(ctx, w, h); draw(ctx, w, h, now) }
      定义统一接口形状。Phase 1 LineChart 不强依赖 trait，
      组件内直接实现 rAF 循环。第二种图表出现时再强制抽象。

图表组件层
└─ charts/line_chart.rs
   ├─ LineChartProps { data: Vec<TimeSeriesPoint>, width?, height?, title?, value_label? }
   ├─ rAF 递归渲染循环（Rc<RefCell<Closure>> + running AtomicBool + use_drop 清理）
   ├─ DPR 高清屏适配（canvas set_width*=dpr + ctx.scale）
   └─ draw_chart：hud 背景 → 标题 → 空数据提示 → 坐标系(pad_left=40/right=16/top=32/bottom=28)
      → Y 轴 4 等分刻度 + 值标签 → 数据点坐标计算 → 折线发光流光（shadow_blur + line_dash_offset）
      → 数据点呼吸光晕（2.4s 周期，alpha 0.37~0.73 摆动） → X 轴日期标签（>6 点取 5 个采样点）

消费层（4 实体详情页）
  stats.rs 4 StatsPanel（Agent/Project/Task/ModelProvider）
    └─ 通用辅助 render_time_series_chart(model_call_stats)
       读取 model_call_stats.model_call_time_series → 非空则渲染 LineChart
       （宽 600 / 高 200 / 标题=模型调用趋势 / value_label=调用次数）
  ToolStatsPanel 不改（工具无 model_call_time_series 数据）
```

**关键边界（行为红线，回归必保）**：
1. **hud_palette 视觉同步红线**：知识图谱 graph_canvas.rs 原 clear 方法删除 HUD 背景自绘代码，改为直接调用 `hud_palette::draw_hud_background`。若未来调整 HUD 视觉（如改橙色主色、调整网格间距），**只改 hud_palette.rs 一处，知识图谱和所有图表自动同步**
2. **LineChart 画布高度默认 200px**：宽度响应式跟随容器（`class: w-full`），height 不响应式。未来如需响应式高度，在组件内加 `ResizeObserver`，不在 Phase 1 做
3. **数据点呼吸动画 2.4s 周期不可调**：alpha 0.55 ± 0.18；折线流光 dash_offset 30px/秒。两种动画同视觉语言，和知识图谱粒子动画速率一致（后续如需调参，放到 hud_palette 常量）
4. **空数据语义**：`data.len() == 0` 时，draw_chart 绘制 HUD 背景 + 中间显示"暂无时序数据"提示文字，不 panic，不绘制空白坐标系
5. **详情页 stats_interval 参数传递**：4 个详情页 StatsOptions 必须传 `stats_interval=Some("daily".to_string())`（或 hourly）。后端 Domain 层当 `with_model_call_stats=true` 时自动设置 `with_time_series=true`，禁止前端忘记 stats_interval 导致 model_call_time_series 永远 None

---

## 三、涉及文件（改动清单 → 查代码直接跳）

| 文件 | 角色 | 变更内容 |
|------|------|---------|
| **抽取 HUD 背景工具** | | |
| 新建 [frontend/src/components/hud_palette.rs](../../frontend/src/components/hud_palette.rs) | HUD 视觉公共工具 | 常量 HUD_PRIMARY/HUD_PRIMARY_RGB/HUD_BASE_BG；hex_to_rgba；draw_hud_background 及 4 子函数（base/radial/grid/corners） |
| 修改 [frontend/src/components/graph_canvas.rs](../../frontend/src/components/graph_canvas.rs) | 知识图谱 Canvas 渲染器 | clear 方法从自绘改为 `hud_palette::draw_hud_background`；删除私有 hex_to_rgba，调用公共 `hud_palette::hex_to_rgba`；约 5 处 Self::hex_to_rgba 替换 |
| [frontend/src/components/mod.rs](../../frontend/src/components/mod.rs) | 组件模块声明 | 注册 `pub mod hud_palette`、`pub mod chart_scene`、`pub mod charts` |
| **图表基础设施** | | |
| 新建 [frontend/src/components/chart_scene.rs](../../frontend/src/components/chart_scene.rs) | ChartRenderer trait 定义 | `trait ChartRenderer { clear(ctx, w, h); draw(ctx, w, h, now_secs) }`（Phase 1 不强约束实现） |
| **折线图组件** | | |
| 新建 [frontend/src/components/charts/mod.rs](../../frontend/src/components/charts/mod.rs) | 子模块声明 | `pub mod line_chart` |
| 新建 [frontend/src/components/charts/line_chart.rs](../../frontend/src/components/charts/line_chart.rs) | LineChart 核心组件 | Props 定义；rAF 递归渲染循环；DPR 适配；draw_chart（坐标系/坐标轴/折线发光/数据点呼吸光晕/X轴标签）；format_timestamp 用 js_sys::Date |
| **StatsPanel 消费时序数据** | | |
| 修改 [frontend/src/components/stats.rs](../../frontend/src/components/stats.rs) | 4 统计面板组件 | 新增 render_time_series_chart 辅助；AgentStatsPanel/ProjectStatsPanel/TaskStatsPanel/ModelProviderStatsPanel 外层 div 包 StatsPanel + 在下方渲染时序图；ToolStatsPanel 不改 |
| **详情页参数检查 & 后端自动 with_time_series** | | |
| 检查+修改 4 详情页（可选） | 详情页 StatsOptions 检查 | [agent_detail](../../frontend/src/pages/hr/agent_detail.rs) / [project_detail](../../frontend/src/pages/project/project_detail.rs) / [task_detail](../../frontend/src/pages/project/task_detail.rs) / [model_provider_detail](../../frontend/src/pages/finance/model_provider_detail.rs) — 确认 stats_interval 有传，缺失则补 |
| 检查+修改 Domain 层（可选） | 自动开启时序 | 搜索 `with_time_series` 在 `src/service/domain/` 下使用，确认 with_model_call_stats=true 时自动 with_time_series=true |
| **零改动面** | | |
| 后端 common/models/TimeSeriesPoint 结构 | 100% 不变 | interval_start / call_count / token_input / token_output 字段保持 |
| 统计 API handler + Domain 取数逻辑 | 100% 不变 | 此前后端已就绪 model_call_time_series |
| 前端知识图谱除 clear 外的渲染逻辑（节点/边/力导向） | 不改动 | HUD 背景抽取是纯重构，知识图谱渲染效果视觉零差异 |

---

## 四、扩展速查表

### 4.1 新增图表类型（以环形图 donut_chart 为例）

| 步骤 | 改动点 | 参考位置 |
|------|--------|---------|
| 1 | frontend/src/components/charts/ 新建 `donut_chart.rs`，复制 LineChart 的 rAF 循环模式（Signal canvas_ref + Rc<RefCell<Closure>> + running 标志 + use_drop） | [line_chart.rs LineChart 组件](../../frontend/src/components/charts/line_chart.rs) |
| 2 | draw_chart 中第一行调用 `hud_palette::draw_hud_background(ctx, w, h)` 确保 HUD 视觉一致 | [draw_chart 第 1 步](../../frontend/src/components/charts/line_chart.rs) draw_chart 函数 |
| 3 | 在 charts/mod.rs 注册；在 stats.rs 或目标页面引用使用；如需强制实现接口形状则 `impl ChartRenderer for DonutChart` | charts/mod.rs 模板 |
| 4 | 如需动画（环形进度条渐满）用 now_secs 参数作为时间变量计算插值百分比 | draw_points 呼吸动画 now_secs 用法（line_chart.rs） |

### 4.2 LineChart 增加双 Y 轴（叠加 Token 消耗 + 调用次数）

当前 LineChart 只显示 call_count（Y 轴 max_value = data.iter().map(|p| p.call_count).max()）。如需叠加 token_input/output：
1. Props 加 `series_type: LineChartSeriesType` 枚举 `CallCount / TokenInput / TokenOutput / Call+TokenDual`
2. draw_line 支持多折线（颜色用不同透明度的橙/蓝），图例新增
3. render_time_series_chart 辅助加默认 `series_type=CallCount` 不变，ModelProviderStatsPanel 可额外渲染一张 Token 消耗趋势图

---

## 五、验收清单（2026-07-25 全部达成 ✅）

见 Plan 文档对应 Git 提交记录 / 对应执行任务。

---

## 六、执行结果摘要（2026-07-25，子代理驱动）

| 模块 | 验证结果 |
|------|---------|
| HUD 背景视觉一致性 | 知识图谱页和折线图页并排：深色基底色值一致（#0a0e1a）、径向光晕中心橙色（#fa520f 8%）、网格 40px 线、四角刻度 12px 长/8px 偏移 → 100% 一致 |
| 4 详情页时序图展示 | Agent 详情页 30 天数据 30 个数据点 → 折线流光 + 呼吸光晕动画流畅；Project / Task / ModelProvider 详情页同样渲染 |
| 空数据场景 | 新建 Agent 无历史 → LineChart 渲染 HUD 背景 + 居中"暂无时序数据"提示，不 panic |
| DPR 2x 屏清晰度 | MacBook Retina 对比（DPR=2）→ 坐标刻度/数值标签无模糊，canvas width 属性设置 600×2=1200，CSS 显示 600px |
| 前端编译 + 测试 | wasm32 build 0 error；前端测试 36 passed / 0 failed |
| Clippy + fmt | 前端 wasm32 + 后端 → 双端零警告；fmt check PASS |

### 与计划的偏离（业务零影响）
1. 原计划 Task 2 设计"LineChart 组件强实现 ChartRenderer trait"→ 实际 Phase 1 务实模式：trait 定义保留，但 LineChart 组件内部直接实现 rAF 循环不强制实现 trait。理由：单图表时 trait 抽象无收益，等环形图出现后再抽不迟（更贴合 §4.1 新增路径）

---

## 七、后续扩展路径（图表体系增强 4 步模板）

> **核心不变量**：hud_palette.rs 统一视觉源 / LineChart rAF+DPR+use_drop 模式 / StatsPanel render_time_series_chart 辅助 不动。

1. **Phase 2 环形图（Project 任务状态分布）**
   - charts/donut_chart.rs：rAF 循环模式同 LineChart，输入 `Vec<(label, count, color)>` 扇区数据，用 `ctx.arc()` + `lineTo 圆心` 每扇区路径；动画用 now_secs 插值起始角度（首帧 0→目标角度 0.6s 缓出）；stats.rs ProjectStatsPanel 替换或叠加到 Task 状态数字卡片
2. **Phase 3 AOP 队列监控时序图**
   - 后端新增统计模块定时采样 AOP event_queue 长度（1 分钟 1 条写入 DuckDB 时序表）；新增 `get_queue_history(interval)` 查询 API；前端 System Dashboard 页面（Phase 4）渲染 LineChart 叠加多条队列曲线（consumer 8 类：调度/Agent循环/任务/消息/工具执行/日志/统计/思考轮次）
3. **图表 tooltip 悬浮交互**
   - LineChart 当前无 tooltip。增加：组件内加 hover_position: Signal<Option<(f64, f64)>>；onmousemove handler 映射（Dioxus onmounted 后 addEventListener）→ 找到最近的 x 值对应数据点；绘制 HUD 风格 tooltip 框（径向渐变白底 + 橙色边 + 数据详情数值标签）
4. **Phase 4 全局 Dashboard 页**
   - 路由 `/dashboard`；后端新增全局统计聚合接口（总 Agent/总任务/7 日调用趋势/各 AOP 队列长度/飞书 WS 健康）；前端组合 6 张图表（2×3 网格）+ 6 个统计卡；使用 Grid + stats 水平 stat 卡片布局模式复用