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
| ④ RAG knowledge cards | `docs/wiki/knowledge/zh/` (54+, **按 AGENTS §2.1.3 图谱法则决策合并/拆分，禁止裸重叠**) | Summary + index (for Agent RAG) | **This skill** |

Specifically this skill owns:

- **Human encyclopedia** (`docs/wiki/zh/content/`, ~353 articles): structured long-form pages for the docs center and human readers. Each page requires exactly 10 numbered sections, a `<cite>` reference block before TOC with **3 related doc links** (design + plan + RAG card repo-relative paths) placed after the source-code list, and section-sourced path links at end of every §2-7.
- **Agent RAG cards** (`docs/wiki/knowledge/zh/`, 54+ growing, **严格按 AGENTS.md §2.1.3 图谱节点组织法则执行；仅 Level 3 互补视角平行卡允许独立存在且必须显式关联声明**): single-topic atomic cards for IDE / RAG retrieval. Strict shape: YAML frontmatter (5 fields: kind, name, category, scope[], source_files[]) → 4 fixed sections. **`source_files[]` MUST include at minimum 1 wiki long-article repo-relative path + 1 design doc path (if exists) + 1 plan doc path (if exists)** alongside 3-8 code anchors.

## When to use (trigger conditions)

- User says any of: "同步最近代码到 wiki", "update the wiki", "sync commits to wiki", "给 wiki 加一张知识卡", "更新知识库"
- A feature/refactor lands and code changes need to be reflected to both human readers and the internal RAG recall set — AND the corresponding ① design / ② plan docs already exist (or at minimum their target repo-relative paths are known from placeholder entries written by ai-orz-doc-maintainer, so this skill can cite them)
- **Do NOT use this skill for** docs/design, docs/plan, docs/archive, or docs/superpowers lifecycle → use `ai-orz-doc-maintainer` instead

## 8-step execution SOP (v2.1 — 图谱节点组织强制对齐版 + 四类互引硬强制)

> 📌 **v2.1 升级要点**：删除旧版 Step 5 中 "Overlaps OK" 条款，新增 Step 0 前置查重（AGENTS §2.1.3.2 5 级决策算法）作为所有 RAG 卡新增/更新的必经前置分支。所有旧 SOP 提到「允许重叠」的措辞一律作废，合法的"多卡并行"仅指 Level 3 声明关联关系的互补视角平行卡。

