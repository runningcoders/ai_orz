# Query 接口分页与 List 接口简化重构

> 🎯 **本文档定位**：重构规划 + 落地结果快照（概览级，不包含代码细节；具体实现以代码路径为准）
>
> 文档角色：plan（要去哪 + 完成状态快照），归档后查阅意图：
> - 新增实体分页查询时，回看"DAO 改造模板 + List/Query 职责划分"两处即可，无需通读全文
> - 若需了解 PagedResult/PaginationParams 定义或 COUNT + LIMIT/OFFSET 双查询模式，直接跳转对应代码文件（见 §涉及文件）
>
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 分层架构规范
> - [API 协议规范](../design/api_protocol_convention.md) — 前后端 DTO 契约约定
> - 参考实现：[mcp_server/sqlite.rs](../../src/service/dao/mcp_server/sqlite.rs) — COUNT + push_query_filters 模式金标准

---

## 一、重构目标（为什么做）

5 个核心实体（Agent/Project/Task/Tool/Skill）查询接口形态不一：query 只有裸 limit 无 offset 无 total，list 接口混杂了查询语义承担了 query 的职责，前端批量查询只能调用 list 传 None 拼接。

| 问题维度 | 解决方式 |
|---------|---------|
| (a) query 接口无 offset 分页 / 不返回 total 数量 | QueryRequest / DAO Query 结构体统一嵌入 `PaginationParams { limit, offset }`，返回 `PagedResult<T> { items, total }` |
| (b) list 接口混杂查询功能（list_projects 带 root_user_id、list_tasks 带 status 过滤等），职责不清 | list 简化为**纯分页语法糖**：GET 只传 pagination，内部固定默认过滤+排序；查询功能全走 POST query 接口 |
| (c) 前后端 5 个实体 × 8 处响应字段不一致（resp.agents / resp.projects 各不同） | 统一 `PagedResult<T>`，前端统一用 `resp.items` 访问 |
| (d) DAO 层 COUNT 和 LIST 查询 WHERE 条件写两份，维护易漂移 | 每 DAO 抽取 `push_query_filters(builder, query)` 函数，两条 SQL 共用 |
| (e) 前端关系图批量查询（按 ids 查 agents/projects/tasks）无处可去，乱用 list 传参 | 前端新增 query_* API 函数，6 个查询场景从 list_* 切换到 query_* |

**收敛后效果**：5 实体 list/query 双接口形态完全对齐（DTO→DAO→Domain→Handler→前端 API 5 层 × 5 实体 = 50 处改动统一模式），新增实体分页仅需按模板复制 push_query_filters + 双查询。

---

## 二、架构思路（怎么做的）

5 层统一模式，每层只做一件事：

```
前端（Dioxus）
  │  list 场景：list_*(limit, offset) → PagedResult<T>
  │  查询场景：query_*(QueryRequest { 过滤, pagination }) → PagedResult<T>
  ▼
Handler（axum）
  │  list handler：纯分页语法糖 → 内部构造最小 Query（默认过滤+排序）→ 调 domain.query
  │  query handler：接收完整 POST body → 调 domain.query
  ▼
Domain（trait + impl）
  │  5 个 Manage trait 的 query 方法统一 Result<PagedResult<Entity>>
  │  实现：dao.query → PagedResult::map(Po → Entity)（一行转换）
  ▼
DAO（SQLite）
  │  push_query_filters：WHERE 条件唯一真相源（COUNT 和 LIST 共用）
  │  SQL1：SELECT COUNT(*) FROM table WHERE 1=1 [filters] → total
  │  SQL2：SELECT columns FROM table WHERE 1=1 [filters] ORDER BY ... LIMIT ? OFFSET ? → items
  ▼
DTO（common::api）
  └─ PaginationParams { limit: Option<usize>, offset: Option<usize> }
  └─ PagedResult<T> { items: Vec<T>, total: usize }
```

**关键边界（行为红线，回归必保）**：
1. **list 语法糖红线**：5 个 list handler**只能接受 pagination 参数**，绝不接受任何查询过滤（ids/status/keyword 等）；默认过滤硬编码 handler 内部（如 list_agents 排除 Deleted、list_projects 取当前用户、list_skills 排除 Expired）
2. **query 唯一查询入口**：任何带 ids/status_in/project_id/assignee 等过滤的调用，必须走 query 接口，禁止通过 list 传参绕过
3. **COUNT 与 LIST WHERE 对齐**：push_query_filters 是唯一条件写入点，禁止在 COUNT 或 LIST SQL 上单独追加条件（防止 total 与 items 过滤口径不一致）
4. **OFFSET 必须配 LIMIT**：SQLite 语法要求，仅 offset 无 limit 时追加 `LIMIT -1`（语义 = 无上限），保证 SQL 合法
5. **PagedResult<T> 字段统一**：前后端所有 list/query 响应都是 `{items, total}`，禁止出现 items/agents/projects/tools/skills/tasks 等多样命名

