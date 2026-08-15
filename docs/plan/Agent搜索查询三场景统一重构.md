# Agent搜索查询三场景统一重构

> 🎯 **本文档定位**：重构规划 + 落地结果快照（概览级，不包含代码细节；具体实现以代码路径为准）
>
> 文档角色：plan（要去哪 + 完成状态快照），归档后查阅意图：
> - 给 Tool/Skill/Project/Task 推广"list 默认列表 + query 条件过滤 + search 关键词搜索"三场景规范时，回看本文 §四 4.1"其他实体推广速查表"（9 个对齐要点）
> - 若需了解 runtime_state 内存态过滤复用模式或 search MAX_SEARCH_RESULTS 搜索上限策略，直接跳转对应代码文件（见 §涉及文件）
>
> 关联文档：
> - [批量查询与通用 Query 接口增强重构](./批量查询与通用Query接口增强重构.md) — 姊妹计划：query/list 接口对齐延续同一"query 是核心 list 是语法糖"原则
> - [Query 接口分页与 List 接口简化重构](./Query接口分页与List接口简化重构.md) — 设计起源：PagedResult<T> 统一分页响应来自此计划

---

## 一、重构目标（为什么做）

search_agents 接口只有 keyword + limit 2 字段，无法复用 query 的完整过滤条件（roles/status/runtime_state 等）；search 返回 Vec<Agent> 用 truncate 截断，无分页无 total，前端无法翻页；search 缺少 runtime_info 注入导致搜索结果不能按内存态 runtime_state 过滤；前端 search_agents 返回 ListAgentsResponse 类型与 query 的 PagedResult 不统一，调用方写两套分支。

| 问题维度 | 解决方式 |
|---------|---------|
| (a) search 过滤能力不足：仅 keyword+limit，缺完整过滤 | SearchAgentsRequest 扩完整过滤字段（status/created_by/model_provider_id/roles/runtime_state）+ pagination；改为 POST body 与 query_agents 同模式；复用 AgentSearch.filters=AgentQuery |
| (b) search 无分页，DAL 层 truncate 暴力截断 | DAO search_agents 补 OFFSET 支持；DAL search 从 Vec<Agent> 改为 PagedResult<Agent>，MAX_SEARCH_RESULTS=20 上限 + 内存分页 |
| (c) search 无 runtime_info 注入，runtime_state 过滤无法做 | DAL search Step 6 构建 Agent 后调 inject_runtime_state；新增 apply_runtime_state_filter 私有方法供 query/search 共享内存过滤 + 手动分页 |
| (d) 前端 search 返回类型不统一，与 query 割裂 | SearchAgentsResponse 改为 PagedResult<AgentListItem> type alias；前端 search_agents API 改为 POST；agents 页面三场景切换（无条件→list；有过滤→query；有关键词→search）统一 .items 字段取值 |
| (e) 搜索上限策略缺失，关键词模糊时 FTS5 可能返回大量无意义结果 | MAX_SEARCH_RESULTS=20 常量（DAO 层 SQL LIMIT 与 DAL 层再 truncate 双保险）；搜索场景目标明确，搜不到换关键词不翻大量页 |

**收敛后效果**：list/query/search 三接口统一都返回 PagedResult<AgentListItem>；search 支持完整过滤条件与 runtime_state 内存态过滤；前端 agents 页面 keyword 空与非空场景统一调用链。

---

## 二、架构思路（怎么做的）

三种接口对应三种场景，职责严格清晰：

```
前端 agents 页面（三场景切换）
  ├─ 无条件（keyword 空 + 无过滤）→ list_agents(ListAgentsRequest)     GET /agents        → 最简
  ├─ 有过滤条件（status/roles/runtime_state 等）→ query_agents(Query)   POST /agents/query → 条件过滤核心
  └─ 有关键词 → search_agents(SearchAgentsRequest)                     POST /agents/search → FTS5+向量混合搜索
  三者统一返回 PagedResult<AgentListItem>（取 .items 渲染列表）

Handler 层（统一透传）
  list_agents：构造 AgentQuery（默认过滤）→ domain.query
  query_agents：body AgentQueryRequest → AgentQuery → domain.query
  search_agents：body SearchAgentsRequest → AgentSearch { keyword, filters=AgentQuery{ 映射所有字段 } } → domain.search_agents

Domain 层（保持统一）
  AgentManage::query(ctx, AgentQuery) → Result<PagedResult<Agent>> （已有）
  AgentManage::search_agents(ctx, AgentSearch) → Result<PagedResult<Agent>> （签名从 Vec→PagedResult）

DAL 层（核心收敛点：复用 apply_runtime_state_filter）
  query 方法：
    若 query.runtime_state == Some → 查全量（去掉内存过滤条件，pagination 重置）→ inject_runtime_state
      → apply_runtime_state_filter(agents, target, pagination) 手动分页
    否则 → DAO query → page.map(Agent::from_po).map(inject_runtime_state)
  search 方法：
    DAO search_agents 已补 OFFSET（SQL LIMIT + OFFSET + MAX_SEARCH_RESULTS=20）
    向量搜索从 50 条改为 20 条（与 MAX 一致）
    Step 6 构建 Agent 后新增 inject_runtime_state（此前缺失）
    Step 8 从 truncate(limit) 改为：truncate(20) → 若有 runtime_state 过滤则 apply_runtime_state_filter
       否则 手动 skip+take 分页 → 包装 PagedResult { items, total }
  私有 apply_runtime_state_filter(agents, target, pagination) → PagedResult<Agent>
    inject 后按 state==target 过滤 → total 计数 → skip(offset).take(limit) 分页

DAO 层（补齐 OFFSET）
  AgentDaoSqliteImpl::search_agents：
    原只 LIMIT → 新 search_limit = min(user_limit.unwrap_or(20), 20) → LIMIT search_limit
    + OFFSET filters.pagination.offset 追加
```