0. **【RAG 卡必过前置】图谱节点 5 级决策判定（对应 AGENTS §2.1.3.2，不可跳过）**
   对本次候选主题集合 T（由 Step 1-2 模块变更推导出来）中的每一个候选主题 T_i，必须先执行：
   - ① 扫 `docs/wiki/knowledge/zh/` 下**所有现存 md**，做三维匹配：name 关键词模糊匹配 + category 精确匹配 + scope[] glob 交集面积 >= 30%。
   - ② 命中 0 张 → 标注为 Level 5（纯新主题），进入 Step 5 的直接新建分支 ✅。
   - ③ 命中 >= 1 张 → 按 Level 1 → 2 → 3 → 4 优先级逐层判定，**一旦命中高层级就停，不再降判**：
     - **Level 1（完全重复）**：scope[] 互为子集 AND §4 硬约束重叠率 > 90% AND 主题同义。动作：吸收合并到旧卡（§2 加 T 独有锚点 + §4 加 T 独有硬约束 + scope[]/source_files[] 取并集）；T 的草稿卡移 `docs/archive/wiki-rag-archive/YYYY-MM-DD_*`（加 Template C 归档头 3 行）；全局重写 Design/Plan/Wiki 中所有指向副卡的相对仓库根路径为主卡路径。⛔ **绝对不允许新建卡**。
     - **Level 2（主卡-子卡 层级包含）**：scope[T] ⊂ scope[旧卡]（真子集）。动作：优先合并到旧卡；只有「合并后 §4 >15 条或 scope 实际为两不相交并集」才允许拆分。拆分后按 AGENTS §2.1.3.3 ② 写双方关联声明（主卡 §2 末行 + 子卡 §3 首句 + 子卡 source_files[] 追加主卡）。
     - **Level 3（总卡-视角细卡 分层/分视角）**：scope 交集 30-80%，典型模式：严格分层 / 协议三角 / 双端视角 / 双端实现。动作：允许独立建卡，但必须按 AGENTS §2.1.3.3 ③ 写**每张对称声明**：每张 §3 首句统一句式 + 每张 source_files[] 末尾追加所有兄弟卡相对仓库根路径（闭环，缺一张=FAIL）。
     - **Level 4（总卡-细卡 总分）**：scope[旧卡] ⊃ scope[T]（真超集），旧卡有总分结构。动作：旧卡保总卡，T 建细卡。按 AGENTS §2.1.3.3 ④ 写声明：总卡 §2 末行列细卡路径 + 细卡 §3 首句 + 细卡 source_files[] 加总卡路径。
   - ④ 判定结果写入产物清单：`RAG_DECISIONS: [T_i → Level N | 合并至<旧卡路径> / 新建（并声明与X/Y卡关系）]`，后面 Step 5 严格按此执行，不得跳脱。
   - ⑤ **【仅 AI Orz 多模块工作区 / 多 Agent 执行框架 两组】_module.yaml 驱动模块子卡豁免分支**（对应 AGENTS §2.1.3.5）：若 T_i 落在这两个目录范围内，跳过上面的 scope 交集判定，改读同目录 `_module.yaml` 中现存子卡的 `role:` / `section:` 标签 → 标签重复 = Level 1 合并归档；标签为新角色 = Level 3 兄弟卡新建，必须把同组其余 5 张子卡相对仓库根路径全部追加到 `source_files[]` 形成闭环。
   - ⑥ **语义别名疑似清单 self-check（AGENTS §2.1.3.6 红线：密集覆盖领域不得写 0 蒙混）**：对所有被判 Level 5（纯新主题）的候选主题，额外执行一次「通读 docs/wiki/knowledge/zh/ 下所有现存卡的 YAML name 标题做语义比对」—— 如果主题属于「日志 / 配置 / 统计 / 构建 / 前端样式」等 6 大高重复率领域，或读标题时发现读起来像"同一件事不同叫法"，就记入 `SEMANTIC_ALIAS_SUSPECTS: [(T_i_name, 疑似现存卡名, 建议人工确认合并 or 新建)]` 疑似清单，哪怕最后仍判 Level 5 也要把这次自我怀疑的判断过程留下来。疑似项数量写入下面 Step 7 的 commit 自我声明签名，绝不允许"为省事 0 条走天下"。
1. **Collect change range** — Identify BASE_SHA → HEAD. Exclude pure-doc commits: `docs(...)` / `docs(cleanup)` / `docs(plan)` / `docs(readme)` / `docs(skill-communication)`. Keep feat/refactor/fix/test/style.
2. **List changed files** — `git diff --name-only BASE..HEAD | grep -v "^docs/"`. Aggregate by module. Also **greps for any `占位：待 wiki 同步后回填` tags** in `docs/design/*.md` + `docs/plan/*.md` to collect placeholder targets that this sync must create.
3. **Hit-map candidate long articles** — Reverse-grep 353 content mds' `<cite>` blocks + 「章节来源」sections against changed files. Add TOP-section root pages by module semantics.
4. **Incrementally update the 353 long articles** on each hit:
   - Append references to new paths in `<cite>`;
   - **CRITICAL (v2.0)**: under the `<cite>` **source code list** add a new subsection **「本文关联的三类文档（四类互引闭环）」** with repo-relative-path links to corresponding ① design doc + ② plan doc + ④ RAG knowledge card(s). If placeholder tags were written in Step 2, now write the REAL created paths (this skill is usually the last executor so it owns back-fill);
   - Add "更新摘要" section after `<cite>`;
   - Expand §5, refresh section-sourced line ranges, fix mermaid diagrams + 「图表来源」 paths.
