# 全实体 FTS5 全文搜索改造 Spec

## Why

当前系统中只有 Memory 实体完成了 FTS5 + 向量混合搜索改造。其余 5 个实体（Skill、Tool、Message、Task、Project、Agent）仍在使用 LIKE 子串匹配或完全没有搜索能力。随着 Agent 成长和数据积累，LIKE 的无分词、无相关性排序、性能退化问题日益突出。本次一步到位为所有实体引入 FTS5 全文搜索，统一弃用 LIKE，建立全项目统一的混合搜索标准。

## What Changes

### FTS5 索引建设（6 个实体）

- **新增** migration：创建 6 个 FTS5 虚拟表（trigram 分词器），覆盖 Skill/Tool/Message/Task/Project/Agent
- **新增** migration：创建 18 个触发器（INSERT/UPDATE/DELETE × 6 表），自动同步 FTS 索引
- **新增** migration：6 张表的存量数据回填

### Skill 搜索改造（已有向量搜索）

- **修改** `search_skills` SQL：LIKE → FTS5 MATCH + BM25 排序
- **修改** DAL 层混合搜索：补全 MatchType 三态（Hybrid/Vector/Keyword），引入 fts_rank
- **修改** 综合排序策略：Hybrid 优先 → Vector → Keyword

### Tool 搜索改造（已有向量搜索）

- **修改** `search_tools` SQL（如果存在）或在 query 中新增关键词搜索方法：LIKE → FTS5 MATCH + BM25
- **修改** DAL 层混合搜索：补全 MatchType 三态，引入 fts_rank
- **修改** 综合排序策略：同 Skill

### Message 搜索改造（从零建设）

- **新增** `MessageQuery.keyword` 字段
- **新增** `search_messages` DAO 方法：FTS5 MATCH + BM25 排序
- **新增** `MessageSearch` 结构体（keyword + query_vector + filters）
- **新增** 向量索引 collection `"messages"`
- **新增** 向量 DAO 方法（search/upsert/delete）
- **新增** DAL 层 `search()` 混合搜索方法
- **新增** 消息创建/更新时自动 upsert 向量索引

### Task 搜索改造（从零建设）

- **新增** `TaskQuery.keyword` 字段
- **新增** `search_tasks` DAO 方法：FTS5 MATCH + BM25 排序
- **新增** `TaskSearch` 结构体
- **新增** 向量索引 collection `"tasks"`
- **新增** 向量 DAO 方法
- **新增** DAL 层 `search()` 混合搜索方法
- **新增** 任务创建/更新时自动 upsert 向量索引

### Project 搜索改造（从零建设）

- **新增** `ProjectQuery.keyword` 字段
- **新增** `search_projects` DAO 方法：FTS5 MATCH + BM25 排序
- **新增** `ProjectSearch` 结构体
- **新增** 向量索引 collection `"projects"`
- **新增** 向量 DAO 方法
- **新增** DAL 层 `search()` 混合搜索方法
- **新增** 项目创建/更新时自动 upsert 向量索引

### Agent 搜索改造（已有 LIKE，无向量）

- **修改** `AgentQuery`：`name` 字段改为 `keyword` 通用搜索字段
- **新增** `search_agents` DAO 方法：FTS5 MATCH + BM25 排序（搜索 name + role + description + soul + capabilities）
- **新增** `AgentSearch` 结构体
- **新增** 向量索引 collection `"agents"`
- **新增** 向量 DAO 方法
- **新增** DAL 层 `search()` 混合搜索方法
- **新增** Agent 创建/更新时自动 upsert 向量索引

### 通用基础设施

- **复用** `escape_fts5_keyword` 工具函数（已在 memory DAO 中实现）
- **复用** `SearchMatchInfo` 模型（已有 fts_rank 字段）
- **复用** MatchType 三态匹配模式
- **复用** 综合排序策略（Hybrid → Vector → Keyword）

## Impact

- Affected specs: enhance-memory-search（搜索模式参照），enhance-skill-system（Skill 搜索增强）
- Affected code:
  - `migrations/` - 新增 FTS5 迁移文件
  - `src/service/dao/skill/` - Skill DAO FTS5 改造
  - `src/service/dao/tool/` - Tool DAO FTS5 改造
  - `src/service/dao/message/` - Message DAO 搜索能力建设
  - `src/service/dao/task/` - Task DAO 搜索能力建设
  - `src/service/dao/project/` - Project DAO 搜索能力建设
  - `src/service/dao/agent/` - Agent DAO FTS5 改造 + 向量索引建设
  - `src/service/dal/skill.rs` - Skill DAL 混合搜索优化
  - `src/service/dal/tool.rs` - Tool DAL 混合搜索优化
  - `src/service/dal/message.rs` - Message DAL 搜索方法新增
  - `src/service/dal/task.rs` - Task DAL 搜索方法新增
  - `src/service/dal/project.rs` - Project DAL 搜索方法新增
  - `src/service/dal/agent.rs` - Agent DAL 搜索方法新增
  - `src/models/vector.rs` - Vectorizable trait 实现新实体

## ADDED Requirements

### Requirement: FTS5 全文索引（6 个实体）

系统 SHALL 为 Skill、Tool、Message、Task、Project、Agent 各创建一个 FTS5 虚拟表，使用 trigram 分词器支持中文，并通过触发器自动同步。

#### Scenario: FTS5 表创建
- **WHEN** 数据库迁移执行
- **THEN** 创建以下 FTS5 虚拟表：
  - `skills_fts`：索引 `name`、`description`、`tags`
  - `tools_fts`：索引 `name`、`description`、`tags`
  - `messages_fts`：索引 `content`
  - `tasks_fts`：索引 `title`、`description`、`tags`
  - `projects_fts`：索引 `name`、`description`、`workflow`、`guidance`、`tags`
  - `agents_fts`：索引 `name`、`role`、`description`、`capabilities`