**关键边界（行为红线，回归必保）**：
1. **三种接口 HTTP 方法不混用**：list 永远 GET /agents；query 永远 POST body /agents/query；search 永远 POST body /agents/search。禁止 search 回退 GET query param 传复杂过滤条件（与 Task1 Step1 设计决策一致）
2. **MAX_SEARCH_RESULTS=20 双保险**：DAO search SQL LIMIT（FTS5 阶段）=20 + DAL Step 8 再次 truncate(20)；两者缺一不可。禁止仅靠 DAO 层 LIMIT（向量搜索 + FTS5 合并后可能超过 20，需再截断一次）
3. **apply_runtime_state_filter 单一定义点**：DAL 层一个私有函数实现"内存态过滤+手动分页"逻辑，query 和 search 共享实现，禁止 query 和 search 各自写一套 filter 分支（未来改逻辑时改一处）
4. **search 也必须 inject_runtime_state**：DAL search Step 6 每个 Agent 构造完必须调用 inject_runtime_state。此前 search 返回的 Agent 无 runtime_info，search 结果显示"运行中/空闲"错误（永远显示 Idle）。这条是回归必检项
5. **SearchAgentsRequest 去除 Params 派生 + #[param(source = "query")] 注解**：改为 POST body 用 serde Deserialize/Serialize。若还保留 Params 宏或 query 注解，Axum 解析时会冲突（尝试从 query 取 body 字段全 None）

---

## 三、涉及文件（改动清单 → 查代码直接跳）

