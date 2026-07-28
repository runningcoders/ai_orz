# 记忆搜索 FTS5 增强与综合搜索 Spec

## Why

记忆系统是 Agent 自主性的核心基础设施。当前搜索存在三类问题：
1. **MATCH 死代码**：`query_short_term` / `query_knowledge_nodes` 使用了 FTS5 的 `MATCH` 语法，但从未创建 FTS 虚拟表，实际调用会运行时报错
2. **LIKE 搜索能力不足**：无分词、无相关性排序、无多词搜索、性能随数据量增长退化
3. **综合搜索不完整**：关系搜索是 TODO 空实现；MatchType::Keyword 从未实际使用；向量阈值硬编码

本次一步到位引入 FTS5 全文搜索，修复所有存量问题，建立「FTS5 关键词 + 向量语义 + 图谱关系」三位一体的综合搜索能力。

## What Changes

### FTS5 全文索引建设

- **新增** migration：创建 `short_term_memory_fts` 和 `knowledge_node_fts` 两个 FTS5 虚拟表
- **新增** migration：创建 6 个触发器（INSERT/UPDATE/DELETE × 2 表），自动同步 FTS 索引
- **新增** migration：存量数据回填 FTS 索引

### DAO 层搜索改造

- **修改** `search_short_term`：LIKE → FTS5 MATCH + BM25 排序
- **修改** `search_knowledge_nodes`：LIKE → FTS5 MATCH + BM25 排序
- **修改** `query_short_term`：移除死代码 MATCH 分支，关键词搜索统一走 search 方法
- **修改** `query_knowledge_nodes`：同上，移除死代码 MATCH 分支
- **新增** `search_relations` DAO 方法：通过 JOIN 知识节点 FTS 实现关系关键词搜索
- **新增** FTS5 关键词转义工具函数

### DAL 层混合搜索优化

- **修改** `search()` 统一排序策略：Hybrid 优先 → Vector → Keyword，引入 BM25 分数
- **修改** `search_relations_internal`：从 TODO 空实现改为调用 DAO 层 `search_relations`
- **修改** 向量距离阈值：从硬编码 0.8 改为 `MemorySearch` 可选参数，默认 0.8
- **修改** `SearchMatchInfo` 补全：关键词命中也附加 `SearchMatchInfo { match_type: Keyword, fts_rank }`

### 模型层扩展

- **修改** `SearchMatchInfo`：新增 `fts_rank: Option<f32>` 字段
- **修改** `MatchType`：确认 Keyword/Vector/Hybrid 三种类型都有实际使用

### MemorySearch 参数扩展

- **修改** `MemorySearch`：新增 `vector_distance_threshold: Option<f32>` 字段

## Impact

- Affected specs: enhance-memory-system（记忆系统基础，搜索能力增强）
- Affected code:
  - `migrations/` - 新增 FTS5 迁移文件
  - `src/service/dao/memory/mod.rs` - MemoryDao trait 新增 search_relations，MemorySearch 新增字段
  - `src/service/dao/memory/sqlite.rs` - 搜索 SQL 改造（LIKE → FTS5 MATCH）
  - `src/service/dal/memory.rs` - 混合搜索排序优化 + search_relations_internal 实现
  - `src/models/vector.rs` - SearchMatchInfo 新增 fts_rank
  - `src/handlers/hr/agent/search_memory.rs` - 神经工具可能需要适配
  - `common/src/api/neural_tools.rs` - SearchMemoryParams 可能新增阈值参数

## ADDED Requirements

### Requirement: FTS5 全文索引

系统 SHALL 为短期记忆和知识节点创建 FTS5 虚拟表，支持 unicode61 分词，并通过触发器自动同步索引。

#### Scenario: FTS5 表创建
- **WHEN** 数据库迁移执行
- **THEN** 创建 `short_term_memory_fts` 虚拟表，索引 `summary` 和 `tags` 字段
- **AND** 创建 `knowledge_node_fts` 虚拟表，索引 `node_name`、`summary`、`node_description` 字段
- **AND** 使用 `tokenize = 'unicode61'` 分词器

#### Scenario: 插入时自动同步 FTS
- **WHEN** 向 `short_term_memory_index` 插入新记录
- **THEN** AFTER INSERT 触发器自动将新记录的 `summary` 和 `tags` 写入 FTS 表
- **AND** 无需应用层额外操作

#### Scenario: 更新时自动同步 FTS
- **WHEN** 更新 `short_term_memory_index` 的 `summary` 或 `tags` 字段
- **THEN** AFTER UPDATE 触发器先删除旧 FTS 条目，再插入新 FTS 条目

#### Scenario: 删除时自动同步 FTS
- **WHEN** 删除 `short_term_memory_index` 的记录
- **THEN** AFTER DELETE 触发器自动删除对应的 FTS 条目

#### Scenario: 存量数据回填
- **WHEN** 迁移执行时主表已有数据
- **THEN** 将所有存量记录的 `summary`、`tags` 等字段回填到 FTS 表

