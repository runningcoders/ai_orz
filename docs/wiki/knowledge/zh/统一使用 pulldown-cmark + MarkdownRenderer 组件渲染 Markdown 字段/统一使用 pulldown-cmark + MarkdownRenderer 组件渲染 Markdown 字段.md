---
kind: design
name: 统一使用 pulldown-cmark + MarkdownRenderer 组件渲染 Markdown 字段
source: session
category: adr
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