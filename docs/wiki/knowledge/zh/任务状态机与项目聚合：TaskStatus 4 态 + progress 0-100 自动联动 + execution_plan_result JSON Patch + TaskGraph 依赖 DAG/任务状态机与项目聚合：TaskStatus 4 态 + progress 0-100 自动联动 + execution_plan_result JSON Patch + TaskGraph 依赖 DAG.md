---
kind: RAG 原子知识卡
name: 任务状态机 + 项目聚合进度追踪：TaskStatus 4 态流转 + execution_plan_result 结构化存储 + TaskGraph 依赖 DAG Mermaid
category: 业务模块 / 项目任务
scope:
  - "src/models/task.rs"
  - "src/models/project.rs"
  - "src/service/dal/task*.rs"
  - "src/service/dal/project*.rs"
  - "src/service/domain/project/**"
  - "common/src/enums/task.rs"
source_files:
  - common/src/enums/task.rs (TaskStatus 枚举：#[repr(i32)] + #[derive(sqlx::Type)] + From<i64> — 0=Cancelled(软删) 1=Pending 2=InProgress 3=Completed；禁止数字比较，要用枚举匹配)
  - src/models/task.rs#L16-L93 (TaskPo 字段表：status TaskStatus 枚举、progress 0-100 整数、dependencies JSON 数组前置任务、execution_plan/execution_result 两大 JSON 字符串字段)
  - src/models/task.rs#L65-L93 (Task 业务实体：po + search_match + stats + model_call_stats + artifacts 五个可选注入；Domain 层按需要聚合查询，非每次全量)
  - src/models/project.rs#L15-L58 (ProjectPo：owner_agent_id(PMO Agent 可空)、execution_plan/execution_result 同步字段、last_followup_at 巡检时间戳、progress_summary 在业务实体实时算不落库)
  - src/models/project.rs#L60-L80 (Project 业务实体：task_graph(Mermaid 字符串，按需注入)、progress_summary(ProjectProgressSummary 结构含 total/completed/in_progress/cancelled/percentage))
  - src/service/domain/project/task_graph.rs#L1-L60 (build_task_graph_mermaid：基于 Task.dependencies 字段构建 DAG；方向 B 完成→A 执行；按 TaskStatus 分颜色样式分类 cancelled/pending_review/...)
  - src/service/domain/project/service.rs#L1-L120 (ProjectDomain::get_project_detail 聚合：基础 PO → 查所有任务 → 算 progress_summary 百分比 → 调 task_graph 生成 → 注入 artifacts；整体是典型 Domain 多 DAL 组合编排)
  - src/service/domain/project/task.rs#L1-L150 (Task Domain 子模块：update_progress 自动流转 status → progress=0→Pending, >0<100→InProgress, =100→Completed；execution_plan JSON 写入严格 Schema)
  - common/src/api/project_task.rs (UpdateTaskProgressRequest：progress + execution_plan_delta + execution_result_delta；不允许整字段覆盖，前端只传增量，后端按 patch 合并)
  - docs/archive/design-archive/task_design.md（§数据库设计 tasks 表完整字段；§状态机 Cancelled=0 软删约定；§一对多 project_id 关联）
  - docs/archive/design-archive/project_design.md（§Project实体 owner_agent_id 语义；§last_followup_at 项目巡检机制与 CronTrigger project_followup 联动）
  - docs/archive/design-archive/project_management_design.md（§任务状态上报 §实时通知 §补偿机制 4 条设计目标落地状态）
  - docs/archive/plan-archive/项目任务增强.md（落地：execution_plan/result 字段加入、progress_summary 聚合算法、TaskGraph DAG 构建）
  - docs/archive/plan-archive/后台任务管理页面与列表清理接口重构.md（落地：任务 query 接口重构、通用 count 与 PagedResult 统一）
  - docs/wiki/zh/content/功能模块/项目管理/任务管理.md（任务四态卡片 UI + 进度条组件 + 依赖图 Mermaid 渲染）
  - docs/wiki/zh/content/项目概述/核心功能特性/任务协作与执行计划/任务生命周期管理.md（生命周期：创建→分配→pending→in_progress→completed + 取消软删路径）
  - docs/wiki/zh/content/项目概述/核心功能特性/任务协作与执行计划/执行计划与结果追踪.md（execution_plan 结构化字段内容规范：步骤列表 + 预计耗时 + 风险；execution_result 输出结构化条目）
  - docs/wiki/zh/content/功能模块/项目管理/项目管理.md（项目详情聚合页：5 Tab（概览/任务列表/依赖图/对话/产物）+ 进度百分比总览卡）
  - docs/wiki/zh/content/前端应用/页面模块/项目管理页面/任务管理功能.md（前端 TaskTable 组件：status 色标 + progress 进度条 + dependencies 列点击跳转）
  - 【平行卡 1】docs/wiki/knowledge/zh/Memory 系统增强与休息沉淀：四层记忆（Core／Working／Short／Long）+ agent_rest 每天 4 点 settle + load_and_settle 向量去重合并/Memory 系统增强与休息沉淀：四层记忆（Core／Working／Short／Long）+ agent_rest 每天 4 点 settle + load_and_settle 向量去重合并.md（另一条系统触发器 project_followup：每 3600s 扫项目 → 唤醒 owner_agent 跟进 + 更新 last_followup_at）
  - 【平行卡 2】docs/wiki/knowledge/zh/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8 类业务消费者注册/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8 类业务消费者注册.md（TaskEventConsumer 处理 task.status_changed 事件，给用户发 SSE 通知）
---

## §1 概述

**本卡角色**：任务状态机与项目聚合的业务领域知识卡。覆盖 `TaskStatus` 四态枚举（禁止数字大小比较，用 match 分支）+ `progress` 自动联动 status 规则、`TaskPo.execution_plan / execution_result` 两字段 JSON 结构化存储规范（后端 patch 合并，不允许前端整字段覆盖）、`Project.progress_summary` 按任务子状态实时百分比计算算法、以及 `task_graph.rs` 基于 `dependencies` 前置任务数组构建的 Mermaid DAG 可视化链路。**定位：写任务推进代码、前端进度条 UI、项目详情聚合查询、排查状态流转错乱时读。**

- **四态状态机（硬顺序）**：`Pending(1)` → `InProgress(2)` → `Completed(3)`；任意状态可跳 `Cancelled(0)`（软删除）。禁止逆向跳转（Completed→InProgress 的"重新打开"应新建任务而非回退，保证历史审计链完整）。`progress` 字段联动规则：写入 progress 时 Domain 自动 → 0=Pending、1-99=InProgress、100=Completed。如果同时传 status + progress → 以 status 为准，progress 裁剪（防止 status=Completed 但 progress=80 的冲突状态进库）。
- **execution_plan / result 结构化 + patch 增量**：`execution_plan` JSON Schema 固定结构：`{ steps: [{ description, estimated_minutes, risk: "低|中|高" }], total_estimated_minutes, notes }`。`execution_result`：`{ completed_steps: [{ description, actual_minutes, output_summary, artifacts: [path] }], risks_mitigated, issues_found, next_actions }`。接口绝不允许整字段 PUT 覆盖——前端通过 `execution_plan_delta / execution_result_delta` 传增量，Domain `patch_execution_json()` 合并原 JSON 并校验 Schema，非法直接 400。
- **Project 聚合按需注入五字段**：`Project` 业务实体有 5 个 `Option<_>` 字段，不是每次查询都全量拉。Domain 提供 5 个明确方法按需调用（性能 + 最小惊讶）：① `inject_stats` → stats；② `inject_model_call_stats` → model_call_stats；③ `inject_task_graph` → task_graph mermaid；④ `inject_artifacts` → artifacts；⑤ `compute_progress_summary` → progress_summary。项目详情页 5 Tab 各调 1-2 个对应注入，列表页只取 PO 不注入，避免 5+ N+1。

---

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 源码锚点 |
|------|------|---------|---------|
| common/enums/task.rs | TaskStatus 枚举 | 0=Cancelled 1=Pending 2=InProgress 3=Completed；#[repr(i32)] + sqlx::Type + From<i64>；禁止数字比较，用 match | 见 enum 定义 |
| models/task.rs PO | 任务持久化对象 | 24 字段，重点：status(TaskStatus)/progress(0-100)/dependencies(JSON 前置 ID 数组)/execution_plan(Option<String> JSON)/execution_result(Option<String> JSON) | `:L16-L63` |
| models/task.rs Task 实体 | 业务聚合容器 | po + 5 个 Option 注入槽位（search_match/stats/model_call_stats/artifacts） | `:L65-L93` |
| models/project.rs PO | 项目持久化对象 | owner_agent_id(可选 PMO)/last_followup_at/plan+result 同步字段；progress_summary 不落库 | `:L15-L58` |
| domain/project/task_graph.rs | 依赖图 DAG 构建 | build_task_graph_mermaid(tasks, direction) → 基于 dependencies 数组；A 依赖 B = A.dependencies 含 B → 图上画 B 指向 A；按 TaskStatus 分颜色样式分类 | `:L1-L60` |
| domain/project/service.rs | 详情聚合编排 | get_project_detail(ctx, project_id) → 基础 PO → 查同项目所有任务 → compute_progress_summary → inject_task_graph → inject_artifacts，典型 Domain 多 DAL 组合 | `:L1-L120` |
| domain/project/task.rs | Task 子域动作 | update_progress(ctx, task_id, progress, plan_delta, result_delta)：自动联动 status + patch_json 合并 + 校验 + 落库 + 发布 task.status_changed AOP 事件 | `:L1-L150` |
| common/api/project_task.rs | DTO 参数约束 | UpdateTaskProgressRequest：progress(0-100) + plan_delta(Option<Value>)+result_delta(Option<Value>)；禁止整字段覆盖 API | 见 DTO 定义 |

**章节来源**
- [models/task.rs:L16-L93](src/models/task.rs#L16-L93)
- [models/project.rs:L15-L80](src/models/project.rs#L15-L80)
- [domain/project/task_graph.rs:L1-L60](src/service/domain/project/task_graph.rs#L1-L60)
- [domain/project/service.rs:L1-L120](src/service/domain/project/service.rs#L1-L120)

---

## §3 架构约定与扩展模式

### 3.1 新增任务状态流转动作（规范 3 步）

1. **在 Domain task.rs 加方法**：`pub async fn complete_task(&self, ctx, task_id, result_delta)`，内部固定动作模式：① 拉 PO → ② 校验当前状态是否可跳目标态（非法 → bail_err!(ErrorCode::TaskStatusInvalidTransition)）→ ③ 改 status + progress + patch result_json → ④ DAL update → ⑤ publish(TaskStatusChangedEvent { task_id, from, to })
2. **对应枚举分支加 handler 分支**：TaskEventConsumer.handle → match event 类型 → 如果是 status_changed → send_sse 给用户 + 若 project_id Some → ProjectProgressSummary 重算
3. **前端 UI 状态颜色同步更新**：TaskStatus 枚举新增态 → 前端 `utils/status.rs` STATUS_COLORS 映射 + 项目进度 summary 分类百分比也要在分母分子加上新态（否则百分比永远缺一块）

### 3.2 dependencies 字段环检测机制

- `dependencies` 写库前 Domain 内必调 `check_dependency_cycle(tasks)` 函数：把 dependencies 数组建成有向图 → Tarjan 或 Kahn 拓扑排序算法 → 存在环 → ErrorCode::TaskDependencyCycle（带循环的任务 ID 链给前端高亮展示）
- 环检测不在 DAO/DAL（SQL 搞不定），必须在 Domain。创建/更新 Task 时如果 dependencies 变动 → 必须触发环检测，不能跳。

### 3.3 execution_plan/result 版本兼容

- JSON patch 用 JSON Merge Patch（RFC 7396）语义，不是 JSON Patch（RFC 6902）。好处：前端可以只传 `{ completed_steps: [新的一步] }`，后端自动追加，不要求传完整 steps 数组。
- 历史版本不做 schema 升级迁移：旧项目的 plan/result 如果缺字段 → 前端渲染时 `unwrap_or_default()`，不允许服务端批量 UPDATE 改 JSON（DuckDB/Postgres 才方便，SQLite JSONPath 支持有限）。

---

## §4 硬约束与回归红线

1. **TaskStatus 禁止数字比较**（铁律对应 AGENTS.md §4.3）：`if status as i32 >= 2` 一律判 clippy error。所有状态判断必须用 match：`match task.po.status { TaskStatus::Completed => ..., _ => ... }`。新增变体时编译器穷尽性检查自动提醒补分支。
2. **status=0 软删除，DAO 常规查询必过滤**：所有 `query/ list/ find_by_*` 必须默认加 `WHERE status != 0`，只有 `TaskDal::query_historical` 特殊方法不带过滤（给审计面板用）。代码 review 时 DAO 每个 SQL 必须检查 status 过滤。
3. **progress 写库必 clamp(0, 100)**：Domain 层无论参数传什么，`let progress = progress.max(0).min(100)`；负数/101 一律截断，不报错以免前端进度滑动条四舍五入导致 100.0001 被拒。
4. **项目进度百分比 = 各子任务 completed 权重平均**：算法固定（`已完成任务数 × 1 + 进行中任务数 × 0.5 + 待处理任务数 × 0）/ 总任务数（不包含 Cancelled）× 100%`，Cancelled 不算分母（否则「取消一个任务」居然让进度跳上去，反直觉）。算法改任何一项 → 前端所有显示百分比的地方必须同步文案说明变更。
5. **execution_plan/result 禁含敏感信息**：Token、MASTER_KEY、密码等明文绝对不能写进这两字段——这两个字段会通过 project 详情 API 暴露给所有有项目权限的用户。Domain 写这两个字段前过 `sanitize_json_secrets(value)` 通用扫描（匹配 `sk-xxx`、`MASTER_KEY`、`Bearer ` 等正则）。
6. **TaskGraph 图渲染依赖循环防 XSS**：build_task_graph_mermaid 的 task_id 作为 Mermaid `id`，必须过 `sanitize_mermaid_id(task_id)`——禁止含有空格、`"`、括号等字符，否则被前端 Mermaid.js 当作图语法节点注入。Mermaid 渲染不是 XSS 安全的，graph 输入用户可控时必须 ID 白名单过滤。
