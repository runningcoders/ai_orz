<!-- 本文件由 tools/wiki_index.py 自动生成，请勿手工编辑；改内容请改源文件后跑 `make wiki-index` -->

# 08-搜索

> 模块索引。A = 设计（为什么）/ B = 落地方案（要去哪）/ C = 知识（是什么）。
>
> 本文件自动生成：改内容请改源文件后跑 `make wiki-index`。

本模块共 36 篇：A 设计 5 · B 落地方案 10 · C 知识 21。

## A 设计（design）

- [实体列表/查询/搜索接口设计规范](<A-设计/entity_list_query_search_design.md>) — `已废弃`
- [全实体 FTS5 全文搜索统一设计](<A-设计/full_entity_fts5_search_design.md>) — `已废弃`
- [记忆搜索 FTS5 增强与综合搜索设计](<A-设计/memory_search_enhancement_design.md>) — `已废弃`
- [本体论驱动知识沉淀设计（Ontology-Driven Knowledge Sedimentation）](<A-设计/ontology_knowledge_sedimentation_design.md>) — `现行`
- [向量搜索与混合搜索架构设计文档](<A-设计/vector_search_architecture.md>) — `已废弃`

## B 落地方案（plan）

- [Agent搜索查询三场景统一重构](<B-落地方案/Agent搜索查询三场景统一重构.md>) — `已完成`
- [HNSW持久化与索引重建异步化重构](<B-落地方案/HNSW持久化与索引重建异步化重构.md>) — `已完成`
- [Project 实体搜索/查询接口统一实施计划](<B-落地方案/Project 实体搜索查询接口统一实施计划（执行蓝图）.md>) — `已完成`
- [Search and Knowledge Graph Implementation Plan](<B-落地方案/Search and Knowledge Graph（执行蓝图）.md>) — `已完成`
- [Task 实体搜索/查询接口统一实施计划](<B-落地方案/Task 实体搜索查询接口统一实施计划（执行蓝图）.md>) — `已完成`
- [向量搜索 Pre-Filter 改造：Payload 落库与谓词下推 实施计划](<B-落地方案/向量搜索Pre-Filter改造.md>) — `进行中`
- [向量搜索增强计划](<B-落地方案/向量搜索增强计划（执行蓝图）.md>) — `已完成`
- [知识图谱遍历查询优化（DFS 批量化 + IN 分块）](<B-落地方案/图谱遍历查询优化.md>) — `已完成`
- [本体论知识沉淀落地](<B-落地方案/本体论知识沉淀落地.md>) — `草案`
- [统一其他实体搜索/查询接口实施计划](<B-落地方案/统一其他实体搜索查询接口实施计划（执行蓝图）.md>) — `已完成`

## C 知识（wiki）

