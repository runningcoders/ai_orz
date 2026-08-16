# 前端 Markdown 渲染全覆盖

> 📦 归档标记（2026-08-16）：归档冻结。保留原因：前端_Markdown_渲染全覆盖 功能已完成并通过验收，文档转为历史快照。生效方案：见源码和 wiki 长文。

> **文档状态**：草稿（Phase A–F 实施冻结，G 可砍）
> 查阅场景：
> - 新接前端时快速理解：哪些字段应该 Markdown 渲染 vs 哪些保留纯文本/代码
> - 新增详情页字段展示时，参考 §四 速查表接入 MarkdownRenderer 组件
> - 排查渲染问题（XSS/样式错乱/Mermaid 未渲染）按 Phase 分层定位
> 关联文档：
> - [ARCHITECTURE.md](../ARCHITECTURE.md) — 唯一权威架构总纲
> - [frontend_architecture.md](../design/frontend_architecture.md) — 前端架构详解
> - [ui_design_system.md](../design/ui_design_system.md) — 设计系统（Tailwind v4 + DaisyUI v5 主题规范）

---

## 一、目标（为什么做）

大量 Markdown 性质字段（Project/Task execution_plan/execution_result、Agent soul、Skill.md、知识图谱 summary、聊天 Text 消息、记忆内容等）当前均为纯文本插值渲染，丢失格式信息；且后端已写入 execution_plan/result 等字段但前端读取链路断裂。

| 问题维度 | 解决方式 |
|---------|---------|
| 前端 Markdown 渲染逻辑内联在 docs.rs 单页 | 抽取为公共 `MarkdownRenderer` 组件（use_memo 缓存 HTML） |
| Project/Task execution_plan/result 只读通道断裂（DTO 缺失字段） | common DTO 补 GetProjectResponse/GetTaskResponse 对应 Option 字段 |
| 详情页大量字段纯文本显示（description/workflow/guidance/soul 等） | 详情视图统一接入 `<MarkdownRenderer>`，列表/表格保留截断纯文本 |
| 聊天 Text 消息纯文本丢失格式 | 聊天页 Text 类型气泡改 MarkdownRenderer compact 变体 |
| Mermaid 流程图/甘特图展示无 WASM 方案 | Vendor mermaid.esm.min.js（独立阶段 G，可砍），wasm-bindgen interop 注入 |

**收敛后效果**：前端全链路 Markdown 渲染（A–F 阶段）统一走 MarkdownRenderer 公共组件；DTO 字段读取通道补齐；列表态保持纯文本截断、详情/聊天/记忆展开态走 Markdown；Mermaid 为可选独立阶段，不影响 A–F。

---

## 二、架构思路（怎么做的）

Phase A→G 分阶段推进，前 6 阶段为必选，G 独立可砍：

```
┌──────────────────────────────────────────────────────────────┐
│  Phase G（可选）：Mermaid 支持                                 │
│  vendor mermaid.esm.min.js → window.__renderMermaid() →       │
│  MarkdownRenderer use_effect 扫描 .language-mermaid 代码块    │
│  + 独立 MermaidDiagram 组件消费 task_graph 字段                │
└──────────────────────────────────────────────────────────────┘
            ↑ 独立可砍，砍掉后 Mermaid 块以代码原文展示
┌──────────────────────────────────────────────────────────────┐
│  Phase F：记忆内容 Markdown                                    │
│  memory_search / agent_memory_panel → content/summary 展开态  │
├──────────────────────────────────────────────────────────────┤
│  Phase E：聊天消息 Markdown                                    │
│  chat.rs → Text 类型气泡 → MarkdownRenderer compact（use_memo）│
├──────────────────────────────────────────────────────────────┤
│  Phase D：详情页字段 Markdown（Project/Task/Agent/Skill 等）   │
│  详情视图 MarkdownRenderer；列表/表格态保留纯文本截断          │
├──────────────────────────────────────────────────────────────┤
│  Phase C：预置技能引导 Markdown + Mermaid 产出规范             │
│  技能 prompt 明确 execution_plan/result 用 Markdown 书写      │
├──────────────────────────────────────────────────────────────┤
│  Phase B：后端 DTO 读取链路补全                                │
│  common/src/api/project.rs + task.rs → GetProjectResponse/    │
│  GetTaskResponse 增 execution_plan/result Option<String>      │
├──────────────────────────────────────────────────────────────┤
│  Phase A：Markdown 渲染基础设施                               │
│  components/markdown.rs → MarkdownRenderer + render_markdown()│
│  + input.css .markdown-compact 变体样式 + docs.rs 改共享      │
└──────────────────────────────────────────────────────────────┘

渲染安全：pulldown-cmark 未启用 ENABLE_HTML → 默认转义原始 HTML →
          dangerous_inner_html 注入为 XSS 安全（不额外引入 sanitize）
```