5. **生成/更新 RAG 知识卡（严格按 Step 0 判定结果执行，禁止 Overlaps OK）**（核心步骤）：由 Step 0 判定结果决定合并 vs 新建，数量由判定自然产出，不再用 "1 card / 500-1500 LOC" 的粗估规则。
   - **Level 1 分支**：执行旧卡 §2/§4/YAML 吸收合并；副卡移归档；全局引用重写（路径重写范围覆盖 docs/design + docs/plan + docs/wiki 全部 md，排除 archive）。结束后不得在非 archive 目录找到副卡残留路径引用。
   - **Level 2 分支（合并）**：与 Level 1 合并动作相同，但**不归档**旧卡（合并进旧卡，旧卡是主卡保留）。
   - **Level 2/3/4 分支（拆分/新建）**：才允许新 md 写盘。写盘后**强制执行关联声明检查**——对照 AGENTS §2.1.3.3 ②/③/④ 对应关系类型的「双方声明条目清单」逐条 grep 验证，缺一条就 Edit 补写，不允许带着孤立卡结束 Step 5。
   - **CRITICAL (v2.1 — 对齐 AGENTS §2.1.3)**: `source_files[]` 数组必须四类齐全：3-8 条源码锚点 → 然后 ① `docs/design/...md`（如有）→ 然后 ② `docs/plan/...md`（如有）→ 然后 **⭐ 至少 1 条对应 ③ Wiki 长文的相对仓库根路径**（同一 run 才写的新长文可先用占位，Step 6 结束前必须解析成真实路径）→ **然后按 AGENTS §2.1.3.3 关系类型追加对应的关联 RAG 卡相对仓库根路径（Level 2 主卡 / Level 3 所有兄弟卡 / Level 4 总卡），0 条=FAIL**。
6. **Create brand-new long articles** if Step 5 produced RAG cards for capabilities with no article yet. Always write §8 Troubleshooting (min 2-3 paths). **After creation, go back and resolve any placeholder wiki-long-article paths in RAG card `source_files[]` from Step 5 to real existing paths (0 wiki refs in a RAG card = fail).** Also resolve any `占位` tags in design/plan doc headers that were waiting on this wiki sync to become real links.
7. **提交与收尾**：
   - 再次**全局扫一遍** docs/design + docs/plan + docs/wiki 下所有 md：(a) 检查有没有 "Overlaps OK" / "重叠允许" / "重叠不用管" 等非法反模式措辞，出现则当场 Edit 删除或替换为合法声明；(b) 检查 Level 1 归档副卡的相对仓库根路径是否还有残留引用，有则替换为主卡路径；(c) 检查 Level 2/3/4 拆分卡是否缺关联声明条目，缺则补齐。
   - **Σ 一致性校验（AGENTS §2.1.3.6 红线，提交前必核对）**：统计 RAG_DECISIONS 中各 Level 实际计数 → Level1:X / Level2合并:M / Level2拆分:K / Level3:P / Level4:Q / Level5:R。必须满足 X+M+K+P+Q+R = N（候选主题总数），不相等说明 Step 0 有主题漏判或重复计数，回 Step 0 修正，禁止带着 Σ≠N 的结果提交。
   - **Commit 消息（末尾必须完整粘贴「重复检查自我声明签名」段，与 AGENTS §2.1.3.6 模板逐字对齐）**：
     ```
     docs(wiki): <scope> — 长文更新X页 + 知识卡合并M张新增N张（含归档K张重复副卡）+ 四类互引与图谱关联对齐（AGENTS §2.1.3 v2.1，BASE..HEAD 摘要）

     —— 重复检查自我声明（AGENTS §2.1.3.6 v2.1）——
     本次候选主题总数：<N> 个
     Step 0 5 级判定结果 →
       Level 1（完全重复 → 合并归档副卡）：<X> 张
       Level 2（主卡-子卡 → 合并到主卡）：<M> 张
       Level 2（主卡-子卡 → 拆分新建子卡 + 声明）：<K> 张
       Level 3（视角兄弟卡 → 独立新建 + 互声明）：<P> 张
       Level 4（总卡-细卡 → 新建细卡 + 声明）：<Q> 张
       Level 5（纯新主题 → 直接新建）：<R> 张
       合计处理 RAG 卡：<X+M+K+P+Q+R> 次（含合并，不与新建重复计数）
     ——
       🔍 语义别名疑似清单（name/scope 不命中但语义读下来近似同主题）：
         • <疑似1：候选主题 T_name → 疑似对应现存卡 <现存卡名>；建议：人工确认后决定合并 or 新建>
         • <疑似2：... >
         • 0 条（如无则写这一句；6 大高重复领域被判 Level5 的必须至少写 1-2 条留痕，即使最后结论仍为 Level5）
     ——
     ```
     大规模变更可按 (infra/core/modules/frontend/cards/merge-pass) 分片提交，但每个分片提交都必须带**完整独立的**自我声明签名段（按该分片实际处理主题数重算 N 和 Σ 分项），禁止只在前一个分片贴签名、后面分片"同上省略"。

## Hard non-negotiables

