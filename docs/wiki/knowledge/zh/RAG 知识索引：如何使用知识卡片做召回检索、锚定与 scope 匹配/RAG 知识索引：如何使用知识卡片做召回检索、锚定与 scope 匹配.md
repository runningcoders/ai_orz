---
kind: rag_knowledge_index
name: RAG 知识索引：如何使用知识卡片做召回检索、锚定与 scope 匹配
category: rag_knowledge_index
scope:
    - 'docs/wiki/knowledge/**'
    - 'docs/wiki/zh/content/**'
    - 'docs/skills/ai-orz-wiki-maintainer.md'
    - 'docs/skills/ai-orz-doc-maintainer.md'
source_files:
    - docs/wiki/knowledge/zh/AI Orz 多 Agent 执行框架（Rust 后端 + Dioxus 前端）/概述.md
    - docs/wiki/knowledge/zh/AI Orz 多 Agent 执行框架（Rust 后端 + Dioxus 前端）/技术栈.md
    - docs/wiki/knowledge/zh/基于 tracing 的结构化日志系统（宏 + 文件滚动 + JSONL 查询）/基于 tracing 的结构化日志系统（宏 + 文件滚动 + JSONL 查询）.md
    - docs/wiki/knowledge/zh/_index.yaml
    - docs/skills/ai-orz-wiki-maintainer.md
    - docs/skills/ai-orz-doc-maintainer.md
    - AGENTS.md#L92-L358
    - docs/wiki/zh/content/架构设计/文档体系规范/四类文档互引闭环管理/四类文档互引闭环管理.md
    - docs/wiki/zh/content/基础设施/知识库与知识图谱/RAG 知识卡索引机制/RAG 知识卡索引机制.md
---

## 1. 整体方案

AI Orz 在 `docs/` 下维护**四类文档完整链路、显式互引、绝不孤立**的知识体系（v2.0 架构，用户定义 SSOT）：① `docs/design/*.md` 设计决策（为什么做）、② `docs/archive/plan-archive/*.md` 落地结果（怎么做+验收）、③ `docs/wiki/zh/content/` 8 大板块 353 篇人类百科长文（是什么，系统化查阅）、④ `docs/wiki/knowledge/zh/` 54+ 张 YAML+4 节原子知识卡（总结+索引，给 Agent RAG 第一召回层）。四类文档之间**通过绝对路径显式互相引用**（而非 v1.0 的"隐性共享源码锚点"）：RAG 卡的 `source_files[]` 强制写对应 wiki 长文路径；wiki 长文的 `<cite>` 区强制写对应 design/plan/RAG 卡路径；design/plan 的「关联文档」段强制写对应 wiki/RAG 卡占位或真实路径。

本卡说明 Agent 如何正确使用知识卡做 RAG 召回、如何用 scope 做文件过滤、如何用 source_files 的 4 类路径做 5 跳召回链路、如何处理同主题多张近似卡的冗余策略。核心约束（v2.0 更新）：召回永远是「知识卡④ → 对应 wiki 长文③（系统化中文百科拿全量上下文）→ 真实源码锚点（精确位置读实现）→ 对应 design①（为什么做的决策背景）→ 对应 plan②（落地结果与扩展路径）」的 5 跳顺序；绝不做"先用长文全文检索、再找知识卡"的反向路径。

## 2. 关键文件与位置

| 文件 | 职责 |
|---|---|
| `docs/wiki/knowledge/zh/_index.yaml` | 知识卡顶层模块导出索引（schema_version=1），声明 2 个顶层模块（AI Orz 多 Agent 执行框架 / 多模块工作区）与 1 个子模块（Playwright E2E） |
| `docs/wiki/knowledge/zh/AI Orz 多 Agent 执行框架…/` | 顶层模块 1：覆盖 Rust 后端 + Dioxus 前端，含 概述/技术栈/架构/编码规范/特殊配置命令 5 张标准模块卡 |
| `docs/wiki/knowledge/zh/AI Orz 多模块工作区…/` | 顶层模块 2：覆盖 workspace 结构，含 E2E 子模块 |
| `docs/wiki/knowledge/zh/<中文描述>/<同名>.md` | 44+ 张独立单主题知识卡（日志/错误/策略/工具/存储/向量/CI等），每张 YAML Front Matter 5 字段 + 4 标准章节 + **source_files[] 强制含对应 wiki 长文相对仓库根路径** |
| `[四类文档互引闭环管理](docs/wiki/zh/content/架构设计/文档体系规范/四类文档互引闭环管理/四类文档互引闭环管理.md)` | ⭐【v2.0 显式互引】本卡对应的人类百科长文（知识卡④→长文③的第二跳入口），告诉人类/Agent 四类文档如何互引、怎么写占位路径、防死锁规则 |
| `[RAG 知识卡索引机制](docs/wiki/zh/content/基础设施/知识库与知识图谱/RAG 知识卡索引机制/RAG 知识卡索引机制.md)` | ⭐【v2.0 显式互引】本主题的另一组长文，讲解 chunk 方式、scope 匹配算法、召回打分权重等更深入的索引机制细节 |
| `docs/skills/ai-orz-wiki-maintainer.md` + `.trae/skills/ai-orz-wiki-maintainer/SKILL.md` | 知识库维护 Skill（wiki 侧）：负责长文③ + 卡④同步，强制写 `<cite>` 区 + `source_files[]` 的四类互引路径 |
| `docs/skills/ai-orz-doc-maintainer.md` + `.trae/skills/ai-orz-doc-maintainer/SKILL.md` | 知识库维护 Skill（doc 侧）：负责 design① + plan② 生命周期，强制写「关联文档」的 wiki/RAG 占位路径 |

