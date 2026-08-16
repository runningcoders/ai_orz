> 📦 归档标记（2026-08-16）：归档冻结。保留原因：文档链接统一与DocLinkClassifier工具链 功能已完成并通过验收，文档转为历史快照。生效方案：见源码和 wiki 长文。

🎯定位：全仓文档链接引用统一为「相对仓库根路径 + #Lx-Ly 行号 fragment」格式；抽出 common DocLinkClassifier 通用分类器（wasm 兼容）；新建 tools/ai-orz-tools workspace member 承载 docs_lint CI 门禁 + docs_migrate 一次性迁移 bin；前端 MarkdownRenderer 渲染期链接后处理 + index.html JS 点击拦截桥
状态：v1.0（2026-08-16，落地快照）
触发场景：文档中 ❌ `file:///Users/...` 硬编码本机绝对路径（换环境断链）、行号冒号 :15-42 与 fragment #L15-L42 混用、前端文档中心点击站内链接 404、CI 无链接格式门禁导致劣币驱逐良币
关联文档段：
- 对应 design 文档：暂无对应 design 文档（强烈建议补写）
- Wiki 长文真实路径：[docs/wiki/zh/content/基础设施/持续集成与发布工作流/文档链接质量门禁.md](docs/wiki/zh/content/基础设施/持续集成与发布工作流/文档链接质量门禁.md)
- RAG 卡真实路径 1：[docs/wiki/knowledge/zh/DocLinkClassifier 通用分类器 + docs_lint 二进制门禁 + docs_migrate 迁移脚本工具链/DocLinkClassifier 通用分类器 + docs_lint 二进制门禁 + docs_migrate 迁移脚本工具链.md](docs/wiki/knowledge/zh/DocLinkClassifier%20通用分类器%20+%20docs_lint%20二进制门禁%20+%20docs_migrate%20迁移脚本工具链/DocLinkClassifier%20通用分类器%20+%20docs_lint%20二进制门禁%20+%20docs_migrate%20迁移脚本工具链.md)
- RAG 卡真实路径 2：[docs/wiki/knowledge/zh/前端 MarkdownRenderer 接入 DocLinkClassifier JS 桥接：data-repo-href 标注 + 点击拦截站内分发/前端 MarkdownRenderer 接入 DocLinkClassifier JS 桥接：data-repo-href 标注 + 点击拦截站内分发.md](docs/wiki/knowledge/zh/前端%20MarkdownRenderer%20接入%20DocLinkClassifier%20JS%20桥接：data-repo-href%20标注%20+%20点击拦截站内分发/前端%20MarkdownRenderer%20接入%20DocLinkClassifier%20JS%20桥接：data-repo-href%20标注%20+%20点击拦截站内分发.md)

---

## 一、目标

| 问题 | 方式 |
|------|------|
| 文档大量 ❌ `file:///Users/...` 本机绝对路径，换环境 / 他人 clone 全部断链 | 统一为相对仓库根路径写法（如 `src/pkg/logging.rs`），永不写绝对路径 |
| 行号引用冒号格式 :15-42 与 fragment #L15-L42 混用，GitHub 仅识别 fragment | 唯一合法格式：#Lx-Ly fragment；legacy :x-y / :Lx-Ly 分类器兼容解析但 lint 禁写 |
| 缺少 Rust 通用分类器组件，前端/后端/tools/lint 各自写一套解析逻辑易分叉 | common::doc_link::DocLinkClassifier 纯 std 实现，wasm 兼容，19 个 UT 覆盖全场景 |
| 缺少 CI 链接格式门禁，劣币驱逐良币 | tools/ai-orz-tools 新 workspace member，bin/docs_lint CI 门禁，exit_code≠0 失败 |
| 存量数百级违规手动改不现实 | bin/docs_migrate 一次性迁移脚本（dry-run 默认 + --apply 写盘），三条规则严格对应 lint |
| AGENTS 规范 §2.1.2 + 4 个 Skill 文件写法滞后 | 5 处 AGENTS 重写 + 4 个 Skill 文件同步（Hard non-negotiables 加路径格式硬约束红线） |
| 前端 docs.rs 与 markdown.rs 双渲染器重复，链接无法统一后处理 | 先消重为 components/markdown.rs 单一事实源 → 渲染期 post_process_doc_links 预拼 data-repo-href → index.html JS 桥 click 拦截 → GitHub 新窗口打开 |

