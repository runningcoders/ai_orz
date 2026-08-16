# 记忆搜索 FTS5 增强与综合搜索设计

> 📦 归档标记（2026-08-16）：归档冻结。保留原因：memory_search_enhancement_design 设计文档归档冻结，设计决策已沉淀至 wiki 长文。生效方案：见源码和 wiki 长文。

> 状态：定稿（2026-07-12 功能落地）
> 查阅场景：理解记忆搜索的 FTS5/向量/图谱关系组合方式、调优搜索排序、排查关键词搜索异常时打开。
> 关联文档：
> - [memory_design.md](../memory_design.md) — 记忆系统四层认知结构
> - [full_entity_fts5_search_design.md](./full_entity_fts5_search_design.md) — 全实体统一搜索标准（本设计是其超集）
> - [vector_search_architecture.md](./vector_search_architecture.md) — HNSW 向量搜索底层
> - [memory_system_enhancement_design.md](./memory_system_enhancement_design.md) — 工具拆分 + 定时沉淀设计（搜索增强的上层能力设计）
> - 【② Plan 落地（真实定稿 2 张）】
>   - [图谱遍历查询优化.md](../plan/图谱遍历查询优化.md) — traverse BFS/DFS IN 分块 400 + 栈批量预取（含 5 个新增测试场景）
>   - [知识图谱推荐起点与组件复用重构.md](../plan/知识图谱推荐起点与组件复用重构.md) — recommend_seed_nodes + 前端 KnowledgeGraph 组件两端复用
> - 【③ Wiki 长文 ≥3 篇（Batch11 精确对齐）】
>   - [知识关系管理.md](docs/wiki/zh/content/项目概述/核心功能特性/四层记忆系统/长期记忆%20(Long-term%20Memory)/知识关系管理.md) — 知识关系 + traverse BFS/DFS 遍历链路 + 图谱冷启动种子推荐
>   - [知识图谱搜索.md](docs/wiki/zh/content/项目概述/核心功能特性/综合搜索能力/知识图谱搜索.md) — FTS5 关键词 + 向量 + 图谱关系三位一体综合搜索排序
>   - [记忆系统管理.md](docs/wiki/zh/content/功能模块/AI%20Agent%20管理/记忆系统管理.md) — 记忆搜索接口管理 + 推荐面板 reasons 展示
> - 【④ RAG 原子知识卡（Batch11 精确对应 1 张 + 横向关联 2 张）】
>   - [记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 三因子推荐](docs/wiki/knowledge/zh/记忆搜索增强三合一：FTS5%20tags%20语义过滤%20+%20图谱%20traverse%20BFS%2FDFS%20遍历%20+%20recommend_seed_nodes%20三因子推荐/记忆搜索增强三合一：FTS5%20tags%20语义过滤%20+%20图谱%20traverse%20BFS%2FDFS%20遍历%20+%20recommend_seed_nodes%20三因子推荐.md) — 三合一总卡（FTS5 搜索 + tags 过滤 + traverse 分块 + 三因子打分 + MAX_SEARCH_RESULTS 20 红线）
>   - [知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出](docs/wiki/knowledge/zh/知识图谱%20traverse：BFS%20levels%20深度返回%20+%20DFS%20栈批量预取%20edge_cache%20+%20IN%20列表%20400%20分块防%20999%20溢出/知识图谱%20traverse：BFS%20levels%20深度返回%20+%20DFS%20栈批量预取%20edge_cache%20+%20IN%20列表%20400%20分块防%20999%20溢出.md) — §1 决策 2 遍历位置 + BFS/DFS 具体实现细节
>   - [recommend_seed_nodes 种子节点推荐：三因子打分 0.45 连通度 0.35 内容丰富度 0.2 分享权重 + KnowledgeGraph 组件两端复用](docs/wiki/knowledge/zh/recommend_seed_nodes%20种子节点推荐：三因子打分%200.45%20连通度%200.35%20内容丰富度%200.2%20分享权重%20+%20KnowledgeGraph%20组件两端复用/recommend_seed_nodes%20种子节点推荐：三因子打分%200.45%20连通度%200.35%20内容丰富度%200.2%20分享权重%20+%20KnowledgeGraph%20组件两端复用.md) — 三因子算法 + 前端 KnowledgeGraph 组件复用拆分

---

## 一、设计目标与关键决策

### 问题背景

记忆系统搜索存在三类存量缺陷：

| 问题 | 严重度 | 说明 |
|-----|-------|-----|
| MATCH 死代码 | P0 阻塞 | `query_short_term` / `query_knowledge_nodes` 使用了 FTS5 MATCH 语法，但从未创建 FTS 虚拟表，实际调用运行时报错 |
| LIKE 能力不足 | P0 质量 | 无分词、无相关性排序、无多词搜索、性能退化 |
| 综合搜索不完整 | P1 体验 | 关系搜索是空实现（原代码未补全）；`MatchType::Keyword` 从未实际使用；向量阈值硬编码 0.8 |

### 关键决策表

| # | 决策问题 | 选择方案 | 选择原因 |
|---|---------|---------|---------|
| 1 | FTS5 分词器选型 | **unicode61（记忆）+ trigram（全实体）二分** | 记忆字段以英文标签和摘要为主，unicode61 分词 + FTS5 默认 AND 语义精度更高；全实体 search 用 trigram 兼容中文 |
| 2 | 死代码清理策略 | **query 方法移除关键词能力，keyword 统一走 search 方法** | 分离「查询」（按条件过滤，如时间/类型）和「搜索」（关键词 + 向量）；避免两接口重复 |
| 3 | 关系搜索实现方式 | **通过 JOIN 知识节点 FTS 搜索节点，再取关联出入边** | 关系本身无文本字段，按关系两端节点文本间接搜索；无需额外关系 FTS 表 |
| 4 | 向量阈值 | **硬编码 → MemorySearch.vector_distance_threshold 可选参数** | 默认 0.8（现有行为不变）；高精度场景可调 0.6~0.7，高召回场景可调 0.9 |
| 5 | SearchMatchInfo 扩展 | **新增 fts_rank: Option<f32>** | Keyword/Hybrid 命中时附带 BM25 评分，前端可展示相关性，DAL 可据此排序 |

