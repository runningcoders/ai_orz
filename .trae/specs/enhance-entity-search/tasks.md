# Tasks

- [x] Task 1: FTS5 迁移文件（6 实体虚拟表 + 18 触发器 + 存量回填）
  - [x] SubTask 1.1: 创建 `migrations/20260712000001_entity_fts5.sql` 迁移文件
  - [x] SubTask 1.2: 创建 `skills_fts`（索引 name, description, tags，trigram）
  - [x] SubTask 1.3: 创建 `tools_fts`（索引 name, description, tags，trigram）
  - [x] SubTask 1.4: 创建 `messages_fts`（索引 content，trigram）
  - [x] SubTask 1.5: 创建 `tasks_fts`（索引 title, description, tags，trigram）
  - [x] SubTask 1.6: 创建 `projects_fts`（索引 name, description, workflow, guidance, tags，trigram）
  - [x] SubTask 1.7: 创建 `agents_fts`（索引 name, role, description, capabilities，trigram）
  - [x] SubTask 1.8: 创建 18 个触发器（6 表 × INSERT/UPDATE/DELETE）
  - [x] SubTask 1.9: 6 张表存量数据回填
  - [x] SubTask 1.10: 编写触发器同步测试（skills 和 messages 的 INSERT/UPDATE/DELETE）

- [x] Task 2: Skill DAO/DAL FTS5 改造
  - [x] SubTask 2.1: 修改 `search_skills` SQL：LIKE → FTS5 MATCH + JOIN 主表 + BM25 排序，返回带 fts_rank
  - [x] SubTask 2.2: 移除 `query` 方法中的 LIKE 关键词分支，改为忽略并 warn
  - [x] SubTask 2.3: DAL 层 `search` 方法补全 MatchType 三态（Hybrid/Vector/Keyword），引入 fts_rank
  - [x] SubTask 2.4: DAL 层综合排序策略：Hybrid 优先 → Vector → Keyword
  - [x] SubTask 2.5: 编写 DAO/DAL 测试（FTS5 搜索、BM25 排序、三态匹配、中文搜索）

- [x] Task 3: Tool DAO/DAL FTS5 改造
  - [x] SubTask 3.1: 新增 `search_tools` DAO 方法（FTS5 MATCH + BM25），当前 query 中的 LIKE 改为独立 search 方法
  - [x] SubTask 3.2: 移除 `query` 方法中的 LIKE 关键词分支，改为忽略并 warn
  - [x] SubTask 3.3: DAL 层 `search` 方法补全 MatchType 三态，引入 fts_rank
  - [x] SubTask 3.4: DAL 层综合排序策略：Hybrid 优先 → Vector → Keyword
  - [x] SubTask 3.5: 编写 DAO/DAL 测试

- [x] Task 4: Message 搜索能力建设（从零）
  - [x] SubTask 4.1: `MessageQuery` 新增 `keyword: Option<String>` 字段
  - [x] SubTask 4.2: 新增 `MessageSearch` 结构体（keyword, query_vector, top_k, filters）
  - [x] SubTask 4.3: 新增 `search_messages` DAO 方法（FTS5 MATCH + BM25，返回带 fts_rank）
  - [x] SubTask 4.4: 新增 `MessageVectorDao` trait + SQLite 实现（search/upsert/delete，collection `"messages"`）
  - [x] SubTask 4.5: Message PO 实现 `Vectorizable` trait，确定 embed 文本（content）
  - [x] SubTask 4.6: DAL 层新增 `search()` 混合搜索方法（向量 + FTS5 + 三态匹配 + 综合排序）
  - [x] SubTask 4.7: 消息创建/更新时自动 upsert 向量索引，删除时清理
  - [x] SubTask 4.8: 编写 DAO/DAL 测试（FTS5 搜索、向量搜索、混合搜索、自动索引）

- [x] Task 5: Task 搜索能力建设（从零）
  - [x] SubTask 5.1: `TaskQuery` 新增 `keyword: Option<String>` 字段
  - [x] SubTask 5.2: 新增 `TaskSearch` 结构体
  - [x] SubTask 5.3: 新增 `search_tasks` DAO 方法（FTS5 MATCH + BM25）
  - [x] SubTask 5.4: 新增 `TaskVectorDao` trait + SQLite 实现（collection `"tasks"`）
  - [x] SubTask 5.5: Task PO 实现 `Vectorizable` trait，确定 embed 文本（title + description）
  - [x] SubTask 5.6: DAL 层新增 `search()` 混合搜索方法
  - [x] SubTask 5.7: 任务创建/更新时自动 upsert 向量索引，删除时清理
  - [x] SubTask 5.8: 编写 DAO/DAL 测试

- [x] Task 6: Project 搜索能力建设（从零）
  - [x] SubTask 6.1: `ProjectQuery` 新增 `keyword: Option<String>` 字段
  - [x] SubTask 6.2: 新增 `ProjectSearch` 结构体
  - [x] SubTask 6.3: 新增 `search_projects` DAO 方法（FTS5 MATCH + BM25）
  - [x] SubTask 6.4: 新增 `ProjectVectorDao` trait + SQLite 实现（collection `"projects"`）
  - [x] SubTask 6.5: Project PO 实现 `Vectorizable` trait，确定 embed 文本（name + description + workflow + guidance）
  - [x] SubTask 6.6: DAL 层新增 `search()` 混合搜索方法
  - [x] SubTask 6.7: 项目创建/更新时自动 upsert 向量索引，删除时清理
  - [x] SubTask 6.8: 编写 DAO/DAL 测试

- [x] Task 7: Agent 搜索能力建设（FTS5 + 向量，从零建向量）
  - [x] SubTask 7.1: `AgentQuery` 的 `name` 字段替换为 `keyword: Option<String>`
  - [x] SubTask 7.2: 新增 `AgentSearch` 结构体
  - [x] SubTask 7.3: 新增 `search_agents` DAO 方法（FTS5 MATCH + BM25，搜索 name/role/description/capabilities）
  - [x] SubTask 7.4: 新增 `AgentVectorDao` trait + SQLite 实现（collection `"agents"`）
  - [x] SubTask 7.5: Agent PO 实现 `Vectorizable` trait，确定 embed 文本（name + role + description + capabilities）
  - [x] SubTask 7.6: DAL 层新增 `search()` 混合搜索方法
  - [x] SubTask 7.7: Agent 创建/更新时自动 upsert 向量索引，删除时清理
  - [x] SubTask 7.8: 编写 DAO/DAL 测试

- [x] Task 8: 阶段验证（编译 + 全量测试）
  - [x] SubTask 8.1: cargo check 编译通过
  - [x] SubTask 8.2: cargo test 全量测试通过（691 个测试 100% 通过）
  - [x] SubTask 8.3: 检查无 warning 回归

# Task Dependencies
- Task 1 无依赖，可立即开始
- Task 2, 3 依赖 Task 1（需要 FTS5 表存在）
- Task 4, 5, 6, 7 依赖 Task 1（需要 FTS5 表存在），彼此之间无依赖
- Task 8 依赖所有任务完成

# Parallelizable Groups
- **Group A**（无依赖）：Task 1
- **Group B**（依赖 Group A，可并行）：Task 2, Task 3, Task 4, Task 5, Task 6, Task 7
- **Final**：Task 8
