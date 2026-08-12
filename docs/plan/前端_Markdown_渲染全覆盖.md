## 背景与结论

调研确认：Project/Task 的 `execution_plan`/`execution_result` 只有写入通道（Update DTO），读取通道断裂（Get DTO 无字段、前端零展示）；Agent/Tool/Skill/Project/Task/Artifact 的 description、Agent soul、Skill.md、Project workflow/guidance、知识图谱 summary、聊天消息、记忆内容等大量 Markdown 性质字段当前都是纯文本插值渲染。

关键技术结论：
- 前端直接复用 `common` crate 的 DTO（`frontend/src/api/project.rs` 中 `use common::api::GetProjectResponse`），common 补字段后前端自动获得。
- 已有 `pulldown-cmark 0.13`（WASM 友好）与 `.markdown-body` 样式（input.css，主题自适应），渲染逻辑目前仅内联在 `frontend/src/pages/system/docs.rs` 的 `render_markdown()`。
- pulldown-cmark 未启用 `ENABLE_HTML`，输出默认转义原始 HTML，`dangerous_inner_html` 注入是 XSS 安全的。
- Mermaid 无成熟纯 Rust/WASM 方案，唯一全量覆盖路径是 vendor mermaid.js + wasm-bindgen interop，做成独立可砍的阶段。

---

## Phase A：Markdown 渲染基础设施

新建 `frontend/src/components/markdown.rs`：
- 抽取 docs.rs 的 `render_markdown()`（Options: TABLES | STRIKETHROUGH | TASKLISTS，可选补 FOOTNOTES）为公共函数。
- 提供 `#[component] MarkdownRenderer { content: String, compact: Option<bool> }`：内部 `render_markdown` + `div.markdown-body` + `dangerous_inner_html`。
- `compact=true` 时附加限高/小字号样式类（用于列表卡片、记忆摘要等）。
- 用 `use_memo` 按 content 缓存 HTML，避免聊天多消息场景每帧重复解析。

配套：
- `frontend/src/components/mod.rs` 注册 `pub mod markdown;`。
- `frontend/styles/input.css` 新增 compact 变体样式（如 `.markdown-compact` 限高、去首尾 margin）。
- `frontend/src/pages/system/docs.rs` 改为调用共享组件（消除重复）。

---

## Phase B：补全执行计划/结果读取链路（后端）

- `common/src/api/project.rs`：`GetProjectResponse` 新增 `execution_plan: Option<String>`、`execution_result: Option<String>`（带 `#[serde(default, skip_serializing_if = "Option::is_none")]`）。
- `common/src/api/task.rs`：`GetTaskResponse` 同上。
- `src/handlers/project/projects/response.rs`：`to_detail()` 映射 `project.po.execution_plan/execution_result`。
- `src/handlers/project/task/response.rs`：`to_detail()` 映射 `task.po.execution_plan/execution_result`。

前端复用 common DTO，无需额外改动即可读到新字段。

---

## Phase C：预置技能引导 Markdown + Mermaid 产出

