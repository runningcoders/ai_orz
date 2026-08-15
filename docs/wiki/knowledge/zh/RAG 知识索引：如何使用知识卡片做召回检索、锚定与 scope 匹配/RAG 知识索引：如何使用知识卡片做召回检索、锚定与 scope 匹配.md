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
    - docs/wiki/knowledge/zh/AI Orz 多 Agent 执行框架（Rust 后端 + Dioxus 前端）/架构设计.md
    - docs/wiki/knowledge/zh/基于 tracing 的结构化日志系统（宏 + 自动上下文字段注入）/基于 tracing 的结构化日志系统（宏 + 自动上下文字段注入）.md
    - docs/wiki/knowledge/zh/统一错误模型：ErrorCode + ErrorType + ErrorField 的跨层错误处理体系/统一错误模型：ErrorCode + ErrorType + ErrorField 的跨层错误处理体系.md
    - docs/wiki/knowledge/zh/_index.yaml
    - docs/skills/ai-orz-wiki-maintainer.md
    - AGENTS.md:L92-L358
---

## 1. 整体方案

AI Orz 在 `docs/wiki/` 下维护两套平行、互不直接索引、但共享源码与设计文档锚点的知识库：人类百科长文区（`docs/wiki/zh/content/` 8 大板块 353 篇结构化长文）用于文档中心与人工系统查阅；Agent RAG 知识卡区（`docs/wiki/knowledge/zh/` 53+ 张 YAML + 4 节原子卡片）作为 IDE 与 Agent 的第一召回层。本卡说明 Agent 如何正确使用知识卡做 RAG 召回、如何用 scope 过滤、如何用 source_files 路径锚点反查真代码，以及如何处理同主题多张近似卡的冗余召回策略。核心约束：召回永远先查知识卡（短、结构化、适合 RAG chunking），命中后再根据 source_files 字段与关联的 design/plan 文档跳到长文和源码获取细节。绝不做"先用长文全文检索、再找知识卡"的反向路径。

## 2. 关键文件与位置