收敛一句话：common 分类器 + tools/lint+migrate + 规范文件更新 + 前端消重+JS 桥 + CI 接入，五部分联合实现「写时有规范、存量可迁移、CI 有门禁、前端可跳转」的文档链接格式全链路闭环。

---

## 二、架构思路

```
┌─────────────────────────────────────────────────────────────────────────┐
│ 规范层（写时约束）                                                       │
│  AGENTS.md §2.1.2 整节重写：唯一合法格式表 + 三环境行为表 + 红线 ❌      │
│  AGENTS.md §一.2 + §二表 + §2.1.3.2决策树 + §2.1.3.3四模板 同步更新      │
│  4 个 Skill 文件（.trae/skills + docs/skills）：Hard non-negotiables 追加 │
│   ⭐【路径格式硬约束】→ file:///绝对/file://伪协议/:冒号行号 → FAIL       │
└─────────────────────────────────────────────────────────────────────────┘
                              ▲
                              │ 写时参照
                              │
┌─────────────────────────────────────────────────────────────────────────┐
│ 分类器层（common，纯 std + wasm 兼容，19 个 UT）                         │
│  DocLinkClassifier::classify(href) -> DocLinkTarget 枚举                 │
│   ├─ SourceFile{relative_path, lines: Option<LineRange>}                │
│   ├─ DesignDoc / PlanDoc / WikiArticle{slug} / RagCard{slug}            │
│   ├─ OtherDoc / External / Invalid                                      │
│  split_line_suffix：先试 #fragment（主格式）再试 :冒号（legacy 兼容）    │
│  strip_legacy_prefix：剥离 file:// + file:/// + /.../ai_orz/ 绝对前缀    │
│  to_github_url(&target, blob_base) -> GitHub blob 绝对 URL              │
│  to_frontend_route_info(&target) -> V2 内部路由升级准备                  │
└─────────────────────────────────────────────────────────────────────────┘
            ▲                           ▲                           ▲
            │ 复用                       │ 复用                       │ 复用
┌───────────┴──────┐        ┌───────────┴───────┐        ┌───────────┴─────────┐
│ tools/ 工具层    │        │ 前端渲染后处理    │        │ （未来）wiki 侧     │
│  docs_lint CI    │        │  utils/doc_link   │        │ cite 节路径校验    │
│  docs_migrate    │        │  post_process_    │        │ （此计划不做）     │
│  + CI workflow   │        │  doc_links()      │        │                    │
└──────────────────┘        └───────────────────┘        └────────────────────┘
docs_lint 三条规则：                          前端四步：
  R1：file:/// 绝对路径（❌）                  ① components/markdown +
  R2：file:// 伪协议（❌）                       pages/docs.rs 消重
  R3：].(rs|sql|toml|sh):L?x-y 冒号行号（❌） ② post_process_doc_links：
lint_content 跳过：代码围栏 +                     站内 <a> 追加 class=
  ❌ 前缀红线示例行 + `行内代码`                 doc-link-intercept +
migrate_content 三条反向：                       data-repo-href=GitHub URL
  与 lint 规则严格 1:1，保证机械替换正确       ③ index.html JS click listener
                                                → preventDefault +
CI: cargo run -p ai-orz-tools --bin docs_lint    window.open(data-repo-href,
   → exit_code≠0 → CI FAIL                       '_blank', 'noopener')
                                              ④ V1 全走 GitHub 新窗口；
                                                 V2 可升级内部 SPA 路由
```

