---
kind: RAG 原子知识卡
name: 本体词表漂移治理与 Prompt 注入：OntologyDal load_lexicon_summary + common 纯函数解析 + 写后认证 + DuckDB 漂移记账

category: 知识图谱与记忆沉淀 / 本体治理
scope:
  - "src/service/dao/ontology/**"
  - "src/service/dal/ontology.rs"
  - "src/service/domain/hr/ontology.rs"
  - "src/handlers/hr/ontology/**"
  - "src/handlers/system/seed/sync_preset_ontology.rs"
  - "src/models/ontology.rs"
  - "src/pkg/stats/ontology_drift.rs"
  - "src/consumer/ontology_certify_consumer.rs"
  - "common/src/ontology.rs"
  - "common/src/enums/ontology.rs"
  - "common/src/api/ontology.rs"
  - "src/service/dal/agent/builder/default.rs"
  - "src/models/prompt_builder.rs"
  - "frontend/src/pages/hr/ontology_panel.rs"
source_files:
  - src/service/dao/ontology/mod.rs
  - src/service/dal/ontology.rs#L62-L124
  - src/service/domain/hr/ontology.rs#L545-L573
  - src/service/domain/runtime/awakening.rs#L343-L393
  - src/models/prompt_builder.rs#L135-L146
  - src/service/dal/agent/builder/default.rs#L145-L230
  - src/consumer/ontology_certify_consumer.rs#L1-L80
  - src/pkg/stats/ontology_drift.rs#L1-L40
  - src/models/ontology.rs#L19-L240
  - common/src/ontology.rs#L1-L120
  - src/handlers/system/seed/sync_preset_ontology.rs#L1-L45
  - frontend/src/pages/hr/ontology_panel.rs#L114-L438
  - frontend/src/pages/hr/ontology_panel.rs (2026-09 3 fix：GET #[param(query)] 补齐、use_effect 同步段移入 spawn、Agent 详情页神经技能包筛选 fix)
  - docs/design/ontology_knowledge_sedimentation_design.md
  - docs/wiki/zh/content/功能模块/知识图谱管理/本体词表与漂移治理.md
  - 【平行卡 1】docs/wiki/knowledge/zh/知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出/知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出.md
  - 【平行卡 2】docs/wiki/knowledge/zh/记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 三因子推荐/记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 三因子推荐.md
---

# 本体词表漂移治理与 Prompt 注入

## §1 整体方案

本体（Ontology）被定位为知识图谱沉淀方法论的**控制落点**：本体与知识图谱合一（TBox/ABox 分层），TBox 是三段沉淀闭环的基石。三段闭环：

1. **感知**（写前引导）：神经技能提示词注入当前本体词表（OntologyDomain.list_lexicon → PromptBuilder.ontology_lexicon → 渲染【本体词表】区块），Agent 沉淀前就知道有哪些规范词
2. **引导**（写时提示）：同义样例告诉 Agent "我想表达的关系/节点，规范词可能长这样"——不做硬校验（漂移入口保留），做软引导
3. **治理**（写后认证 + 漂移记账）：runtime 尾部 publish(MemoryTermsWritten) → OntologyCertifyConsumer（Async）异步执行 certify + DuckDB 漂移记账。认证通过 → `is_published` 置位（质量认证）；漂移 → `needs_review` 软门禁（不拦截不撤回）

漂移**不物化**（惰性解析纯函数）：今天漂移的词，管理员明天加同义映射或收编为新词，下一次解析自动愈合——历史漂移记录原文（OntologyDriftEvent 只记 raw_term），不入图谱不入 SQLite，避免双写不一致。

**PromptBuilder 词表注入设计要点**：区块位置固定为第 4 块（人设 → 神经技能 → 必加载技能 → **本体词表** → 用户画像 → ... → trace_id + 当前消息），稳定性递减排序保证词表区块跨次沉淀运行逐字节相同（前缀缓存友好）。预算 3000 chars，裁剪优先级：关系词（行形态 `LexiconTermSummary`）> 实体类 > 同义样例。

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键结构/入口 |
|------|------|--------------|
| [src/service/dao/ontology/mod.rs](src/service/dao/ontology/mod.rs) | OntologyDao trait + 查询参数 | OntologyClassQuery / OntologyRelationTypeQuery / OntologySynonymQuery；insert_class / query_classes / find_class_by_term_key（term_key 语义锚点查找） |
| [src/models/ontology.rs](src/models/ontology.rs) | PO 定义 | OntologyClassPo(term_key/display_name/description/required_fields/status) / OntologyRelationTypePo(target_classes/range_classes) / OntologySynonymMappingPo(raw_term/target_key)；三表全局不带 organization_id |
| [src/service/dal/ontology.rs#L62-L124](src/service/dal/ontology.rs#L62-L124) | OntologyDal 单例 + trait | try_dal() 可选依赖降级；active_all_pagination() 大 limit 模拟全量（词表量级几十条）；warn_if_lexicon_truncated() 截断留痕；**load_lexicon_summary()** 三表 Active 全量 → OntologyLexiconSummary（classes + relation_types + synonyms） |
| [src/service/domain/hr/ontology.rs#L545-L573](src/service/domain/hr/ontology.rs#L545-L573) | OntologyDomain 业务逻辑 | **certify_memory_terms()** 运行时写后认证（ResolvedTerm 三态：Canonical/ViaSynonym/Drift）；**list_lexicon()** 词表注入视图（调 OntologyDal.load_lexicon_summary） |
| [common/src/ontology.rs](common/src/ontology.rs) | 纯函数解析层（跨 crate 共享） | **TermKind** 枚举（Class/Relation）；**normalize()** trim + ASCII 小写；**resolve()** 漂移判定：先查 synonyms→Canonical/ViaSynonym，再查 term_key→Canonical，都无→Drift；OntologyLexiconSummary(classes/relation_types/synonyms)；**漂移是惰性计算**，无 IO 无副作用 |
| [src/consumer/ontology_certify_consumer.rs#L1-L80](src/consumer/ontology_certify_consumer.rs#L1-L80) | 写后认证消费者（Async） | 订阅 `memory.terms.written`；调 OntologyDomain.certify_memory_terms（消费者→Domain 合法方向）+ 写 OntologyDriftEvent 到 DuckDB；异步不阻塞写路径（对齐 design 决策 #16）|
| [src/pkg/stats/ontology_drift.rs#L1-L40](src/pkg/stats/ontology_drift.rs#L1-L40) | 漂移统计事件 | OntologyDriftEvent（StatsEvent derive）：timestamp/agent_id/kind/raw_term/caller_organization_id；**只记原文，不解析结论**（解析随本体进化过期，原文不可变）|
| [src/handlers/system/seed/sync_preset_ontology.rs#L1-L45](src/handlers/system/seed/sync_preset_ontology.rs#L1-L45) | 种子词表同步 | GET preview（只读对比缺口）/ POST sync（仅补缺注入）；策略固定「仅补缺」（term_key 物理存在即跳过，不覆盖管理页本地修改）；走同步返回（量级几十条毫秒级，不走后台任务）|
| [frontend/src/pages/hr/ontology_panel.rs#L114-L438](frontend/src/pages/hr/ontology_panel.rs#L114-L438) | 前端漂移看板 + 词表管理 | HrOntologyLexicon 组件：漂移看板（DuckDB GetDriftDashboard 聚合）+ 实体类/关系类型/同义映射三 tab 管理 + 种子同步预览/注入；漂移看板展示 relation/class 漂移覆盖率、TOP 漂移词 |
| 【Wiki 长文】本体词表与漂移治理.md | 系统化上下文（10 章） | [docs/wiki/zh/content/功能模块/知识图谱管理/本体词表与漂移治理.md](docs/wiki/zh/content/功能模块/知识图谱管理/本体词表与漂移治理.md) |
| 【① Design】ontology_knowledge_sedimentation_design.md | 决策快照（16 条关键决策）| [docs/design/ontology_knowledge_sedimentation_design.md](docs/design/ontology_knowledge_sedimentation_design.md) |
| 【平行卡 1】知识图谱 traverse | ABox 遍历搜索视角（与本卡 TBox 词表治理视角互补）| [知识图谱 traverse 卡](docs/wiki/knowledge/zh/知识图谱%20traverse：BFS%20levels%20深度返回%20+%20DFS%20栈批量预取%20edge_cache%20+%20IN%20列表%20400%20分块防%20999%20溢出/知识图谱%20traverse：BFS%20levels%20深度返回%20+%20DFS%20栈批量预取%20edge_cache%20+%20IN%20列表%20400%20分块防%20999%20溢出.md) |
| 【平行卡 2】记忆搜索增强三合一 | FTS5 + 向量 + 图谱 搜索三合一（与本卡漂移治理互补）| [记忆搜索增强三合一卡](docs/wiki/knowledge/zh/记忆搜索增强三合一：FTS5%20tags%20语义过滤%20+%20图谱%20traverse%20BFS%EF%BC%8FDFS%20遍历%20+%20recommend_seed_nodes%20三因子推荐/记忆搜索增强三合一：FTS5%20tags%20语义过滤%20+%20图谱%20traverse%20BFS%EF%BC%8FDFS%20遍历%20+%20recommend_seed_nodes%20三因子推荐.md) |
| [src/service/domain/runtime/awakening.rs#L343-L393](src/service/domain/runtime/awakening.rs#L343-L393) (2026-09 增量) | 词表注入运行时调用方 | `OntologyDal::try_dal()` → `load_lexicon_summary` → `builder.ontology_lexicon()`；同时调 workspace_context / compacted_context 等 |
| [src/models/prompt_builder.rs#L135-L146](src/models/prompt_builder.rs#L135-L146) (2026-09 增量) | PromptBuilder.trait ontology_lexicon 方法 | trait 新增 `ontology_lexicon(&OntologyLexiconSummary)` 默认空操作；Remote/Cli Agent 不参与词表注入 |
| [src/service/dal/agent/builder/default.rs#L145-L230](src/service/dal/agent/builder/default.rs#L145-L230) (2026-09 增量) | DefaultPromptBuilder 词表渲染 | `build_lexicon_section()` 预算 3000 chars；裁剪优先级关系词 > 实体类 > 同义样例；区块固定为第 4 块稳定性递减排序 |

## §3 架构约定

1. **本体三表全局无 org_id**：`ontology_classes / ontology_relation_types / ontology_synonym_mappings` 不带 organization_id（对齐 memory 域先例），单部署单库即作用域边界；`term_key` 语义锚点保证未来多租户迁移
2. **词表量级为个位数十词**：全量载入成本可忽略，OntologyDal.load_lexicon_summary 一次性拉取 Active 全量到内存
3. **漂移是惰性计算**：common::ontology::resolve() 纯函数（trim + ASCII 小写归一化 → 查 synonyms → 查 term_key），无 IO 无副作用；**不在图谱表或 SQLite 物化漂移标记**（本体进化后历史漂移自动愈合）
4. **写后认证走 AOP Async 管道**：runtime 尾部只 publish(MemoryTermsWritten)，不调 Domain/DAL/Stats；OntologyCertifyConsumer（Async 模式）内完成 certify + Stats 打点；写路径零耦合零阻塞
5. **词表注入可选依赖**：PromptBuilder.ontology_lexicon 默认实现空操作；OntologyDal.try_dal() 返回 Option（未初始化 → None → 跳过注入），本体子系统未初始化不阻断主流程
6. **种子同步策略固定「仅补缺」**：term_key 物理存在（含退役行）即跳过，不覆盖管理页本地修改（管理页是词表唯一修改入口）
7. **漂移看板数据来源**：DuckDB 漂移事件聚合（ontology_drift_events 表），展示 relation/class 漂移覆盖率、TOP 漂移词、趋势
8. **（2026-09 运行时落地）词表注入链路固定为 Domain 统一门面**：`awakening.rs` → `OntologyDal::try_dal()` → `load_lexicon_summary()` → `OntologyDomain.list_lexicon()` → `OntologyLexiconSummary` → `builder.ontology_lexicon()` → `build_lexicon_section()` 渲染。handler 层禁止自行拼接词表或跳过 OntologyDal 直查三表。PromptBuilder.trait.ontology_lexicon 默认空操作，Remote/Cli Agent 自动跳过

## §4 约束清单

1. ✅ 本体三表不带 organization_id（全局共享）；term_key 是语义锚点，跨表/跨环境引用一律用它
2. ✅ 漂移不物化（惰性解析纯函数）——历史漂移随本体进化自动愈合
3. ✅ 写后认证走 AOP Async 管道，写路径零耦合零阻塞
4. ✅ DuckDB OntologyDriftEvent **只记原文，不解析结论**（解析随本体进化过期，原文不可变）
5. ✅ Prompt 词表注入预算 3000 chars；裁剪优先级固定：关系词 > 实体类 > 同义样例
6. ✅ Prompt 词表区块位置固定为第 4 块（稳定性递减排序，前 3 块更稳定）
7. ✅ 种子词表同步仅补缺，不覆盖管理页本地修改；走同步返回（量级几十条毫秒级）
8. ✅ common/src/ontology.rs 是跨 crate 共享的纯函数层（domain 校验 / DAL 看板 / 统计消费），禁止引入 IO/时钟/副作用
9. ✅ OntologyDal.try_dal() 供可选依赖场景优雅降级（本体子系统未初始化不阻断主流程）
10. ✅ warn_if_lexicon_truncated() 在词表量级正常时永不触发（正常几十条，上限 1000）；触发即异常打 warn 提醒
11. ✅ （2026-09-XX 新增）本体词表运行时注入必须走 `OntologyDal.load_lexicon_summary` → `OntologyDomain.list_lexicon` → `common::ontology` 纯函数解析链路；handler 层不得自行拼接词表或跳过 OntologyDal 直查三表

## §5 历史演进

| 日期 | commit | 事件 | 变更内容 |
|------|--------|------|---------|
| 2026-08 之前 | 初版 | 本体词表仅用于写后认证（OntologyCertifyConsumer certify_memory_terms），未注入 Prompt | src/consumer/ontology_certify_consumer.rs；src/service/domain/hr/ontology.rs |
| 08c3e720 → 14a4b688 → db46a354 → 5b52c72c | 2026-09 | PromptBuilder ontology_lexicon trait 方法 + DefaultPromptBuilder.build_lexicon_section 3000 chars 预算；区块前移为第 4 块（稳定性递减排序）；运行时注入链路 awakening → OntologyDal → OntologyDomain → PromptBuilder 完整落地 | src/models/prompt_builder.rs；src/service/dal/agent/builder/default.rs#L145-L230；src/service/domain/runtime/awakening.rs#L343-L393 |
| abec62c0 + 9c7ba27f + c49317b8 | 2026-09 | 前端本体管理页 3 fix：① GET 请求 #[param(source = "query")] 补齐 ② use_effect 同步段移入 spawn 异步段 ③ Agent 详情页神经技能包筛选 fix | frontend/src/pages/hr/ontology_panel.rs |