| 文件 | 职责 |
|---|---|
| `docs/wiki/knowledge/zh/_index.yaml` | 知识卡顶层模块导出索引（schema_version=1），声明 2 个顶层模块（AI Orz 多 Agent 执行框架 / 多模块工作区）与 1 个子模块（Playwright E2E） |
| `docs/wiki/knowledge/zh/AI Orz 多 Agent 执行框架…/` | 顶层模块 1：覆盖 Rust 后端 + Dioxus 前端，含 概述/技术栈/架构/编码规范/特殊配置命令 5 张标准模块卡 |
| `docs/wiki/knowledge/zh/AI Orz 多模块工作区…/` | 顶层模块 2：覆盖 workspace 结构，含 E2E 子模块 |
| `docs/wiki/knowledge/zh/<中文描述>/<同名>.md` | 44 张独立单主题知识卡（日志/错误/策略/工具/存储/向量/CI等），每张 YAML Front Matter 5 字段 + 4 标准章节 |
| `docs/wiki/zh/content/` | 人类百科长文 353 篇（8 大板块），知识卡命中后需要更系统上下文时跳转至此区；注意：知识卡内部 **不直接链接** 长文，但两者共同锚定到相同的源码路径与 docs/design/*.md 文档 |
| `docs/skills/ai-orz-wiki-maintainer.md` + `.trae/skills/ai-orz-wiki-maintainer/SKILL.md` | 知识库维护 Skill：当需要从代码变更 → 回写知识卡时的完整 SOP 与硬约束 |

## 3. 架构与设计约定

### 3.1 召回优先级：知识卡 → 源码路径 → 设计文档 → 长文
RAG 召回链路固定 4 跳：(1) 先与 53+ 张独立知识卡 + 2 张顶层模块卡做 chunk 向量语义匹配 → 命中 N 张；(2) 看命中卡的 `source_files[]` 数组，跳真实源码文件与范围行号读实现；(3) 若需要设计动机（为什么这样做），再跳 `docs/design/*.md`；(4) 若还需要更完整的系统化学习上下文，再跳对应的 8 大板块长文。跳过 (1) 直接读 (4) 是反模式（召回噪音大、token 消耗巨）。

### 3.2 scope 字段的文件过滤机制
每张卡的 YAML `scope[]` 是 glob 模式数组（`src/pkg/tool_registry/**`、`docs/design/logging_design.md` 等）。RAG 引擎在「用户明确传入一组文件上下文」或「IDE 当前打开文件列表已知」的场景下，必须先用 scope glob 匹配传入文件集，再从匹配通过的卡片里做语义召回（不匹配的卡直接丢弃，不参与向量打分）；绝不做"全 53 卡一律打分"的无脑召回——这是 scope 字段存在的唯一理由（避免"日志系统卡"被"代码中出现了一次 log_info 字样"的配置文件误召回）。

### 3.3 source_files 字段的锚点与路径写法约定
`source_files[]` 是卡片与真实知识源的硬锚（不是 scope 的重复，scope 是 glob 过滤、source_files 是精准入口）。写法约定：
- 写 3-10 个**真实存在的相对项目根路径**（不要写绝对 `/Users/...`）；
- 可选但推荐加 `:Ln-Lm` 行号后缀（不要写无效范围；范围一旦漂移，宁可不写后缀也不要写错的）；
- 设计文档（`docs/design/*.md`）和 AGENTS.md 都允许出现在 source_files 中（例如架构类卡通常锚定设计文档）；
- 知识卡 `source_files[]` 与对应的人类长文 `cite + 章节来源` 通常指向**同一份源码**——这是两套知识库互相印证的"隐性关联"，不要求完全相同，但核心锚点必须一致（不要卡片指向 DAO 层而长文只引用 Handler 层）。

### 3.4 同主题多张平行卡的冗余策略
因为知识卡是 Agent 历次"增量再生成"的产物，同一主题（如结构化日志、配置系统、统一错误模型）会存在 2-5 张语义相近但描述角度、范围行号不同的卡片。召回时这些近似卡会同时打分较高。处理策略：**全部召回、并行阅读、不做去重、不删旧卡**。冗余通常是"同流程的不同切面"（例如一张讲宏的使用、另一张讲上下文字段注入），合在一起信息更完整。写新卡时，也不要为了"唯一性"强行去搜已有 53 张卡再改命名——语义相近就允许并存，这是设计意图。

### 3.5 4 章节固定标题 + YAML 5 字段的解析约定
RAG 解析引擎对卡片的 chunk 方式是：YAML Front Matter 5 字段作为结构化元数据（做过滤、分类、scope 匹配、按 kind 分桶召回时使用）；正文 4 节按节切 chunk（§1 低权重做主题概述、§2 表格权重最高做路径快速跳、§3 权重中等做约定理解、§4 权重最高做强制约束匹配）。因此 **4 节标题、5 字段**必须一字不差，不允许增删字段或更改节标题为近似词。卡片创建流程由 ai-orz-wiki-maintainer Skill 强制合规。

## 4. 约定与约束
- 召回链路永远是「知识卡 → source_files 源码/设计文档 → 长文」顺序，禁止反查长文再找卡。
- scope glob 不匹配用户传入文件集的卡 → 直接丢出召回候选集，不参与向量打分。
- 不允许在知识卡 §4「约定与约束」写"看长文 X 节"或"卡内容与长文对应"的互链语句——两套知识库不直接互相索引，关联是通过源码/设计文档的公共锚点隐性完成的。
- 若 RAG 命中卡后发现 source_files 路径**已被重命名/移动**，本次问答不报错、仍然使用该卡上下文，但要提醒后续"下次 wiki 同步时修复该卡的 source_files 路径"；绝不应该因路径漂移而把一张卡标记为"无效"直接丢出召回集。
- 新增知识卡时，YAML 的 `name == 目录名 == md 文件名`（三者必须完全相同，包括中文标点与括号），否则 IDE 文件扫描会失去该卡。
- scope 字段的 glob 数组不允许写 `['**']` 这个 catch-all（只有 2 张顶层模块 Overview 卡允许写 `['**']`）；独立主题卡必须收敛到 2-5 个真覆盖的 glob 模式，不然召回噪音会爆表。
- §4 必须至少 5 条 bullet，不多于 15 条；这是召回后 Agent 最直接读的一节，要写"能直接用"的操作约束，不要写概念。
- §2 表格行数 4-12 行；不要把 20+ 文件列进关键文件表（那 20+ 文件写在 source_files 锚点即可，表格是给人类快速扫入口用的）。
- 本卡自身（kind=rag_knowledge_index）的召回优先级：当用户问「如何使用知识卡 / 为什么召回不到 / RAG 怎么匹配 scope / 为什么有两张一样的卡 / source_files 写哪几个」这类**元问题**时，应该被高分命中；代码实现类问题不应该命中本卡。
