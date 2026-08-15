---
kind: design
name: Mermaid 图表通过 vendor mermaid.js + 全局 JS 暴露方式集成
source: session
category: adr
---

# Mermaid 图表通过 vendor mermaid.js + 全局 JS 暴露方式集成

_来源：b156529 → eb09a60 提交周期内记录的编码计划——内容为规划时意图，实现可能滞后或有出入。_

**状态：** accepted

## 背景
技能引导文档要求支持 Mermaid 流程图/甘特图/依赖图，但当前无成熟纯 Rust/WASM 方案可用。

## 决策驱动
- 离线可用（vendor 静态资源）
- 与 DaisyUI data-theme 暗色主题联动
- 独立可砍阶段，不阻塞 A–F Markdown 渲染

## 备选方案
- **vendor mermaid.esm.min.js 并通过 window.__renderMermaid 暴露全局函数** — 优点：零 Rust 绑定成本、DOM 插入后调用、主题跟随 data-theme
- **寻找纯 Rust/WASM Mermaid 实现** _（已否决）_；缺点：生态不成熟，覆盖度不足
- **完全放弃 Mermaid，仅以代码块原文展示** _（已否决）_；缺点：失去可视化能力

## 决策
将 mermaid.esm.min.js 放入 frontend/public/vendor/，在 index.html 中以 script type=module 引入并暴露 window.__renderMermaid(container)；MarkdownRenderer 挂载后用 use_effect 对容器内 .language-mermaid 代码块触发渲染，并新增 MermaidDiagram 组件消费 GetProjectResponse.task_graph。

## 影响
Mermaid 渲染自包含、可随时移除而不影响 Markdown 基础能力；需关注 ~2-3MB 包体积与 DOM 加载时序；暗色主题下需确保 mermaid 主题同步。