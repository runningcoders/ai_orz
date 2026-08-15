---
name: "ai-orz-doc-maintainer"
description: "AI Orz 4-docs full lifecycle doc side: writes design/plan/archive, converts superpowers blueprints to compact 7-section plans, ENFORCES AGENTS §2.1 (file-head 4-metadata + code-block judgment + path-first citations) AND 4-doc cross-citation (links wiki long-articles + RAG cards from design/plan headers). Invoke on design doc authoring, plan archive, docs cleanup, placeholder sweep, or superpowers blueprint reduction."
---

# AI Orz Doc Maintainer

Manages two of AI Orz's **4-document complete chain** (alongside ai-orz-wiki-maintainer which handles wiki long-articles + RAG cards), with **mandatory explicit cross-citation** across all 4 doc types — no doc type can exist in isolation.

The four doc types defined by project SSOT (always maintained and linked together):

| # | Type | Location | Answers | Owner Skill |
|---|------|----------|---------|-------------|
| ① Design | `docs/design/*.md` | Why (decision snapshot) | **This skill** |
| ② Plan | `docs/plan/*.md` | How + landing result | **This skill** |
| ③ Wiki long articles | `docs/wiki/zh/content/` (8 sections, ~353) | What is it (encyclopedia) | ai-orz-wiki-maintainer |
| ④ RAG knowledge cards | `docs/wiki/knowledge/zh/` (54+, **按 AGENTS §2.1.3 图谱法则决策合并/拆分，禁止裸重叠**) | Summary + index (for Agent RAG) | ai-orz-wiki-maintainer |

Specifically this skill enforces **AGENTS.md §2.1** across `design/` decision snapshots, `plan/` 7-section landing summaries, `archive/` tombstoned references, and lifecycle of temporary `docs/superpowers/*` execution artifacts (must be disposed within 7 days of feature completion). On top, it owns the **DOC-SIDE HALF of the 4-doc cross-citation**: every new design/plan must carry, in its "关联文档" header block + §3 inventory table, explicit repo-relative-path links to (iii) the corresponding wiki long-article final target path and (iv) the corresponding RAG card final target path. If the wiki side hasn't been synced yet, write **precise placeholder relative paths (final real file names)** with tag `占位：待 ai-orz-wiki-maintainer 同步后回填真实路径有效性` (never write vague "TBD" paths; the paths MUST be the final real names so wiki-maintainer can back-grep them).

## When to use (trigger conditions)

- User says any of: "写一份 design 文档", "精简 superpowers 蓝图为 plan 归档", "给文档扫雷代码块", "补全 AGENTS 规范文档头", "apply AGENTS §2.1", "placeholders 大扫除", "docs cleanup"
- Just finished a multi-step feature and the superpowers plan / checklist / tasks files still need to go through their 7-day end-of-life disposal
- A doc review reveals: implementation-snapshot code blocks, missing 🎯 file-head metadata, TBD/TODO placeholders, broken cross-path links, or **missing wiki/RAG card cross-links in design/plan headers** (v2.0 trigger)

**Do NOT use for `docs/wiki/` (encyclopedia + RAG cards)** → route to `ai-orz-wiki-maintainer`. This skill never writes wiki content or RAG cards; it only **writes placeholder target paths for them** in design/plan headers and, if this skill runs LAST (after wiki-maintainer already executed), resolves those placeholders to real existing paths.

## 3 high-level workflows (v2.0 — cross-citation hard-enforced)

### Workflow A — New design doc (`docs/design/`)

1. Place correctly: `docs/design/<topic>_design.md` (match existing naming style).
2. Apply **Template A** (5 chapters: Goals & decision table / ASCII architecture / file inventory table by layer / boundary & behavior redlines / extension patterns). Decision table (§1.X with Q/Plan/Reason) is mandatory.
3. **⭐ CRITICAL (v2.0) File-head 4-metadata "关联文档" section MUST carry 4 more entries besides AGENTS references**: (a) related plan doc OR explicit "暂无对应 plan 文档"; (b) **precise placeholder relative path (final real file name) for the matching wiki long-article** with `占位：待 ai-orz-wiki-maintainer 同步后回填真实路径有效性` tag; (c) **precise placeholder relative path (final real file name) for the matching RAG knowledge card** with same placeholder tag. Path names must be FINAL REAL TARGET NAMES (same as the md basename + 3-level dirs that wiki-maintainer will actually create).
4. Inventory table §三 rows must carry **clickable repo-relative paths with `#Ln-Lm`** anchors (e.g. `src/xxx.rs#L12-L50`). Do NOT invent paths for files not yet created. Add a last row **"落地索引（四类互引）"** listing wiki long-article + RAG card paths (same as header).
5. Run the **code-block judgment table** below. Keep contract blocks and add a path line immediately under; delete snapshot blocks and replace with a 1-line path citation.