### Requirement: FTS5 全文搜索

系统 SHALL 使用 FTS5 MATCH 语法进行全文搜索，支持 BM25 相关性排序。

#### Scenario: 单关键词搜索
- **WHEN** 搜索关键词 `"数据分析"`
- **THEN** 使用 FTS5 MATCH 语法匹配，返回包含该词的记忆
- **AND** 结果按 BM25 相关性评分排序

#### Scenario: 多关键词搜索
- **WHEN** 搜索关键词 `"数据分析 方法"`
- **THEN** FTS5 默认 AND 语义，返回同时包含两个词的记忆
- **AND** BM25 评分综合两个词的相关性

#### Scenario: FTS5 特殊字符转义
- **WHEN** 搜索关键词包含 FTS5 语法字符（如 `*`、`"`、`(`、`)`）
- **THEN** 对关键词进行转义，作为字面量匹配，不被解释为 FTS5 语法
- **AND** 不抛出 FTS5 语法错误

#### Scenario: 空关键词降级
- **WHEN** 搜索关键词为空字符串
- **THEN** 不执行 FTS5 搜索，返回空结果或走纯向量搜索

### Requirement: 关系关键词搜索

系统 SHALL 支持按关键词搜索知识图谱中的关系，通过 JOIN 知识节点 FTS 索引实现。

#### Scenario: 关键词搜关系
- **WHEN** 搜索 `memory_type = Relation` 且有关键词
- **THEN** 通过 `knowledge_node_fts` MATCH 搜索匹配的知识节点
- **AND** 查询这些节点关联的所有关系（出入边）
- **AND** 返回关系和匹配的节点

### Requirement: 向量距离阈值可配置

系统 SHALL 支持通过 `MemorySearch` 参数自定义向量距离阈值，默认值 0.8。

#### Scenario: 使用默认阈值
- **WHEN** `MemorySearch.vector_distance_threshold` 为 None
- **THEN** 使用默认阈值 0.8 过滤向量搜索结果

#### Scenario: 自定义阈值
- **WHEN** `MemorySearch.vector_distance_threshold` 为 Some(0.6)
- **THEN** 使用 0.6 作为阈值，过滤掉距离大于 0.6 的结果

### Requirement: 综合搜索排序策略

系统 SHALL 对混合搜索结果按「Hybrid 优先 → Vector → Keyword」分组排序。

#### Scenario: 三种匹配类型同时存在
- **WHEN** 搜索结果包含 Hybrid（关键词+向量双命中）、Vector（仅向量）、Keyword（仅关键词）三种结果
- **THEN** Hybrid 结果排在最前
- **AND** Hybrid 内部按 vector_distance 升序排序
- **AND** Vector 结果排在 Hybrid 之后，按 vector_distance 升序排序
- **AND** Keyword 结果排在最后，按 fts_rank 升序排序（BM25 评分越小越相关）

#### Scenario: 关键词命中附加 MatchInfo
- **WHEN** 记忆仅被 FTS5 关键词匹配（未被向量匹配）
- **THEN** 该记忆的 `search_match.match_type` 为 `Keyword`
- **AND** `search_match.fts_rank` 为 BM25 评分值

## MODIFIED Requirements

### Requirement: 短期记忆搜索

现有 `search_short_term` 使用 `LIKE '%keyword%'` 子串匹配，SHALL 改为 FTS5 MATCH + BM25 排序。

#### Scenario: FTS5 搜索
- **WHEN** 调用 `search_short_term(keyword = "test")`
- **THEN** SQL 使用 `short_term_memory_fts MATCH ?` 查询
- **AND** 结果按 `bm25()` 评分排序

### Requirement: 知识节点搜索

现有 `search_knowledge_nodes` 使用 `LIKE '%keyword%'` 子串匹配，SHALL 改为 FTS5 MATCH + BM25 排序。

#### Scenario: FTS5 搜索
- **WHEN** 调用 `search_knowledge_nodes(keyword = "test")`
- **THEN** SQL 使用 `knowledge_node_fts MATCH ?` 查询
- **AND** 结果按 `bm25()` 评分排序

### Requirement: query 方法清理

现有 `query_short_term` 和 `query_knowledge_nodes` 包含不可用的 `MATCH` 分支，SHALL 移除关键词搜索能力，关键词搜索统一走 `search_*` 方法。

#### Scenario: query 方法不再支持关键词
- **WHEN** 调用 `query_short_term(MemoryQuery { keyword: Some("test") })`
- **THEN** 忽略 keyword 参数（不添加关键词过滤条件）
- **AND** 按其他条件正常查询
- **AND** 日志记录 warn 提示 keyword 在 query 方法中已被忽略

## REMOVED Requirements

### Requirement: query 方法中的 MATCH 分支

**Reason**: 使用了 FTS5 MATCH 语法但从未创建 FTS 虚拟表，是死代码，实际调用会运行时报错。
**Migration**: 关键词搜索统一走 `search_*` 方法（使用正确的 FTS5 MATCH）。
