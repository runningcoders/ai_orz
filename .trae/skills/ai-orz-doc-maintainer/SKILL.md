---
name: "ai-orz-doc-maintainer"
description: "AI Orz docs historical-snapshot side (one-way reference model v2.1): full lifecycle of design/plan/archive — authoring design decision snapshots, converting superpowers blueprints to compact 7-section plans, and streamlining+archiving completed docs (Workflow D). ENFORCES AGENTS §2.1 (file-head 4-metadata + code-block judgment + path-first citations). Design/plan link only to each other + upper authority docs, NEVER to wiki/RAG. Invoke on design doc authoring, plan archive, docs cleanup, or superpowers blueprint reduction."
---

# AI Orz Doc Maintainer

Manages the **historical-snapshot half** of AI Orz's 4-document system (alongside ai-orz-wiki-maintainer which owns the living docs: wiki long-articles + RAG cards). References are **one-way (v2.1)**: ③ wiki / ④ RAG cards cite ① design / ② plan + source code; design/plan **never** link back to wiki/RAG — frozen docs cannot track wiki reorganizations, reverse links rot permanently.

The four doc types defined by project SSOT:

| # | Type | Location | Answers | Owner Skill |
|---|------|----------|---------|-------------|
| ① Design | `docs/design/*.md` | Why (decision snapshot) | **This skill** |
| ② Plan | `docs/plan/*.md` | How + landing result | **This skill** |
| ③ Wiki long articles | `docs/wiki/zh/content/` (8 sections, ~353) | What is it (encyclopedia) | ai-orz-wiki-maintainer |
| ④ RAG knowledge cards | `docs/wiki/knowledge/zh/` (54+, **按 AGENTS §2.1.3 图谱法则决策合并/拆分，禁止裸重叠**) | Summary + index (for Agent RAG) | ai-orz-wiki-maintainer |

Specifically this skill enforces **AGENTS.md §2.1** across `design/` decision snapshots, `plan/` 7-section landing summaries, `archive/` tombstoned references, and lifecycle of temporary `docs/superpowers/*` execution artifacts (must be disposed within 7 days of feature completion). Design/plan "关联文档" sections may link ONLY to: each other (frozen↔frozen archaeology chain) + upper authority docs (AGENTS.md, LAYERED_ARCHITECTURE_PRACTICE.md, etc.). **Any link from a design/plan doc into `docs/wiki/` = FAIL** (v2.1 one-way model).

## When to use (trigger conditions)

- User says any of: "写一份 design 文档", "精简 superpowers 蓝图为 plan 归档", "给文档扫雷代码块", "补全 AGENTS 规范文档头", "apply AGENTS §2.1", "docs cleanup", "归档已完成的 design/plan"
- Just finished a multi-step feature and the superpowers plan / checklist / tasks files still need to go through their 7-day end-of-life disposal
- A feature is complete and its design/plan docs are now pure history → streamline + archive (Workflow D)

**Do NOT use for `docs/wiki/` (encyclopedia + RAG cards)** → route to `ai-orz-wiki-maintainer`. This skill never writes wiki content or RAG cards, and never writes links pointing to them.

## 4 high-level workflows (v2.1 — one-way reference model)

### Workflow A — New design doc (`docs/design/`)

1. Place correctly: `docs/design/<topic>_design.md` (match existing naming style).
2. Apply **Template A** (5 chapters: Goals & decision table / ASCII architecture / file inventory table by layer / boundary & behavior redlines / extension patterns). Decision table (§1.X with Q/Plan/Reason) is mandatory.
3. File-head 4-metadata "关联文档" section carries: (a) related plan doc OR explicit "暂无对应 plan 文档"; (b) upper authority docs when relevant. **No wiki/RAG links — not even placeholders** (v2.1: the wiki side owns all citation into snapshots).
4. Inventory table §三 rows must carry **clickable repo-relative paths with `#Ln-Lm`** anchors (e.g. `src/xxx.rs#L12-L50`). Do NOT invent paths for files not yet created.
5. Run the **code-block judgment table** below. Keep contract blocks and add a path line immediately under; delete snapshot blocks and replace with a 1-line path citation.

### Workflow B — Superpowers blueprint → Plan archive (highest frequency, `docs/plan/`)

1. **Confirm the feature is 100% complete and accepted** (do not archive still-running blueprints).
2. Copy `docs/superpowers/plans/<date>-<topic>.md` to `docs/plan/<中文主题名>.md` — **strip the date prefix** (naming per AGENTS §文件落位与命名约定 table); **never mutate in place**.
3. **Strip ALL checkboxes / Task-Step lists / writing-plans skill markers ("For agentic workers …") / placeholders (TBD / TODO / 酌情 / 参考 Task)**. Replace checklists with static result tables; keep only the 7 sections of Template B.
4. **Strip ALL implementation-snapshots**: function bodies, test code, `cargo test`, `git push`, failure dumps. Replace with path-first 1-line citations.
5. Fill mandatory file-head 4-metadata: 🎯定位 / 状态(枚举值) / 查阅场景 / **关联文档** (= related design doc OR "暂无对应 design 文档（强烈建议补写）" + upper authority docs; **no wiki/RAG links**).
6. Apply **Template B** (7 chapters: 目标 / 架构思路 / 涉及文件清单 / 分发点速查表 / 验收清单 / 执行结果摘要 / 后续扩展路径; no checkboxes, no commands, no code snapshots).
7. Finally `git rm docs/superpowers/plans/<original>`; if historically useful → move to `docs/archive/superpowers-archive/` with a tombstone header.

