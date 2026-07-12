# Tasks

- [x] Task 1: FTS5 迁移文件（虚拟表 + 触发器 + 存量回填）
  - [x] SubTask 1.1: 创建 `migrations/20260712000000_memory_fts5.sql` 迁移文件
  - [x] SubTask 1.2: 创建 `short_term_memory_fts` 虚拟表（fts5，索引 summary + tags，trigram 分词）
  - [x] SubTask 1.3: 创建 `knowledge_node_fts` 虚拟表（fts5，索引 node_name + summary + node_description，trigram 分词）
  - [x] SubTask 1.4: 创建 6 个触发器（short_term_memory_index 的 INSERT/UPDATE/DELETE + long_term_knowledge_node 的 INSERT/UPDATE/DELETE）
  - [x] SubTask 1.5: 存量数据回填（INSERT INTO fts_table SELECT ... FROM main_table）
  - [x] SubTask 1.6: 编写测试验证触发器自动同步（插入/更新/删除后 FTS 表数据正确）

- [x] Task 2: DAO 层搜索 SQL 改造（LIKE → FTS5 MATCH）
  - [x] SubTask 2.1: 新增 FTS5 关键词转义工具函数 `escape_fts5_keyword(keyword: &str) -> String`
  - [x] SubTask 2.2: 修改 `search_short_term` SQL：LIKE → FTS5 MATCH + JOIN 主表 + BM25 排序
  - [x] SubTask 2.3: 修改 `search_knowledge_nodes` SQL：LIKE → FTS5 MATCH + JOIN 主表 + BM25 排序
  - [x] SubTask 2.4: 移除 `query_short_term` 中的 MATCH 死代码分支，keyword 参数改为忽略并 warn
  - [x] SubTask 2.5: 移除 `query_knowledge_nodes` 中的 MATCH 死代码分支，同上
  - [x] SubTask 2.6: 编写 DAO 层测试（FTS5 单词搜索、多词 AND、特殊字符转义、BM25 排序验证）

- [x] Task 3: 模型层扩展（SearchMatchInfo + MemorySearch）
  - [x] SubTask 3.1: `SearchMatchInfo` 新增 `fts_rank: Option<f32>` 字段
  - [x] SubTask 3.2: `MemorySearch` 新增 `vector_distance_threshold: Option<f32>` 字段
  - [x] SubTask 3.3: 编写模型层测试

- [x] Task 4: DAL 层混合搜索优化
  - [x] SubTask 4.1: `search_short_term_internal` 改造：FTS5 搜索结果附加 `SearchMatchInfo { match_type: Keyword, fts_rank }`，向量命中附加 `Hybrid`
  - [x] SubTask 4.2: `search_knowledge_nodes_internal` 同上改造
  - [x] SubTask 4.3: 向量距离阈值从硬编码改为读取 `MemorySearch.vector_distance_threshold`，默认 0.8
  - [x] SubTask 4.4: `search()` 统一排序策略改为：Hybrid 优先 → Vector → Keyword，组内分别按 distance/rank 排序
  - [x] SubTask 4.5: 实现 `search_relations_internal`：通过 knowledge_node_fts MATCH 搜索节点 → 查关联关系 → 返回 Relation + KnowledgeNode
  - [x] SubTask 4.6: 编写 DAL 层测试（三路混合排序、Keyword MatchInfo、关系搜索、阈值可配置）

- [x] Task 5: 阶段验证（编译 + 全量测试）
  - [x] SubTask 5.1: cargo check 编译通过
  - [x] SubTask 5.2: cargo test 全量测试通过（615 个测试 100% 通过）
  - [x] SubTask 5.3: 检查无 warning 回归

# Task Dependencies
- Task 1 无依赖，可立即开始
- Task 3 无依赖，可并行
- Task 2 依赖 Task 1（需要 FTS5 表存在才能测试 MATCH 查询）
- Task 4 依赖 Task 2（需要 DAO 层 FTS5 搜索）+ Task 3（需要 SearchMatchInfo.fts_rank）
- Task 5 依赖所有任务完成

# Parallelizable Groups
- **Group A**（无依赖，可立即开始）：Task 1, Task 3
- **Group B**（依赖 Group A）：Task 2
- **Group C**（依赖 Group B）：Task 4
- **Final**：Task 5
