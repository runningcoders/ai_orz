---
kind: wiki_knowledge_card
name: 前端 MarkdownRenderer 接入 DocLinkClassifier JS 桥接：data-repo-href 标注 + 点击拦截站内分发
category: 前端文档渲染链接分发
scope:
  - "frontend/src/components/markdown.rs"
  - "frontend/src/utils/doc_link.rs"
  - "frontend/src/layouts/app_layout.rs"
  - "common/src/doc_link.rs"
source_files:
  - frontend/src/components/markdown.rs#L1-L340
  - frontend/src/utils/doc_link.rs#L1-L180
  - frontend/src/layouts/app_layout.rs#L80-L160
  - frontend/src/hooks/use_require_auth.rs#L1-L100
  - common/src/doc_link.rs#L1-L320
  - docs/superpowers/plans/2026-08-16-docs-link-unification-and-classifier.md
  - docs/wiki/zh/content/前端系统/前端应用.md
  - docs/wiki/zh/content/架构设计/前端系统/前端 Markdown 渲染与站内链接分发.md
  - docs/wiki/zh/content/核心模块/记忆系统/记忆系统架构.md
  - docs/wiki/zh/content/功能模块/用户与权限/用户管理模块 API.md
  - docs/wiki/knowledge/zh/DocLinkClassifier 通用分类器 + docs_lint 二进制门禁 + docs_migrate 迁移脚本工具链/DocLinkClassifier 通用分类器 + docs_lint 二进制门禁 + docs_migrate 迁移脚本工具链.md
---

# 前端 MarkdownRenderer 接入 DocLinkClassifier JS 桥接

## §1 整体方案

fcb08db6 变更把前端 Dioxus Markdown 渲染组件统一接入 DocLinkClassifier（common crate，通过 WASM 调用），彻底解决旧方案中 markdown 链接 `<a href="path">` 直接跳转**导致的 4 类问题**：
1. 旧问题 A：wiki 链接 `docs/wiki/zh/content/功能模块/xxx.md` 点过去是 404（不是前端路由，是静态文件路径，实际渲染页面前端路由是 `/wiki/功能模块/xxx` slug）→ 需要分发到 Dioxus Router Wiki 路由页。
2. 旧问题 B：代码链接 `src/pkg/foo.rs#L15-L42` 点过去也是 404（前端没这个路由）→ 应该新窗口 `window.open()` 跳到 `https://github.com/ai-orz/ai_orz/blob/<branch>/<path>#L15-L42`（GitHub blob 原生高亮）。
3. 旧问题 C：文档中相对路径 `../design/thinking_task_policy_engine_design.md` 链接（当前渲染的 wiki 长文在 docs/wiki/zh/content/...，相对路径指向 design 文档）→ 前端需要把相对路径先归一化到绝对仓库路径，再判断是 GitHub 新窗口还是站内 DesignViewer 页。
4. 旧问题 D：Markdown 里 XSS 风险链接（`javascript:alert(1)`、`data:text/html;base64,...`）→ 未在白名单直接拒绝渲染或 rel="noopener noreferrer nofollow" + 安全属性。

