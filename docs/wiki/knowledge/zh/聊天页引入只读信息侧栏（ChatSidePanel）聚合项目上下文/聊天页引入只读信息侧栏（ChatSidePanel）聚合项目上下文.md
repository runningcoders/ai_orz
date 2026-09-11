---
kind: design
name: 聊天页引入只读信息侧栏（ChatSidePanel）聚合项目上下文
source: session
category: adr
scope:
    - 'frontend/src/**/ChatSidePanel*'
source_files:
    - docs/wiki/zh/content/前端应用/页面模块/消息与工作区页面/聊天侧面板/聊天侧面板.md
    - frontend/src/components/chat/chat_side_panel.rs
    - frontend/src/components/charts/line_chart.rs
    - frontend/src/components/hud_palette.rs
    - frontend/src/components/stats.rs
---

# 聊天页引入只读信息侧栏（ChatSidePanel）聚合项目上下文

_来源：eb09a60 → 46c56db 提交周期内记录的编码计划——内容为规划时意图，实现可能滞后或有出入。_

**状态：** accepted

## 背景
沟通页面缺失项目总览、进行中任务、执行计划/结果、产物等上下文；默认对话模式则完全没有侧栏信息。后端 API 已就绪（`with_progress_summary` / `with_task_graph` / `with_artifacts`）。

## 决策驱动
- 只读设计（不引入创建/编辑逻辑）
- 桌面端静态列 + 移动端抽屉复用左侧栏模式
- SSE 新消息触发防抖刷新（2s）
- localStorage 持久化面板开关状态

## 备选方案
- **右侧固定/抽屉式侧栏，按对话模式动态 Tab 组装** — 优点：复用 Agent 组件、懒加载任务详情、产物分组展示、N+1 查询规避；缺点：约 700 行新组件、需处理移动端布局差异
- **在主消息流内嵌上下文卡片** _（已否决）_ — 优点：实现简单；缺点：与消息流耦合、无法承载多层级信息（任务/产物/Agent）

## 决策
新增 `ChatSidePanel` 组件，项目对话模式提供「总览/任务/产物/Agent」Tab，默认对话模式提供「Agent/我」Tab；数据通过并行请求加载，任务详情懒加载并缓存，产物按 task_id 分组展示。

## 影响
聊天页获得完整的上下文感知能力；面板纯只读，编辑操作仍走各自详情页；移动端需处理抽屉遮罩与关闭交互；refresh_tick 机制避免 SSE 竞态导致的过期请求。

## 后续迭代（caf7bb02→HEAD：Agent 运行统计 Tab 增量）
在 Agent Tab 内新增运行统计子块：唤醒次数（Agent 调用总数 + 瞬时 QPS）+ Token 消耗卡片（模型调用次数 / 输入 Token / 输出 Token）+ 三线趋势折线图（分钟级 input/output/total tokens）。实现要点：
- 统计数据独立拉取：共享轮询主链路不带 stats 参数（零额外开销），统计仅在 Agent Tab 挂载时触发 get_agent 请求（with_stats=true + with_model_call_stats=true）
- 防抖刷新与代际丢弃：refresh_tick（SSE / 手动刷新）变化时 debounce 2s 重新加载；stats_gen 代际号防止过期请求返回覆盖最新数据
- AgentStatsPanelCompact 紧凑面板：320x180 原生 Canvas 渲染（避免 600px 图 CSS 缩放文字过小）；复用 LineChart + HudPalette 橙光光晕配色（HUD_PRIMARY + HUD_SECONDARY + HUD_TERTIARY 三色区分三线）
- 复用 refresh_tick 防抖机制：与 ToolCallsTab 的刷新模式一致

## 后续迭代（28b0e3eb→19cbaf59：stats_poll_tick 独立信号 + 静默期不再停摆）
Agent 统计 Tab 的刷新驱动从"只靠 refresh_tick"升级为"refresh_tick + stats_poll_tick"：
- **独立信号**：chat 页 3s 主轮询循环每 10 拍（30s）递增一次 `stats_poll_tick`（对齐后端 DuckDB 周期落盘节奏），单独成信号以便只命中 Agent 统计 Tab，不牵动项目总览/工具 Tab 的事件驱动语义
- **静默期不再停摆**：之前 SSE 停了（对话静默期）→ refresh_tick 不再递增 → Agent 统计不再轮询 → Token 消耗面板停在旧数据。stats_poll_tick 独立于 SSE，静默期仍按 30s 周期刷新
- **`x_axis_format` 单点修复配套**：LineChart 单点（只有今日一樽日桶数据）时 X 轴显示真实日期而非 UTC 零点对齐的伪 08:00（详见 Canvas HUD 可视化卡 §4 硬约束第 12 条）