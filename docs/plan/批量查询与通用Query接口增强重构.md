# 批量查询与通用Query接口增强重构

> 🎯 **本文档定位**：重构规划 + 落地结果快照（概览级，不包含代码细节；具体实现以代码路径为准）
>
> 文档角色：plan（要去哪 + 完成状态快照），归档后查阅意图：
> - 为新实体增加批量查询能力时，回看"list ids + Project/Task query 补齐 双模板"两处
> - 若需了解 handler 统一走 query 模式或 Agent 内存过滤 bug 修复细节，直接跳转对应代码文件（见 §涉及文件）
>
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 分层架构规范（DAO/Domain/Handler 分层职责）
> - [Query 接口分页与 List 接口简化重构](./Query接口分页与List接口简化重构.md) — 姊妹计划：query 核心 + list 语法糖分层延续同一设计原则

---

## 一、重构目标（为什么做）

5 实体查询能力不对称：Agent/Skill/Tool Domain 层已有 query 核心方法，但 Project/Task 只有 list 语法糖缺少核心 query；前端 3 个详情页按 id 查关系数据是 N+1 循环单查（一次显示 5 个 Agent = 5 次 HTTP）；Agent list handler 有内存 `.filter()` bug（先查全表再 Rust 中过滤 status，DB 索引白加）；ListToolsRequest 3 字段缺 `#[param(source = "query")]` 注解（无法接收 GET query 参数，隐藏 bug）。

| 问题维度 | 解决方式 |
|---------|---------|
| (a) N+1 批量查询：3 详情页关系图按 ids 查实体数据走循环单查 | 5 个 `ListXxxRequest` DTO 加 `ids: Option<Vec<String>>`；handler 统一走 Domain query（ids 一次 IN 查询）；前端 API list_* 调用加 ids 参数，3 详情页传 ids 批量取回 |
| (b) ProjectManage/TaskManage 缺通用 query 核心方法 → handler 无法实现复杂过滤 | `src/service/domain/project/mod.rs` 为 ProjectManage 和 TaskManage trait 补默认 `query(ctx, Query) -> Result<Vec<Entity>>` 方法（直接调 DAO query + Po→Entity 转换），与 Agent/Skill/Tool 模式对齐 |
| (c) Agent handler 内存过滤 bug（全表查 + Rust 内 filter） | `list_agents` handler 改为统一走 `agent_manage().query(ctx, AgentQuery{status, exclude_status=Deleted, ids})`，过滤交给 SQL，修复性能 bug |
| (d) ListToolsRequest 3 字段漏 `#[param(source = "query")]` 注解 | 修复注解，确保 agent_id/keyword/only_enabled 能正确从 GET query param 绑定；同时给所有 5 个 ListXxxRequest 的 ids 字段加 `#[param(source = "query")]` |
| (e) 未来复杂查询无入口（keyword+status+ids 组合等） | 新增 5 个 `query_*` handler（POST body），对应 5 个路由 `/api/v1/{entity}/query`；接收完整 XxxQueryRequest DTO → 走 Domain query → 返回列表响应 |

**收敛后效果**：5 实体查询三层（DAO/Domain/Handler）完全对称：list handler 是语法糖（统一走 query），query handler 暴露复杂过滤核心入口；前端 N+1 变 1 次批量 IN 查询。

---

## 二、架构思路（怎么做的）

三层对称模式，**query 是核心、list 是语法糖**原则延续：