---

## 三、涉及文件（改动清单 → 查代码直接跳）

按 5 实体 × 5 层矩阵索引（共约 50 处改动，统一模式）：

### DTO 层（common/src/api/）

| 文件 | 角色 | 变更内容 |
|------|------|---------|
| [common/src/api/mod.rs](../../common/src/api/mod.rs) | 分页类型定义 | 金标准参考：PaginationParams（第 55-83 行）+ PagedResult<T> 定义（零改动） |
| [common/src/api/agent.rs](../../common/src/api/agent.rs) | Agent DTO | AgentQueryRequest 移除裸 `limit: Option<usize>` → 加 `#[serde(flatten)] pagination: PaginationParams`；ListAgentsRequest 简化为只含 pagination |
| [common/src/api/project.rs](../../common/src/api/project.rs) | Project DTO | ProjectQueryRequest 同上；ListProjectsRequest 简化同上 |
| [common/src/api/task.rs](../../common/src/api/task.rs) | Task DTO | TaskQueryRequest 同上；ListTasksRequest 简化同上 |
| [common/src/api/tool.rs](../../common/src/api/tool.rs) | Tool DTO | ToolQueryRequest 移除裸 `limit + offset` 两字段 → 加 flatten pagination；ListToolsRequest 简化 |
| [common/src/api/skill.rs](../../common/src/api/skill.rs) | Skill DTO | SkillQueryRequest 同上；ListSkillsRequest 简化同上 |

### DAO 层（src/service/dao/）

| 文件 | 角色 | 变更内容 |
|------|------|---------|
| [src/service/dao/agent/mod.rs + sqlite.rs](../../src/service/dao/agent/) | Agent DAO | AgentQuery 结构加 pagination；trait query 签名改 PagedResult<AgentPo>；sqlite 抽取 push_query_filters（ids/status/created_by/model_provider_id/roles 6 条件）+ COUNT + ORDER BY created_at DESC + LIMIT/OFFSET |
| [src/service/dao/project/mod.rs + sqlite.rs](../../src/service/dao/project/) | Project DAO | 同模式；push_query_filters 包含软删除过滤 status!=0 + ids + root_user_id + status_in；排序 ORDER BY priority DESC, created_at DESC |
| [src/service/dao/task/mod.rs + sqlite.rs](../../src/service/dao/task/) | Task DAO | 同模式；push_query_filters 包含 status!=0 + ids + assignee_type/assignee_id + project_id + status_in；排序 priority DESC, created_at DESC |
| [src/service/dao/tool/mod.rs + sqlite.rs](../../src/service/dao/tool/) | Tool DAO | 同模式 + agent_id 条件用 INNER JOIN agent_tools（COUNT 和 LIST 都动态拼接 JOIN 子句）；push_query_filters 用 `t.` 表别名；排序 agent 过滤用 at.created_at ASC，无过滤用 t.created_at DESC |
| [src/service/dao/skill/mod.rs + sqlite.rs](../../src/service/dao/skill/) | Skill DAO | 同模式；push_query_filters 包含 ids/status/exclude_status/category/author_id/parent_skill_id/tags 7 条件；排序 ORDER BY updated_at DESC |

### Domain 层

| 文件 | 角色 | 变更内容 |
|------|------|---------|
| [src/service/domain/hr/mod.rs](../../src/service/domain/hr/mod.rs) | HR Trait | `AgentManage::query` 返回改 Result<PagedResult<Agent>>；`SkillManage::query_skills` 返回改 Result<PagedResult<Skill>> |
| [src/service/domain/hr/agent.rs](../../src/service/domain/hr/agent.rs) | Agent 实现 | `AgentDomainImpl::query` 调用 dao.query → `.map(|po| Agent::from_po(po))`（用 PagedResult::map） |
| [src/service/domain/hr/skill.rs](../../src/service/domain/hr/skill.rs) | Skill 实现 | 同 Agent 模式 |
| [src/service/domain/project/mod.rs](../../src/service/domain/project/mod.rs) | Project Trait | `ProjectManage::query` + `TaskManage::query` 返回改 PagedResult |
| [src/service/domain/project/{project,task}.rs](../../src/service/domain/project/) | Project/Task 实现 | 同模式，PagedResult::map 转实体 |
| [src/service/domain/finance/mod.rs](../../src/service/domain/finance/mod.rs) | Finance Trait | `ToolProviderManage::query_tools` 返回改 PagedResult |
| [src/service/domain/finance/tool_provider.rs](../../src/service/domain/finance/tool_provider.rs) | Tool 实现 | 同模式 |