---

## 三、涉及文件清单

| 层次 | 文件路径 | 职责 |
|------|----------|------|
| common 分类器（新建） | [common/src/doc_link.rs](common/src/doc_link.rs) | LineRange 结构体 + DocLinkTarget 8 变体枚举 + DocLinkClassifier（classify/split_line_suffix/parse_lines/strip_legacy_prefix/to_github_url/to_frontend_route_info）+ FrontendRouteInfo 枚举 + 19 个 inline UT |
| common 注册 | [common/src/lib.rs](common/src/lib.rs) | pub mod doc_link |
| tools/Cargo.toml（新建） | [tools/Cargo.toml](tools/Cargo.toml) | ai-orz-tools workspace member，deps: walkdir 2 + regex 1 |
| tools/src/lib.rs（新建） | [tools/src/lib.rs](tools/src/lib.rs) | collect_target_files() 扫描目标（AGENTS.md + docs/**/*.md + .trae/skills/**/*.md）；Violation 结构；lint_content() 三规则 + 跳过围栏/❌/反引号；migrate_content() 三条反向替换；5 个 UT |
| tools/bin/docs_lint（新建） | [tools/src/bin/docs_lint.rs](tools/src/bin/docs_lint.rs) | CI 门禁：遍历文件 → lint_content → eprintln 违规 → 0→SUCCESS，>0→FAIL(ExitCode=1) |
| tools/bin/docs_migrate（新建） | [tools/src/bin/docs_migrate.rs](tools/src/bin/docs_migrate.rs) | 一次性迁移：默认 dry-run（WOULD + 不写盘）；--apply 才 fs::write；计数 + summary |
| workspace Cargo | [Cargo.toml](Cargo.toml)（workspace root） | members 数组追加 "tools" |
| AGENTS.md | [AGENTS.md](AGENTS.md) | 5 处重写：§一.2 能力表措辞 + §二路径格式铁律表 + §2.1.2 整节新内容 + §2.1.3.2 决策树措辞 + §2.1.3.3 四模板路径示例 |
| Skill 文件×4 | .trae/skills/ai-orz-wiki-maintainer/SKILL.md、.trae/skills/ai-orz-doc-maintainer/SKILL.md、docs/skills/ai-orz-wiki-maintainer.md、docs/skills/ai-orz-doc-maintainer.md | 每文件 4 处同步：Hard non-negotiables 追加⭐路径硬约束 + ❌ `file://` 示例路径改为相对写法 + wiki 长文措辞改「相对仓库根」+ 占位机制不变仅路径写法规范 |
| frontend utils（新建） | [frontend/src/utils/doc_link.rs](frontend/src/utils/doc_link.rs) | BLOB_BASE 常量 + post_process_doc_links(html, blob_base)：char_indices UTF-8 安全迭代 + 非 http <a> 标签追加 data-repo-href + class=doc-link-intercept；外链自动补 target=_blank noopener |
| frontend utils 注册 | [frontend/src/utils/mod.rs](frontend/src/utils/mod.rs) | pub mod doc_link |
| frontend 渲染器 | [frontend/src/components/markdown.rs](frontend/src/components/markdown.rs) | push_html 之后调用 post_process_doc_links；此文件成为 Markdown 渲染唯一事实源 |
| frontend docs.rs 消重 | [frontend/src/pages/system/docs.rs](frontend/src/pages/system/docs.rs) | 删除与 markdown.rs 重复的 pulldown-cmark 渲染段，统一 use components::markdown::render_markdown |
| frontend index.html | [frontend/index.html](frontend/index.html) | body 末尾追加 click listener JS 脚本（10 行，__renderMermaid 同模式）：拦截 a.doc-link-intercept → preventDefault → window.open(data-repo-href, '_blank', 'noopener') |
| CI workflow | [.github/workflows/rust.yml](.github/workflows/rust.yml) | clippy step 后新增 Docs link lint step：cargo run -p ai-orz-tools --bin docs_lint |

