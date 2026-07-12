# Checklist

## FTS5 迁移文件（6 实体）

- [x] skills_fts 虚拟表创建（fts5，索引 name + description + tags，trigram 分词）
- [x] tools_fts 虚拟表创建（fts5，索引 name + description + tags，trigram 分词）
- [x] messages_fts 虚拟表创建（fts5，索引 content，trigram 分词）
- [x] tasks_fts 虚拟表创建（fts5，索引 title + description + tags，trigram 分词）
- [x] projects_fts 虚拟表创建（fts5，索引 name + description + workflow + guidance + tags，trigram 分词）
- [x] agents_fts 虚拟表创建（fts5，索引 name + role + description + capabilities，trigram 分词）
- [x] skills 表的 INSERT/UPDATE/DELETE 触发器
- [x] tools 表的 INSERT/UPDATE/DELETE 触发器
- [x] messages 表的 INSERT/UPDATE/DELETE 触发器
- [x] tasks 表的 INSERT/UPDATE/DELETE 触发器
- [x] projects 表的 INSERT/UPDATE/DELETE 触发器
- [x] agents 表的 INSERT/UPDATE/DELETE 触发器
- [x] 6 张表存量数据回填
- [x] 触发器同步测试：skills INSERT/UPDATE/DELETE
- [x] 触发器同步测试：messages INSERT/UPDATE/DELETE

## Skill DAO/DAL FTS5 改造

- [x] search_skills SQL 改为 FTS5 MATCH + JOIN 主表 + BM25 排序
- [x] search_skills 返回值携带 fts_rank
- [x] query 方法中 LIKE 关键词分支移除，改为忽略并 warn
- [x] DAL 层 search 方法补全 MatchType 三态（Hybrid/Vector/Keyword）
- [x] DAL 层综合排序：Hybrid 优先 → Vector → Keyword
- [x] 测试：FTS5 搜索正确返回结果
- [x] 测试：BM25 相关性排序验证
- [x] 测试：三态匹配（Hybrid/Vector/Keyword）
- [x] 测试：中文关键词搜索

## Tool DAO/DAL FTS5 改造

- [x] 新增 search_tools DAO 方法（FTS5 MATCH + BM25，返回带 fts_rank）
- [x] query 方法中 LIKE 关键词分支移除，改为忽略并 warn
- [x] DAL 层 search 方法补全 MatchType 三态
- [x] DAL 层综合排序：Hybrid 优先 → Vector → Keyword
- [x] 测试：FTS5 搜索正确返回结果
- [x] 测试：BM25 相关性排序验证
- [x] 测试：三态匹配
- [x] 测试：中文关键词搜索

## Message 搜索能力建设

- [x] MessageQuery 新增 keyword 字段
- [x] MessageSearch 结构体定义
- [x] search_messages DAO 方法（FTS5 MATCH + BM25，返回带 fts_rank）
- [x] MessageVectorDao trait + SQLite 实现（collection "messages"）
- [x] Message PO 实现 Vectorizable trait（embed 文本：content）
- [x] DAL 层 search() 混合搜索方法
- [x] 消息创建/更新时自动 upsert 向量索引
- [x] 消息删除时自动清理向量索引
- [x] 测试：FTS5 搜索消息内容
- [x] 测试：向量搜索消息
- [x] 测试：混合搜索三态匹配
- [x] 测试：向量索引自动维护

## Task 搜索能力建设

- [ ] TaskQuery 新增 keyword 字段
- [x] TaskSearch 结构体定义
- [x] search_tasks DAO 方法（FTS5 MATCH + BM25）
- [x] TaskVectorDao trait + SQLite 实现（collection "tasks"）
- [x] Task PO 实现 Vectorizable trait（embed 文本：title + description）
- [x] DAL 层 search() 混合搜索方法
- [x] 任务创建/更新时自动 upsert 向量索引
- [x] 任务删除时自动清理向量索引
- [x] 测试：FTS5 搜索任务
- [x] 测试：向量搜索任务
- [x] 测试：混合搜索三态匹配
- [x] 测试：向量索引自动维护

## Project 搜索能力建设

- [x] ProjectQuery 新增 keyword 字段
- [x] ProjectSearch 结构体定义
- [x] search_projects DAO 方法（FTS5 MATCH + BM25）
- [x] ProjectVectorDao trait + SQLite 实现（collection "projects"）
- [x] Project PO 实现 Vectorizable trait（embed 文本：name + description + workflow + guidance）
- [x] DAL 层 search() 混合搜索方法
- [x] 项目创建/更新时自动 upsert 向量索引
- [ ] 项目删除时自动清理向量索引
- [x] 测试：FTS5 搜索项目
- [x] 测试：向量搜索项目
- [x] 测试：混合搜索三态匹配
- [x] 测试：向量索引自动维护

## Agent 搜索能力建设

- [x] AgentQuery.name 替换为 keyword
- [x] AgentSearch 结构体定义
- [x] search_agents DAO 方法（FTS5 MATCH + BM25，搜索 name/role/description/capabilities）
- [x] AgentVectorDao trait + SQLite 实现（collection "agents"）
- [x] Agent PO 实现 Vectorizable trait（embed 文本：name + role + description + capabilities）
- [x] DAL 层 search() 混合搜索方法
- [x] Agent 创建/更新时自动 upsert 向量索引
- [x] Agent 删除时自动清理向量索引
- [x] 测试：FTS5 搜索 Agent
- [x] 测试：向量搜索 Agent
- [x] 测试：混合搜索三态匹配
- [x] 测试：向量索引自动维护

## 最终验证

- [x] cargo check 编译通过
- [x] cargo test 全量测试通过
- [x] 无新增 warning 回归