更新 `src/service/domain/system/seed/skills/TEMPLATE_PROJECT_MANAGEMENT/skill.md`：
- 在 `update_project` / `update_task` 的 `execution_plan`/`execution_result` 参数说明中，明确要求用 **Markdown** 书写，支持 **Mermaid** 代码块（流程图/甘特图/依赖图）。
- 给出标准模板示例（阶段划分 + `- [ ]` 任务清单 + ```mermaid 图 + 执行结果小节：实际产出/遇到的问题/耗时/artifact 链接）。
- 同步更新集成测试中对 skill 文档内容的断言（`tests/integration/` 中 preset_skills 相关用例，确保新版结构通过）。

---

## Phase D：详情页 Markdown 渲染（字段类）

将以下页面的对应字段从纯文本插值改为 `<MarkdownRenderer>`：
- `frontend/src/pages/project/project_detail.rs`：description、**workflow（新增展示）**、**guidance（新增展示）**、**execution_plan（新增）**、**execution_result（新增）**。
- `frontend/src/pages/project/task_detail.rs`：description、**execution_plan（新增）**、**execution_result（新增）**。
- `frontend/src/pages/hr/agent_detail.rs`：description；**新增 soul 只读展示区块**（当前只读模式未展示 soul）。
- `frontend/src/pages/hr/skill_detail.rs`：description；skill.md 增加「渲染预览 / 源码」切换（源码沿用现有 CodeEditor）。
- `frontend/src/pages/finance/tool_detail.rs`：description。
- `frontend/src/pages/finance/model_provider_detail.rs`：description。
- Artifact 详情页（`frontend/src/pages/project/` 下 artifact 相关）：description。
- `frontend/src/pages/hr/knowledge_graph.rs`：节点详情面板的 node_description / summary。

列表/表格内保持纯文本截断（不改），仅详情视图用 Markdown。JSON 类字段（parameters_schema、runtime_config、MCP config、webhook_body_template）保持现状 `<pre>`/code。

---

## Phase E：聊天消息 Markdown 渲染

- `frontend/src/pages/message/chat.rs`：仅 **Text 类型**消息气泡改为 `<MarkdownRenderer compact>`（用 `use_memo` 缓存）；ToolCall、附件类型保持现状不渲染。
- 注意保持气泡宽度、代码块换行、与现有 tool-card 样式协调。

---

## Phase F：记忆内容 Markdown 渲染

- `frontend/src/pages/hr/memory_search.rs`：content / summary 展开时用 Markdown 渲染。
- `frontend/src/pages/hr/agent_memory_panel.rs`：短期记忆 content / 知识节点 summary 展开时用 Markdown 渲染（列表态可保留截断预览）。

---

## Phase G：Mermaid 支持（独立可砍阶段）

1. 将 `mermaid.esm.min.js` vendor 到 `frontend/public/vendor/`（离线可用；可参考 build.rs 现有 copy_docs 模式在 build.rs 中复制并声明 rerun-if-changed）。
2. `frontend/index.html` 增加 `<script type="module">` 引入并暴露全局渲染函数（如 `window.__renderMermaid(container)`），主题跟随 DaisyUI `data-theme`。
3. `frontend/src/components/markdown.rs`：
   - `MarkdownRenderer` 挂载后（`use_effect`）对容器内 `.language-mermaid` 代码块调用全局渲染函数。
   - 新增 `MermaidDiagram` 组件渲染裸 Mermaid 字符串，消费 `GetProjectResponse.task_graph`。
4. `frontend/src/pages/project/project_detail.rs`：新增「任务依赖图」区块渲染 task_graph（Mermaid）。

成本提示：此阶段主要工作在 JS 加载时序（DOM 插入后再 run）与暗色主题适配，自包含、可随时移除不影响 A–F。

---

## Test Plan

- 后端：`cargo test`（补 DTO/handler 映射的单元/集成测试，重点跑 preset_skills 与 project/task 相关集成用例）；`cargo clippy -D warnings`。
- 前端：`cargo build --target wasm32-unknown-unknown` + `cargo clippy --target wasm32-unknown-unknown -- -D warnings`；`cargo test`（前端现有 46 测试）。
- MarkdownRenderer 组件新增轻量单元测试（渲染标题/列表/表格/代码块、compact 变体）。
- 手工验证：项目/任务详情页 execution_plan/result 渲染、Agent soul 展示、skill.md 预览切换、聊天 Text 消息 Markdown、记忆面板渲染；Mermaid 图渲染与主题切换（若实施 Phase G）。

## Assumptions

- 仅改展示与 DTO 读取链路，不改变 Agent 写入逻辑与 DB schema（字段已存在）。
- pulldown-cmark 默认 HTML 转义视为足够 XSS 防护；不额外引入 sanitize 依赖。
- 列表/表格态保留纯文本截断，Markdown 仅用于详情/展开视图与聊天。
- Phase G 依赖 vendor mermaid.js（约 2-3MB），若不希望引入 JS 依赖可整体砍掉，mermaid 代码块将以代码原文展示。