- **⭐【新增 v2.1 强制】RAG 卡必须先过 Step 0 5 级决策，禁止裸新建**：跳过 Step 0 前置查重 → 直接新建 RAG 卡 = FAIL。Level 1 完全重复场景（scope 子集 + §4 重叠率 > 90%）仍然新建卡（即使附了"Overlaps 说明"）= FAIL。任何拆分场景（Level 2/3/4）结束后，必须满足 AGENTS §2.1.3.3 对应关系的"双方声明条目清单"全部命中，缺一条视为孤立重复卡 = FAIL，需回退补齐。整个代码库文档中不再允许出现 "Overlaps OK" / "允许重叠" / "重叠不用管" 等反模式措辞。
- **⭐【新增 v2.1 强制】Σ 一致性校验必须通过，否则禁止提交**：Step 7 Σ 核对 X+M+K+P+Q+R ≠ N → Step 0 存在漏判/重复计数/判错层级，按 FAIL 处理，回 Step 0 修正后再走完全流程。绝不允许通过"四舍五入数字凑 Σ=N"蒙混过关。
- **⭐【新增 v2.1 强制】Commit 消息必须带完整「重复检查自我声明签名」段**：任何 wiki-maintainer 发起的 docs(wiki): 系列提交，消息末尾缺失 `—— 重复检查自我声明（AGENTS §2.1.3.6 v2.1）——` 标记的签名段 = FAIL，视同未执行 Step 0，需回滚重新跑完整流程（包括 Σ 校验）后再提交。分片提交时每个分片都必须带独立签名段，禁止"同上省略"。
- **Both wiki bases must be updated in the same run**: never only articles or only cards (human/RAG desync = #1 failure).
- **⭐ 4-doc cross-citation coverage底线**: (a) Every NEW RAG card → 100% has ≥1 wiki long-article repo-relative path in `source_files[]` (0 = fail). (b) Every NEW/UPDATED wiki article → 100% `<cite>` "本文关联三类文档" section has ≥1 design/plan repo-relative path + ≥1 corresponding RAG card path.
- No code snapshots anywhere except mermaid graph blocks. Replace any implementation-detail code with 1-line path links.
- Content articles: always 10 numbered section anchors, §8 never omitted. Source-code references use repo-relative paths with `#Ln-Lm` line fragments (e.g. `src/pkg/logging.rs#L15-L42`). **Doc-to-doc cross-links (design/plan/RAG paths in cite section) use repo-relative paths too (e.g. `docs/design/xxx.md`) — AGENTS §2.1.2 format.**
- RAG cards: exactly the 5 YAML fields; `name == directory name == md basename` (all three equal, Chinese); `scope[]` holds globs, not file paths; `source_files[]` holds 3-10 anchors optionally with `#Ln-Lm` plus mandatory doc cross-links; section titles fixed §1-§4 verbatim. §2 table has a row linking to the wiki long article.
- **⭐【路径格式硬约束】文档与 RAG 卡中所有路径引用（cite 节 / 章节来源 / source_files[] / 关联文档头部）必须使用 AGENTS §2.1.2 相对路径格式（行号 `#Lx-Ly`）**：出现 `file:///` 绝对路径 / `file://` 伪协议 / legacy 冒号行号 → 执行结果 FAIL，改完再过。

## Fallbacks

- If line ranges drifted after BASE..HEAD diffs: prefer dropping `#Ln-Lm` rather than pointing to wrong ranges.
- If user wants only one specific commit sync: skip Step 1, start directly at Step 2 with that commit's changed files. Still MUST apply 4-doc cross-citation rules.
- If user explicitly says "only add a knowledge card": skip Steps 3-4/6, do only card creation (Step 5). However the new RAG card **still requires at least 1 wiki long-article path in source_files[]** — create a placeholder target path pointing to where the article SHOULD be, and add a note that a follow-up wiki-maintainer sync is needed to land the matching long article.
- If design/plan docs for a new feature don't exist yet: write placeholder paths in `source_files[]` / cite section formatted as `（占位：待 ai-orz-doc-maintainer 落地后回填真实路径）`. Wiki-maintainer is the last executor most of the time; if doc-maintainer runs after, it owns replacing the placeholders.
- Full cross-reference spec, 8-hard-constraint / 7-hard-constraint tables, placeholder conventions, and anti-deadlock rules are maintained in `docs/skills/ai-orz-wiki-maintainer.md` for local lookup.