新方案链路：`MarkdownRenderer 组件` → pulldown-cmark 解析 Event 流 → 遇到 `Event::Start(Tag::Link(_, target, _))` 时 **先调 DocLinkClassifier::classify(&target)** → 拿到 LinkKind 后进行：① 渲染 `<a>` 时加入 `data-repo-href="{normalize后的标准形式}"`、`data-link-kind="{VariantName}"` 两个自定义属性（便于 e2e 和 CSS targetting）② `href` 对于站内用 `javascript:;`（空）或真实 Router Link，对于代码/design/plan 外部仓库文档先放 `#`，由**全局 click 拦截器**在 `app_layout.rs` 里统一处理点击 → 读 data-repo-href + data-link-kind → 按 8 类策略分发（Wiki 跳路由、代码跳 GitHub 新窗口、ExternalHttp 新窗口、FileProtocol/Legacy 直接禁止点击 + toast 提示 migrate）。

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键结构/宏/入口 |
|------|------|----------------|
| [frontend/src/components/markdown.rs](frontend/src/components/markdown.rs) | 【fcb08db6 核心 1】统一 MarkdownRenderer 组件（Dioxus） | `pub fn MarkdownRenderer<'a>(cx: Scope<'a>, props: MarkdownProps<'a>) -> Element<'a>`；props.src = 待渲染 markdown 字符串（或 props.src_path=文件路径，前端 fetch 后渲染）；内部 `pulldown_cmark::Parser::new_ext(&md, ENABLE_TABLES | ENABLE_FOOTNOTES | ENABLE_STRIKETHROUGH)` 解析 → Event 流手工匹配 → Tag::Link 分支调 `frontend::utils::doc_link::process_link_target(target)`（WASM 调用 classify）→ 产出 `<a class="doc-link" data-repo-href=... data-link-kind=...>` 属性 + 安全 rel/noopen |
| [frontend/src/utils/doc_link.rs](frontend/src/utils/doc_link.rs) | 【fcb08db6 核心 2】前端链接处理 utils（WASM 调 common + 分类后处理） | `fn process_link_target(raw: &str) -> ProcessedLink { kind: LinkKind, normalized_href: String, safe_to_render: bool, click_behaviour: ClickBehaviour }`；`ClickBehaviour` 枚举 = `InternalRouter(to)` / `GithubBlobNewWindow(url)` / `ExternalNewWindow(url)` / `BlockedWithToast(msg)` / `Noop`；WASM 侧 classify = **复用 common::doc_link::classify**（因为 frontend crate 依赖 common，WASM 编译后同样可链接，不需要再实现 JS 版——如果 common 有 WASM 不兼容的代码，DocLinkClassifier 本身是纯字符串处理，`wasm32-unknown-unknown` build 0 warnings）|
| [frontend/src/layouts/app_layout.rs](frontend/src/layouts/app_layout.rs) | 【fcb08db6 核心 3】全局 a 标签点击拦截（delegation）| `document.addEventListener("click", |evt| ...)` 在 AppLayout mounted 钩子注册；点击事件 target 匹配 `a.doc-link[data-repo-href]` → 阻止默认行为 preventDefault → 读 dataset.repoHref 和 dataset.linkKind → 构造 ClickBehaviour 分发：InternalRouter = `router.push(to)` / GithubBlobNewWindow = `window.open(github_url, "_blank", "noopener,noreferrer")` / BlockedWithToast = toast 弹「该链接格式已废弃，请运行 docs_migrate 修复」。**事件委托**（挂 document 上）vs 每个 a 单独 onclick——渲染 1000 条链接性能更佳，且动态渲染后插入的 DOM 新链接也天然生效（不用重新绑定）。|
| [common/src/doc_link.rs](common/src/doc_link.rs) | 分类器（单一事实源，前后端共用）| 与 T6 卡完全相同；前端 utils/doc_link.rs 里 `use common::doc_link::{classify, normalize_to_repo_relative, LinkKind}`——**没有重写一份**！前端只是把分类结果进一步转成 ClickBehaviour + Render attribute。|
| [frontend/src/components/markdown.rs: 安全过滤段](#L220-L270) | XSS 安全过滤（Tag::Link 之外）| HTML sanitize：pulldown_cmark Event::Html / Event::InlineHtml 默认直接跳过（不渲染，防止 `<script>` 注入）；img 标签 `src=` 协议白名单 https/http/data:image，其他协议（file:/ftp:/javascript:）直接丢掉 img；Mermaid 代码块识别 ```` ```mermaid```` → 渲染成 `<div class="mermaid-diagram" data-mermaid-src>`，避免 mermaid.js 处理 `<script>` 标记。 |
| 【Level3 兄弟卡】DocLinkClassifier 工具链总卡 | 分类器 + lint + migrate | [DocLinkClassifier 工具链卡](docs/wiki/knowledge/zh/DocLinkClassifier%20通用分类器%20+%20docs_lint%20二进制门禁%20+%20docs_migrate%20迁移脚本工具链/DocLinkClassifier%20通用分类器%20+%20docs_lint%20二进制门禁%20+%20docs_migrate%20迁移脚本工具链.md) |
| 【Wiki 长文】前端 Markdown 渲染与站内链接分发.md | 系统化上下文 + Troubleshooting | [前端 Markdown 渲染与站内链接分发](docs/wiki/zh/content/架构设计/前端系统/前端%20Markdown%20渲染与站内链接分发.md) |
| 【Wiki 长文】前端应用.md（旧总览）| 增量补充 cite 互引 | [前端应用](docs/wiki/zh/content/前端系统/前端应用.md) |
| 【② Plan】docs-link-unification-and-classifier | 7 章落地快照（真实）| [docs/superpowers/plans/2026-08-16-docs-link-unification-and-classifier.md](docs/superpowers/plans/2026-08-16-docs-link-unification-and-classifier.md) |

## §3 架构约定

本卡是 [DocLinkClassifier 工具链卡](docs/wiki/knowledge/zh/DocLinkClassifier%20通用分类器%20+%20docs_lint%20二进制门禁%20+%20docs_migrate%20迁移脚本工具链/DocLinkClassifier%20通用分类器%20+%20docs_lint%20二进制门禁%20+%20docs_migrate%20迁移脚本工具链.md) 描述的**文档链接统一工具链**体系中**前端渲染与点击分发**模块的细粒度独立召回卡；按 AGENTS §2.1.3 Level 4 保留。

本卡与 [DocLinkClassifier 工具链卡](docs/wiki/knowledge/zh/DocLinkClassifier%20通用分类器%20+%20docs_lint%20二进制门禁%20+%20docs_migrate%20迁移脚本工具链/DocLinkClassifier%20通用分类器%20+%20docs_lint%20二进制门禁%20+%20docs_migrate%20迁移脚本工具链.md) 构成 **工具链后端 + 前端渲染** 互补视角；按 §2.1.3 Level 3 也互相对标。

1. **前端 classify 绝不重写（直接 use common::doc_link）**：前端 utils/doc_link.rs 中 **不允许** 自己 `Regex::new(r"file://")` 分类——必须调用 common 同一个函数，保证前端「点击认为是 LegacyColon」= docs_lint「扫出违规 LegacyColon」= docs_migrate「迁移该 LegacyColon」三者一致。如果分类漂移，**唯一修法 = 改 common/src/doc_link.rs**。
2. **点击处理全局委托 vs 单元素 onclick**：只允许 AppLayout mounted 钩子挂 document click event listener（委托）；**禁止** MarkdownRenderer 组件在每个 `<a>` 上都写 onclick={move |_| ...}——Dioxus 为 1000 个 a 生成 1000 个闭包 scope 会内存暴涨、Markdown 文档 1MB 以上时渲染卡顿严重（fcb08db6 变更前旧方案卡顿的直接原因）。事件委托 = 1 个 listener，且支持异步插入 DOM（未来 SSE 推送 Markdown 增量时也生效）。
3. **data-repo-href = 单一标准化 URL 真相**：`<a>` 渲染后 `data-repo-href` 属性值必须 = `normalize_to_repo_relative(target)` 后的结果（前端/后端/CI 通用形式）；点击拦截器逻辑**不再次 parse**，直接根据 `data-link-kind` 枚举名做分发（if kind == "RepoRelative" { 新窗口 GitHub } else if kind == "WikiLong" { 路由 } ...）。classify 逻辑在渲染时做一次（CPU 成本高），点击时只是 string compare kind（O(1)）。
4. **站内跳转用 Dioxus Router Link（如果可能）**：InternalRouter 类的链接，MarkdownRenderer 直接渲染 `<Link to={to} class="doc-link" data-repo-href=...>`（Dioxus Router 组件）而非裸 `<a href="javascript:;">` + 拦截器 push——Link 组件有 active class 高亮（导航栏内的 Markdown 链接高亮激活状态正确）。**只有无法在渲染时决定跳转目标（如跨系统跳转需要后端查 slug）**时才退回 `<a>` + 拦截器。
5. **代码链接（RepoRelative 到 src/** 或 migrations/ 的路径）= 统一 GitHub new window**：前端不会把 Rust 源码渲染成代码浏览页（功能未实现前）→ 一律 window.open `https://github.com/ai-orz/ai_orz/blob/<HEAD_SHA|branch>/<path>#<Lx-Ly>`。branch/commit 来源前端通过 env var `GIT_COMMIT_SHA`（build.rs 注入）+ fallback "main"，保证新窗口打开的是**用户当前构建版本对应的源码**（不会出现 main 分支和本地代码不同步 → 行号错位）。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止前端 utils/doc_link.rs 或 markdown.rs 手工实现 classify**：如果发现前端源码里有独立的 regex 判断链接类型（`if target.starts_with("file://") {`）而不是 `use common::doc_link::classify` → 直接 fail，三者（前端/lint/migrate）分类漂移风险极高。
2. ❌ **禁止点击拦截器把 FileProtocol / LegacyColon 跳错位置**：FileProtocol 类（`data-link-kind="FileProtocol"`）的链接，点击必须 BlockedWithToast + toast 文本 = "链接格式已废弃(file:// 伪协议)，请运行 docs_migrate 修复"，**不允许帮用户静默 normalize 跳转**——否则 docs_lint CI 永远 0 violation（因为 migrate 没跑，前端自动跳转掩盖了文档里的坏链接）。LegacyColon 同理，Blocked + 明确提示修复方式。
3. ✅ **强制 ClickBehaviour 分发测试 30+ 条（Dioxus wasm-bindgen-test）**：前端单元测试 30 条矩阵：① 10 条 InternalRouter 跳转（不同 Wiki 目录层级的 md → 对应前端路由 slug 正确）② 5 条 RepoRelative 新窗口（GitHub URL 正确、#Lx-Ly 正确、noopener,noreferrer rel）③ 2 条 ExternalHttp（外部 http + https 都新窗口）④ 5 条 Blocked（FileProtocol / LegacyColon 单行 / LegacyColon 范围 / Invalid / javascript: 协议）⑤ 2 条 Design/Plan 文档（docs/design/ 或 docs/archive/plan-archive/ 新窗口或 DesignViewer，看配置）⑥ 3 条 XSS（`javascript:` 拦截为 Blocked、`data:text/html,` 拦截、`<img src=ftp://>` 丢弃）⑦ 3 条 data-repo-href 正确性（分类后的 normalized 与预期 compare）。wasm-pack test headless 全 pass。
4. ✅ **强制事件委托点击验证（Playwright e2e 触发）**：测试文档中 500 条链接的超大 Markdown → Playwright 加载后 document.querySelectorAll('a.doc-link').length == 500 → 随机点击 10 条 → 点击事件捕获统计 = 10（listener 只触发 1 次/条，而不是 N 次冒泡）→ 性能指标 Markdown 1MB 文档首次渲染 ≤ 800ms（fcb08db6 之前旧方案 onclick 绑定同量级文档 = 2.5s+，新委托方案 ~600ms）。
5. ❌ **禁止 MarkdownRenderer 默认允许 HTML 标签通过**：pulldown-cmark Parser 默认关闭 ENABLE_HTML flag；ENABLE_HTML flag 只在 `props.allow_unsafe_html = true`（明确 opt-in）时启用，且启用后必须加 sanitize（HTMLPurifier 或 ammonia crate）——不能裸通过 `<script>alert(1)</script>`。
6. ✅ **四类互引闭环**：本卡 source_files[] 含 Wiki 长文 2 篇（Markdown 渲染+前端应用旧总览）+ 兄弟卡（DocLinkClassifier 总卡）+ Plan 1 篇；Markdown 渲染 Wiki 长文 cite 区必须回链本卡 + DocLinkClassifier 总卡 + Plan。
7. ❌ **禁止点击拦截器读取 `href` 属性做逻辑**：拦截器逻辑只允许读 `data-repo-href` + `data-link-kind`；`href` 为了 SEO fallback 可能是真实 URL 也可能是 `#` / `javascript:;`——不允许从 href 二次 parse classification 破坏「渲染时 classify 一次 = 唯一真相」原则。
8. ✅ **Mermaid 代码块安全沙箱**：识别 mermaid 代码块后，必须把代码块源码 `escapeHtml()` 后写到 `data-mermaid-src` 属性，不能 eval/innerHTML；mermaid.js 初始化时从 dataset 取文本。（防止 mmd 图中通过 `graph TD\nA --> B<img src=x onerror=alert(1)>` 注入脚本到 SVG）。