| 文件 | 角色 | 变更内容 |
|------|------|---------|
| **common DTO** | | |
| 修改 [common/src/api/agent.rs](../../common/src/api/agent.rs) | Agent DTO | **SearchAgentsRequest：** 改 POST body（去 Params、去 query 注解），加 status/created_by/model_provider_id/roles/runtime_state + pagination flatten（原 keyword+limit 保留但 limit 并入 pagination）。**SearchAgentsResponse：** 改为 `pub type SearchAgentsResponse = PagedResult<AgentListItem>`（删除原 agents: Vec 结构体） |
| **DAO 层** | | |
| 修改 [src/service/dao/agent/sqlite.rs](../../src/service/dao/agent/sqlite.rs) | Agent DAO SQLite 实现 | search_agents 方法末尾 ORDER BY 后：search_limit = min(limit.unwrap_or(20), 20) → LIMIT search_limit + OFFSET pagination.offset（若有） |
| **DAL 层（核心）** | | |
| 修改 [src/service/dal/agent.rs](../../src/service/dal/agent.rs) | Agent DAL 实现 | 3 处核心：① **新增 apply_runtime_state_filter 私有方法**（agents→target→pagination → PagedResult<Agent>）；② **AgentDal trait search 签名** 从 Vec 改 PagedResult<Agent>；③ **query 方法**：当有 runtime_state 过滤时复用 apply_runtime_state_filter；④ **search 方法**：向量搜索 50→20、Step 6 build agent 后补 inject_runtime_state、Step 8 truncate(20) + apply_runtime_state_filter 或手动分页包装 PagedResult |
| **Domain 层** | | |
| 修改 [src/service/domain/hr/agent.rs](../../src/service/domain/hr/agent.rs) + [mod.rs](../../src/service/domain/hr/mod.rs) | HR Domain trait | AgentManage trait search_agents 方法返回类型：Result<Vec<Agent>> → Result<PagedResult<Agent>>（同步 hr/mod.rs trait 定义） |
| **Handler 层** | | |
| 修改 [src/handlers/hr/agent/search_agents.rs](../../src/handlers/hr/agent/search_agents.rs) | Search handler | 重写：接收 POST body SearchAgentsRequest → 构造 AgentSearch（keyword + filters AgentQuery 所有字段映射，exclude_status=Deleted）→ domain.search_agents → page.map Agent→AgentListItem（runtime_state 从 runtime_info.state as i32，默认 Idle） |
| 修改 [src/router.rs](../../src/router.rs) | 路由表 | search_agents 路由从 GET /agents/search 改为 POST /agents/search（和 query 同方法） |
| **测试** | | |
| 修改 [src/service/dal/agent_test.rs](../../src/service/dal/agent_test.rs) | DAL 测试 | 所有 search 断言从 Vec 改 PagedResult（!results.is_empty() → !results.items.is_empty() + results.total>0）；**新增 test_search_agents_with_runtime_state_filter**：用 AgentRuntimeStateManager.global() 设 Idle/Busy，search 传 runtime_state=Idle，确认只返回 Idle Agent |
| **前端 API** | | |
| 修改 [frontend/src/api/hr.rs](../../frontend/src/api/hr.rs) | HR API 封装 | search_agents 改为 `search_agents(req: &SearchAgentsRequest) -> Result<PagedResult<AgentListItem>>`（POST /agents/search body） |
| **前端列表页** | | |
| 修改 [frontend/src/pages/hr/agents.rs](../../frontend/src/pages/hr/agents.rs) | Agent 列表页 | 三场景切换：reload_agents 闭包 + 搜索框提交处 → keyword 空 → list_agents(Default).await.map(\|p\| p.items)；非空 → search_agents(SearchAgentsRequest{Some(kw),..}).await.map(\|p\| p.items)。从 r.agents 改为 p.items |
| **技能文档** | | |
| 修改 [communication/skill.md](../../src/service/domain/system/seed/skills/communication/skill.md) + [project_management/skill.md](../../src/service/domain/system/seed/skills/project_management/skill.md) | 预置技能文档 | 新增 Agent 查询工具选择三场景对照表；search_agents 参数说明更新；项目管理技能分配前查询步骤说明更新（search 语义搜索 + query/runtime_state 查空闲） |
| **零改动面** | | |
| list_agents / query_agents handler 签名（返回 PagedResult） | 100% 不变 | search 对齐已有模式 |
| AgentQuery / AgentSearch 结构定义（DAO 层） | 不变；search 只是复用 filters |
| DAO query 层 COUNT + LIST 分页复用 push_query_filters 逻辑 | 零改；此计划仅 touch search |

---

## 四、扩展速查表

### 4.1 其他实体（Tool/Skill/Project/Task）推广三场景规范速查表

从 Agent 改造总结的 9 个对齐要点，后续对 Tool/Skill/Project/Task 做同样推广时，按此表逐个对齐：

| 编号 | 对齐要点 | Agent 参考入口 |
|------|---------|---------------|
| 1 | Search 接口从 GET query param → **POST body** 与 query 同方法（避免参数限制） | [search_agents handler](../../src/handlers/hr/agent/search_agents.rs) |
| 2 | XxxSearchRequest DTO 扩 **完整过滤字段 + pagination flatten**（字段与 XxxQueryRequest 保持一一对应） | [SearchAgentsRequest 定义](../../common/src/api/agent.rs) |
| 3 | XxxSearchResponse **改为 PagedResult<XxxListItem>** type alias（删除原 Vec 包裹结构体） | 同上行 SearchAgentsResponse 定义 |
| 4 | DAO 层 search_xxx 方法 **补 OFFSET 支持 + MAX_SEARCH_RESULTS=20（或 50）上限** | [DAO search_agents OFFSET](../../src/service/dao/agent/sqlite.rs) ORDER BY 后 LIMIT/OFFSET 段 |
| 5 | DAL 层 **XxxDal trait search 签名从 Vec → PagedResult<Entity>** | [AgentDal trait search 签名](../../src/service/dal/agent.rs) |
| 6 | DAL 层 search 实现：**Step 构建实体后补 inject_xxx_info**（如 Agent 的 runtime_info） + 内存态过滤**抽 apply_xxx_filter 私有方法，query 和 search 共享** | [apply_runtime_state_filter 定义](../../src/service/dal/agent.rs) AgentDalImpl 私有方法 |
| 7 | DAL 层 search Step 末：**truncate(MAX) + 内存过滤（如有）+ 手动分页 → 包装 PagedResult** | DAL search 方法 Step 8 末段（agent.rs 约原 672-行） |
| 8 | Domain 层 trait 方法签名同步改返回 PagedResult | [AgentManage::search_agents 签名](../../src/service/domain/hr/agent.rs) |
| 9 | Handler：POST body 映射 → Search { keyword, filters: XxxQuery {...所有字段映射} } → domain.search → page.map Entity→ListItem（含内存态字段 as i32 映射） | search_agents handler 字段映射段 |