## 3. 架构与设计约定

### 3.1 召回优先级（v2.0 5 跳顺序）：知识卡 → Wiki 长文 → 源码锚点 → Design → Plan
RAG 召回链路**固定 5 跳**（v2.0，核心动机是"知识卡是原子短卡、必须跳长文拿系统化上下文，绝不在知识卡命中后直接读源码"）：
1. **先卡匹配**：与 54+ 张独立知识卡 + 2 张顶层模块卡做 chunk 向量语义匹配 → 命中 N 张；
2. **跳对应 Wiki 长文（v2.0 强制新增）**：查命中卡 `source_files[]` 中以相对仓库根路径形式写的 wiki 长文链接，**立即跳长文的系统化中文百科获取该主题全量上下文**（这是 v2.0 相比 v1.0 的最大差异——知识卡只是索引/摘要，长文才是真正阅读材料，不要跳过）；
3. **跳真实源码锚点**：长文 `<cite>` 区 / `章节来源` 段 + 卡 `source_files[]` 中源码路径（附 `#Ln-Lm` 行号 fragment）→ 跳精确文件位置读真实现；
4. **跳 Design 文档（为什么做）**：若仍需要设计动机、决策权衡历史、行为红线等「为什么」信息 → 跳卡 `source_files[]` 或长文 `<cite>` 区里列出的 `docs/design/*_design.md`；
5. **跳 Plan 文档（怎么做+落地结果）**：若还需要落地的改动清单、扩展入口、验收数据等「实施脉络」→ 跳卡 `source_files[]` 或长文 `<cite>` 区里列出的 `docs/archive/plan-archive/*.md`。

跳过 (1) 直接做长文全文检索 → 反模式（召回噪音大、token 消耗巨）；跳过 (2) 在卡命中后直接跳源码 → v2.0 明确禁止（短卡没有系统化上下文，直接跳源码很容易读偏模块边界）。

### 3.2 scope 字段的文件过滤机制（不变）
每张卡的 YAML `scope[]` 是 glob 模式数组（`src/pkg/tool_registry/**`、`docs/design/logging_design.md` 等）。RAG 引擎在「用户明确传入一组文件上下文」或「IDE 当前打开文件列表已知」的场景下，必须先用 scope glob 匹配传入文件集，再从匹配通过的卡片里做语义召回（不匹配的卡直接丢弃，不参与向量打分）；绝不做"全 54 卡一律打分"的无脑召回——这是 scope 字段存在的唯一理由（避免"日志系统卡"被"代码中出现了一次 log_info 字样"的配置文件误召回）。

### 3.3 source_files 字段的锚点与路径写法约定（v2.0 四类齐全）
`source_files[]` 是卡片与真实知识源的硬锚（不是 scope 的重复，scope 是 glob 过滤、source_files 是精准入口）。v2.0 **强制四类齐全**。写法约定：
- **顺序**：推荐顺序（不要乱）= 3~8 个源码锚点 → 对应 design 文档路径（若有）→ 对应 plan 文档路径（若有）→ **⭐ 至少 1 条对应 wiki 长文相对仓库根路径（强制）** → 同主题兄弟平行近似卡 0~N 条（可选，辅助 RAG 关联链召回）；
- **源码锚点**：真实存在的相对项目根路径（不写任何协议前缀，直接 `src/...`），可选 `#Ln-Lm` 行号 fragment（AGENTS §2.1.2 主格式）；范围漂移时宁可不写后缀也不要写错范围；
- **另外三类文档路径（design/plan/wiki 长文/兄弟 RAG 卡）**：**一律写相对仓库根路径**（如 `docs/design/x_design.md`；与 `AGENTS §2.1.2` 路径引用统一规范一致——GitHub 原生解析 + IDE 可点 + 文档中心通跳；永不写 `file://` 伪协议与本机绝对路径）；
- **wiki 长文路径数量底线**：每张独立主题卡 **至少 1 条**对应主组长文；同主题跨 8 大板块有多组长文时 ≥2 条；绝不允许创建一张 `source_files[]` 中 0 条 wiki 长文路径的卡（v1.0 允许、v2.0 禁止）。创建卡时若长文还没同步，先占位写精确目标最终相对路径，待 wiki-maintainer 落地后回填真实有效性；
- 卡 `source_files[]` 中的 wiki 长文 + 对应长文 `<cite>` 区中的卡路径 —— **必须形成双向引用闭环**（不是 v1.0 的隐性共享源码锚点，而是显式互跳）。