### Workflow C — Full-repo docs sweep (design/plan/archive only)

1. Baseline count code blocks + placeholders across design/plan/archive.
2. Judge per the code-block table below; 0 snapshot residues after pass.
3. Grep placeholders: `TBD|TODO|待确认|酌情|参考 Task|如果需要` → eliminate each.
4. Add missing file-head 4-metadata to every hand-maintained md in scope.
5. **Reverse-link sweep (v2.1)**: grep design/plan for links pointing INTO `docs/wiki/` → delete them (one-way model violation). Reading-chain discovery is owned entirely by wiki `<cite>` / RAG `source_files[]`.

### Workflow D — Streamline + archive completed design/plan (v2.1 new)

| Step | Action | Checkpoint |
|-----|------|-----------|
| D1 | **Classify living-spec vs historical-decision**: design docs continuously referenced by AGENTS.md body / doc-index tables (sqlx_guide, logging_design, api_protocol_convention, …) → **stay in `docs/design/`** (archiving breaks AGENTS links); all other landed historical designs + ALL completed plans → archiving flow | Retention list shown to user first |
| D2 | **Streamline**: apply code-block judgment table (0 snapshot blocks), delete checkboxes, clear placeholders, collapse plans to Template B 7-section skeleton | 0 snapshot blocks / 0 checkboxes / 0 placeholders |
| D3 | **Move**: `git mv docs/design/xxx.md docs/archive/design-archive/xxx.md` (plans → `docs/archive/plan-archive/`), apply Template C tombstone header | git mv preserves history |
| D4 | **Archived copies carry no cross-quadrant refs**: delete any wiki/RAG links from the header (design↔plan mutual links may stay — both frozen, archaeology chain) | Archived file self-contained |
| D5 | **Notify wiki-maintainer to redirect**: report old-path→new-path mapping; ai-orz-wiki-maintainer batch-rewrites wiki `<cite>` + RAG `source_files[]` old paths to new | 0 broken links on ③④ side |

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

- **Template A** — `docs/design/*.md` design decisions (5 chapters + §1.X mandatory decision table; 关联文档 links design↔plan + authority docs only)
- **Template B** — `docs/plan/*.md` plan summaries (7 chapters + §4 quick-reference tables + §7 4-step extension path; no checkboxes/no commands/no code snapshots)
- **Template C** — `docs/archive/*.md` archived references (single tombstone header `> 📦 归档标记（YYYY-MM-DD）：被 [新文档/SHA] 取代…`; original body NEVER modified)

## Non-negotiables

- Placeholder count = 0 after every pass (TBD/TODO/酌情/参考 Task/如果需要 are ALL banned; v2.1 removed the "占位 cross-citation tag" exception — cross-citation placeholders no longer exist).
- Snapshot count = 0 after every pass.
- File-head 4-metadata coverage = 100% for manually maintained docs (architecture summaries like AGENTS/wikis are exempt).
- **⭐ One-way reference (v2.1)**: design/plan 关联文档 may link ②↔① + upper authority docs ONLY. Any NEW link from design/plan into `docs/wiki/` = FAIL. Legacy reverse links are removed by Workflow C step 5 / Workflow D step 4.
- Superpowers blueprints ≥ 7 days old must be disposed (archive plan or delete or tombstone).
- Never mutate superpowers blueprints in place; always copy → prune → remove original.
- All path citations (under kept contract blocks, or replacements for deleted snapshots) use clickable **repo-relative** paths with optional `#Ln-Lm` ranges (AGENTS §2.1.2). No `file://` pseudo-protocol, no absolute local paths, no legacy colon line numbers (`path:15-42`) anywhere.

## Fallbacks

- Unsure which quadrant a file belongs to → read AGENTS §2 first; when uncertain, treat it as a plan (least-destructive).
- A superpowers blueprint covers a multi-day, still-running feature → delay disposition; the 7-day rule starts ticking from feature completion, not creation.
- If wiki/RAG content for the feature doesn't exist yet → do nothing on the doc side; wiki-maintainer will cite your design/plan paths when it syncs (one-way: you never cite it, it cites you).
- Full spec, one-way reference matrix, Workflow D details, and acceptance checklists live in `docs/skills/ai-orz-doc-maintainer.md` for local lookup.