- **AND** 所有表使用 `tokenize = 'trigram'` 分词器

#### Scenario: 触发器自动同步
- **WHEN** 任何主表（skills/tools/messages/tasks/projects/agents）发生 INSERT/UPDATE/DELETE
- **THEN** 对应的 AFTER 触发器自动同步 FTS 索引
- **AND** 无需应用层额外操作

#### Scenario: 存量数据回填
- **WHEN** 迁移执行时主表已有数据
- **THEN** 将所有存量记录的文本字段回填到 FTS 表

### Requirement: FTS5 全文搜索（统一标准）

所有实体的关键词搜索 SHALL 使用 FTS5 MATCH 语法 + BM25 相关性排序，弃用 LIKE。

#### Scenario: 关键词搜索
- **WHEN** 用户或 Agent 搜索关键词
- **THEN** 使用 FTS5 MATCH 语法匹配
- **AND** 结果按 BM25 相关性评分排序
- **AND** 关键词经 `escape_fts5_keyword` 转义处理

#### Scenario: 特殊字符安全
- **WHEN** 关键词包含 FTS5 语法字符（`*`、`"`、`(`、`)`）
- **THEN** 转义后作为字面量匹配，不抛出语法错误

### Requirement: 混合搜索（向量 + FTS5）

已有向量搜索的实体（Skill、Tool）SHALL 补全 MatchType 三态匹配和综合排序策略。新增搜索能力的实体（Message、Task、Project、Agent）SHALL 同时建设向量搜索和 FTS5 混合搜索。

#### Scenario: 三态匹配
- **WHEN** 同时提供关键词和向量查询
- **THEN** 结果按 Hybrid 优先 → Vector → Keyword 三级排序
- **AND** 每条结果附加 `SearchMatchInfo`，标记匹配类型和评分

#### Scenario: 纯关键词搜索
- **WHEN** 仅提供关键词，无向量
- **THEN** 仅执行 FTS5 搜索
- **AND** 所有结果 `match_type` 为 `Keyword`

#### Scenario: 纯向量搜索
- **WHEN** 仅提供向量，无关键词
- **THEN** 仅执行向量搜索
- **AND** 所有结果 `match_type` 为 `Vector`

### Requirement: 向量索引自动维护

新增搜索能力的实体（Message、Task、Project、Agent）SHALL 在创建/更新时自动 upsert 向量索引，删除时自动删除向量索引。

#### Scenario: 创建时索引
- **WHEN** 创建新消息/任务/项目/Agent
- **THEN** 自动将文本字段 embed 并 upsert 到对应向量 collection

#### Scenario: 更新时索引
- **WHEN** 更新文本字段
- **THEN** 重新 embed 并更新向量索引

#### Scenario: 删除时清理
- **WHEN** 删除记录
- **THEN** 自动删除对应的向量索引

### Requirement: Message 内容搜索

系统 SHALL 支持按关键词搜索消息内容。

#### Scenario: 搜索消息内容
- **WHEN** 搜索关键词 "部署方案"
- **THEN** 返回 content 字段包含该关键词的消息列表
- **AND** 结果按 BM25 相关性排序

### Requirement: Task 搜索

系统 SHALL 支持按关键词搜索任务标题和描述。

#### Scenario: 搜索任务
- **WHEN** 搜索关键词 "性能优化"
- **THEN** 返回 title 或 description 包含该关键词的任务列表
- **AND** 结果按 BM25 相关性排序

### Requirement: Project 搜索

系统 SHALL 支持按关键词搜索项目名称、描述、工作流和指导信息。

#### Scenario: 搜索项目
- **WHEN** 搜索关键词 "数据平台"
- **THEN** 返回 name、description、workflow、guidance 包含该关键词的项目列表
- **AND** 结果按 BM25 相关性排序

### Requirement: Agent 搜索

系统 SHALL 支持按关键词搜索 Agent 的名称、角色、描述和能力。现有的 `AgentQuery.name` LIKE 搜索 SHALL 被替换为 `keyword` FTS5 搜索。

#### Scenario: 搜索 Agent
- **WHEN** 搜索关键词 "前端开发"
- **THEN** 在 name、role、description、capabilities 中 FTS5 匹配
- **AND** 返回匹配的 Agent 列表
- **AND** 结果按 BM25 相关性排序

## MODIFIED Requirements

### Requirement: Skill 关键词搜索

现有 `search_skills` 使用 LIKE，SHALL 改为 FTS5 MATCH + BM25 排序。DAL 层 SHALL 补全 MatchType 三态匹配。

### Requirement: Tool 关键词搜索

现有 Tool query 中的 LIKE 关键词搜索 SHALL 改为独立的 `search_tools` 方法，使用 FTS5 MATCH + BM25 排序。DAL 层 SHALL 补全 MatchType 三态匹配。

### Requirement: AgentQuery

现有 `AgentQuery.name: Option<String>` LIKE 搜索 SHALL 替换为 `AgentQuery.keyword: Option<String>` FTS5 搜索。

## REMOVED Requirements

### Requirement: Skill LIKE 关键词搜索

**Reason**: LIKE 无分词、无相关性排序，已被 FTS5 MATCH 替代。
**Migration**: 关键词搜索统一走 `search_skills` FTS5 方法。

### Requirement: Tool LIKE 关键词搜索

**Reason**: 同上。
**Migration**: 关键词搜索统一走 `search_tools` FTS5 方法。

### Requirement: Agent name LIKE 搜索

**Reason**: 仅搜 name 一个字段，已被 keyword FTS5 多字段搜索替代。
**Migration**: `AgentQuery.name` 替换为 `AgentQuery.keyword`。
