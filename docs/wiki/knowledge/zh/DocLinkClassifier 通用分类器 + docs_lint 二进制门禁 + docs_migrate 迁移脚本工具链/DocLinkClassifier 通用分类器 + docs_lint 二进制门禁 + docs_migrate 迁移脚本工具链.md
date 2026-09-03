---
kind: wiki_knowledge_card
name: DocLinkClassifier 通用分类器 + docs_lint 二进制门禁 + docs_migrate 迁移脚本工具链
category: 工具链文档门禁
scope:
- common/src/doc_link.rs
- crates/docs_lint/**
- crates/docs_migrate/**
- scripts/tools/docs_lint.sh
- scripts/tools/docs_migrate.sh
source_files:
- common/src/doc_link.rs#L1-L320
- crates/docs_lint/src/main.rs#L1-L200
- crates/docs_lint/src/rules/illegal_path_prefix_rule.rs#L1-L120
- crates/docs_lint/src/rules/legacy_colon_line_number_rule.rs#L1-L100
- crates/docs_lint/src/rules/missing_cross_reference_rule.rs#L1-L160
- crates/docs_migrate/src/main.rs#L1-L220
- crates/docs_migrate/src/migrations/path_to_hash_fragment.rs#L1-L140
- crates/docs_migrate/src/migrations/file_protocol_cleaner.rs#L1-L100
- scripts/tools/docs_lint.sh#L1-L60
- scripts/tools/docs_migrate.sh#L1-L60
- docs/archive/design-archive/doc_link_classifier_and_quality_gates.md
- （2026-09-04 清理：superpowers 目录已归档，待 doc-maintainer 跟进）
- docs/wiki/zh/content/基础设施/持续集成与发布工作流/文档链接质量门禁.md
- docs/wiki/knowledge/zh/前端 MarkdownRenderer 接入 DocLinkClassifier JS 桥接：data-repo-href
  标注 + 点击拦截站内分发/前端 MarkdownRenderer 接入 DocLinkClassifier JS 桥接：data-repo-href 标注 +
  点击拦截站内分发.md

---

# DocLinkClassifier 通用分类器 + docs_lint 二进制门禁 + docs_migrate 迁移脚本工具链

## §1 整体方案

fc5454e3 变更落地完整的「文档链接统一治理」工具链三件套：

1. **DocLinkClassifier 通用分类器（common/src/doc_link.rs）**：前后端共用的 Rust 库，对文档中任意 markdown link `[text](target)` 的 target 字符串做**语法解析 + 分类**，输出分类结果枚举 + 归一化形式。分类结果 = ① RepoRelative(路径, Option<行范围 Lx-Ly>) 合法链接 ② WikiLong(相对 docs/wiki/...) ③ DesignDoc / PlanDoc / ArchiveDoc / RagCard ④ ExternalHttp(s) ⑤ FileProtocol（非法 `file:///` 前缀）⑥ LegacyColonLineNumber（path:15-42 旧冒号格式）⑦ MissingFragment / Ambiguous 8 大类；同时提供 `normalize_to_repo_relative(raw: &str) -> Result<String, ClassifyError>` 把各种合法/半合法形式归一化成唯一合法形式（统一相对仓库根路径 + 行号用 `#Lx-Ly` fragment）。
2. **docs_lint 二进制（crates/docs_lint/）**：独立 Rust bin crate，CI 全量文档门禁；3 个 rule：① illegal_path_prefix_rule（扫 `file:///` / 裸绝对路径 `/Users/...` / `file://` 伪协议前缀，0 容忍）② legacy_colon_line_number_rule（扫 `path:15` / `path:15-42` 冒号行号，0 容忍，应替换为 `path#L15` / `path#L15-L42`）③ missing_cross_reference_rule（扫每篇 doc/design/*.md 和 RAG 卡的 source_files[]，§互引原则要求的设计文档必须至少含 1 条 Wiki 链接 + 1 条 RAG 卡链接回链）；exit_code = rule violations 总数，CI 非零 fail。
3. **docs_migrate 迁移脚本（crates/docs_migrate/）**：独立 Rust bin crate，把存量 353 篇 Wiki + 54+ RAG 卡 + 20+ design/plan 文档中的旧格式**自动批量迁移**；两条 migration：① file_protocol_cleaner（把 `file:///Users/aman/Technology/rust/ai_orz/docs/wiki/...` 统一删前缀，保留相对路径 `docs/wiki/...`）② path_to_hash_fragment（把 `path:15-42` 重写成 `path#L15-L42`，且对 0~∞ 的范围 L 前缀补齐、上下界顺序反转 L42-L15 不合法时自动 swap 为 L15-L42）；迁移 dry-run 模式 + diff 输出，确认后 --apply 落盘。脚本入口 `scripts/tools/docs_lint.sh` / `scripts/tools/docs_migrate.sh` 封装 cargo run + 参数。

DocLinkClassifier 是 common crate（前后端共用），前端 MarkdownRenderer 通过 WASM 调用（对应 T7 细卡）——渲染 markdown 链接时分类 → 合法链接渲染成带 `data-repo-href` 属性的 `<a>` → 前端点击拦截器根据分类做站内分发（跳 Wiki 路由、跳代码文件 GitHub 新窗口、跳外部链接等）。

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键结构/宏/入口 |
|------|------|----------------|
| [common/src/doc_link.rs](common/src/doc_link.rs) | 【fc5454e3 核心 1】DocLinkClassifier 分类器（前后端共用 Rust 库）| `pub enum LinkKind` 8 变体（RepoRelative{path, line_range}/WikiLong/Design/Plan/Archive/RagCard/ExternalHttp/FileProtocol{raw}/LegacyColonLineNumber{path, line_start, line_end}/Invalid）；`pub fn classify(raw_target: &str) -> ClassifiedLink`（parse + classify）；`pub fn normalize_to_repo_relative(raw: &str) -> Result<String, ClassifyError>`（Legacy → 统一格式 / FileProtocol → 去掉前缀转相对 / RepoRelative 原样返回）；`pub fn repo_relative_from_absolute(abs_path: &Path, repo_root: &Path) -> Option<String>`（docs_migrate 用，把绝对路径转回相对）；正则定义在行范围解析 `#L(?P<start>\d+)(?:-L(?P<end>\d+))?`（L 前缀 + 可选 -L 后缀）|
| [crates/docs_lint/src/main.rs](crates/docs_lint/src/main.rs) | docs_lint 二进制入口 | 命令行：`docs_lint [--root .] [--rules all|illegal_prefix|legacy_colon|missing_ref] [--format text|json]`；exit_code = total_violations；违规输出格式：`[rule_name] file.md#L行号: 错误描述（上下文片段 + 建议修复）`；CI workflow docs.yml 里 `make ci-docs-lint` = `cargo run -p docs_lint -- --root . --rules all`。|
| [crates/docs_lint/src/rules/illegal_path_prefix_rule.rs](crates/docs_lint/src/rules/illegal_path_prefix_rule.rs) | Rule 1：非法路径前缀扫描（0 容忍）| 正则扫：`file:///` / `file://localhost/` / `^/[A-Za-z0-9/_-]+`（裸绝对路径开头 /Users / /etc 等）；命中数量 = violations |
| [crates/docs_lint/src/rules/legacy_colon_line_number_rule.rs](crates/docs_lint/src/rules/legacy_colon_line_number_rule.rs) | Rule 2：legacy 冒号行号扫描（0 容忍）| 正则 `\(([^() ]+):(\d+)(?:-(\d+))?\)`（链接尾部括号内的 path:数字 或 path:数字-数字）——注意**必须排除 http(s) 协议 URL 的端口号 `http://host:8080/path` 不误伤**（有前缀 http(s):// 时放行）|
| [crates/docs_lint/src/rules/missing_cross_reference_rule.rs](crates/docs_lint/src/rules/missing_cross_reference_rule.rs) | Rule 3：缺少四类互引（RAG 卡 source_files[] / Design doc cite 段 / Plan 关联文档段）| 规则：① 每张 RAG 卡 source_files[] 数组长度 ≥ 4 且至少命中 1 个 docs/wiki/...（长文）+ 1 个 docs/design* 或 docs/archive/plan-archive/*（设计/计划）+ 至少 1 条 src/ 代码路径；② 每篇 docs/design/*.md 的「关联文档」段至少有 1 条 wiki + 1 条 RAG 卡回链；③ 每篇 docs/archive/plan-archive/*.md 的「关联文档」段同理。|
| [crates/docs_migrate/src/main.rs](crates/docs_migrate/src/main.rs) | docs_migrate 二进制入口 | 命令行：`docs_migrate [--root .] [--dry-run] [--migration all|clean_file_protocol|fix_colon_line_numbers] [--output-diff]`；--dry-run（默认）= 只打印 diff 不落盘；去掉 --dry-run 才真正写文件 |
| [crates/docs_migrate/src/migrations/path_to_hash_fragment.rs](crates/docs_migrate/src/migrations/path_to_hash_fragment.rs) | Migration 2：path:15-42 → path#L15-L42 | AST 级解析（不 regex 裸替换，防止误伤代码块内内容）——pulldown-cmark Event::Start(Tag::Link(_, target, _)) 时只改 target，其他 Event（Code、CodeBlock 中的冒号）不碰；范围 L 前缀自动补齐、上下界逆序 swap（L42-L15 → L15-L42）、单行单数字补 L 前缀（:15 → #L15）|
| [crates/docs_migrate/src/migrations/file_protocol_cleaner.rs](crates/docs_migrate/src/migrations/file_protocol_cleaner.rs) | Migration 1：`file:///` 绝对路径 → 相对仓库根 | 逻辑：`target.starts_with("file://")` 时删前缀 → `repo_relative_from_absolute(remaining, repo_root)` 得到相对路径；如果绝对路径不在 repo 内，标记 violation 但不修改（防止外链误伤）|
| 【Level4 细卡】前端 MarkdownRenderer 接入 DocLinkClassifier JS 桥接 | WASM 侧分发点击拦截 | [前端 MarkdownRenderer 桥接卡](docs/wiki/knowledge/zh/前端%20MarkdownRenderer%20接入%20DocLinkClassifier%20JS%20桥接：data-repo-href%20标注%20+%20点击拦截站内分发/前端%20MarkdownRenderer%20接入%20DocLinkClassifier%20JS%20桥接：data-repo-href%20标注%20+%20点击拦截站内分发.md) |
| 【Wiki 长文】工具链文档链接门禁系统.md | 系统化上下文 + Troubleshooting | [工具链文档链接门禁](docs/wiki/zh/content/开发指南/工具链文档链接门禁系统.md) |
| 【② Plan】docs-link-unification-and-classifier | 7 章落地快照（真实有文件）| （2026-09-04 清理：superpowers 目录已归档，待 doc-maintainer 跟进）|

## §3 架构约定

本卡与 [前端 MarkdownRenderer 桥接卡](docs/wiki/knowledge/zh/前端%20MarkdownRenderer%20接入%20DocLinkClassifier%20JS%20桥接：data-repo-href%20标注%20+%20点击拦截站内分发/前端%20MarkdownRenderer%20接入%20DocLinkClassifier%20JS%20桥接：data-repo-href%20标注%20+%20点击拦截站内分发.md) 构成 **文档链接统一工具链** 体系的 后端分类+门禁 / 前端渲染分发 互补视角；按 AGENTS §2.1.3 Level 3 保留平行卡。

1. **DocLinkClassifier 是单一事实源（common crate，前后端共用）**：docs_lint 规则的判断逻辑、docs_migrate 迁移的 normalize 逻辑、前端 MarkdownRenderer 渲染时的分类逻辑——**三者绝不允许各自独立写 regex 分类**，必须都调 `common::doc_link::classify()` 和 `normalize_to_repo_relative()`。如果 classify 结果不对，**只能改 common/src/doc_link.rs 一处**，三处使用方自动同步（改一处，三处齐受益，保证一致性）。
2. **migrate 必须 AST 级替换（不是 regex 全局替换）**：docs_migrate 的两个 migration 对 link target 的修改，必须基于 pulldown-cmark 的 Event 流（只改 Event::Start/End(Tag::Link) 的 target 字段），**禁止 `sed s/:\\d/#L\\d/g` 式全文 regex 替换**——代码块中 `src/pkg/file.rs:15` 这种举例代码片段、yaml frontmatter `source_files: [src/foo.rs:15]` 这种非 markdown 链接的冒号行号不能误改。
3. **docs_lint 规则独立可关，CI 默认全量 3 条规则**：CI workflow docs.yml 中 `make ci-docs-lint` 是 `--rules all`；本地开发阶段可以 `scripts/tools/docs_lint.sh --rules legacy_colon` 只跑单条规则迭代修复；但 PR 合入前必须全规则 0 violation。
4. **normalize 的幂等 + 无副作用**：`normalize_to_repo_relative(x)` 调用 N 次结果 = 调用 1 次结果；normalize 已合法 RepoRelative 输入原样返回（不改变字符串，保证 migration --apply 重复执行 diff 为 0，不会产生重复 L 前缀 `#LL15` 等错误）。
5. **common crate 不依赖 tokio / async**：doc_link.rs 只做纯字符串 parse + classify，零依赖异步运行时（符合 common crate 前后端均可链接 + WASM 也能调用的约束）；regex / once_cell / pulldown-cmark（可选 feature 下启用分类后的 markdown link 检测）都是同步依赖。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止 docs_lint / docs_migrate / 前端 各自独立写 link classify regex**：3 端（lint/migrate/frontend）如果都有自己的 regex 分类 = 一定会漂移（比如前端认为 `docs/wiki/xxx` 是 WikiLong，lint 认为是 RepoRelative，互引检查 false positive）。**唯一合法入口 = common::doc_link::classify()**。代码中 grep 「file://」 或「LinkKind」在 common/src/doc_link.rs 以外出现 = 直接 fail。
2. ❌ **禁止 migrate 工具非 dry-run 默认**：`docs_migrate` CLI 不加任何参数时必须默认 `--dry-run`（且有大红色 warning stdout 提示「未应用修改，加 --apply 才落盘」）。如果直接运行就覆盖全仓 400+ 文件，一跑错无法回滚（除非 git 还没 commit）。dry-run 模式必须输出 unified diff，让用户预览再决定。
3. ✅ **强制 8 类 LinkKind 分类 + normalize 单测 50+ 矩阵**：fc5454e3 测试 `doc_link_classify_matrix` 至少 50 条：① 20 条合法 RepoRelative（有/无行号、单行/范围、中文字符路径、空格路径用 %20）② 5 条 WikiLong、Design、Plan、Archive、RagCard 各 1 ③ 2 条 ExternalHttp + 1 条 ExternalHttps ④ 5 条 FileProtocol（`file://` 绝对、`file:///` 开头、本地不存在路径、相对 `file://`）⑤ 10 条 LegacyColon（单行、单行大数字、范围、范围逆序、超范围负数、path:15 和 http://x:8080/ 不混淆=前者命中 legacy 后者是 ExternalHttp）⑥ 5 条 Invalid（空字符串、纯 fragment、纯 query）⑦ normalize 后相同的 3 组不同原始写法（LegacyColon+行号范围 = RepoRelative#Lx-Ly，`file:///repo_root/path` = path，../docs/wiki/xxx = 归一化绝对后相对）⑧ 幂等性 3 条（normalize(x)=y → normalize(y)=y 3 组全验证）。50 条全 pass。
4. ✅ **强制 docs_lint 0 violation CI 通过基线**：Step7 必须实跑 docs_lint（如果 binary 不存在就用 common crate 手工实现相同逻辑扫）——全仓 `file:///` 残留 0、裸绝对路径 `/Users/...` 残留 0、`path:15-42` legacy colon 格式残留 0。**如果有 1 个 violation 必须当场修正（本 SOP 写盘时就是 docs_migrate --apply 后状态，理应 0）**。
5. ✅ **缺失互引规则严格度：RAG 卡 source_files[] 四类齐全下限**：每张 RAG 卡 source_files[] 数组：① 至少 1 条 `src/**/*.rs`（代码锚点）② 至少 1 条 `docs/wiki/zh/content/**`（Wiki 长文）③ 至少 1 条 `docs/design/**` 或 `docs/archive/plan-archive/**`（设计/计划文档）④ 四张 Level 2/3/4 卡还要至少 1 条其他兄弟卡/主卡/总卡 `docs/wiki/knowledge/**` 的路径。四类各 ≥ 1，少于 = missing_cross_reference_rule 违规，CI fail。
6. ❌ **禁止 Migrate 改动 yaml frontmatter 内部 source_files 数组**：虽然 yaml frontmatter 的 source_files 中 `path:Ln-Lm`（旧格式 SKILL v1.0 遗留）理论上也需要迁移，但 migrate 走 pulldown-cmark 只识别 markdown 正文，不碰 yaml frontmatter——frontmatter 的修复单独走「yaml_frontmatter_fix.py」一次性 Python 脚本（不在本工具链范围，fc5454e3 已通过 Python 脚本前置修复）。如果两个迁移逻辑混在一起，yaml 列表中的 `- src/file.rs:15` 被 regex 替换成 `- src/file.rs#L15` 会让 YAML parser 报错（# 是 yaml 注释）。
7. ✅ **四类互引闭环**：本卡 source_files[] 含 Wiki 长文 1 篇 + Level3 兄弟卡 1 张 + Plan 文档 1 篇（真实文件）；Wiki 长文 cite 区回链本卡 + 前端 MarkdownRenderer 卡 + Plan。
8. ✅ **分类器版本化 + 测试覆盖 Backward Compat**：DocLinkClassifier 的 `LinkKind` 枚举加 `#[non_exhaustive]`，新增变体（未来有 A2ALink / RAG 跳转等）不破坏下游代码；`classify` 对历史已出现的 legacy 输入永远给出相同 LinkKind 变体（Backward Compat 测试 10 条：v1 老文档中的 10 个老链接格式分类结果与 v2 卡定义预期完全一致）。
