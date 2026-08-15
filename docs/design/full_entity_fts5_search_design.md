# 全实体 FTS5 全文搜索统一设计

> 🎯 **本文档定位**：全实体搜索改造的设计决策大纲（为什么引入 FTS5、6 实体统一搜索标准怎么定、混合搜索排序策略；字段级实现细节以实际代码为准）
>
> 状态：定稿（2026-07-12 功能落地）
>
> 查阅场景：需要为新实体引入全文搜索、理解 FTS5 + 向量混合搜索策略、排查搜索相关性排序问题时打开。
>
> 关联文档：
> - [vector_search_architecture.md](./vector_search_architecture.md) — 向量搜索底层架构
> - [memory_search_enhancement_design.md](./memory_search_enhancement_design.md) — 记忆搜索 FTS5 改造（本设计参照）
> - [AGENTS.md](../../AGENTS.md) — 分层架构规范 §3.1 DAO/DAL 分层约定

---

## 一、设计目标与关键决策

### 问题背景

当前系统仅 Memory 实体完成了 FTS5 + 向量混合搜索改造，其余 6 个实体（Skill、Tool、Message、Task、Project、Agent）仍使用 LIKE 子串匹配或完全无搜索能力，带来三类问题：

| 问题维度 | 现状痛点 |
|---------|---------|
| 性能退化 | LIKE 无索引，数据量增长后线性退化 |
| 搜索质量 | 无分词、无相关性排序（BM25）、匹配仅按主键顺序 |
| 实现不一致 | 每个实体各写一套 LIKE 逻辑，无统一标准 |

### 关键决策表

| # | 决策问题 | 选择方案 | 选择原因 |
|---|---------|---------|---------|
| 1 | 全文搜索引擎选型 | **FTS5 trigram 分词** | SQLite 内置零依赖；trigram 支持中文无词典分词；memory 改造已验证可用性 |
| 2 | 哪些实体引入 | **Skill / Tool / Message / Task / Project / Agent 全部 6 个** | 一步到位统一标准；增量仅新增迁移 + DAO 方法，风险可控 |
| 3 | 索引同步方式 | **AFTER 触发器（INSERT/UPDATE/DELETE × 6 表）** | 数据库级保证一致性，无需应用层额外写同步代码 |
| 4 | 混合搜索排序策略 | **Hybrid 优先 → Vector → Keyword 三态** | 语义和关键词双命中的结果最相关；Vector 语义次之；纯关键词兜底 |
| 5 | MatchType 实现位置 | **DAL 层统一** | DAO 层只暴露纯 search_xxx（FTS5）和纯向量搜索，DAL 层负责合并 + 排序 + 打 MatchInfo |
| 6 | 新增向量搜索的实体 | **Message / Task / Project / Agent**（Skill/Tool 已有） | 统一混合搜索能力；embed 文本选取各实体核心字段 |

### 收敛后效果

全项目搜索统一标准：**关键词走 FTS5 MATCH + BM25，语义走向量，两者都有走 Hybrid 合并排序**。新增实体搜索时只需按 6 步模板复用。

---

## 二、架构思路

三层分离，职责对齐 AGENTS §3.1 DAO/DAL/Domain 分层：

```
Handler（前端/API 入口，不变）
  │  传入 Query { keyword, query_vector, filters }
  ▼
DAL（混合搜索策略层）── MatchType 三态判定 + Hybrid→Vector→Keyword 排序
  │  合并两路结果（FTS5 列表 + 向量列表），打 SearchMatchInfo
  ▼
DAO（原子搜索层）
  ├─ FTS5 DAO：search_xxx(keyword) → JOIN 主表 + BM25 排序
  └─ Vector DAO：upsert/search/delete_vector（collection 同名）
     ▲
     │ 自动维护（创建/更新/删除时 upsert/delete 向量索引）
   Entity CRUD（Domain 层，触发 DAO）
```

**向量索引 collection 命名约定**（与实体表名一一对应）：

| 实体 | collection | embed 文本 |
|------|-----------|-----------|
| Skill | `"skills"` | name + description |
| Tool | `"tools"` | name + description |
| Message | `"messages"` | content |
| Task | `"tasks"` | title + description |
| Project | `"projects"` | name + description + workflow + guidance |
| Agent | `"agents"` | name + role + description + capabilities |

---

## 三、涉及文件清单

按 AGENTS §3.2 目录结构索引：

