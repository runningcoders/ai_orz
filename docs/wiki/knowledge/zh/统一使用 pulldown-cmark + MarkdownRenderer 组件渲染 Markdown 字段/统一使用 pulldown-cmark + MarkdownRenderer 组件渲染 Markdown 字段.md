---
kind: design
name: 统一使用 pulldown-cmark + MarkdownRenderer 组件渲染 Markdown 字段
source: session
category: adr
scope:
    - 'frontend/src/components/**/Markdown*'
    - 'frontend/src/components/markdown.rs'
    - 'frontend/src/pages/project/artifact_detail.rs'
source_files:
    - docs/wiki/zh/content/前端应用/组件系统/基础组件/Markdown渲染组件.md
    - frontend/src/pages/project/artifact_detail.rs#L39-L156 (2026-09-18 增量：产物详情页 preview_mode 预览/编辑 toggle + MarkdownRenderer compact:false 接入；描述字段以 compact 模式渲染)
    - frontend/src/components/markdown.rs (2026-10-12 增量 51de056b：移除 use_memo 固化 HTML——改每渲染直接计算 + 渲染期同步 html_signal 供 mermaid 效果订阅；回归守卫 markdown_renderer_content_switch_fix_anchors_present 断言组件区禁 use_memo()
---

# 统一使用 pulldown-cmark + MarkdownRenderer 组件渲染 Markdown 字段

_来源：eb09a60 → 46c56db 提交周期内记录的编码计划——内容为规划时意图，实现可能滞后或有出入。_

**状态：** accepted

## 背景
项目中大量字段（description、execution_plan/result、skill.md、聊天消息、记忆内容等）原为纯文本插值，缺乏富文本展示；需要统一的 Markdown 渲染能力以支持表格、任务清单、代码块等格式。

## 决策驱动
- 复用 common crate DTO 减少前端改动
- WASM 友好（pulldown-cmark 0.13），避免引入 JS 依赖
- XSS 安全（默认 HTML 转义，dangerous_inner_html 注入安全）
- 性能（use_memo 缓存 HTML 输出）

## 备选方案
- **pulldown-cmark 在 Rust 端渲染后注入 HTML** — 优点：无 JS 依赖、WASM 原生、XSS 安全、可缓存；缺点：Mermaid 图无法渲染（需额外处理）
- **JS 侧 markdown-it / marked 渲染** _（已否决）_ — 优点：生态丰富；缺点：增加 WASM bundle 体积、引入 JS 依赖、与现有 DaisyUI 主题集成复杂

## 决策
新建 `frontend/src/components/markdown.rs` 中的 `MarkdownRenderer` 组件，基于 pulldown-cmark 渲染并缓存 HTML，通过 `dangerous_inner_html` 注入到 `.markdown-body` div；列表/表格态保持纯文本截断，仅详情/展开视图使用 Markdown。

## 影响
所有 Markdown 字段获得一致的渲染体验；Mermaid 代码块将以源码形式展示（Phase G 可选引入 mermaid.js）；无需 sanitize 依赖，XSS 风险由 pulldown-cmark 默认转义兜底。

**2026-09-18 增量**（commit bc2b2c61）：产物详情页（`frontend/src/pages/project/artifact_detail.rs`）接入 MarkdownRenderer——`preview_mode` signal 驱动「预览/编辑」toggle 按钮组：预览态渲染 `MarkdownRenderer { content, compact: false }`（详情主视图完整排版），编辑态保留 CodeEditor 直接改源码；产物描述字段以 `compact: true` 紧凑模式渲染。至此 Markdown 字段覆盖面：项目详情 / 任务详情 / 聊天消息 / 记忆内容 / 产物详情。

**2026-10-12 修订（commit 51de056b）：撤销 use_memo 缓存，改每渲染直接计算**：上文「决策驱动」中的「use_memo 缓存 HTML 输出」在 Dioxus 0.7 位置 diff 复用语义下是错误优化——`use_memo` 闭包只捕获普通 String prop、**零 signal 订阅**，Memo 依赖集为空永不重算；组件实例被位置 diff 复用时（知识图谱节点详情面板切换节点、SSE 流式更新、会话切换），「内容」栏恒显示首个渲染节点的固化 HTML。终态实现：① HTML 在每次渲染直接 `match directory` 计算（Some 分支 `read()` 订阅目录 signal，Agent 名预载完成后历史气泡 chip 自动刷新）；② 渲染期把最新 html `set` 进 `html_signal`（PartialEq 相等短路，内容不变不产生版本推进），供 mermaid 增强 effect 订阅——`use_effect` 只订阅 signal、无法跟踪普通 prop，这是 signal 同步的唯一原因；③ 回归守卫 `markdown_renderer_content_switch_fix_anchors_present` 结构断言组件代码区禁止再出现 `use_memo(`。性能取舍：Markdown 解析为纯 CPU 轻量操作，内容不变时 Dioxus 本身短路重渲染，缓存收益不抵陈旧内容缺陷。