### Handler 层（后端 10 个文件）

| 文件 | 角色 | 变更内容 |
|------|------|---------|
| [query_agents.rs](../../src/handlers/hr/agent/query_agents.rs) 等 5 个 query handler | Query 入口 | 接收 QueryRequest（POST body）→ 构造 DAO Query → domain.query → 返回 PagedResult<ListItem> |
| [list_agents.rs](../../src/handlers/hr/agent/list_agents.rs) | List Agent | 语法糖：只接受 pagination，内部构造 AgentQuery（exclude_status=Deleted，其余默认）→ domain.query → PagedResult.map 转 ListItem |
| [list_projects.rs](../../src/handlers/project/project/list_projects.rs) | List Project | 语法糖：pagination + 固定 root_user_id=ctx.uid() + status!=0 |
| [list_tasks.rs](../../src/handlers/project/task/list_tasks.rs) | List Task | 语法糖：pagination + 固定 status!=0 |
| [list_tools.rs](../../src/handlers/finance/tool/list_tools.rs) | List Tool | 语法糖：pagination（无默认过滤） |
| [list_skills.rs](../../src/handlers/hr/skill/list_skills.rs) | List Skill | 语法糖：pagination + 固定 exclude_status=Expired |

### 前端层（API + 页面 15 文件）

| 文件 | 角色 | 变更内容 |
|------|------|---------|
| [frontend/src/api/hr.rs](../../frontend/src/api/hr.rs) | HR API | list_agents/list_skills 简化签名：只接受 (limit, offset)；新增 query_agents(req)/query_skills(req)（POST） |
| [frontend/src/api/project.rs](../../frontend/src/api/project.rs) | Project API | list_projects/list_tasks 同上；新增 query_projects/query_tasks |
| [frontend/src/api/finance.rs](../../frontend/src/api/finance.rs) | Finance API | list_tools 同上；新增 query_tools |
| [pages/hr/agent_detail.rs](../../frontend/src/pages/hr/agent_detail.rs) | Agent 详情 | 关系图任务/项目批量查询：list_tasks 传参 → 改用 query_tasks(assignee_id+assignee_type) + query_projects(ids) |
| [pages/project/project_detail.rs](../../frontend/src/pages/project/project_detail.rs) | Project 详情 | 关系图 assignee_ids 批量查 Agent：list_agents(Some(&ids)) → query_agents(ids) |
| [pages/project/task_detail.rs](../../frontend/src/pages/project/task_detail.rs) | Task 详情 | id 查 Agent/Project：list_x(Some(&[id])) → 改用 query_x(ids) |
| [pages/project/tasks.rs](../../frontend/src/pages/project/tasks.rs) | 任务列表 | 筛选（project_id/status/at）从 list_tasks 传参 → query_tasks(req) |
| [pages/hr/agents.rs](../../frontend/src/pages/hr/agents.rs) + [skills.rs](../../frontend/src/pages/hr/skills.rs) | 列表页 | list_x(None) → list_x(None, None)；resp.agents/skills → resp.items |
| [pages/finance/tools.rs](../../frontend/src/pages/finance/tools.rs) | 工具页 | 同上 |
| [pages/project/projects.rs](../../frontend/src/pages/project/projects.rs) + [artifacts.rs](../../frontend/src/pages/project/artifacts.rs) | 项目页 | 同上，list_projects 适配 |
| [pages/project/task_edit_modal.rs](../../frontend/src/pages/project/task_edit_modal.rs) | 任务编辑弹窗 | list_agents/list_projects 调用适配新签名 |
| [pages/message/chat.rs](../../frontend/src/pages/message/chat.rs) | 聊天页 | list_projects 调用适配 |
| [hooks/use_workspace_data.rs](../../frontend/src/hooks/use_workspace_data.rs) | 工作区数据 | list_agents/list_projects/list_tasks 适配 |

### 零改动面（验证架构稳定性）
- common::api::PaginationParams + PagedResult<T> 定义（金标准零改动）
- DB 表结构 / 所有实体 Po 字段定义
- 路由（handler 路径不变）

---

## 四、扩展速查表（新增实体分页时的 5 步模板）

### 4.1 DAO 改造模板（以 Foo 实体为例）

