---
kind: knowledge_card
name: 项目领域与制品聚合：ProjectService 六能力 + TaskGraph DAG 依赖编排 + Artifact 制品双关联 + 对话上下文聚合
category: 领域建模
scope:
  - src/service/domain/project/**/*.rs
  - src/models/{project,task}.rs
  - src/service/dal/{project,task,artifact}.rs
  - src/service/dao/{project,task,artifact}/**/*.rs
  - src/handlers/project/**/*.rs
source_files:
  - src/service/domain/project/service.rs#L21-L476
  - src/service/domain/project/task_graph.rs#L17-L66
  - src/service/domain/project/artifact.rs#L26-L531
  - src/models/project.rs#L16-L358
  - src/service/dal/project.rs
  - src/handlers/project/mod.rs
  - docs/design/project_design.md
  - docs/design/project_management_design.md
  - docs/plan/项目任务增强.md
  - docs/wiki/zh/content/功能模块/项目管理/项目管理.md
  - docs/wiki/zh/content/项目概述/核心功能特性/任务协作与执行计划/多 Agent 协作机制.md
  - docs/wiki/zh/content/数据模型/项目和任务模型/制品和附件.md
  - docs/wiki/zh/content/架构设计/分层架构设计/Domain 层编排/Domain 层编排.md
---

## §1 概述与定位

本知识卡描述 ai_orz 项目的统一 Project Domain 架构，覆盖 Project/Task/Artifact 三模块整合、任务 DAG 依赖编排、产物双关联（项目级+任务级）、对话上下文聚合四大核心能力。在以下场景触发读取：新增 Project/Task/Artifact 管理面 API、排查任务依赖环形检测或产物归属校验、理解 Project Domain 按需注入模式（task_graph/artifacts/progress_summary）时。所有 CRUD、状态流转、聚合查询均通过 ProjectDomain Trait 六大子管理入口统一暴露，Handler 层禁止直接调用 DAL/DAO。

## §2 关键文件表

| 文件 | 角色 | 核心入口/约束 |
|------|------|---------------|
| [project/service.rs](src/service/domain/project/service.rs) | ProjectManage 核心业务 | `create/get_project/list_by_user/start/complete/archive/transition_status` 六能力；状态流转合法性校验矩阵（§382-475 行）；`get_project` 按需注入 task_graph/artifacts/progress_summary |
| [project/task_graph.rs](src/service/domain/project/task_graph.rs) | 任务 DAG 图构建 | `build_task_graph_mermaid` 入口；dependencies 字段建边方向：前置→当前；TaskStatus→category 映射（Cancelled/Pending/PendingReview/InProgress/Completed/Archived → 6 类样式） |
| [project/artifact.rs](src/service/domain/project/artifact.rs) | ArtifactManage 产物管理 | 创建三入口（attachment/project-level/task-level）；双关联校验：`validate_project_and_task` 强制 task.project_id == project_id；generated_content 乐观锁 expected_updated_at 冲突 409 |
| [models/project.rs](src/models/project.rs) | Project 业务实体聚合 | PO 单向持有 + 7 个 Option 按需字段（search_match/stats/model_call_stats/task_graph/artifacts/progress_summary）；`progress_summary_from_tasks` 实时聚合；`to_prompt_summary` 对话上下文摘要 |
| [dal/project.rs](src/service/dal/project.rs) | ProjectDal 数据业务层 | ProjectFetchOptions 三选项 with_task_graph/with_artifacts/with_progress_summary；纯 PO↔实体转换，Domain 层注入聚合 |
| [project_design.md](docs/design/project_design.md) | 项目基础实体设计 | projects 表 STRICT 模式；root_user_id 所有权 + owner_agent_id 执行负责人双轨；软删除 status = 0 默认过滤 |
| [project_management_design.md](docs/design/project_management_design.md) | 项目管理增强设计 | 单向持有链 Project→Task→Artifact；分层边界 DAO(仅 PO)→DAL(转换)→Domain(仅实体)→Handler(实体+DTO)；写操作接收实体，读操作接收 ID |
| [项目任务增强.md](docs/plan/项目任务增强.md) | 任务增强 Plan 快照 | pkg/utils/graph 零业务约束；按需返回 6 步模板；Artifact 复用 ArtifactDetail DTO 禁建重复结构 |