⭐ **落地索引（四类互引）**
- Wiki 长文：[docs/wiki/zh/content/基础设施/持续集成与发布工作流/文档链接质量门禁.md](docs/wiki/zh/content/基础设施/持续集成与发布工作流/文档链接质量门禁.md)
- RAG 卡 1（工具链）：[docs/wiki/knowledge/zh/DocLinkClassifier 通用分类器 + docs_lint 二进制门禁 + docs_migrate 迁移脚本工具链/DocLinkClassifier 通用分类器 + docs_lint 二进制门禁 + docs_migrate 迁移脚本工具链.md](docs/wiki/knowledge/zh/DocLinkClassifier%20通用分类器%20+%20docs_lint%20二进制门禁%20+%20docs_migrate%20迁移脚本工具链/DocLinkClassifier%20通用分类器%20+%20docs_lint%20二进制门禁%20+%20docs_migrate%20迁移脚本工具链.md)
- RAG 卡 2（前端桥接）：[docs/wiki/knowledge/zh/前端 MarkdownRenderer 接入 DocLinkClassifier JS 桥接：data-repo-href 标注 + 点击拦截站内分发/前端 MarkdownRenderer 接入 DocLinkClassifier JS 桥接：data-repo-href 标注 + 点击拦截站内分发.md](docs/wiki/knowledge/zh/前端%20MarkdownRenderer%20接入%20DocLinkClassifier%20JS%20桥接：data-repo-href%20标注%20+%20点击拦截站内分发/前端%20MarkdownRenderer%20接入%20DocLinkClassifier%20JS%20桥接：data-repo-href%20标注%20+%20点击拦截站内分发.md)

---

## 四、分发点速查表

| 分发场景 | 入口组件 / 命令 | 核心行为 |
|----------|----------------|----------|
| 写规范前查规则 | AGENTS.md §2.1.2 | 格式表 + 三环境行为 + 红线 ❌ |
| 写代码解析路径 | common::doc_link::DocLinkClassifier::classify() | 8 变体枚举 + 行号范围解析 + legacy 前缀剥离 |
| CI 质量门禁 | cargo run -p ai-orz-tools --bin docs_lint | 三规则扫描 → violation 计数 > 0 → ExitCode=1 FAIL |
| 存量机械迁移 | cargo run -p ai-orz-tools --bin docs_migrate -- --apply | dry-run 预览正确性后才 apply；三规则与 lint 严格 1:1 |
| 前端 wiki 链接跳转 | render_markdown() → post_process_doc_links → index.html click 拦截 | 站内链接 → GitHub blob 新窗口；外链自动补 target=_blank noopener |
| （V2 升级用）前端内部路由映射 | DocLinkClassifier::to_frontend_route_info() | 返回 FrontendRouteInfo 枚举，映射到 Route enum |
| Skill 自检查硬约束 | Skill Hard non-negotiables 末尾⭐路径硬约束段落 | ❌ `file:///` 绝对路径 / ❌ `file://` 伪协议 / ❌ legacy 冒号行号 → 执行 FAIL |

---

## 五、验收清单

