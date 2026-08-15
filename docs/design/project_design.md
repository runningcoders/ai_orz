# 项目系统设计文档

> 🎯 **本文档定位**：项目实体模型、任务关联一对多、项目级负责人与进度跟踪、软删除分层设计
> 状态：v1.0（2026-08-15 整理）
> 查阅场景：新增项目 CRUD 接口、排查项目任务级联/归属过滤、理解项目进度计算触发条件时打开；字段级 PO/DAO 定义直接看代码
>
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 整体分层架构
> - [task_design.md](./task_design.md) — 任务实体基础设计（项目的一对多子实体）
> - [project_management_design.md](./project_management_design.md) — 项目管理增强：DAG 依赖/产物/进度实时计算
> - 【③ Wiki 长文（Batch9 新增）】
>   - [项目管理.md](docs/wiki/zh/content/功能模块/项目管理/项目管理.md) — 项目详情 5 Tab 聚合 + 管理面 API 映射
>   - [多 Agent 协作机制.md](docs/wiki/zh/content/项目概述/核心功能特性/任务协作与执行计划/多%20Agent%20协作机制.md) — Project Domain 作为唯一编排层承载 Agent 思考循环
>   - [制品和附件.md](docs/wiki/zh/content/数据模型/项目和任务模型/制品和附件.md) — Artifact 项目级+任务级双关联结构
>   - [Domain 层编排.md](docs/wiki/zh/content/架构设计/分层架构设计/Domain%20层编排/Domain%20层编排.md) — Project Domain 六大子管理入口分层职责
> - 【④ RAG 原子知识卡（Batch9 新增）】
>   - [项目领域与制品聚合：ProjectService 六能力 + TaskGraph DAG 依赖编排 + Artifact 制品双关联 + 对话上下文聚合](docs/wiki/knowledge/zh/%E9%A1%B9%E7%9B%AE%E9%A2%86%E5%9F%9F%E4%B8%8E%E5%88%B6%E5%93%81%E8%81%9A%E5%90%88%EF%BC%9AProjectService%20%E5%85%AD%E8%83%BD%E5%8A%9B%20+%20TaskGraph%20DAG%20%E4%BE%9D%E8%B5%96%E7%BC%96%E6%8E%92%20+%20Artifact%20%E5%88%B6%E5%93%81%E5%8F%8C%E5%85%B3%E8%81%94%20+%20%E5%AF%B9%E8%AF%9D%E4%B8%8A%E4%B8%8B%E6%96%87%E8%81%9A%E5%90%88/%E9%A1%B9%E7%9B%AE%E9%A2%86%E5%9F%9F%E4%B8%8E%E5%88%B6%E5%93%81%E8%81%9A%E5%90%88%EF%BC%9AProjectService%20%E5%85%AD%E8%83%BD%E5%8A%9B%20+%20TaskGraph%20DAG%20%E4%BE%9D%E8%B5%96%E7%BC%96%E6%8E%92%20+%20Artifact%20%E5%88%B6%E5%93%81%E5%8F%8C%E5%85%B3%E8%81%94%20+%20%E5%AF%B9%E8%AF%9D%E4%B8%8A%E4%B8%8B%E6%96%87%E8%81%9A%E5%90%88.md) — ProjectManage 六能力 + FetchOptions 按需注入模式 + 10 条分层红线

## 简介

ai_orz 项目系统用于组织和管理相关任务，支持任务分组、项目级状态跟踪、负责人分配和软删除。项目与任务是一对多关系，任务通过 `project_id` 关联到项目，项目不存储任务列表，保证数据一致性。

## 数据库设计

### projects 表结构

