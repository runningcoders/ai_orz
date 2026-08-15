# 统计图表 Phase 2：环形图 + Project 任务状态分布

> 🎯 **本文档定位**：统计图表组件扩展规划 + 落地结果快照（概览级，不含代码细节；字段级实现以代码路径为准）
>
> 状态：完成（2026-07-25 验收通过）
> 查阅场景：新增 HUD 风格图表组件或集成新图表时回看「DonutSlice 数据结构 + HUD 绘图约定」两处即可，无需通读全文
>
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 项目架构规范 §1.2 前端技术栈（HUD Canvas 可视化）
> - [frontend_architecture.md](../design/frontend_architecture.md) — 前端架构 §图表渲染章节
> - [ui_design_system.md](../design/ui_design_system.md) — UI 设计系统 HUD 风格规范

---

## 一、目标（为什么做）

Phase 1 实现了折线图（LineChart），Project 详情页仅有任务统计文字网格，用户无法一眼看清项目任务健康度，存在以下问题：

| 问题维度 | 解决方式 |
|---------|---------|
| (a) Project 概览任务状态仅文字网格，无可视化 | 新增 HUD 风格 DonutChart 环形图组件，展示 6 种任务状态占比 |
| (b) 环形图与折线图 HUD 视觉风格不统一 | 复用 Phase 1 的 `hud_palette` 背景（深色径向渐变 + 网格 + 四角装饰）+ 2.4s 呼吸光晕周期 |
| (c) Canvas 文字模糊问题（高 DPI 屏下） | 组件采用职责分离：Canvas 只画环形图，图例由 Dioxus + DaisyUI badge 渲染保证文字清晰 |
| (d) 任务状态颜色与业务语义脱节 | 新增 `task_status_color` 辅助函数，6 种状态对应 HUD 风格鲜艳色值，与 badge 语义对齐 |

**收敛后效果**：Project 详情页概览 Tab「项目概览」卡片从文字网格升级为「环形图 + 图例 + 中心总数」组合展示，用户一眼看清任务健康度；DonutChart 组件与 LineChart HUD 风格 100% 对齐，可复用至其他分布场景（Agent 类型、消息渠道等）。

---

## 二、架构思路（怎么做的）

职责分离 + HUD 风格统一：

```
通用数据结构层（业务无关）
  DonutSlice { label: String, value: u64, color: String }
    → 组件消费通用结构，不绑定任务状态语义，便于未来复用
  │
  ▼
组件渲染层（职责分离）
  ├─ Canvas 绘图部分（DonutChart::draw_chart）：
  │   1. draw_hud_background() — 复用 Phase 1 hud_palette 背景
  │   2. 多色扇区 shadow_blur 发光 + 扇区间隙（约 1.15 度）
  │   3. 外圈呼吸光晕 2.4s 周期（与 LineChart 对齐）
  │   4. 中心显示总数 + "任务总数"标签（主色橙 80% 不透明度）
  │   5. 空数据（total=0）显示「暂无数据」文字提示
  │   6. rAF 渲染循环 + 高清屏 DPR 适配（与 LineChart 模式一致）
  └─ Dioxus DOM 图例部分（组件内部渲染）：
      彩色圆点 + 标签 + 数值 + 百分比，DaisyUI 类名，
      避免 Canvas 字体模糊问题
  ▲
  │  调用方构造 Vec<DonutSlice>，颜色由调用方提供
调用方辅助层（业务语义注入）
  task_status_color(status: i32) -> &'static str
    6 种状态 → HUD 风格鲜艳颜色 hex：
    0 已取消=#ef4444 红、1 待审核=#f59e0b 橙黄、2 待处理=#3b82f6 蓝
    3 进行中=#fa520f HUD 主色橙、4 已完成=#10b981 绿、5 已归档=#6b7280 灰
  │
  ▼
Project 详情页集成层
  task_status_counts 数组按「进行中(3)→待处理(2)→待审核(1)→已完成(4)→已归档(5)→已取消(0)」顺序：
    → 让 HUD 主色橙（进行中）最显眼，绿色（已完成）紧跟其后
  filter(|s| s.value > 0) 过滤 0 值状态避免图例冗余
  无任务时显示「暂无任务」文字提示
```

**关键边界（行为红线，回归必保）**：
1. DonutChart 消费通用 `Vec<DonutSlice>`，不绑定任务状态语义，颜色由调用方提供（组件不做业务色彩决策）
2. HUD 视觉严格对齐 LineChart：深色径向渐变背景 + 网格 + 四角装饰 + shadow_blur 发光 + 2.4s 呼吸光晕周期
3. 图例 Dioxus + DaisyUI 渲染，Canvas 只画几何图形（避免 Canvas 文字在高 DPI 屏模糊）
4. 任务统计顺序：进行中(3)→待处理(2)→待审核(1)→已完成(4)→已归档(5)→已取消(0)，HUD 主色橙放在第一位最显眼
5. 空数据（total=0 或 donut_slices 为空）显示文字提示，不绘制环形

