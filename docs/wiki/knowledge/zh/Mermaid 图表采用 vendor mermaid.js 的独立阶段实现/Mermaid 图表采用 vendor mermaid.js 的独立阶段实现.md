---
kind: design
name: Mermaid 图表采用 vendor mermaid.js 的独立阶段实现
source: session
category: adr
scope:
    - 'frontend/src/components/**/Mermaid*'
source_files:
    - docs/wiki/zh/content/前端应用/组件系统/图表组件/Mermaid图表支持.md
---

# Mermaid 图表采用 vendor mermaid.js 的独立阶段实现

_来源：eb09a60 → 46c56db 提交周期内记录的编码计划——内容为规划时意图，实现可能滞后或有出入。_

**状态：** accepted

## 背景
执行计划/结果中要求支持流程图、甘特图、依赖图等 Mermaid 语法，但当前无成熟纯 Rust/WASM 方案。

## 决策驱动
- 与 Markdown 渲染解耦（可单独移除）
- 离线可用（vendor 文件）
- 主题跟随 DaisyUI data-theme
- 最小侵入（仅 DOM 插入后调用全局函数）

## 备选方案
- **vendor mermaid.js + wasm-bindgen 全局渲染函数** — 优点：功能完整、主题适配简单、可独立砍掉不影响 A-F；缺点：增加 2-3MB JS 依赖、DOM 时序复杂（需等待容器挂载）
- **纯 Rust Mermaid 解析器** _（已否决）_ — 优点：零 JS 依赖；缺点：无成熟方案，工作量巨大
- **直接以代码块展示 Mermaid 源码** _（已否决）_ — 优点：零成本；缺点：无法可视化图表

## 决策
将 mermaid.esm.min.js 放入 `frontend/public/vendor/`，在 `index.html` 暴露 `window.__renderMermaid(container)`，`MarkdownRenderer` 挂载后对 `.language-mermaid` 代码块调用该函数；新增 `MermaidDiagram` 组件消费 `GetProjectResponse.task_graph`。

## 影响
Mermaid 渲染自包含且可随时移除；窄面板下需 `overflow-x-auto` 防溢出；暗色主题需确保 mermaid 主题匹配 DaisyUI。