| 步骤 | 内容 | 参考位置 |
|------|------|---------|
| 1 | `FooQuery` 结构体加 `pagination: PaginationParams` | [agent/mod.rs :: AgentQuery](../../src/service/dao/agent/mod.rs) |
| 2 | 定义 `fn push_query_filters(builder, query)`：所有 WHERE 条件（含默认软删除）都写这里 | [agent/sqlite.rs :: push_query_filters](../../src/service/dao/agent/sqlite.rs) |
| 3 | query 方法：① COUNT SELECT + ① LIST SELECT + push_query_filters 各调一次 + ORDER BY + LIMIT/OFFSET 收尾 + 返回 PagedResult {items, total} | [agent/sqlite.rs :: query](../../src/service/dao/agent/sqlite.rs) |

> 代码入口：选 5 实体中字段最相近的一个 DAO 整文件复制，改表名/字段名即可。推荐 ToolDAO 作为复杂 JOIN 场景模板，AgentDAO 作为单表简单模板。

### 4.2 List 语法糖默认过滤速查

| 实体 | list 默认过滤 | 默认排序 |
|------|--------------|---------|
| Agent | exclude_status = Deleted | created_at DESC |
| Project | root_user_id = ctx.uid() + status != 0 | priority DESC, created_at DESC |
| Task | status != 0 | priority DESC, created_at DESC |
| Tool | （无，全量） | 无 agent 过滤：t.created_at DESC；有 agent 过滤：at.created_at ASC |
| Skill | exclude_status = Expired | updated_at DESC |

> 代码入口：各 handler list_xxx.rs 内部构造 Query 的 Default::default() 覆盖部分。

---

## 五、验收清单（2026-07-24 全部达成 ✅）

见 Plan 文档对应 Git 提交记录 / 对应执行任务。

---

## 六、执行结果摘要（2026-07-24，子代理驱动）

| 模块 | 验证结果 |
|------|---------|
| DAO 层（5 实体 query + PagedResult） | 单测适配通过；COUNT 与 LIST 查询 total 口径一致 |
| Domain 层（5 trait 方法签名变更） | 编译通过，PagedResult::map 语义正确 |
| Handler 层（5 query + 5 list = 10 handler） | 路由不变；集成测试返回结构统一 PagedResult |
| 后端 lib 全量测试 | 850+ passed / 0 failed |
| 前端单元测试 | 全部 PASS |
| 前端 release build | 编译成功（无 PagedResult 字段访问遗留错误） |
| Clippy 后端 + 前端 wasm32 | 双端零错误零警告 |

### 与计划的偏离（业务零影响）
1. 原计划 Task 13 步骤 1-4 关系图改造代码块用具体 req 构造示例 → 实际执行时 QueryRequest 字段名与示例一致，无调整；但前端 Default::default() 派生在部分 DTO 上需手工实现（已有宏派生）
2. 原计划 list_projects 移除 root_user_id 查询参数 → 实际验证：之前前端调用本就不传 root_user_id，一直由后端 ctx 取，无前端破坏性变更

---

## 七、后续扩展路径（新增分页实体 4 步模板）

> **核心不变量**：PaginationParams / PagedResult / push_query_filters 模式不动。

1. **DTO 层**：参考 [common/src/api/agent.rs](../../common/src/api/agent.rs)
   - FooQueryRequest：加 `#[serde(flatten)] pub pagination: PaginationParams`（去裸 limit/offset）
   - ListFoosRequest：只保留 pagination 字段（GET query param）
2. **DAO 层**：选 [Agent DAO](../../src/service/dao/agent/sqlite.rs)（简单）或 [Tool DAO](../../src/service/dao/tool/sqlite.rs)（有 JOIN）复制
   - mod.rs：FooQuery 结构加 pagination；trait query 返回改 Result<PagedResult<FooPo>>
   - sqlite.rs：① push_query_filters（包含默认过滤条件）② query：COUNT + LIST 双 SQL，都 push_query_filters + ORDER BY + LIMIT/OFFSET
3. **Domain + Handler + 前端**：
   - Domain：Manage trait 改 query 返回 + impl 用 PagedResult::map 转
   - Handler：2 文件 — query_foos（POST body）+ list_foos（语法糖 + 固定默认过滤）
   - 前端：api/foo.rs（list_foos 简化 + query_foos）+ 页面调用点按 list/query 语义选择
4. **测试验证**：
   - 后端：cargo test --lib 验证 dao 测试 + handler 测试
   - 前端：cargo build --release 确保 items 字段访问无编译错误