---

## 三、涉及文件清单（读代码直接跳）

按分层索引，每行带可点击路径链接：

| 文件 | 角色 | 变更内容 |
|------|------|---------|
| **图表组件层（通用 HUD 风格）** | | |
| [frontend/src/components/charts/donut_chart.rs](../../frontend/src/components/charts/donut_chart.rs) | DonutChart 组件（新建） | 组件 Props（data/width/height/center_label）；DonutSlice 结构；rAF 渲染循环；draw_chart 函数（HUD 背景、扇区发光、呼吸光晕、中心标签、空数据提示）；Dioxus DOM 图例；3 个单元测试 |
| [frontend/src/components/charts/mod.rs](../../frontend/src/components/charts/mod.rs) | charts 模块注册 | 新增 `pub mod donut_chart;`（与 line_chart 并列） |
| **调用方辅助层（业务语义颜色）** | | |
| [frontend/src/utils/status.rs](../../frontend/src/utils/status.rs) | 状态颜色辅助 | 新增 `task_status_color(status: i32) -> &'static str`，6 种状态返回 HUD 风格鲜艳 hex 颜色 |
| **页面集成层（Project 详情页）** | | |
| [frontend/src/pages/project/project_detail.rs](../../frontend/src/pages/project/project_detail.rs) | 项目详情页 | 顶部导入 DonutChart/DonutSlice/task_status_color；扩展任务状态统计逻辑（6 种状态全量 + 顺序调整 + 过滤 0 值）；「项目概览」卡片中替换原文字网格为 DonutChart 集成（无任务时显示「暂无任务」） |
| **零改动面（验证架构稳定性）** | | |
| 后端所有代码 / 前端 Phase 1 LineChart 组件 / hud_palette / 其余页面模块 | 对外契约不变 | 纯前端新增组件和页面集成，零后端改动；Phase 1 组件零修改，保持独立 |

---

## 四、分发速查表（新增 HUD 图表或集成 DonutChart 时套用）

### 4.1 HUD 图表组件绘图约定速查（新增图表组件时）

| HUD 绘图要素 | 实现方式 | 参考入口路径 |
|------------|--------|------------|
| HUD 背景（深色径向渐变 + 网格 + 四角装饰） | 直接调用 `hud_palette::draw_hud_background(ctx, width, height)` | [donut_chart.rs::draw_chart 首部](../../frontend/src/components/charts/donut_chart.rs) |
| 呼吸光晕动画周期 | 统一 2.4s：`pulse_period = 2.4`；`glow_alpha = 0.25 + sin(phase) * 0.15`；发光色用 `hud_palette::HUD_PRIMARY` | [donut_chart.rs::draw_chart §外圈呼吸光晕](../../frontend/src/components/charts/donut_chart.rs) |
| 高清屏 DPR 适配 | `canvas.set_width/height(width * dpr)`；`ctx.scale(dpr, dpr)`；rAF 循环外首次设置 | [donut_chart.rs::use_effect 内 DPR 适配](../../frontend/src/components/charts/donut_chart.rs) |
| rAF 渲染循环模式 | `Rc<RefCell<Option<Closure<dyn FnMut()>>>>` 自引用模式；running AtomicBool 标志；use_drop cleanup；与 LineChart 模式一致 | [donut_chart.rs::use_effect 内 rAF 循环](../../frontend/src/components/charts/donut_chart.rs) |
| hex→rgba 转换 | `hud_palette::hex_to_rgba(hex_color, alpha)` 返回 rgba() 字符串 | [donut_chart.rs::外圈发光 stroke 样式](../../frontend/src/components/charts/donut_chart.rs) |
| shadow_blur 发光效果 | `ctx.set_shadow_blur(8.0)` + `ctx.set_shadow_color(&slice.color)` 填充前设置，完成后 `set_shadow_blur(0.0)` 复位 | [donut_chart.rs::绘制扇区段](../../frontend/src/components/charts/donut_chart.rs) |

> 组件统一入口：[frontend/src/components/charts/mod.rs](../../frontend/src/components/charts/mod.rs)，注册新组件模块并 pub use 导出。

### 4.2 DonutChart 集成速查（在页面中使用环形图）