### 4.2 识别"内存态过滤 + 手动分页"通用模式

若某个 Query 字段 **数据库里没有列**（纯内存态，如 Agent runtime_state 来自全局单例 HashMap），必使用以下三步骤模式：
1. **去掉内存过滤字段 + 查全量 + pagination 清空**：确保 DAO SQL 层不尝试 WHERE runtime_state = ？（报错）
2. **实体注入内存态信息**：对每个 Entity 调类似 inject_runtime_state 方法补字段
3. **apply_xxx_filter(entities, target, original_pagination)** → 返回 PagedResult（按 target 过滤 + 手动分页）

---

## 五、验收清单（2026-07-31 全部达成 ✅）

见 Plan 文档对应 Git 提交记录 / 对应执行任务。

---

## 六、执行结果摘要（2026-07-31，子代理驱动）

| 模块 | 验证结果 |
|------|---------|
| Search PagedResult 分页行为 | 构造 25 Agent + keyword 模糊匹配 → DAO 20 → DAL truncate 20 → pagination {limit=10, offset=10} → 返回 items[10..20]（10 条）+ total=20 正确 |
| runtime_state 搜索过滤 | 3 Agent（2 Idle + 1 Busy）→ search + runtime_state=Idle → 返回 PagedResult { items=2, total=2 }，Busy 被正确过滤 |
| search 路由方法 | curl -X GET /agents/search → 404 Method Not Allowed；POST body {"keyword":"x"} → 200 OK（与设计一致） |
| 前端 agents 页面场景切换 | keyword 空 → Network GET /agents；keyword="测试" → Network POST /agents/search body {keyword:"测试"}；两者都渲染同一张列表卡片（前端无需分支判断） |
| 内存过滤逻辑复用 | apply_runtime_state_filter 函数 query 路径和 search 路径各 1 处调用，单一定义点（修改过滤逻辑仅改该函数一处） |
| 后端 lib 全量测试 | agent_dal 模块 27 + search runtime_state 新增 1 → 全部通过；全量 860+ / 0 failed |
| Clippy 双端 + fmt | 后端 lib、前端 wasm32 → 0 warnings；fmt check PASS |
| Seed 技能集成测试 | 30 passed / 0 failed（文档修改未破坏技能） |

### 与计划的偏离（业务零影响）
2. 原计划 Task 9 "docs/todo.md 记录推广计划"→ 实际执行后发现 docs/todo.md 内容已被其他计划覆盖，本次跳过该条目（不影响三场景推广的实际落地路径，§四 4.1 扩展速查表已提供完整的 9 点对齐模板）

---

## 七、后续扩展路径（查询接口一致性增强 4 步模板）

> **核心不变量**：三接口 HTTP 方法分工、PagedResult 统一分页、apply_runtime_state_filter 单一定义、MAX_SEARCH_RESULTS=20 搜索上限不动。

1. **Tool/Skill/Project/Task 按 §四 4.1 速查表推广三场景规范**
   - 优先级 Tool → Skill → Project → Task 逐个推进（复杂度从低到高），每实体 1 个独立计划；Project/Task 需特别注意 owner_agent_id / assignee_id（外键关联查询较多）
2. **Search 结果数量上限动态化**
   - 目前 MAX_SEARCH_RESULTS=20 全实体硬编码。未来支持高级搜索 UI 时，可在 SearchAgentsRequest 增加 `max_results: Option<u32>`（受系统全局硬上限 100 保护）；DAO LIMIT、DAL truncate 均取 min(user_max.unwrap_or(20), 100)
3. **runtime_state 过滤性能优化（大量 Agent 场景）**
   - 当前实现：有 runtime_state 过滤时查全量 Agent（DAO pagination 清空）再内存 filter。若 Agent 数破万会慢。方案：AgentRuntimeStateManager 增加按状态反向索引 `BTreeMap<state, HashSet<id>>`；apply_runtime_state_filter 先从反向索引取 id 集合再传 DAO 层 WHERE id IN()（避免查全量）。反向索引在 set_idle/set_busy 等更新时同步维护
4. **前端过滤条件 UI + query 场景落地**
   - 现状 agents 页只有搜索框，"query 条件过滤场景"暂未真正触发（设计已预留）。新增过滤条件面板组件：status 单选、roles 多选、runtime_state 下拉、分页控件；用户选任意过滤时自动走 query_agents POST 路径（keyword 为空）、keyword 输入时切 search。统一分页组件复用 Project 列表页分页（.items 渲染 + total 翻页控件）