**关键边界 / 行为红线（回归必保）**：
1. **列表/表格态保留纯文本截断**，仅详情视图 / 聊天 / 记忆展开态用 Markdown；避免列表页过度渲染成本
2. `JSON 类字段`（parameters_schema、runtime_config、MCP config、webhook_body_template）保持现状 `<pre>`/code，**不做 Markdown 渲染**
3. MarkdownRenderer 必须 **use_memo 按 content 缓存 HTML**，禁止聊天多消息场景每帧重复解析 pulldown-cmark
4. pulldown-cmark Options = TABLES | STRIKETHROUGH | TASKLISTS（可选 FOOTNOTES）；**ENABLE_HTML 永远不开启**，依赖转义保证 XSS 安全
5. Phase G（Mermaid）永远**自包含可移除**：删除 G 不影响 A–F 任何一处；引入代价约 2-3MB vendor JS

---

## 三、涉及文件清单（读代码直接跳）

| 文件 | 角色 | 摘要 |
|------|------|------|
| **common DTO 层（Phase B）** | | |
| [common/src/api/project.rs](common/src/api/project.rs) | Project DTO | GetProjectResponse 增 execution_plan: Option<String> / execution_result（serde default + skip_none） |
| [common/src/api/task.rs](common/src/api/task.rs) | Task DTO | GetTaskResponse 同上两字段 |
| **Handler 响应映射（Phase B）** | | |
| [src/handlers/project/projects/response.rs](src/handlers/project/projects/response.rs) | Project 响应 | to_detail() 映射 project.po.execution_plan/execution_result |
| [src/handlers/project/task/response.rs](src/handlers/project/task/response.rs) | Task 响应 | to_detail() 映射 task.po.execution_plan/execution_result |
| **前端组件（Phase A 基础设施）** | | |
| [frontend/src/components/markdown.rs](frontend/src/components/markdown.rs) | 公共组件 | MarkdownRenderer { content, compact }；抽取 render_markdown()；use_memo 按 content 缓存；Phase G 同文件增 MermaidDiagram |
| [frontend/src/components/mod.rs](frontend/src/components/mod.rs) | 组件注册 | pub mod markdown; |
| [frontend/styles/input.css](frontend/styles/input.css) | 样式 | .markdown-body 现有主题；新增 .markdown-compact 限高/去首尾 margin 变体 |
| [frontend/src/pages/system/docs.rs](frontend/src/pages/system/docs.rs) | docs 页 | 改调用共享 MarkdownRenderer 组件（消除重复） |
| **前端详情页（Phase D）** | | |
| [frontend/src/pages/project/project_detail.rs](frontend/src/pages/project/project_detail.rs) | 项目详情 | description / workflow（新增）/ guidance（新增）/ execution_plan（新增）/ execution_result（新增）→ MarkdownRenderer |
| [frontend/src/pages/project/task_detail.rs](frontend/src/pages/project/task_detail.rs) | 任务详情 | description / execution_plan / execution_result → MarkdownRenderer |
| [frontend/src/pages/hr/agent_detail.rs](frontend/src/pages/hr/agent_detail.rs) | Agent 详情 | description；新增 soul 只读展示区块（当前未展示） |
| [frontend/src/pages/hr/skill_detail.rs](frontend/src/pages/hr/skill_detail.rs) | Skill 详情 | description；skill.md 增加「渲染预览 / 源码」切换（源码沿用 CodeEditor） |
| [frontend/src/pages/finance/tool_detail.rs](frontend/src/pages/finance/tool_detail.rs) | Tool 详情 | description |
| [frontend/src/pages/hr/knowledge_graph.rs](frontend/src/pages/hr/knowledge_graph.rs) | 知识图谱 | 节点详情面板 node_description / summary → MarkdownRenderer |
| **前端聊天（Phase E）** | | |
| [frontend/src/pages/message/chat.rs](frontend/src/pages/message/chat.rs) | 聊天页 | 仅 Text 类型消息气泡改 `<MarkdownRenderer compact>`；ToolCall / 附件类型保持现状 |
| **前端记忆（Phase F）** | | |
| [frontend/src/pages/hr/memory_search.rs](frontend/src/pages/hr/memory_search.rs) | 记忆搜索 | content / summary 展开态 MarkdownRenderer；折叠态保留截断预览 |
| [frontend/src/pages/hr/agent_memory_panel.rs](frontend/src/pages/hr/agent_memory_panel.rs) | 记忆面板 | 短期记忆 content / 知识节点 summary 展开态 MarkdownRenderer |
| **Seed 技能（Phase C）** | | |
| TEMPLATE_PROJECT_MANAGEMENT/skill.md（seed 目录） | 项目管理技能 | update_task/update_project 参数说明强制 Markdown + Mermaid 书写；标准模板示例 |
| **Phase G（Mermaid，可砍）** | | |
| frontend/public/vendor/mermaid.esm.min.js | Vendor JS | build.rs copy_docs 同款模式复制 + rerun-if-changed 声明 |
| frontend/index.html | 入口 HTML | `<script type="module">` 暴露全局 `window.__renderMermaid(container)`；主题跟随 data-theme |