| 文件 | 角色 | 变更摘要 |
|------|------|---------|
| **迁移层（FTS5 索引基础）** | | |
| [migrations/20260712000001_entity_fts5.sql](../../migrations/20260712000001_entity_fts5.sql) | 6 实体 FTS5 表 + 18 触发器 | skills_fts/tools_fts/messages_fts/tasks_fts/projects_fts/agents_fts；触发器 AFTER INSERT/UPDATE/DELETE 自动同步；存量数据回填 |
| **DAO 层（原子搜索）** | | |
| [src/service/dao/skill/sqlite.rs](../../src/service/dao/skill/sqlite.rs) | Skill FTS5 | search_skills SQL：LIKE → FTS5 MATCH + BM25；query 移除 LIKE 关键词分支 |
| [src/service/dao/tool/sqlite.rs](../../src/service/dao/tool/sqlite.rs) | Tool FTS5 | 新增 search_tools 方法；query 移除 LIKE 分支 |
| [src/service/dao/message/sqlite.rs](../../src/service/dao/message/sqlite.rs) | Message FTS5 + 向量 | 新增 search_messages；新增 MessageVectorDao；PO 实现 Vectorizable |
| [src/service/dao/task/sqlite.rs](../../src/service/dao/task/sqlite.rs) | Task FTS5 + 向量 | 新增 search_tasks；TaskVectorDao；Vectorizable 实现 |
| [src/service/dao/project/sqlite.rs](../../src/service/dao/project/sqlite.rs) | Project FTS5 + 向量 | 新增 search_projects；ProjectVectorDao；Vectorizable 实现 |
| [src/service/dao/agent/sqlite.rs](../../src/service/dao/agent/sqlite.rs) | Agent FTS5 + 向量 | 新增 search_agents；AgentVectorDao；Vectorizable 实现 |
| **DAL 层（混合搜索）** | | |
| [src/service/dal/skill.rs](../../src/service/dal/skill.rs) | Skill 混合 | 补全 MatchType 三态；综合排序 Hybrid→Vector→Keyword |
| [src/service/dal/tool.rs](../../src/service/dal/tool.rs) | Tool 混合 | 同上 |
| [src/service/dal/message.rs](../../src/service/dal/message.rs) | Message 混合 | 新增 search() 混合方法 |
| [src/service/dal/task.rs](../../src/service/dal/task.rs) | Task 混合 | 同上 |
| [src/service/dal/project.rs](../../src/service/dal/project.rs) | Project 混合 | 同上 |
| [src/service/dal/agent.rs](../../src/service/dal/agent.rs) | Agent 混合 | 同上 |
| **模型层** | | |
| [common/src/models/vector.rs](../../common/src/models/vector.rs) | Vectorizable trait | 6 个 PO 各自实现 embed 文本；SearchMatchInfo 含 fts_rank |
| **零改动面** | | |
| 前端 API DTO 路由、Domain 业务逻辑、已有 Memory 搜索实现 | 零改动 | 对外契约不变；与 memory 搜索解耦 |

---

## 四、关键边界（行为红线）

1. **FTS5 关键词转义**：所有 keyword 入参必须先经 `escape_fts5_keyword()` 转义，禁止直接拼 FTS5 MATCH 语法串（避免语法错误和注入）
2. **MatchType 判定不跨层**：MatchType 打标签只在 DAL 层做，DAO 层不感知向量/混合；Handler 层不传 MatchType（仅传 keyword + query_vector）
3. **向量索引自动维护不变**：PO 的 CRUD 钩子中自动 upsert/delete 向量，禁止应用层手动调用向量 DAO（遗忘则导致搜索漏结果）
4. **综合排序稳定**：Hybrid 组内按 vector_distance 升序，Vector 同，Keyword 按 fts_rank 升序；同组内次级排序主键降序（保证确定性）
5. **三态兼容**：仅 keyword → 仅 FTS5；仅 vector → 仅向量；都传 → 混合；都不传 → 不搜索（按其他条件查询正常）

---

## 五、扩展模式（新增实体全文搜索 6 步模板）

以新增 `Artifact` 实体全文搜索为例：

1. **迁移层**：[migrations/](../../migrations/) 新增 `XXXXXX_artifact_fts5.sql`
   - `artifacts_fts` 虚拟表（trigram，索引 name + description + content）
   - AFTER INSERT/UPDATE/DELETE 触发器 × 3
   - 存量数据回填 INSERT INTO fts SELECT ... FROM main

2. **DAO 层**：[src/service/dao/artifact/sqlite.rs](../../src/service/dao/artifact/sqlite.rs)
   - `ArtifactQuery` 新增 `keyword: Option<String>`
   - 新增 `search_artifacts(keyword) -> Vec<(ArtifactPo, f32)>`：FTS5 MATCH + JOIN + BM25
   - 新增 `ArtifactVectorDao` trait + SQLite impl（collection = `"artifacts"`）

3. **模型层**：[common/src/models/artifact.rs](../../common/src/models/artifact.rs)
   - `ArtifactPo` 实现 `Vectorizable` trait，确定 embed 文本

4. **DAL 层**：[src/service/dal/artifact.rs](../../src/service/dal/artifact.rs)
   - 新增 `search(ArtifactSearch { keyword, query_vector, filters })` 混合方法
   - MatchType 三态判定 + Hybrid→Vector→Keyword 排序（直接抄 skill DAL 模板）
   - 返回每项带 `SearchMatchInfo`

5. **CRUD 钩子自动维护向量**：Artifact 创建/更新/删除时调用 VectorDao upsert/delete
6. **Handler/API**：`ArtifactQuery` keyword 字段接入，前端搜索框传入即可，无需额外改造
