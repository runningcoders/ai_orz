---
kind: knowledge_card
name: 模型提供商信息气泡：浮层定位工具复用 + ModelProviderBubble 懒加载卡片
category: 前端可视化
scope:
- frontend/src/components/model_provider_bubble.rs
- frontend/src/components/avatar_bubble.rs
- frontend/src/pages/hr/agents.rs
- frontend/src/pages/hr/agent_detail.rs
source_files:
- frontend/src/components/model_provider_bubble.rs#L25-L46
- frontend/src/components/model_provider_bubble.rs#L47-L115
- frontend/src/components/model_provider_bubble.rs#L118-L193
- frontend/src/components/avatar_bubble.rs#L104-L230
- frontend/src/pages/hr/agents.rs#L608-L616
- frontend/src/pages/hr/agent_detail.rs#L1174-L1183
- frontend/src/api/finance.rs#L29-L60
- docs/wiki/zh/content/前端应用/页面模块/HR 管理页面/Agent 管理功能.md
- docs/wiki/zh/content/前端应用/组件系统/组件系统.md
- docs/wiki/knowledge/zh/UI Design System 组件设计系统：6 层组件分层 + Hooks 3 个 + Store 2 个 + DaisyUI 主题 + 交互组件复用约束/UI Design System 组件设计系统：6 层组件分层 + Hooks 3 个 + Store 2 个 + DaisyUI 主题 + 交互组件复用约束.md
---

# 模型提供商信息气泡：浮层定位工具复用 + ModelProviderBubble 懒加载卡片

## §1 概述与定位

Agent 列表页（`agents.rs`）与详情页（`agent_detail.rs`）中的「模型提供商」标识，从纯文本/Router Link 升级为**点击弹出信息气泡**（Ref daed0b60）：触发层是等宽字体文本 chip，聚焦后弹出浮层卡片，展示提供商基础信息（名称/类型/能力徽章、模型名、描述）与**最近 24 小时精简统计**（调用量/平均 QPS/输入输出 Token），卡片底部提供「在详情页打开 →」跳转 `模型提供商详情页`。

组件实现为 `frontend/src/components/model_provider_bubble.rs` 的 `ModelProviderBubble`，**不复制任何浮层定位逻辑**——完整复用头像气泡 `avatar_bubble.rs` 中以 `pub(crate)` 开放的定位工具（`BubbleAnchor` / `resolve_anchor` / `viewport_size` / `focused_element` / `scroll_ancestor_rect` / `BubbleAlign`）。数据零后端改动：复用 `get_model_provider` API 的 `with_model_call_stats=true` 一次拉齐详情 + 统计。

## §2 关键文件表

| 角色 | 路径 | 关键锚点 |
|------|------|----------|
| 信息气泡组件（触发层 + 浮层 + 卡片内容） | frontend/src/components/model_provider_bubble.rs | L25-L46 统计窗口常量 + `provider_stats_request`（最近 24h + hourly）；L47-L115 `ModelProviderBubble` 组件（dropdown + onfocus + position:fixed 锚点 + 懒加载三态缓存）；L118-L193 卡片内容（`provider_bubble_card_content` / `provider_stats_grid`） |
| 浮层定位工具（pub(crate) 开放复用） | frontend/src/components/avatar_bubble.rs | L104 `BubbleAlign::dropdown_class`；L126-L163 `BubbleAnchor` 结构体 + `style()` 定位样式；L165 `viewport_size`；L173 `focused_element`；L188 `scroll_ancestor_rect`；L208 `resolve_anchor`（触发盒实测 DomRect 泛化签名） |
| 列表页接入点 | frontend/src/pages/hr/agents.rs | L608-L616 `akind == "local"` 分支渲染 `ModelProviderBubble`（外部 Agent 分支仍为纯文本） |
| 详情页接入点 | frontend/src/pages/hr/agent_detail.rs | L1174-L1183 概览区「模型提供商」字段渲染 `ModelProviderBubble` |
| 数据源 API（零后端改动） | frontend/src/api/finance.rs | L29 `get_model_provider`：`with_model_call_stats=true` + `stats_start_time/stats_end_time/stats_interval` 参数 |
| Wiki 长文（Agent 管理功能） | docs/wiki/zh/content/前端应用/页面模块/HR 管理页面/Agent 管理功能.md | 接入点上下文与交互约定 |
| Wiki 长文（组件系统） | docs/wiki/zh/content/前端应用/组件系统/组件系统.md | 组件归属与增量记录 |
| 总卡（组件设计系统） | docs/wiki/knowledge/zh/UI Design System 组件设计系统：6 层组件分层 + Hooks 3 个 + Store 2 个 + DaisyUI 主题 + 交互组件复用约束/UI Design System 组件设计系统：6 层组件分层 + Hooks 3 个 + Store 2 个 + DaisyUI 主题 + 交互组件复用约束.md | Level 4 总卡：6 层组件分层与全站复用红线 |