---

## 四、分发速查表（新增同类功能第一站）

### 4.1 新增详情页的 Markdown 字段展示

| 改动点 | 位置 | 新增时参考 |
|--------|------|-----------|
| DTO 字段缺失（后端已写前端未读） | common/src/api/<实体>.rs → GetXxxResponse 补 Option<String> 字段 | 同 §三 Project/Task 模式 |
| Handler 响应映射未透传 | src/handlers/**/response.rs → to_detail() 加对应映射 | 同 project/projects/response.rs |
| 详情页接入 | `<MarkdownRenderer content={field} compact={false} />` 替换 `{field}` 纯文本插值 | 参考 project_detail.rs / agent_detail.rs 现有字段块 |

> 代码入口：[components/markdown.rs MarkdownRenderer](frontend/src/components/markdown.rs)

### 4.2 新增 Phase G Mermaid 消费方（项目任务图之外场景）

| 改动点 | 位置 | 参考 |
|--------|------|------|
| 独立 MermaidDiagram 组件调用 | MarkdownRenderer 同文件 → 传 Mermaid 字符串 | Phase G:3 模式，容器内 .language-mermaid 统一渲染 |
| 页面区块新增 Mermaid 渲染 + 主题切换联动 | 详情页对应区块；data-theme 变更时 useEffect 重新渲染 | 参考 project_detail.rs 「任务依赖图」区块 |

---

## 五、验收清单

见 Plan 文档对应 Git 提交记录 / 对应执行任务。

---

## 六、执行结果摘要

| 模块 | 验证结果 |
|------|---------|
| 后端：common DTO + Handler 映射 | 待执行（零业务逻辑变更，仅透传字段） |
| 前端：Phase A 组件 + 文档页消重 | 待执行；预计新增 1 个组件测试 |
| 前端：Phase D 详情页 × 6 页面 | 待手工验证（各字段渲染正确性 + 样式协调） |
| 前端：Phase E 聊天 Text 气泡 | 待手工验证（气泡宽度/代码块换行/tool-card 样式协调） |
| 前端：Phase G Mermaid（可选） | 待决定是否纳入（评估 2-3MB vendor JS 成本收益） |
| 质量门槛 | fmt + 双端 clippy -D warnings + 全量测试通过 |

### 与计划的偏离（如有）
1. 仅改展示与 DTO 读取链路，不改变 Agent 写入逻辑与 DB schema（字段已存在）
2. 不额外引入 HTML sanitize 依赖，默认 pulldown-cmark 转义视为足够防护

---

## 七、后续扩展路径（4 步模板）

> **核心不变量**：渲染统一走 MarkdownRenderer；列表态纯文本 / 详情态 Markdown 的二分原则；ENABLE_HTML 永不开启。

1. **common DTO 补字段**：[common/src/api/](common/src/api/) 对应实体 GetXxxResponse → 需要前端 Markdown 展示的字段补 Option<String>（缺省 None 向后兼容）
2. **Handler 响应映射**：[src/handlers/**/response.rs](src/handlers/) → to_detail() 中加对应字段映射（PO → Response）
3. **详情页接入 MarkdownRenderer**：[frontend/src/pages/](frontend/src/pages/) 详情页 → 详情区块用 `<MarkdownRenderer content={} compact={false} />` 替换纯文本插值；列表页不改
4. **Phase G Mermaid 扩展（后续）**：[frontend/src/components/markdown.rs](frontend/src/components/markdown.rs) 同目录增 MermaidDiagram 调用；确认 vendor 成本可接受后在项目详情 / 知识图谱详情等更多页面接入依赖图 / 甘特图渲染