### Workflow B — Superpowers blueprint → Plan archive (highest frequency, `docs/plan/`)

1. **Confirm the feature is 100% complete and accepted** (do not archive still-running blueprints).
2. Copy `docs/superpowers/plans/<date>-<topic>.md` to `docs/plan/<same>.md` — **never mutate in place**.
3. **Strip ALL checkboxes / Task-Step lists / writing-plans skill markers ("For agentic workers …") / placeholders (TBD / TODO / 酌情 / 参考 Task)**. Replace checklists with static result tables; keep only the 7-sections of Template B.
4. **Strip ALL implementation-snapshots**: function bodies, test code, `cargo test`, `git push`, failure dumps. Replace with path-first 1-line citations.
5. **⭐ CRITICAL (v2.0) Fill mandatory file-head 4-metadata**: 🎯定位 / 状态(枚举值 4 选一) / 查阅场景 / **关联文档**. "关联文档" MUST include (same as Workflow A): related design doc OR "暂无对应 design 文档（强烈建议补写）" + **wiki long-article placeholder/real path** + **RAG card placeholder/real path**. Plan is a landing-result snapshot → RAG card link is STRICTER than design: never write "暂缺", always write a precise placeholder even if it's the only doc referencing that card name.
6. Apply **Template B** (7 chapters + §3 inventory last row "落地索引（四类互引）" listing wiki + RAG paths + §5 acceptance table includes "四类互引占位路径已写入" checkmark row + §6 execution summary includes "四类互引覆盖率" row).
7. Finally `git rm docs/superpowers/plans/<original>`; if historically useful → move to `docs/archive/superpowers-archive/` with a tombstone header.
8. If you (doc-maintainer) are executing AFTER wiki-maintainer has already synced the matching wiki + RAG cards for this feature, run `find docs/wiki/knowledge -name "<placeholder-basename>.md"` to confirm real paths exist → replace ALL placeholders in header + §三 + §五 + §六 with REAL existing repo-relative paths (this skill owns resolution when it's the last executor).

### Workflow C — Full-repo docs sweep (code-block + placeholder + header + cross-citation pass)

1. Baseline count code blocks, placeholders, AND **missing wiki/RAG cross-links** across design/plan/archive.
2. Judge per the code-block table below; 0 snapshot residues after pass.
3. Grep placeholders: `TBD|TODO|待确认|酌情|参考 Task|如果需要` → eliminate each.
4. Add missing file-head 4-metadata to every hand-maintained md. Architecture summaries (AGENTS / wikis) are exempt from the file-head rule.
5. **⭐ CRITICAL (v2.0 cross-citation coverage pass)**:
   - Grep every design/plan md checking for wiki long-article links in `关联文档`; if any are 0 → write precise placeholder path matching the topic's natural 8-section tree location + card dir name.
   - Grep every docs/plan md checking for RAG card link; plan is landing result → 0 RAG card ref = FAIL, always write placeholder.
   - Grep for `占位：待 ai-orz-wiki-maintainer 同步后回填真实路径有效性` tags → run `find` to see if the referenced wiki/card mds now exist; if yes, replace the tagged placeholder with real paths and remove the "占位" tag.

## Code-block judgment table (AGENTS §2.1 iron law — unchanged)

| Case | Type | Keep? | Action if NO |
|------|------|-------|--------------|
| Trait signatures w/o `{ impl }` | Contract ✅ | Optional | Keep → path guide `> 当前实现：[file.rs::Name](src/path/file.rs#Lx-Ly)` under block |
| Struct field lists / Enum variant lists | Contract ✅ | Optional | Same as above |
| SQL `CREATE TABLE` schemas | Contract ✅ | Optional (recommended in design docs) | Same + link migrations dir |
| ASCII tree / flow diagrams (pure text art) | Contract ✅ | Keep strongly | No path required — this is architecture intent |
| Function bodies, match branches, loops, control flow | Snapshot ❌ | NO | Delete → 1-line path guide only |
| Full test code, `cargo test` params, `git commit` commands, bash scripts | Snapshot ❌ | NO | Delete → keep only table rows of "N passed" |
| Task/Step checklists (`- [x] Step …`) in non-superpowers docs | Snapshot ❌ | NO | Delete → replace with static tables |
| Inline 1-2 token snippets like `foo(ctx, ..)` | Contract inline ✅ | Keep | Leave inline, no block |

## Template quick reference (full spec in `docs/skills/ai-orz-doc-maintainer.md`)

Always hand off to these 3 canonical templates from AGENTS §2.1 (as updated for v2.0 4-doc cross-citation):

- **Template A** — `docs/design/*.md` design decisions (5 chapters + §1.X mandatory decision table + **关联文档 section with wiki/RAG placeholders** + §三 inventory last row "落地索引")
- **Template B** — `docs/plan/*.md` plan summaries (7 chapters + §4 quick-reference tables + §7 4-step extension path; no checkboxes/no commands; **关联文档强制 wiki/RAG** + §三 "落地索引" row + §五 acceptance 4-doc coverage row + §六 execution summary 4-doc coverage row)
- **Template C** — `docs/archive/*.md` archived references (single tombstone header; original body NEVER modified)

## Non-negotiables

- Placeholder count = 0 after every pass (placeholder ≠ cross-citation "占位" tags — tags ARE allowed and expected when wiki side not yet synced; what's banned are TBD/TODO/酌情/参考 Task/如果需要).
- Snapshot count = 0 after every pass.
- File-head 4-metadata coverage = 100% for manually maintained docs.
- **⭐ 4-doc cross-citation DOC-SIDE COVERAGE 底线**: (a) Every NEW/UPDATED design → 关联文档 has ≥1 plan (or "暂无") + **≥1 wiki placeholder/real + ≥1 RAG placeholder/real**. (b) Every NEW/UPDATED plan → 关联文档 has ≥1 design + **≥1 wiki path + ≥1 RAG path** — PLANS ARE LANDING RESULTS; writing a plan with 0 RAG card link = FAIL. (c) All placeholder paths = precise final target names, not "todo"/"待填"/"tbd" vague words.
- Superpowers blueprints ≥ 7 days old must be disposed (archive plan or delete or tombstone).
- Never mutate superpowers blueprints in place; always copy → prune → remove original.
- All path citations (under kept contract blocks, or replacements for deleted snapshots, AND ALL 4-doc cross-links) use clickable **repo-relative** paths with optional `#Ln-Lm` ranges (AGENTS §2.1.2). No `file://` pseudo-protocol anywhere.
- **⭐【路径格式硬约束】文档与 RAG 卡中所有路径引用（cite 节 / 章节来源 / source_files[] / 关联文档头部）必须使用 AGENTS §2.1.2 相对路径格式（行号 `#Lx-Ly`）**：出现 `file:///` 绝对路径 / `file://` 伪协议 / legacy 冒号行号 → 执行结果 FAIL，改完再过。

## Fallbacks

- Unsure which quadrant a file belongs to → read AGENTS §2 first; when uncertain, treat it as a plan (least-destructive).
- A superpowers blueprint covers a multi-day, still-running feature → delay disposition; the 7-day rule starts ticking from feature completion, not creation.
- If wiki-maintainer runs AFTER doc-maintainer, and the actual generated wiki long article / RAG card has a SLIGHTLY different path than your placeholder → wiki-maintainer is authoritative and will own going back into design/plan docs to replace the path. Anti-deadlock rule: whoever executes LAST owns placeholder resolution.
- Full cross-reference spec, 4-doc matrix tables, placeholder conventions, anti-deadlock rules, and detailed acceptance checklists live in `docs/skills/ai-orz-doc-maintainer.md` for local lookup.