## §3 架构与约定

本卡为总卡《UI Design System 组件设计系统：6 层组件分层 + Hooks 3 个 + Store 2 个 + DaisyUI 主题 + 交互组件复用约束》之下的 Level 4 细卡：总卡负责全站 6 层组件分层与复用红线，本卡聚焦「浮层信息气泡」这一具体交互范式及其在 Agent 页的模型提供商接入实现。

```
触发层（文本 chip, tabindex=0, role=button）
  │ onfocus → 读 getBoundingClientRect 实测触发盒
  ├─ resolve_anchor(box_rect, vw, vh, align, scroll_ancestor bounds)
  │    → BubbleAnchor（向上优先 / 上方不够翻向下 / 再不够限高滚动）
  └─ 首次聚焦触发懒加载 get_model_provider(with_model_call_stats)
       三态缓存 Signal<Option<Result<Response>>>：None=未加载 / Some(Err)=失败可重试 / Some(Ok)=命中缓存
浮层（dropdown-content orz-popover w-64 p-3, position:fixed + BubbleAnchor.style()）
  ├─ provider_bubble_card_content：名称 + ptype/capability 徽章 + model_name + desc + 统计格
  └─ Link「在详情页打开 →」→ /finance/model-providers/{id}
```

交互与数据约定：
- **触发即拉取，命中即缓存**：`focused` Signal 首次 `true` 时 spawn 请求；三态缓存避免重复请求，失败态点击重试。
- **统计口径**：`STATS_WINDOW_MS = 24h`，`stats_interval = "hourly"`（小写，来自后端 `TimeRange::suggested_interval()` 契约）；统计格由 `provider_stats_grid` 渲染 2 列 grid（调用/平均 QPS/输入/输出 Token），大数走 `utils::number::format_compact_count`（k/M 进位）。
- **徽章风格**：`ptype` / 能力标签统一 `badge orz-tag badge-sm`（ProviderType 实现Display，`to_string()` 输出；capability 用 `is_embedding()` 判断）。

## §4 硬约束与红线

1. **浮层触发必须用 onfocus，禁止 onclick**：dropdown 展开依赖 `tabindex=0` 聚焦态，`onclick` 无法维持 DaisyUI `dropdown-open` 类与焦点链，会导致浮层定位与关闭行为错乱（与头像气泡同源约束）。
2. **浮层定位必须复用 avatar_bubble pub(crate) 工具，禁止复制实现**：`resolve_anchor` 已泛化为触发盒实测矩形签名（`&web_sys::DomRect`），正方形头像与长条文本 chip 统一处理；新浮层组件**禁止**自带一套锚点/视口/滚动祖先计算。
3. **resolve_anchor 参数收敛 ≤7**：签名保持 5 参（box_rect/vw/vh/align/bounds），新增能力优先扩展 `BubbleAlign` 或包矩形结构体，**禁止**回退到散装坐标参数（clippy `too_many_arguments` 零容忍）。
4. **stats_interval 必须小写**：`"hourly"` / `"daily"` / `"minutely"`，与后端枚举解析严格对齐，禁止 `"Hourly"` 等变体。
5. **format_compact_count 走完整路径**：`use crate::utils::number::format_compact_count`（该工具未 glob 再导出），禁止假设可从 `utils` 根直接导入。
6. **浮层必须 position:fixed + 实测锚点**：禁止 `absolute` 依赖滚动容器定位；锚点样式只能来自 `BubbleAnchor::style()`，保证滚动/缩放下不漂移。
7. **非 local Agent 分支不渲染气泡**：外部 Agent 无 model_provider_id，列表/详情页分支判定（`akind == "local"` / `a.kind == "local"`）不可移除。