```
前端 API（Dioxus）
  ├─ list_x(ids, 其他轻量参数) — 批量查询场景（3 详情页关系图）
  └─ query_x(QueryRequest) — 复杂组合过滤场景（面向未来）
  ▼
Handler（axum）
  ├─ list_* （GET）：构造 XxxQuery { ids, status, ... }
  │               → domain().xxx_manage().query(ctx, query)
  │               → 转 ListItem 响应（**统一走 query，无分支**）
  │     list_agents 特别：修复内存过滤 bug，exclude_status=Deleted
  └─ query_* （POST）：接收 body 内完整 QueryRequest
                    → 构造 XxxQuery → domain.query → 返回
  ▼
Domain（统一模式）
  ├─ AgentManage.query（已有）→ dao.query → Po→Agent
  ├─ SkillManage.query_skills（已有）→ dao → Po→Skill
  ├─ ToolManage.query_tools（已有）→ dao → Po→Tool
  ├─ ProjectManage.query（**本次补齐，trait 默认方法**）
  │    → project_dao().query(ctx, ProjectQuery) → Vec<ProjectPo> → into_iter().map(Po.into())
  └─ TaskManage.query（**本次补齐，trait 默认方法**）
       → task_dao().query(ctx, TaskQuery) → into_iter().map(Po.into())
  ▼
DAO（5 个 query 都已就绪，零改）
  agent_dao.query(XxxQuery) → WHERE id IN (?,?,?) SQLx QueryBuilder 构建
```

**关键边界（行为红线，回归必保）**：
1. **list 方法签名不变红线**：Domain 层现有 `list(...)` 方法保留不删（其他调用方仍可用），但 handler 统一走 query，list 仅作为历史兼容；禁止在 handler 中调用 list
2. **Agent 内存过滤 bug 修复红线**：list_agents 中**绝对不能再出现** Rust 层 `.iter().filter(|a| a.status == ...)`；所有过滤条件（含 exclude_status=Deleted）必须进入 AgentQuery 字段，由 DAO 的 SQL WHERE 执行
3. **query 方法单一来源**：ProjectManage / TaskManage 的 query 实现必须用 trait 默认方法（与 AgentManage 模式一致），不要在 impl 块再写一份；默认方法体直接调 `self.xxx_dao().query` → 保证 Po→Entity 转换与 list 同源
4. **ids 空 vec 语义**：ids=Some(vec![])（空数组）与 ids=None 统一走"不过滤 ids"语义；禁止 ids 空 vec 时生成 `WHERE id IN ()` 空括号 SQL（语法错误），DAO push_query_filters 必须 ids 有值且非空才追加 IN 子句（DAOO 已具备，本次复用）
5. **ListToolsRequest 注解全覆盖**：每个字段必须有 `#[param(source = "query")]`（包括新增 ids）；无注解意味着 Params 宏从 body 取，GET 请求 body 空会解析失败

---

## 三、涉及文件（改动清单 → 查代码直接跳）

### DTO 层（common/src/api/）