| 验收项 | 结果 |
|--------|------|
| common/doc_link.rs 19 个 UT 全部 PASS（主格式 fragment、legacy 冒号、❌ `file://` 前缀剥离、文档四类变体、外链、GitHub URL 归一化、边界空串、%20 编码保留） | ✓ 已落地 |
| common --target wasm32-unknown-unknown 编译通过（纯 std 零依赖） | ✓ 已落地 |
| AGENTS.md 5 处 + 4 个 Skill 文件×4处 全部更新；❌ `file://` 示例仅剩 §2.1.2 红线示例行（lint 自动跳过） | ✓ 已落地 |
| tools/ai-orz-tools 5 个 UT（lint 双违规 + 跳过围栏/反引号 + legacy 冒号 + 不误伤 GitHub 外链 + migrate 全链路）全部 PASS | ✓ 已落地 |
| docs_migrate --apply 全量替换后 docs_lint 结果为 0 violations；残留手动清理到 0 | ✓ 已落地 |
| frontend 双渲染器消重：markdown.rs 成为唯一事实源，docs.rs 重复 pulldown-cmark 段全部删除改为 import render_markdown | ✓ 已落地 |
| post_process_doc_links UTF-8 安全迭代；中文路径 / %20 空格编码 链接后处理正确，data-repo-href GitHub URL 拼对 | ✓ 已落地 |
| index.html click listener 成功拦截 .doc-link-intercept；取消拦截器后浏览器默认退化仍可跳（原 href 保留策略正确） | ✓ 已落地 |
| CI rust.yml Docs link lint step 接入；clippy 通过后执行 docs_lint，violation≠0 → workflow FAIL | ✓ 已落地 |
| 前端 clippy --target wasm32-unknown-unknown -- -D warnings 零警告；全 workspace clippy 通过 | ✓ 已落地 |
| 四类互引占位路径已写入 | ✓ |

---

## 六、执行结果摘要

| 指标 | 值 |
|------|----|
| 新建文件数 | 9 个（common/doc_link.rs + tools/{Cargo.toml, src/lib.rs, bin/docs_lint.rs, bin/docs_migrate.rs} + frontend/utils/doc_link.rs） |
| 修改文件数 | 12 个（workspace Cargo.toml + common/lib.rs + AGENTS.md + Skill×4 + frontend/{utils/mod.rs, components/markdown.rs, pages/system/docs.rs, index.html} + CI rust.yml） |
| 新增代码行（约） | 1800 行（common 分类器约 450 行 + tools 约 360 行 + AGENTS+Skill 规范约 300 行 + 前端后处理约 220 行 + docs_migrate 大规模迁移 diff 不计入） |
| 分类器 UT 覆盖率 | 19 个 UT，覆盖：主格式 fragment 3 / legacy 冒号 2 / 前缀剥离 2 / 文档四类 5 / 外链+边界 3 / GitHub 输出 3 / %20 编码 1 |
| lint 规则数 | 3 条（R1 绝对路径 / R2 伪协议 / R3 legacy 冒号行号） |
| lint 跳过机制 | 3 层（代码围栏 + ❌ 红线示例行 + 行内反引号代码）→ 不自咬 |
| migrate 替换可靠性 | 三规则反向严格 1:1 对应 lint；先 dry-run 抽样 10 条 review 后才 --apply |
| 全 workspace 测试 | common 19 + tools 5 + 既有 1124 不回归 = 1148+ 全部 PASS |
| docs_lint 终态 | 0 violations，N files（620+）通过 |
| 四类互引覆盖率 | design 暂缺 + wiki 1/1 + RAG 2/2 = 75%（design 强烈建议补写） |

---

## 七、后续扩展路径

1. **common 层**：DocLinkClassifier::to_frontend_route_info 正式启用，V2 前端 docs 模块接入 Route 枚举（DocsDesign/DocsPlan/DocsWikiArticle/DocsRagCard），实现文档中心站内 SPA 导航，不走 GitHub 外链。
2. **domain 层**：wiki-maintainer cite 节格式校验接入 tools/ 工具链（第四类规则 R4），四类互引覆盖率从「手动抽查」升级为「CI 自动计数」，design 0 wiki/RAG / plan 0 RAG = CI FAIL。
3. **handler 层**：tools/docs_lint 暴露 HTTP 接口（/api/v1/admin/docs_lint），前端文档中心接入 lint 结果可视化面板，实时展示当前 HEAD 违规详情 + 修复建议。
4. **前端**：升级 V2 站内路由后，前端点击 doc-link-intercept 时先判断 target 是否属于 docs/* 可内部路由的路径 → 优先内部 SPA 导航；仅 SourceFile.rs / .sql 等代码路径继续走 GitHub blob 新窗口。