## §3 架构与约定

```
Handler (参数透传 + Response 映射)
    ↓ 仅调用 ProjectDomain Trait
Project Domain (六大子管理入口)
  ├─ ProjectManage: create/get/list/start/complete/archive + transition_status
  ├─ TaskManage:    CRUD + 依赖 DAG 编排
  ├─ ArtifactManage: attachment/generated_content 双来源 + 项目级/任务级双关联
  ├─ 按需注入区:    get_project → with_task_graph / with_artifacts / with_progress_summary
  └─ 对话上下文:    to_prompt_summary → Prompt 注入
    ↓ 纯业务实体 ↔ PO
DAL 层 (PO↔实体转换 + FetchOptions)
    ↓ 仅 PO
DAO 层 (SQLite sqlx + STRICT)
```

**核心机制要点：**

1. **ProjectManage 六能力 + 统一状态流转**：create/get/list_by_user/start/complete/archive 六个独立语义入口，`transition_status` 作为通用入口承载合法性矩阵校验（Active→PendingReview→InProgress→Completed→Archived），禁止 Deleted 通过状态接口删除。
2. **TaskGraph DAG 依赖编排**：Task.dependencies 存储"前置任务 ID 列表"，图上方向为 前置→当前（执行流向）；TaskStatus→6 类 category（done/doing/todo/cancelled/pending_review/archived），MermaidRenderer 输出 class 语法绑定前端 CSS；跨项目依赖边自动补 external 节点不丢边。
3. **Artifact 双关联 + 三来源**：project_id 必选（权限校验+存储路径），task_id 可选（None=项目级，Some=任务级）；来源三枚举 attachment（引用 Finance 资产，不搬运文件）/generated_content（Agent 写入自有文本 + 乐观锁）/remote_url（预留）；`validate_project_and_task` 强制校验归属一致性。
4. **按需返回 FetchOptions 模式**：get_project 三选项 with_task_graph/with_artifacts/with_progress_summary 独立控制，Option<T> + skip_serializing_if，未请求时不查询、不序列化，旧调用不传参数时响应字节级不变。
5. **对话上下文聚合**：`Project::to_prompt_summary()` 提取 6 个关键字段（ID/名称/描述/状态/负责Agent/运作流程/指导建议），空字段跳过换行，最小化 Prompt token 占用；配合 `enrich_ctx!` 宏注入 project_id + agent_id 到 RequestContext 全链路。

## §4 硬约束与红线

1. **Handler 层禁止直接调用 DAL/DAO**：所有 Project/Task/Artifact 操作必须通过 `ProjectDomain` Trait 六大子入口，违反为架构红线。
2. **pkg/utils/graph 零业务依赖**：禁止引用 Project/Task/Artifact 等任何 domain 类型，新增图形渲染仅实现 GraphRenderer trait。
3. **按需返回 Option 字段约定**：task_graph/artifacts/progress_summary 所有新增大字段一律 `Option<T>` + `skip_serializing_if = "Option::is_none"`，不传 with_* 参数时零查询零序列化。
4. **Artifact 跨域归属校验**：任何 Artifact 操作必须通过 `validate_project_and_task` 校验 task.project_id == project_id，禁止绕过创建未归属产物。
5. **软删除默认过滤**：所有 find_by_id/list_by_*/count_* 查询默认添加 `AND "status" != 0`，Task Cancelled(0)、Project Deleted(0) 均视为软删除。
6. **状态流转合法性矩阵**：transition_status 禁止 Active→Completed 跳级、禁止通过状态接口删除（status=0），非法流转统一返回 InvalidRequest。
7. **对话上下文最小化**：to_prompt_summary 禁止将 execution_plan/execution_result 大字段默认注入，空 workflow/guidance 跳过换行。
8. **状态着色 category 固定命名**：done/doing/todo/cancelled/pending_review/archived 6 类名称不可变更，前端 CSS 强依赖。
9. **Artifact generated_content 乐观锁**：PUT 更新必须携带 expected_updated_at，冲突返回 409，防止多人编辑覆盖。
10. **跨项目依赖边不丢不 panic**：MermaidRenderer 遇到边指向外部节点 ID 时自动补 `(external) <id>` 节点，禁止 panic 或静默丢弃边。