---

## 二、架构思路

记忆搜索「FTS5 关键词 + 向量语义 + 图谱关系」三位一体：

```
search_memory Handler
  │  MemorySearch { keyword, query_vector, vector_distance_threshold, traversal_* }
  ▼
MemoryDal.search() ── 三路入口 ──┐
  ├─ keyword  ──► DAO.search_short_term / search_knowledge_nodes (FTS5 MATCH + BM25)
  ├─ vector   ──► VectorDao.search (HNSW, threshold 过滤)
  └─ relation ──► DAO.search_relations (JOIN node_fts MATCH → 查关联边)
                                 │
                                 ▼
                          三路结果按 MatchType 合并
                                 │
                                 ▼
               排序：Hybrid(向量+关键词双命中) → Vector → Keyword
               组内：Hybrid/Vector 按 vector_distance 升序；Keyword 按 fts_rank 升序
```

**FTS5 索引表清单（记忆专属）**：

| FTS 虚拟表 | 索引字段 | 主表 |
|-----------|---------|-----|
| `short_term_memory_fts` | summary, tags | `short_term_memory_index` |
| `knowledge_node_fts` | node_name, summary, node_description | `long_term_knowledge_node` |

同步方式：6 个 AFTER 触发器（INSERT/UPDATE/DELETE × 2 表）自动同步 + 迁移时存量回填。

---

## 三、涉及文件清单

| 文件 | 角色 | 变更摘要 |
|------|------|---------|
| **迁移层** | | |
| [migrations/20260712000000_memory_fts5.sql](../../migrations/20260712000000_memory_fts5.sql) | 记忆 FTS5 基础 | 2 虚拟表（unicode61）+ 6 触发器 + 存量回填 |
| **DAO 层（原子搜索 + 死代码清理）** | | |
| [src/service/dao/memory/mod.rs](../../src/service/dao/memory/mod.rs) | MemoryDao trait | 新增 `search_relations()`；`MemorySearch` 新增 `vector_distance_threshold`；移除 query 方法 keyword 语义 |
| [src/service/dao/memory/sqlite.rs](../../src/service/dao/memory/sqlite.rs) | SQLite impl | search_short_term / search_knowledge_nodes：LIKE → FTS5 MATCH + BM25；新增 search_relations；新增 `escape_fts5_keyword()` 转义工具；移除 query_short_term / query_knowledge_nodes 中的 MATCH 死代码 |
| **DAL 层（混合搜索 + 关系实现）** | | |
| [src/service/dal/memory.rs](../../src/service/dal/memory.rs) | MemoryDal 混合层 | search_short_term_internal / search_knowledge_nodes_internal：三路合并排序 + SearchMatchInfo 打标签；search_relations_internal 从空实现补为调用 DAO；向量阈值从硬编码读 MemorySearch 参数 |
| **模型层** | | |
| [common/src/models/vector.rs](../../common/src/models/vector.rs) | SearchMatchInfo | 新增 `fts_rank: Option<f32>` 字段 |
| **零改动面** | | |
| 向量索引 upsert/delete 钩子、memory domain create/update、前端搜索页 UI | 零改动 | 向量索引维护机制不变；前端 API DTO 兼容新增字段 |

---

## 四、关键边界（行为红线）

1. **query 方法不做搜索**：`query_short_term(keyword=Some(...))` 只忽略 keyword 并打 warn 日志，禁止再走任何关键词搜索（搜索统一走 search_*）
2. **空关键词不触 FTS5**：keyword 为空字符串时，FTS5 分支不执行，直接走纯向量或返回空；禁止传 `MATCH ''`（会语法错）
3. **Hybrid 判定标准**：同一条记忆同时出现在 FTS5 结果集和向量结果集中 → 判定为 Hybrid，match_type = Hybrid，同时携带 vector_distance 和 fts_rank
4. **escape_fts5_keyword 必加**：所有用户传入 keyword 必须转义；特殊字符 * " ( ) : 一律作为字面量，禁止作为 FTS5 语法操作符

---

## 五、扩展模式

### 场景 1：记忆新增可搜索字段（如 long_term_memory 的 extra_tags）

1. 迁移层：新增 migration，`ALTER TABLE knowledge_node_fts` 不支持 → 新建 `knowledge_node_fts_v2` + 新触发器 + 回填 + 删旧表（或用 `insert into fts select ...` 重建）
2. DAO 层：[memory/sqlite.rs](../../src/service/dao/memory/sqlite.rs) 的 FTS5 MATCH 字段列表追加新字段
3. 触发器同步：INSERT/UPDATE 触发器的 SELECT 列表同步追加（自动同步保证一致性）

### 场景 2：新增第四路搜索入口（如按记忆标签集合精确过滤）

1. DAO 层：[memory/mod.rs](../../src/service/dao/memory/mod.rs) 新增 `list_by_tags_exact(tags)` 原子方法
2. DAL 层：[memory.rs](../../src/service/dal/memory.rs) 的 search() 顶部新增过滤分支：先 list_by_tags_exact 拿到候选集 → 再与三路搜索结果求交集（不破坏原有排序）