---
name: "ai-orz-wiki-maintainer"
description: "AI Orz 4-docs full lifecycle wiki side: syncs code changes into human encyclopedia long articles (8 sections, 353 files) + Agent RAG cards (YAML+4-section, 54+ cards) and ENFORCES 4-doc cross-citation with design/plan docs. Invoke on 'sync code to wiki', 'update knowledge base', 'add RAG card', or post-feature landing."
---

# AI Orz Wiki Maintainer

Manages two of AI Orz's **4-document complete chain** (alongside ai-orz-doc-maintainer which handles design/plan), with **mandatory explicit cross-citation** across all 4 doc types — no doc type can exist in isolation.

The four doc types defined by project SSOT (always maintained and linked together):

| # | Type | Location | Answers | Owner Skill |
|---|------|----------|---------|-------------|
| ① Design | `docs/design/*.md` | Why (decision snapshot) | ai-orz-doc-maintainer |
| ② Plan | `docs/plan/*.md` | How + landing result | ai-orz-doc-maintainer |
| ③ Wiki long articles | `docs/wiki/zh/content/` (8 sections, ~353) | What is it (encyclopedia) | **This skill** |
| ④ RAG knowledge cards | `docs/wiki/knowledge/zh/` (54+, overlaps OK) | Summary + index (for Agent RAG) | **This skill** |

Specifically this skill owns:

- **Human encyclopedia** (`docs/wiki/zh/content/`, ~353 articles): structured long-form pages for the docs center and human readers. Each page requires exactly 10 numbered sections, a `<cite>` reference block before TOC with **3 related doc links** (design + plan + RAG card absolute paths) placed after the source-code list, and section-sourced path links at end of every §2-7.
- **Agent RAG cards** (`docs/wiki/knowledge/zh/`, 54+ growing, overlaps allowed): single-topic atomic cards for IDE / RAG retrieval. Strict shape: YAML frontmatter (5 fields: kind, name, category, scope[], source_files[]) → 4 fixed sections. **`source_files[]` MUST include at minimum 1 wiki long-article absolute path + 1 design doc path (if exists) + 1 plan doc path (if exists)** alongside 3-8 code anchors.

## When to use (trigger conditions)

- User says any of: "同步最近代码到 wiki", "update the wiki", "sync commits to wiki", "给 wiki 加一张知识卡", "更新知识库"
- A feature/refactor lands and code changes need to be reflected to both human readers and the internal RAG recall set — AND the corresponding ① design / ② plan docs already exist (or at minimum their target absolute paths are known from placeholder entries written by ai-orz-doc-maintainer, so this skill can cite them)
- **Do NOT use this skill for** docs/design, docs/plan, docs/archive, or docs/superpowers lifecycle → use `ai-orz-doc-maintainer` instead

## 7-step execution SOP (v2.0 — cross-citation hard-enforced)

1. **Collect change range** — Identify BASE_SHA → HEAD. Exclude pure-doc commits: `docs(...)` / `docs(cleanup)` / `docs(plan)` / `docs(readme)` / `docs(skill-communication)`. Keep feat/refactor/fix/test/style.
2. **List changed files** — `git diff --name-only BASE..HEAD | grep -v "^docs/"`. Aggregate by module. Also **greps for any `占位：待 wiki 同步后回填` tags** in `docs/design/*.md` + `docs/plan/*.md` to collect placeholder targets that this sync must create.
3. **Hit-map candidate long articles** — Reverse-grep 353 content mds' `<cite>` blocks + 「章节来源」sections against changed files. Add TOP-section root pages by module semantics.
4. **Incrementally update the 353 long articles** on each hit:
   - Append references to new paths in `<cite>`;
   - **CRITICAL (v2.0)**: under the `<cite>` **source code list** add a new subsection **「本文关联的三类文档（四类互引闭环）」** with absolute-path links to corresponding ① design doc + ② plan doc + ④ RAG knowledge card(s). If placeholder tags were written in Step 2, now write the REAL created paths (this skill is usually the last executor so it owns back-fill);
   - Add "更新摘要" section after `<cite>`;
   - Expand §5, refresh section-sourced line ranges, fix mermaid diagrams + 「图表来源」 paths.