### 3.4 同主题多张平行卡的冗余策略（不变）
因为知识卡是 Agent 历次"增量再生成"的产物，同一主题（如结构化日志、配置系统、统一错误模型）会存在 2-5 张语义相近但描述角度、范围行号不同的卡片。召回时这些近似卡会同时打分较高。处理策略：**全部召回、并行阅读、不做去重、不删旧卡**。冗余通常是"同流程的不同切面"（例如一张讲宏的使用、另一张讲上下文字段注入），合在一起信息更完整。写新卡时，也不要为了"唯一性"强行去搜已有 54 张卡再改命名——语义相近就允许并存，这是设计意图。

### 3.5 4 章节固定标题 + YAML 5 字段的解析约定（不变）
RAG 解析引擎对卡片的 chunk 方式是：YAML Front Matter 5 字段作为结构化元数据（做过滤、分类、scope 匹配、按 kind 分桶召回时使用）；正文 4 节按节切 chunk（§1 低权重做主题概述、§2 表格权重最高做路径快速跳、§3 权重中等做约定理解、§4 权重最高做强制约束匹配）。因此 **4 节标题、5 字段**必须一字不差，不允许增删字段或更改节标题为近似词。卡片创建流程由 ai-orz-wiki-maintainer Skill v2.0 强制四类互引合规。

## 4. 约定与约束
- 召回链路永远是「知识卡④ → Wiki 长文③ → source_files 源码 → design① → plan②」5 跳顺序，**绝对禁止跳过 Wiki 长文直接从卡跳源码**（短卡缺系统化上下文）；也绝对禁止反查长文全文再找知识卡。
- scope glob 不匹配用户传入文件集的卡 → 直接丢出召回候选集，不参与向量打分。
- ⭐【v2.0 核心新约束】知识卡的 `source_files[]` **必须至少有 1 条对应 wiki 长文的相对仓库根路径**（0 条 = 无效卡）。若长文还未创建，可写入精确占位目标路径；但一张卡对外发布（进入召回集）时 source_files[] 中的 wiki 路径必须全部指向真实存在的文件。
- ⭐【v2.0 删除的旧约束】v1.0 中"知识卡与长文互不直接索引、只通过源码/设计文档隐性关联"——**完全废弃**。所有知识卡必须在 `source_files[]` 与 §2 关键文件表中至少显式链接一次对应 wiki 长文；长文也必须在 `<cite>` 区反向链接该卡，形成双向闭环。
- 若 RAG 命中卡后发现 source_files 某条路径**已被重命名/移动**，本次问答不报错、仍然使用该卡上下文，但要登记"下次 wiki 同步时修复该卡路径"；绝不应该因某条路径漂移而把一张卡整体标记为"无效"丢出召回集。
- 新增知识卡时，YAML 的 `name == 目录名 == md 文件名`（三者必须完全相同，包括中文标点与括号），否则 IDE 文件扫描会失去该卡。
- scope 字段的 glob 数组不允许写 `['**']` 这个 catch-all（只有 2 张顶层模块 Overview 卡允许写 `['**']`）；独立主题卡必须收敛到 2-5 个真覆盖的 glob 模式，不然召回噪音会爆表。
- §4 必须至少 5 条 bullet，不多于 15 条；这是召回后 Agent 最直接读的一节，要写"能直接用"的操作约束，不要写概念。
- §2 表格行数 4-12 行；**强烈推荐在 §2 表格中增一行显式链接对应 wiki 长文**（如本卡 §2 中做法），给人类读者一眼可点的入口（与 source_files[] 中强制 1 条 wiki 路径互为呼应）。
- 本卡自身（kind=rag_knowledge_index）的召回优先级：当用户问「如何使用知识卡 / 为什么召回不到 / RAG 怎么匹配 scope / 为什么有两张一样的卡 / source_files 写哪几个 / 卡和 wiki 长文怎么互引」这类**元问题**时，应该被高分命中；代码实现类问题不应该命中本卡。
- 所有四类文档互引引用（卡→长文、长文→卡、design/plan→wiki/RAG 等）的路径写法统一：**一律写相对仓库根路径**（源码引用带 `#Ln-Lm` 行号 fragment，如 `src/pkg/logging.rs#L15-L42`；文档引用直接相对路径，如 `docs/design/x_design.md`）；永不写 `file://` 伪协议与本机绝对路径（AGENTS §2.1.2，docs_lint CI 门禁强制）。