| 字段名 | 类型 | 约束 | 说明 |
|--------|------|------|------|
| `id` | TEXT | NOT NULL PRIMARY KEY | 项目唯一 ID (UUID) |
| `name` | TEXT | NOT NULL | 项目名称 |
| `description` | TEXT | NOT NULL DEFAULT '' | 项目详细描述 |
| `status` | INTEGER | NOT NULL DEFAULT 1 | 项目状态：0=已删除(软删除)，1=活跃，2=进行中，3=已完成，4=已归档 |
| `priority` | INTEGER | NOT NULL DEFAULT 0 | 项目优先级，数值越大优先级越高 |
| `tags` | TEXT | NOT NULL DEFAULT '' | 标签列表(JSON 数组) |
| `root_user_id` | TEXT | NOT NULL | 根用户 ID，项目最终归属用户，用于用户级过滤和统计 |
| `owner_agent_id` | TEXT | | 项目负责人 Agent ID，NULL 表示无负责人 |
| `start_at` | INTEGER | | 开始时间戳(毫秒)，NULL 表示未开始 |
| `due_at` | INTEGER | | 截止时间戳(毫秒)，NULL 表示无截止时间 |
| `end_at` | INTEGER | | 实际结束时间戳(毫秒)，完成后填写，NULL 表示未结束 |
| `created_by` | TEXT | NOT NULL | 创建人 ID |
| `modified_by` | TEXT | NOT NULL | 最后修改人 ID |
| `created_at` | INTEGER | NOT NULL | 创建时间戳(毫秒) |
| `updated_at` | INTEGER | NOT NULL | 最后更新时间戳(毫秒) |

**启用 SQLite STRICT 模式**，严格类型校验，保证 sqlx 类型推断正确。

## 枚举定义

### ProjectStatus 项目状态

| 枚举值 | 数值 | 含义 |
|--------|------|------|
| `Deleted` | 0 | 已删除（软删除），查询默认过滤 |
| `Active` | 1 | 活跃，可接收新任务 |
| `InProgress` | 2 | 进行中 |
| `Completed` | 3 | 已完成 |
| `Archived` | 4 | 已归档，完成后长期归档保存 |

状态完全对齐任务状态，保持统一语义。

## 设计关系

### 项目与任务的关系

- **扁平结构设计**：项目不维护任务列表，任务通过 `project_id` 字段关联项目
- **优势**：避免双向同步问题，增删任务无需修改项目记录，数据一致性更好
- **查询方式**：查询项目下的任务直接通过任务表 `WHERE project_id = ?` 过滤

## DAO 接口设计

### 核心 CRUD 方法

| 方法 | 说明 |
|------|------|
| `insert` | 插入新项目 |
| `find_by_id` | 根据 ID 查询项目，自动过滤已删除项目 |
| `list_by_root_user` | 根据 root_user_id 列出用户所有项目，支持分页 |
| `list_by_root_user_and_status` | 根据 root_user_id + 状态过滤列出项目，支持多个状态同时筛选（最多 5 个） |
| `update` | 更新项目完整信息 |
| `update_status` | 单独更新项目状态 |
| `count_by_root_user` | 统计用户的未删除项目总数 |
| `count_by_root_user_and_status` | 统计用户指定状态的项目数 |

## 设计决策

### 1. root_user_id 设计目的

任何项目最终归属一个用户（root_user_id）。这保证：
- 可以按用户统计和过滤所有项目
- 权限控制可以基于 root_user_id 快速过滤
- 即使项目转移，所有权关系依然清晰

### 2. owner_agent_id 设计目的

项目可绑定一个负责人 Agent：
- 用于项目级自动化，由负责人 Agent 处理项目内任务
- 可空设计表示不需要 Agent 自动化的手动项目
- 区别于 root_user_id：root_user_id 是所有权，owner_agent_id 是执行负责人

### 3. 软删除约定

- 已删除项目 `status = 0`
- 所有查询默认添加 `AND "status" != 0` 过滤条件
- 保留历史数据便于审计和恢复

### 4. 多状态查询设计

由于 sqlx 不支持动态 IN 子句（需要 QueryBuilder 且类型推断复杂），当前使用静态可选条件方案：
- 最多支持同时筛选 5 个状态（满足绝大多数场景）
- 使用 `(? IS NOT NULL AND "status" = ?)` 模式实现可选匹配
- 空值不参与匹配，所有状态都保留

### 5. SQL 关键字转义

`status` 是 SQLite 保留关键字，作为列名时必须用双引号 `"status"` 转义。

## 版本历史

| 日期 | 变更 | 作者 |
|------|------|------|
| 2026-04-15 | 初始数据层设计，完成 PO + DAO 开发 | 王挺 |

## 后续待开发

- [ ] DAL 层业务逻辑实现
- [ ] Domain 层领域服务
- [ ] HTTP API 接口
- [ ] 项目任务统计接口