- [Canvas HUD 可视化：GraphCanvas 知识图谱 + 图表场景LineDonut + 仪表盘Gauge双版 + HudPalette橙光光晕](<C-知识/Canvas HUD 可视化：GraphCanvas 知识图谱 + 图表场景LineDonut + 仪表盘Gauge双版 + HudPalette橙光光晕/Canvas HUD 可视化：GraphCanvas 知识图谱 + 图表场景LineDonut + 仪表盘Gauge双版 + HudPalette橙光光晕.md>)
- [Embedding Provider 生命周期与重建触发条件](<C-知识/Embedding Provider 生命周期：ModelProviderStatus Disabled(2) + 创建不阻塞策略 + 重建触发条件矩阵/Embedding Provider 生命周期：ModelProviderStatus Disabled(2) + 创建不阻塞策略 + 重建触发条件矩阵.md>)
- [Entity Query List Search 三分查询模式：push_query_filters 复用 WHERE + PagedResult T](<C-知识/Entity Query List Search 三分查询模式：push_query_filters 复用 WHERE + PagedResult T map 全链路 + list query search 三 Handler 职责二分/Entity Query List Search 三分查询模式：push_query_filters 复用 WHERE + PagedResult T map 全链路 + list query search 三 Handler 职责二分.md>)
- [FTS5 关键词搜索](<C-知识/FTS5 关键词搜索.md>)
- [PromptBuilder 工作空间与本体词表注入 + 前缀缓存优化：workspace_context, ontology_lexicon, 稳定性递减排序](<C-知识/PromptBuilder 工作空间与本体词表注入 + 前缀缓存优化：workspace_context, ontology_lexicon, 稳定性递减排序/PromptBuilder 工作空间与本体词表注入 + 前缀缓存优化：workspace_context, ontology_lexicon, 稳定性递减排序.md>)
- [RAG 知识索引：如何使用知识卡片做召回检索、锚定与 scope 匹配](<C-知识/RAG 知识索引：如何使用知识卡片做召回检索、锚定与 scope 匹配/RAG 知识索引：如何使用知识卡片做召回检索、锚定与 scope 匹配.md>)
- [SQLite + SQLx 0.8 存储工程规范：STRICT 表模式 + FTS5 全文检索 + 枚举 i32 类型安全 + .sqlx 离线构建 + sqlx::test 隔离](<C-知识/SQLite + SQLx 0.8 存储工程规范：STRICT 表模式 + FTS5 全文检索 + 枚举 i32 类型安全 + .sqlx 离线构建 + sqlx::test 隔离/SQLite + SQLx 0.8 存储工程规范：STRICT 表模式 + FTS5 全文检索 + 枚举 i32 类型安全 + .sqlx 离线构建 + sqlx::test 隔离.md>)
- [全文搜索（框架层）](<C-知识/全文搜索.md>)
- [向量存储抽象 + Vectorizable trait 统一索引](<C-知识/向量存储抽象 VectorStore + 多后端 + Vectorizable trait 统一索引入口 + embed_entity/向量存储抽象 VectorStore + 多后端 + Vectorizable trait 统一索引入口 + embed_entity.md>)
- [向量语义搜索](<C-知识/向量语义搜索.md>)
- [本体属性层：ontology_properties TBox 第四张词表 + knowledge_property EAV 版本化 ABox + 四类型列软门禁写入 + property_filters 结构化反查](<C-知识/本体属性层：ontology_properties TBox 第四张词表 + knowledge_property EAV 版本化 ABox + 四类型列软门禁写入 + property_filters 结构化反查/本体属性层：ontology_properties TBox 第四张词表 + knowledge_property EAV 版本化 ABox + 四类型列软门禁写入 + property_filters 结构化反查.md>)
- [本体属性管理](<C-知识/本体属性管理.md>)
- [本体词表与漂移治理](<C-知识/本体词表与漂移治理.md>)
- [本体词表漂移治理与 Prompt 注入：OntologyDal load_lexicon_summary + common 纯函数解析 + 写后认证 + DuckDB 漂移记账](<C-知识/本体词表漂移治理与 Prompt 注入：OntologyDal load_lexicon_summary + common 纯函数解析 + 写后认证 + DuckDB 漂移记账/本体词表漂移治理与 Prompt 注入：OntologyDal load_lexicon_summary + common 纯函数解析 + 写后认证 + DuckDB 漂移记账.md>)
- [混合搜索引擎](<C-知识/混合搜索引擎.md>)
- [知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出](<C-知识/知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出/知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出.md>)
- [知识图谱全局点线视图：get_knowledge_graph 全量聚合端点 + R1R2R4 口径 + 边方向服务端 resolve + 双渲染器箭头](<C-知识/知识图谱全局点线视图：get_knowledge_graph 全量聚合端点 + R1R2R4 口径 + 边方向服务端 resolve + 双渲染器箭头/知识图谱全局点线视图：get_knowledge_graph 全量聚合端点 + R1R2R4 口径 + 边方向服务端 resolve + 双渲染器箭头.md>)
- [知识图谱可视化](<C-知识/知识图谱可视化.md>)
- [知识图谱搜索](<C-知识/知识图谱搜索.md>)
- [知识图谱联想与关系边补写：update_memory relations 唯一建边入口 + 正文引用对遍历不可达 + 沉淀探索引导](<C-知识/知识图谱联想与关系边补写：update_memory relations 唯一建边入口 + 正文引用对遍历不可达 + 沉淀探索引导/知识图谱联想与关系边补写：update_memory relations 唯一建边入口 + 正文引用对遍历不可达 + 沉淀探索引导.md>)
- [记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 度数排序推荐](<C-知识/记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 三因子推荐/记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 三因子推荐.md>)
