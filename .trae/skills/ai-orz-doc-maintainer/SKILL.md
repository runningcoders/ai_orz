---
name: "ai-orz-doc-maintainer"
description: "Writes and refactors AI Orz docs/design, docs/plan, docs/archive lifecycle; converts superpowers execution blueprints to compact 7-section plan archives; enforces AGENTS §2.1: file-head 4-metadata + contract-vs-snapshot code-block judgment table + path-first citations. Invoke when user asks to write a design doc, archive a plan, clean up docs placeholders, apply doc spec, or reduce a superpowers blueprint."
---

# AI Orz Doc Maintainer

Enforce **AGENTS.md §2.1** across all four quadrants of `docs/`: `design/` decision snapshots, `plan/` 7-section landing summaries, `archive/` tombstoned references, and lifecycle of temporary `docs/superpowers/*` execution artifacts (must be disposed within 7 days of feature completion).

## When to use (trigger conditions)

- User says any of: "写一份 design 文档", "精简 superpowers 蓝图为 plan 归档", "给文档扫雷代码块", "补全 AGENTS 规范文档头", "apply AGENTS §2.1", "placeholders 大扫除", "docs cleanup"
- Just finished a multi-step feature and the superpowers plan / checklist / tasks files still need to go through their 7-day end-of-life disposal
- A doc review reveals: implementation-snapshot code blocks, missing 🎯 file-head metadata, TBD/TODO placeholders, broken cross-path links

**Do NOT use for `docs/wiki/` (encyclopedia + RAG cards)** → route to `ai-orz-wiki-maintainer`. This skill never touches wiki.

## 3 high-level workflows

### Workflow A — New design doc (`docs/design/`)

1. Place correctly: `docs/design/<topic>_design.md` (match existing naming style).
2. Apply **Template A** (5 chapters: Goals & decision table / ASCII architecture / file inventory table by layer / boundary & behavior redlines / extension patterns). Decision table (§1.X with Q/Plan/Reason) is mandatory.
3. Inventory table rows must carry **clickable absolute paths with `file:///.../ai_orz/... :Ln-Lm`** anchors. Do NOT invent paths for files not yet created.
4. Run the **code-block judgment table** below. Keep contract blocks and add a path line immediately under; delete snapshot blocks and replace with a 1-line path citation.

### Workflow B — Superpowers blueprint → Plan archive (highest frequency, `docs/plan/`)

1. **Confirm the feature is 100% complete and accepted** (do not archive still-running blueprints).
2. Copy `docs/superpowers/plans/<date>-<topic>.md` to `docs/plan/<same>.md` — **never mutate in place**.
3. **Strip ALL checkboxes / Task-Step lists / writing-plans skill markers ("For agentic workers …") / placeholders (TBD / TODO / 酌情 / 参考 Task)**. Replace checklists with static result tables; keep only the 7-sections of Template B.
4. **Strip ALL implementation-snapshots**: function bodies, test code, `cargo test`, `git push`, failure dumps. Replace with path-first 1-line citations.
5. Fill the **mandatory file-head 4-metadata**: 🎯定位 / 状态(枚举值 4 选一) / 查阅场景 / 关联文档.
6. Apply **Template B** (7 sections: Goals table / ASCII architecture & redlines / File inventory & change summary with clickable paths / Dispatcher quick-reference tables / Acceptance table / Execution results table (no commands!) / 4-step Future Extension Path).
7. Finally `git rm docs/superpowers/plans/<original>`; if historically useful → move to `docs/archive/superpowers-archive/` with a tombstone header.

### Workflow C — Full-repo docs sweep (code-block + placeholder + header pass)

1. Baseline count code blocks and placeholders across design/plan/archive.
2. Judge per the table below; 0 snapshot residues after pass.
3. Grep placeholders: `TBD|TODO|待确认|酌情|参考 Task|如果需要` → eliminate each.
4. Add missing file-head 4-metadata to every hand-maintained md. Architecture summaries (AGENTS / wikis) are exempt from the file-head rule.

## Code-block judgment table (AGENTS §2.1 iron law)

| Case | Type | Keep? | Action if NO |
|------|------|-------|--------------|
| Trait signatures w/o `{ impl }` | Contract ✅ | Optional | Keep → path guide `> 当前实现：[file.rs::Name](file:///abs/path/file.rs#Lx-Ly)` under block |
| Struct field lists / Enum variant lists | Contract ✅ | Optional | Same as above |
| SQL `CREATE TABLE` schemas | Contract ✅ | Optional (recommended in design docs) | Same + link migrations dir |
| ASCII tree / flow diagrams (pure text art) | Contract ✅ | Keep strongly | No path required — this is architecture intent |
| Function bodies, match branches, loops, control flow | Snapshot ❌ | NO | Delete → 1-line path guide only |
| Full test code, `cargo test` params, `git commit` commands, bash scripts | Snapshot ❌ | NO | Delete → keep only table rows of "N passed" |
| Task/Step checklists (`- [x] Step …`) in non-superpowers docs | Snapshot ❌ | NO | Delete → replace with static tables |
| Inline 1-2 token snippets like `foo(ctx, ..)` | Contract inline ✅ | Keep | Leave inline, no block |

## Template quick reference (full spec in `docs/skills/ai-orz-doc-maintainer.md`)

Always hand off to these 3 canonical templates from AGENTS §2.1:

- **Template A** — `docs/design/*.md` design decisions (5 chapters + mandatory §1.X decision table)
- **Template B** — `docs/plan/*.md` plan summaries (7 chapters + §4 quick-reference tables + §7 4-step extension path; no checkboxes, no commands)
- **Template C** — `docs/archive/*.md` archived references (add a single tombstone header; original body is NEVER modified)

## Non-negotiables

- Placeholder count is 0 after every pass.
- Snapshot count is 0 after every pass.
- File-head 4-metadata coverage = 100% for manually maintained docs.
- Superpowers blueprints ≥ 7 days old must be disposed (archive plan or delete or tombstone).
- Never mutate superpowers blueprints in place; always copy → prune → remove original.
- All path citations (under kept contract blocks, or replacements for deleted snapshots) use clickable **absolute** file paths with optional `#Ln-Lm` ranges. Wiki alone uses relative `file://relative` scheme.

## Fallbacks

- Unsure which quadrant a file belongs to → read AGENTS §2 first; when uncertain, treat it as a plan (least-destructive).
- A superpowers blueprint covers a multi-day, still-running feature → delay disposition; the 7-day rule starts ticking from feature completion, not creation.