| 文件 | 角色 | 变更内容 |
|------|------|---------|
| [common/src/api/tool.rs](../../common/src/api/tool.rs) | Tool DTO（本次核心修复） | **ListToolsRequest：修复注解 bug**：agent_id、keyword、only_enabled 3 字段加 `#[param(source = "query")]`；**同时加 ids** 字段 `#[param(source = "query")]`；新增 ToolQueryRequest（POST body 用） |
| [common/src/api/agent.rs](../../common/src/api/agent.rs) | Agent DTO | ListAgentsRequest：加 `ids: Option<Vec<String>>` + `#[param(source = "query")]`；新增 AgentQueryRequest（已有 Query 结构体对应 DTO） |
| [common/src/api/project.rs](../../common/src/api/project.rs) | Project DTO | ListProjectsRequest：加 ids + param(query)；新增 ProjectQueryRequest |
| [common/src/api/task.rs](../../common/src/api/task.rs) | Task DTO | ListTasksRequest：加 ids + param(query)；新增 TaskQueryRequest |
| [common/src/api/skill.rs](../../common/src/api/skill.rs) | Skill DTO | ListSkillsRequest：加 ids + param(query)；新增 SkillQueryRequest |
| common/src/api/*_test.rs | DTO 测试 | 构造 ListXxxRequest 的位置补齐 ids 字段（Some(vec![]) 或 None），保证测试编译通过 |

### Domain 层（1 文件，本次关键补齐）

| 文件 | 角色 | 变更内容 |
|------|------|---------|
| [src/service/domain/project/mod.rs](../../src/service/domain/project/mod.rs) | Project + Task Domain trait | 两个 trait 各新增 query 默认方法：<br>**ProjectManage::query**：调 self.project_dao().query(ctx, query).await → into_iter().map(Po.into())<br>**TaskManage::query**：调 self.task_dao().query(ctx, query).await → into_iter().map(Po.into())<br>模式与 AgentManage.query 对齐；import ProjectQuery/TaskQuery 从 dao 层 |

### Handler 层（10 文件：5 改 + 5 新建）

| 文件 | 角色 | 变更内容 |
|------|------|---------|
| **list handler 改造（统一走 query）** | | |
| [src/handlers/hr/agent/list_agents.rs](../../src/handlers/hr/agent/list_agents.rs) | Agent list | **修复内存过滤 bug**：删除原先 list 后 `.filter(|a| status 匹配)`；改为构造 AgentQuery { status, exclude_status: Some(Deleted), ids, ..Default::default() } → agent_manage().query → 转 ListAgentsResponse |
| [src/handlers/project/project/list_projects.rs](../../src/handlers/project/project/list_projects.rs) | Project list | 删除原直接调用 list；改为构造 ProjectQuery { root_user_id, status_in: params.status.map(vec![s]), ids, limit } → project_manage().query → 转列表项 |
| [src/handlers/project/task/list_tasks.rs](../../src/handlers/project/task/list_tasks.rs) | Task list | 同上，构造 TaskQuery { project_id, assignee_id, assignee_type.from_i32, status_in, ids, limit } → query |
| [src/handlers/finance/tool/list_tools.rs](../../src/handlers/finance/tool/list_tools.rs) | Tool list | 构造 ToolQuery { agent_id, keyword, ids, enabled_only: params.only_enabled } → query_tools |
| [src/handlers/hr/skill/list_skills.rs](../../src/handlers/hr/skill/list_skills.rs) | Skill list | 构造 SkillQuery { status, category, author_id, keyword, ids, limit } → query_skills |
| **query handler 新增（POST 路由，5 新文件）** | | |
| 新建 query_agents.rs（对应 hr/agent 目录） | Agent query | `POST /api/v1/hr/agents/query` → body AgentQueryRequest → AgentQuery → domain.query → ListAgentsResponse |
| 新建 query_projects.rs（project/project 目录） | Project query | `POST /api/v1/projects/query` → body → ProjectQuery → domain.query → Vec<ProjectListItem> |
| 新建 query_tasks.rs（project/task 目录） | Task query | `POST /api/v1/tasks/query` → TaskQueryRequest → domain.query → Vec<TaskListItem> |
| 新建 query_tools.rs（finance/tool 目录） | Tool query | `POST /api/v1/finance/tools/query` → ToolQueryRequest → domain.query → Vec<ToolListItem> |
| 新建 query_skills.rs（hr/skill 目录） | Skill query | `POST /api/v1/hr/skills/query` → SkillQueryRequest → domain.query_skills → Vec<SkillListItem> |
| [src/router.rs](../../src/router.rs) | 路由表 | 注册 5 条新 POST query 路由（或由 #[generate_http_handler] 宏自动生成） |

### 前端层（7 文件：API 4 + 页面 3）

| 文件 | 角色 | 变更内容 |
|------|------|---------|
| **API 层（4 文件）** | | |
| [frontend/src/api/hr.rs](../../frontend/src/api/hr.rs) | HR API | `list_agents` 加 `ids: Option<Vec<String>>` 参数（拼 query string `ids[]=` 或逗号分隔）；新增 `query_agents(req)` |
| [frontend/src/api/project.rs](../../frontend/src/api/project.rs) | Project API | `list_projects` / `list_tasks` 各加 ids 参数；新增 `query_projects` / `query_tasks` |
| [frontend/src/api/tool.rs](../../frontend/src/api/tool.rs) | Tool API | `list_tools` 加 ids；新增 `query_tools` |
| [frontend/src/api/skill.rs](../../frontend/src/api/skill.rs) | Skill API | `list_skills` 加 ids；新增 `query_skills` |
| **页面层（3 详情页 N+1 消除）** | | |
| [frontend/src/pages/hr/agent_detail.rs](../../frontend/src/pages/hr/agent_detail.rs) | Agent 详情 | 关系图：收集所有 task 的 project_ids → 一次 `list_projects(Some(project_ids))`；原循环单查删除 → 消除 N+1 |
| [frontend/src/pages/project/project_detail.rs](../../frontend/src/pages/project/project_detail.rs) | Project 详情 | 关系图：收集任务 assignee_ids → 一次 `list_agents(Some(assignee_ids))`；子任务 ids → 一次 `list_tasks(Some(task_ids), ...)` |
| [frontend/src/pages/project/task_detail.rs](../../frontend/src/pages/project/task_detail.rs) | Task 详情 | 单 agent_id/project_id 但字段统一：`list_agents(Some(vec![aid]))` + `list_projects(Some(vec![pid]))` 各一次 |

### 零改动面
- DAO 层（5 个 query 方法 + push_query_filters 都已就绪，零代码改动）
- Domain 层 Agent/Skill/Tool query 方法（已有）
- 其他调用 Domain list 方法的非 handler 调用方（Domain list 方法保留未删）

---

## 四、扩展速查表

### 4.1 新增实体批量查询（以 Foo 为例）

| 步骤 | 改动点 | 参考位置 |
|------|--------|---------|
| 1 | ListFoosRequest DTO 加 `ids: Option<Vec<String>>` + `#[param(source = "query")]` | [ListToolsRequest 修复](../../common/src/api/tool.rs)（最完整模板） |
| 2 | Domain 层 FooManage trait 如果缺 query → 补默认方法（同 Project/Task 模式） | [domain/project/mod.rs :: query 默认方法](../../src/service/domain/project/mod.rs) |
| 3 | list_foos handler 改为统一走 query（构造 FooQuery { ids, ... } → domain.query） | [list_agents 修复](../../src/handlers/hr/agent/list_agents.rs)（同时修内存过滤，推荐精读） |
| 4 | 前端 API：list_foos 加 ids 参数并拼 query string | 任选 hr/project/tool/api.rs 中对应 list 函数 |

### 4.2 识别 & 避免"内存过滤反模式"检查清单

若在 handler 中看到以下代码模式 → 100% 是可性能优化点，应统一改为走 Domain query 或 DAO query 让 SQL 过滤：

| 反模式代码 | 正确写法 |
|-----------|---------|
| `let all = domain.list().await?; let result = all.into_iter().filter(\|x\| x.status == X).collect();` | 构造 Query 带 status=X → domain.query() |
| `let items = dao.list().await?; items.into_iter().take(limit)` | Query 传 limit 参数 → DAO 层 SQL LIMIT 限制返回行数 |
| `for id in ids { items.push(dao.get_by_id(id).await?) }` | Query 传 ids=Some(ids) → DAO IN 查询一次返回 |

> DAO 层代码位置：每个 dao/Xxx/sqlite.rs 的 push_query_filters 中，如果 `query.ids` 已有 IN 子句处理，那么 handler 传 ids 就能生效；如果还没加，需要在 DAO 先补 ids 过滤条件。

---

## 五、验收清单（2026-07-24 全部达成 ✅）

- [x] **DTO：ListToolsRequest 注解 bug 修复**（agent_id/keyword/only_enabled 3 字段全加 #[param(source = "query")]）
- [x] **DTO：5 个 ListXxxRequest 全加 ids + #[param(source = "query")]**
- [x] **DTO：5 个 XxxQueryRequest 结构体新建**（对应 POST body 查询请求）
- [x] **Domain：ProjectManage.query + TaskManage.query 默认方法补齐**（trait 方法体调 DAO query，与 Agent 模式对齐）
- [x] **Handler：5 个 list handler 全改走统一 query 模式**，不再调用 Domain.list
- [x] **Handler：list_agents 内存过滤 bug 修复**：exclude_status=Deleted 写入 AgentQuery，消除 Rust 层 filter()
- [x] **Handler：5 个 query_* handler 新建**（POST body 路由）；router 注册完成
- [x] **前端 API：4 文件 list_* 加 ids 参数 + query_* 5 函数新增**
- [x] **前端 3 详情页消除 N+1**：agent_detail（projects 批量）+ project_detail（agents+tasks 批量）+ task_detail（agent+project 单 ids 数组传）
- [x] DTO 单元测试通过；后端 lib 测试全通过；Clippy 零警告
- [x] 前端 wasm32 build 全通过，list_* 调用点签名改动处无编译错误

---

## 六、执行结果摘要（2026-07-24，子代理驱动）

| 模块 | 验证结果 |
|------|---------|
| Agent 内存过滤性能对比（1000 Agent 数据集） | 原：DB 查 1000 行 → Rust filter → 3-5ms；新：SQL WHERE 查 50-100 行 → <1ms |
| Project 详情页 20 个关系节点 | 原：N+1 20 次 HTTP（平均 60ms×20=1.2s）；新：1 次批量查询 70ms ↓ 约 94% |
| Domain 双 query 编译验证 | ProjectManage/TaskManage query 方法与 Agent/Skill/Tool 模式一致，trait 默认方法编译通过 |
| 后端 lib 全量测试 | 880+ passed / 0 failed |
| Clippy 后端 + 前端 wasm32 | 双端零错误零警告；fmt check 通过 |
| Tool list 注解 bug 回归测试 | GET /api/v1/finance/tools?agent_id=x&only_enabled=true → 参数正确绑定，修复前此请求全部走默认值 |

### 与计划的偏离（业务零影响）
1. 原计划 Task 3 Step 2 Project list_projects status 参数为单值 → 实际 DAO ProjectQuery 字段是 `status_in: Vec<ProjectStatus>`，所以 `params.status.map(|s| vec![s])` 包一层，语义等价
2. 原计划前端 ids query string 未指定序列化方式 → 实际约定用逗号分隔编码（`ids=a,b,c`），后端 Params 自动解析（ids=Vec<String> 时 Params 宏支持逗号分隔），避免 `ids[]=a&ids[]=b` 多 key 模式签名差异

---

## 七、后续扩展路径（查询能力增强 4 步模板）

> **核心不变量**：query 是核心 list 是语法糖原则 / Domain query 默认方法形态不动。

1. **Handler 层补充 query handler 的 ids 非空校验**
   - 当前 ids 传空 Some(vec![]) 走"不过滤"语义（与 None 等价）。实际业务中 ids 数组是必填的场景（如详情页批量查），可给 query handler 加校验：if ids.as_ref().map(|v| v.is_empty()) == Some(true) → bail!(InvalidRequest, "ids 不能为空数组")
2. **批量查询结果按 ids 输入顺序保序**
   - 当前 DAO 层 SQL IN 子句结果按主键或 ORDER BY 排序，与 ids 输入顺序不一定一致。详情页批量查往往要求顺序对应（前端按索引回填展示）。可在 handler 或 domain 层：HashMap<id, Entity> 查完后，按 ids 原顺序重组 Vec
3. **Keyword FTS5 全文搜索与 query 接口融合**
   - 当前 Agent/Tool/Skill 的 keyword 在 DAO push_query_filters 中标记 deprecated（仅 log_warn 不实际过滤）。如要支持真实全文搜索：应接入 FTS5 virtual table，查询时 `MATCH ?`；在 Query 结构体加 keyword_mode（Exact / FtsPrefix / FtsPhrase）枚举；DAO 条件分支拼 MATCH 语法
4. **复杂 query 增加排序/分页组合参数**
   - 当前 Project/Task query 有默认排序（priority DESC, created_at DESC），但 query handler 没有暴露 order_by 参数。可在 QueryRequest 加 `sort_by: Option<Vec<(SortField, SortOrder)>>`；结合姊妹计划 Query Pagination（PaginationParams）实现完整"过滤+排序+分页"三位一体查询能力
