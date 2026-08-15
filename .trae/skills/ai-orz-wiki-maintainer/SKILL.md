---
name: "ai-orz-wiki-maintainer"
description: "Synchronizes code changes into AI Orz docs/wiki dual knowledge base: human long articles (8 sections, 353 files) plus Agent RAG knowledge cards (YAML+4-section, 53+ atomic cards). Invoke whenever user asks to 'sync code to wiki', 'update wiki knowledge base', 'add knowledge card', or after a feature lands so docs/wiki reflects the latest implementation and RAG stays fresh."
---

# AI Orz Wiki Maintainer

Maintain the two parallel, independent knowledge bases under `docs/wiki/`:

- **Human encyclopedia** (`docs/wiki/zh/content/`, ~353 articles): structured long-form pages for the docs center and human readers. Each page requires exactly 10 numbered sections (Intro / Project Structure / Core Components / Architecture Overview / Detailed Analysis / Dependencies / Performance / Troubleshooting / Conclusion / Appendix), a `<cite>` reference block before the TOC, and section-sourced path links at the end of every §2-7.
- **Agent RAG cards** (`docs/wiki/knowledge/zh/`, 53+ growing, overlaps allowed): single-topic atomic cards for IDE / RAG retrieval. Strict shape: YAML frontmatter (5 fields: kind, name, category, scope[], source_files[]) → 4 fixed sections (1 Overview / 2 Key File Table / 3 Architecture & Conventions / 4 Constraints bullets). Never delete old parallel cards or cross-link to content articles.

## When to use (trigger conditions)

- User says any of: "同步最近代码到 wiki", "update the wiki", "sync commits to wiki", "给 wiki 加一张知识卡", "更新知识库"
- A feature/refactor lands and code changes need to be reflected to both human readers and the internal RAG recall set
- **Do NOT use this skill for** docs/design, docs/plan, docs/archive, or docs/superpowers lifecycle → use `ai-orz-doc-maintainer` instead

## 7-step execution SOP

1. **Collect change range** — Identify BASE_SHA → HEAD. Exclude pure-doc commits: `docs(...)` / `docs(cleanup)` / `docs(plan)` / `docs(readme)` / `docs(skill-communication)`. Keep feat/refactor/fix/test/style.
2. **List changed files** — `git diff --name-only BASE..HEAD | grep -v "^docs/"`. Aggregate by module for visibility.
3. **Hit-map candidate long articles** — Reverse-grep 353 content mds' `<cite>` blocks + 「章节来源」sections against changed files. Add TOP-section root pages by module semantics.
4. **Incrementally update the 353 long articles** on each hit: append references to new paths, add a "更新摘要" section after `<cite>`, expand §5 with natural-language descriptions of new capabilities, refresh section-sourced line ranges, fix mermaid diagrams and their 「图表来源」 paths.
5. **Generate NEW RAG knowledge cards** (core step): ~1 card per 500-1500 net lines of code change (typically 5-15 cards per sync). Cards are topic-oriented (NOT 1:1 mapped to articles). Allow overlaps with existing cards. Strictly respect the YAML+4 shape.
6. **Create brand-new long articles** if Step 5 produced RAG cards for capabilities that have no article in the 8 section tree yet. Always write §8 Troubleshooting even with only 2-3 minimum paths.
7. **Commit and push**: Message prefix `docs(wiki): <scope> — 长文更新X页 + 知识卡新增Y张（BASE..HEAD摘要）`. May split into chunks (infra/core/modules/frontend/cards) if large.

## Hard non-negotiables

- **Both bases must be updated in the same run**: never only articles or only cards (human/RAG desync is the #1 failure mode).
- No code snapshots anywhere except mermaid graph blocks. Replace any implementation-detail code with 1-line path links.
- Content articles: always 10 numbered section anchors, §8 never omitted. All references use `file://relative-path` (relative to project root).
- RAG cards: exactly the 5 YAML fields; `name == directory name == md basename` (all three equal, Chinese); `scope[]` holds globs, not file paths; `source_files[]` holds 3-10 anchors (optionally with `:Ln-Lm` suffix); section titles fixed §1-§4 verbatim.

## Fallbacks

- If line ranges drifted after BASE..HEAD diffs: prefer dropping `:Ln-Lm` rather than pointing to wrong ranges.
- If user wants only one specific commit sync: skip Step 1, start directly at Step 2 with that commit's changed files.
- If user explicitly says "only add a knowledge card": skip Steps 3-4/6, do only card creation (Step 5).
- Full cross-reference spec and detailed 8-hard-constraint / 7-hard-constraint tables are maintained in the project spec at `docs/skills/ai-orz-wiki-maintainer.md` for local lookup.