5. **Generate NEW RAG knowledge cards** (core step): ~1 card per 500-1500 net LOC change (typical 5-15 cards). Topic-oriented (NOT 1:1 to articles). Overlaps OK.
   - **CRITICAL (v2.0)**: `source_files[]` array MUST be 4-doc-complete: 3-8 code anchors → then ① `docs/design/...md` (if any) → then ② `docs/plan/...md` (if any) → then **⭐ at least 1 absolute path to corresponding ③ wiki long article** (placeholder allowed if creating both in same run, but must resolve to real path by end of Step 6) → optionally brother parallel cards 0-N.
6. **Create brand-new long articles** if Step 5 produced RAG cards for capabilities with no article yet. Always write §8 Troubleshooting (min 2-3 paths). **After creation, go back and resolve any placeholder wiki-long-article paths in RAG card `source_files[]` from Step 5 to real existing paths (0 wiki refs in a RAG card = fail).** Also resolve any `占位` tags in design/plan doc headers that were waiting on this wiki sync to become real links.
7. **Commit and push**: Message prefix `docs(wiki): <scope> — 长文更新X页 + 知识卡新增Y张 + 四类互引补齐（BASE..HEAD 摘要）`. May split into chunks (infra/core/modules/frontend/cards) if large.

## Hard non-negotiables

- **Both wiki bases must be updated in the same run**: never only articles or only cards (human/RAG desync = #1 failure).
- **⭐ 4-doc cross-citation coverage底线**: (a) Every NEW RAG card → 100% has ≥1 wiki long-article absolute path in `source_files[]` (0 = fail). (b) Every NEW/UPDATED wiki article → 100% `<cite>` "本文关联三类文档" section has ≥1 design/plan absolute path + ≥1 corresponding RAG card path.
- No code snapshots anywhere except mermaid graph blocks. Replace any implementation-detail code with 1-line path links.
- Content articles: always 10 numbered section anchors, §8 never omitted. Source-code references use `file://relative-path` (relative to project root). **Doc-to-doc cross-links (design/plan/RAG paths in cite section) use `file:///absolute-full-path` clickable format.**
- RAG cards: exactly the 5 YAML fields; `name == directory name == md basename` (all three equal, Chinese); `scope[]` holds globs, not file paths; `source_files[]` holds 3-10 anchors optionally with `:Ln-Lm` plus mandatory doc cross-links; section titles fixed §1-§4 verbatim. §2 table has a row linking to the wiki long article.

## Fallbacks

- If line ranges drifted after BASE..HEAD diffs: prefer dropping `:Ln-Lm` rather than pointing to wrong ranges.
- If user wants only one specific commit sync: skip Step 1, start directly at Step 2 with that commit's changed files. Still MUST apply 4-doc cross-citation rules.
- If user explicitly says "only add a knowledge card": skip Steps 3-4/6, do only card creation (Step 5). However the new RAG card **still requires at least 1 wiki long-article path in source_files[]** — create a placeholder target path pointing to where the article SHOULD be, and add a note that a follow-up wiki-maintainer sync is needed to land the matching long article.
- If design/plan docs for a new feature don't exist yet: write placeholder paths in `source_files[]` / cite section formatted as `（占位：待 ai-orz-doc-maintainer 落地后回填真实路径）`. Wiki-maintainer is the last executor most of the time; if doc-maintainer runs after, it owns replacing the placeholders.
- Full cross-reference spec, 8-hard-constraint / 7-hard-constraint tables, placeholder conventions, and anti-deadlock rules are maintained in `docs/skills/ai-orz-wiki-maintainer.md` for local lookup.
