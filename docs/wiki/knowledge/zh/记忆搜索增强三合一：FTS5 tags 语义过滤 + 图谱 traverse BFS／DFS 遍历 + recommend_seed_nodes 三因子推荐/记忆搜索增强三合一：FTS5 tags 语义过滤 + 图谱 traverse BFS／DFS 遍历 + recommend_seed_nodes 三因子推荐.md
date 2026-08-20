---
kind: rag_card
name: 记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 三因子推荐
category: 知识架构
scope:
- src/service/dal/memory.rs
- src/service/dao/memory/**/*.rs
- src/service/dao/memory/*.rs
- src/service/dao/memory/vector.rs
- src/service/domain/runtime/memory.rs
- src/handlers/hr/agent/search_memory.rs
- src/handlers/hr/agent/recommend_seed_nodes.rs
- common/src/api/neural_tools.rs
- common/src/models/graph.rs
- src/models/agent.rs
- migrations/*memory_fts5.sql
source_files:
- src/service/dao/memory/sqlite.rs#L439-L520
- src/service/dao/memory/sqlite.rs#L884-L970
- src/service/dao/memory/vector.rs#L81-L124
- src/service/dal/memory.rs#L97-L177
- src/service/dal/memory.rs#L314-L380
- src/service/dal/memory.rs#L518-L576
- src/handlers/hr/agent/search_memory.rs#L24-L100
- common/src/models/graph.rs
- src/models/memory.rs
- migrations/
- docs/archive/design-archive/memory_search_enhancement_design.md
- docs/archive/design-archive/vector_search_architecture.md
- docs/archive/plan-archive/知识图谱推荐起点与组件复用重构.md
- docs/wiki/zh/content/项目概述/核心功能特性/四层记忆系统/长期记忆 (Long-term Memory)/知识关系管理.md
- docs/wiki/zh/content/项目概述/核心功能特性/综合搜索能力/知识图谱搜索.md
- docs/wiki/zh/content/项目概述/核心功能特性/四层记忆系统/短期记忆 (Short-term Memory)/记忆搜索机制.md
- docs/wiki/zh/content/架构设计/记忆系统架构.md
- docs/wiki/zh/content/功能模块/AI Agent 管理/记忆系统管理.md
- docs/wiki/zh/content/数据模型/消息和记忆模型/记忆和向量系统.md

---

# §1 概述（一句话定位 + 解决什么问题）

**定位**：记忆领域搜索能力三位一体——FTS5 关键词 + tags 语义过滤（SQLite unicode61 分词器 + BM25 评分）、知识图谱 traverse（BFS 层级返回 / DFS 栈批量预取 / IN 列表 400 分块防溢出）、recommend_seed_nodes 种子节点推荐（三因子打分 0.45 连通度 + 0.35 内容丰富度 + 0.2 分享权重），三者统一封装在 MemoryDal.search 入口，前端搜索/图谱页面/Agent 神经工具三条链路共享同一实现。

**解决三类存量缺陷**（对应 Design §1.1）：
1. **死代码**：`query_short_term` / `query_knowledge_nodes` 名义使用 FTS5 MATCH 但从未创建 FTS 虚拟表，实际调用报错
2. **质量差**：LIKE 搜索无分词/无排序/无多词，语义召回靠运气
3. **冷启动难**：知识图谱页面空画布用户不知从哪点起，无推荐起步锚点

---

# §2 关键文件与核心锚点速查表

| 文件锚点（点击跳转） | 角色 | 核心契约 / 红线 |
|---------------------|------|-----------------|
| [DAO FTS5 search_short_term](src/service/dao/memory/sqlite.rs#L439-L520) | 短期记忆关键词搜索底层 | FTS5 MATCH + BM25 fts_rank；unicode61 分词器；escape_fts5_keyword 必须转义用户输入特殊字符；空关键词直接返回空 |
| [DAO FTS5 search_knowledge_nodes](src/service/dao/memory/sqlite.rs#L884-L970) | 知识节点关键词搜索底层 | node_name/summary/node_description/tags 四字段索引；JOIN 知识节点 FTS 表 + tags JSON 语义过滤 |
| [DAL search 三合一合并排序](src/service/dal/memory.rs#L97-L177) | 三路结果合并入口 | Hybrid（双命中）→ Vector → Keyword 优先级；Hybrid/Vector 按 vector_distance 升序，Keyword 按 fts_rank 升序；MAX_SEARCH_RESULTS=20 上限 |
| [DAL recommend_seed_nodes 三因子打分](src/service/dal/memory.rs#L314-L380) | 种子节点推荐算法 | ① query_knowledge_nodes limit=500 上限保护 → ② list_relations_batch 一次拉关系 → ③ HashMap 入/出边度数 → ④ 连通度*0.45 + 内容丰富度*0.35 + 分享权重*0.2 → 倒序 truncate(limit) |
| [DAL traverse_knowledge_graph 图谱遍历](src/service/dal/memory.rs#L518-L576) | BFS/DFS 策略实现 | BFS levels 深度逐层返回 + visited 去重；DFS 栈批量预取 edge_cache + IN 列表 400 分块防 SQLite 999 参数溢出 |
| [Handler search_memory 参数解析](src/handlers/hr/agent/search_memory.rs#L24-L100) | HTTP + 神经工具双入口 | traversal_depth 默认 0（兼容旧调用）；traversal_strategy BFS/DFS；tags 过滤走 Vec<String> JSON 解析 |
| [搜索增强 Design 决策表](docs/archive/design-archive/memory_search_enhancement_design.md) | 为什么 / 关键决策 5 条 | §决策 1：unicode61 vs trigram 分词二分；§决策 3：关系搜索 JOIN 方式；§决策 4：向量阈值参数化 |
| [推荐起点 Plan 落地快照](docs/archive/plan-archive/知识图谱推荐起点与组件复用重构.md) | 怎么做 + 结果 | 5 步度数统计流程 + KnowledgeGraph 组件两端复用 |
| [知识图谱搜索 Wiki 长文](docs/wiki/zh/content/项目概述/核心功能特性/综合搜索能力/知识图谱搜索.md) | 人类百科 | §5 搜索算法详解 §8 故障排查 |

---

# §3 架构约定与数据流（业务语义层面，不贴实现代码）

**三位一体搜索总流**：
```
search_memory (Handler / 神经工具)
  ├─ keyword 非空  → DAO.search_short_term (FTS5 MATCH + BM25)
  │                 → DAO.search_knowledge_nodes (同上)
  │                 → tags 语义过滤：tags 与 keyword 相关性打分
  ├─ query_vector 非空 → VectorDao.search (HNSW/LanceDB，threshold 参数化默认 0.8)
  ├─ traversal_depth>0 → DAL.traverse_knowledge_graph (BFS/DFS)
  │                        seed_node_ids 为空 → 先语义搜索拿种子
  │                        IN 列表 400 分块 → edge_cache 批量预取
  └─ recommend_seed_nodes 独立入口 (前端图谱页专用)
       → limit 默认 5，min(limit, 50) 保护
       → agent_id=None 时 include_shared=true（含 published 全局节点）
       → 三因子打分排序后返回 SeedNodeRecommendation[]
                         │
                         ▼
            DAL 三路结果合并 + MatchType 打标签
            排序规则：Hybrid > Vector > Keyword
            组内排序：Hybrid/Vector vector_distance 升序；Keyword fts_rank 升序
            截断：MAX_SEARCH_RESULTS = 20
                         │
                         ▼
            SearchMatchInfo：match_type + fts_rank(Option) + vector_distance(Option)
            前端可显示相关性徽章；DAL 可据此再排序
```

**FTS5 索引表清单（记忆专属）**：
| FTS 虚拟表 | 索引字段 | 分词器 | 主表 | 同步方式 |
|-----------|---------|--------|-----|---------|
| short_term_memory_fts | summary, tags | unicode61 | short_term_memory_index | 6 个 AFTER 触发器（I/U/D × 2 表） |
| knowledge_node_fts | node_name, summary, node_description | unicode61 | long_term_knowledge_node | 同上 + migration 存量回填 |

**三条核心红线**（§4 完整列表见下节）：
1. **query 方法不做搜索**：keyword 传 query 只忽略并打 warn，搜索统一走 search_*
2. **escape_fts5_keyword 必加**：所有用户 keyword 必须经转义，* " ( ) : 作为字面量禁止作为 FTS5 语法操作符
3. **traverse IN 分块 400 红线**：SQLite 参数上限 999，查关系时 node_ids 每 400 一组 chunked 分批，禁止单条 SQL 把 500 个节点 ID 塞进 IN(...)

---

# §4 硬约束 / 必守红线 / 扩展入口

**§4.1 必守红线（9 条，违反 = FAIL）**

| # | 红线 | 验证方式 | 代码锚点 |
|---|------|---------|---------|
| 1 | **FTS5 空关键词保护**：keyword 为空字符串时，FTS5 分支直接 return Ok(vec![])；禁止传 `MATCH ''`（SQLite 语法错误） | 空关键词搜索集成测试断言不报错 | [sqlite.rs search_short_term 入口 if keyword.is_empty()](src/service/dao/memory/sqlite.rs#L439-L445) |
| 2 | **escape_fts5_keyword 必调用**：所有用户输入 keyword 必须走转义函数处理 FTS5 特殊字符；禁止把用户原始字符串直接拼入 MATCH 表达式 | grep 所有 MATCH 拼接处前必须有 escape_fts5_keyword 调用 | [sqlite.rs escape_fts5_keyword](src/service/dao/memory/sqlite.rs) + 两处 search 入口调用 |
| 3 | **Hybrid 判定标准**：同一条记忆 ID 同时出现在 FTS5 结果集 AND 向量结果集中 → MatchType=Hybrid，同时携带 vector_distance 和 fts_rank | Hybrid 单元测试：构造一个双命中断言 match_type=Hybrid | [dal/memory.rs search 合并逻辑](src/service/dal/memory.rs#L140-L177) |
| 4 | **MAX_SEARCH_RESULTS=20 上限**：search 最终结果 >20 强制 truncate(20)；禁止返回全量让前端截断（Token 与性能红线） | 搜索 >20 条匹配时断言 items.len()=20 | [dal/memory.rs search 末尾 truncate](src/service/dal/memory.rs#L170-L177) |
| 5 | **recommend_seed_nodes DAL 层 500 节点保护**：query_knowledge_nodes 的 limit 参数硬编码 500；禁止无上限拉节点 | grep query_knowledge_nodes limit 参数必须 Some(500) | [dal/memory.rs#L314-L320](src/service/dal/memory.rs#L314-L320) |
| 6 | **recommend_seed_nodes handler 层 50 保护**：用户 limit 参数 unwrap_or(5).min(50)；禁止传 1000 拉爆前端卡片渲染 | 集成测试传 limit=999 断言返回 ≤50 条 | [handlers/recommend_seed_nodes.rs](src/handlers/hr/agent/recommend_seed_nodes.rs) |
| 7 | **agent_id=None 全局推荐必须 include_shared=true**：全局推荐池含 published 跨 Agent 共享节点；空结果率高是严重体验缺陷 | agent_id=None 时 MemoryQuery.include_shared = true（grep 验证） | [dal/memory.rs recommend_seed_nodes agent_id match None 分支](src/service/dal/memory.rs#L320-L330) |
| 8 | **traverse IN 分块 400 红线**：list_relations_batch(node_ids) 必须 chunks(400) 分批；禁止单 IN 列表超过 400 | chunks(400) grep 必须存在 | [dal/memory.rs traverse_knowledge_graph IN 部分](src/service/dal/memory.rs#L530-L545) |
| 9 | **DFS 栈 + edge_cache 批量预取**：DFS 策略禁止 N+1 查询（对每个节点单独查出入边）；必须用 stack.push 批量预取 + edge_cache 记录已查节点 ID 跳过重复 | 集成测试断言 DFS 遍历 1000 节点 SQL 查询数 < 节点数 / 100 | [dal/memory.rs DFS 分支](src/service/dal/memory.rs#L545-L570) |
| 10 | **tags 过滤永远是 AND 不是 OR**：多标签多选组件需注明「同时满足」，OR 语义交关键词/向量搜索覆盖，避免能力重叠 | 集成测试 tags=["红色", "蓝色"] 只返回同时含两者的节点 | [dao/memory/sqlite.rs search_knowledge_nodes tags IN 过滤](src/service/dao/memory/sqlite.rs#L884-L970) |
| 11 | **is_published 冗余字段只在节点写入时同步**：tags 含 "public"/"published" 自动置 1；读查询走 is_published 字段 + 部分索引，禁止读查询扫全表 json_each(tags) 判断 | grep 所有 SELECT 子句，不应出现 `WHERE EXISTS(SELECT 1 FROM json_each(tags) ...='public')` 过滤 | [migrations DDL idx_ltkn_is_published](migrations/) 部分索引 |
| 12 | **BFS 遍历 max_depth clamp(1,6)**：调用方传 1000 也强制截断到 6，防止全图扫爆 IO | 集成测试传 max_depth=999 → clamp 生效实际只遍历 ≤ 6 层 | [dal/memory.rs traverse入口 max_depth 保护](src/service/dal/memory.rs#L518-L530) |
| 13 | **推荐 reasons 文案不能暴露内部量纲**：只输出「高连接度：关联 18 个节点」这类自然语言，禁止把 connectivity_score=0.9213 这种内部归一化值直接暴露给前端 | grep SeedNodeRecommendation.reasons，不应出现 0-1 的小数 | [dal/memory.rs recommend_seed_nodes 生成 reasons](src/service/dal/memory.rs#L355-L375) |
| 14 | **tags 过滤 SQL 必须用占位符拼接防注入**：禁止直接 `format!("AND json_each.value IN ({})", tags.join(","))`；placeholders 数量与 tags 长度动态生成 + bind 参数 | 故意构造 tags = "'); DROP TABLE" 应返回空不报错 | [dao/memory/sqlite.rs search_knowledge_nodes tags 过滤](src/service/dao/memory/sqlite.rs#L884-L970) |
| 15 | **空关键词短路**：keyword.trim().is_empty() 且 query_vector=None 时，必须返回空 Vec；禁止啥都不填就返回全表数据导致前端卡顿 + DB 扫表 | 空关键词 + 无向量测试断言 items.len()=0 | [sqlite.rs search_short_term/search_knowledge_nodes](src/service/dao/memory/sqlite.rs#L439-L445) 入口 if is_empty() 分支 |
| 16 | **节点写入后 tags 变更需同步 is_published 刷新 + 推荐缓存失效**：任何节点/边写入或删除 AOP 事件，recommend_seed_nodes 内存缓存（如未来接入）立即失效；否则新节点推荐不到 | tags 更新 DAO 更新后同步 is_published；AOP 订阅写入事件 | [dao/memory/sqlite.rs UPDATE tags 与 is_published 同步逻辑](src/service/dao/memory/sqlite.rs) |

**§4.2 扩展入口速查**

| 扩展需求 | 改动位置（N 处同步） | 参考锚点 |
|---------|---------------------|---------|
| 新增 FTS5 可索引字段（如 extra_tags） | ① migration：重建 FTS 虚拟表（ALTER 不支持）+ 触发器追加 → ② DAO search 方法 MATCH 字段列表追加 → ③ Vectorizable::vectorize_text 追加对应字段（确保向量语义对齐） | [migrations/20260712000000_memory_fts5.sql](migrations/20260712000000_memory_fts5.sql) |
| 新增第四路搜索（tags 精确集合匹配） | ① MemoryDao 新增 list_by_tags_exact(tags) 原子方法 → ② DAL search() 顶部先取候选集 → ③ 与现有三路结果求交集（不破坏原有 Hybrid/Vector/Keyword 排序规则） | [dao/memory/mod.rs](src/service/dao/memory/mod.rs) trait 追加方法 |
| 推荐策略新增维度（如时间衰减热度 / PageRank） | ① RecommendSeedNodesParams 加 ranking_mode 枚举 → ② DAL HashMap 聚合阶段计算 recency_score / pagerank_score → ③ sort_by 改为复合排序 → ④ 前端推荐区下拉切换 ranking_mode | [dal/memory.rs#L345-L375](src/service/dal/memory.rs#L345-L375) |
| traverse 新增策略（如 Personalized PageRank 随机游走） | ① TraversalStrategy 枚举追加 PPR 变体 → ② DAL traverse_knowledge_graph match 追加分支 → ③ neural_tools.rs SearchMemoryParams.traversal_strategy 枚举同步 → ④ 前端 KnowledgeGraph 策略切换下拉 | common/src/enums/memory.rs TraversalStrategy 定义位置 |