| 集成步骤 | 操作说明 | 参考入口路径 |
|--------|--------|------------|
| 构造 DonutSlice 数据 | 迭代业务数据，为每类生成 `DonutSlice { label, value, color }`；颜色调用 `task_status_color(status)` 或其他业务色映射 | [project_detail.rs::task_status_counts 映射](../../frontend/src/pages/project/project_detail.rs) |
| 过滤 0 值 | `.filter(|s| s.value > 0)`，避免图例出现 0% 项冗余 | [project_detail.rs::donut_slices filter](../../frontend/src/pages/project/project_detail.rs) |
| 组件 Props 设置 | `data: donut_slices.clone()`，`width/height: Some(240.0)`，`center_label: Some("任务总数".to_string())` | [project_detail.rs::DonutChart rsx](../../frontend/src/pages/project/project_detail.rs) |
| 空状态降级 | if donut_slices.is_empty() → 显示文字提示（如「暂无任务」），不渲染 DonutChart 组件 | [project_detail.rs::项目概览卡片空状态](../../frontend/src/pages/project/project_detail.rs) |

---

## 五、验收清单（2026-07-25 全部达成 ✅）

见 Plan 文档对应 Git 提交记录 / 对应执行任务。

---

## 六、执行结果摘要（2026-07-25，子代理驱动）

| 模块 | 验证结果 |
|------|---------|
| 前端 wasm32 编译 | 零 error，零 warning |
| 前端全量测试 | 38 passed（新增 3 个 donut_chart 测试，100% 通过） |
| 后端 lib 全量测试 | 746 passed / 0 failed（Phase 2 不涉及后端，零回归） |
| common 全量测试 | 50 passed / 0 failed |
| 总测试数统计 | 834 passed（Phase 1 831 → Phase 2 834，+3 donut_chart 测试） |
| 视觉验证（trunk serve） | 有任务项目：环形图发光 + 呼吸光晕 + 中心标签 + 图例百分比 正常；无任务项目：「暂无任务」提示 正常 |

### 与计划的偏离（如有）
无重大偏离，4 个 Task 按计划顺序执行完成。视觉验证阶段扇区间隙和中心标签位置经过微调，保证在 240px 画布下视觉最佳，属于细节优化而非架构偏离。

---

## 七、后续扩展路径（新增 HUD 图表 / DonutChart 复用 4 步模板）

> **核心不变量**：hud_palette 绘图约定 / DonutSlice 通用数据结构 / 2.4s 呼吸光晕周期 不动。

1. **新增 HUD 图表组件（如柱状图 BarChart、雷达图 RadarChart）**：
   - 结构参考 [donut_chart.rs](../../frontend/src/components/charts/donut_chart.rs) 模式：组件 Props + rAF 循环（Rc<RefCell<Closure>> + running AtomicBool + use_drop cleanup）+ DPR 适配
   - 背景统一调用 `hud_palette::draw_hud_background(ctx, width, height)`
   - 发光效果使用 `set_shadow_blur(8.0)` + HUD_PRIMARY 色系
   - 呼吸光晕使用 `pulse_period = 2.4` 对齐全局节奏
   - 完成后在 [charts/mod.rs](../../frontend/src/components/charts/mod.rs) 注册 `pub mod xxx_chart;`

2. **DonutChart 复用至其他分布场景（Agent 类型分布、消息渠道分布、用户角色分布等）**：
   - 步骤参考 §四 速查表 4.2：构造 DonutSlice → 过滤 0 值 → 设置 Props → 空状态降级
   - 颜色映射参考 `task_status_color`：新增 `<domain>_status_color` 辅助函数放到 [utils/status.rs](../../frontend/src/utils/status.rs)，业务色彩 hex 值对齐 UI 设计系统语义色
   - 图例百分比由组件内部计算（`slice.value / total * 100.0`），调用方无需关心

3. **DonutChart 交互增强（未来点击扇区跳转筛选）**：
   - 当前组件纯展示无交互，如需点击扇区触发动作，可在 draw_chart 中记录每个扇区的角度范围；canvas onclick 事件中判断点击坐标落入哪个扇区，通过 `EventHandler` 回调通知调用方
   - 保持向后兼容：新增 `on_slice_click: Option<EventHandler<(usize, DonutSlice)>>` Prop，为 None 时行为不变

4. **中心内容自定义（当前仅总数 + 标签）**：
   - 如需在中心显示其他内容（如完成率百分比、趋势箭头），可新增 `center_content: Option<Element>` Prop，为 None 时用原有总数 + 标签渲染逻辑
   - 图例与环形图的组合布局当前使用 `flex items-center gap-4 flex-wrap`，如需要图例放下方或右侧，可新增 `layout: Option<DonutChartLayout>`（Horizontal/Vertical）枚举控制

完成。

