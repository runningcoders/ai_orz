# 任务系统设计文档

## 简介

ai_orz 任务系统用于管理用户待办、项目任务、系统自动化任务等。支持多级分配、优先级排序、状态跟踪和软删除。

## 数据库设计

### tasks 表结构

| 字段名 | 类型 | 约束 | 说明 |
|--------|------|------|------|
| `id` | TEXT | NOT NULL PRIMARY KEY | 任务唯一 ID (UUID) |
| `title` | TEXT | NOT NULL | 任务标题 |
| `description` | TEXT | NOT NULL DEFAULT '' | 任务详细描述 |
| `status` | INTEGER | NOT NULL DEFAULT 1 | 任务状态：0=已取消(软删除)，1=待处理，2=进行中，3=已完成 |
| `priority` | INTEGER | NOT NULL DEFAULT 0 | 任务优先级，数值越大优先级越高 |
| `tags` | TEXT | NOT NULL DEFAULT '' | 标签列表(JSON 数组) |
| `due_at` | INTEGER | | 截止时间戳(毫秒)，NULL 表示无截止时间 |
| `root_user_id` | TEXT | NOT NULL | 根用户 ID，任务最终归属用户，用于用户级过滤和统计 |
| `assignee_type` | INTEGER | NOT NULL | 分配对象类型：1=用户，2=Agent |
| `assignee_id` | TEXT | NOT NULL | 分配对象 ID |
| `project_id` | TEXT | | 所属项目 ID，NULL 表示无项目 |
| `start_at` | INTEGER | | 开始时间戳(毫秒)，NULL 表示未开始/未设定 |
| `end_at` | INTEGER | | 实际结束时间戳(毫秒)，完成后填写，NULL 表示未结束 |
| `dependencies` | TEXT | | 前置任务 ID 列表(JSON 数组)，NULL 表示无依赖 |
| `created_by` | TEXT | NOT NULL | 创建人 ID |
| `modified_by` | TEXT | NOT NULL | 最后修改人 ID |
| `created_at` | INTEGER | NOT NULL | 创建时间戳(毫秒) |
| `updated_at` | INTEGER | NOT NULL | 最后更新时间戳(毫秒) |

**启用 SQLite STRICT 模式**，严格类型校验，保证 sqlx 类型推断正确。

## 枚举定义

### TaskStatus 任务状态

| 枚举值 | 数值 | 含义 |
|--------|------|------|
| `Canceled` | 0 | 已取消（软删除），查询默认过滤 |
| `Pending` | 1 | 待处理 |
| `InProgress` | 2 | 进行中 |
| `Completed` | 3 | 已完成 |
| `Archived` | 4 | 已归档，完成后长期归档保存 |

### AssigneeType 分配对象类型

| 枚举值 | 数值 | 含义 |
|--------|------|------|
| `User` | 1 | 分配给用户 |
| `Agent` | 2 | 分配给 Agent |

## DAO 接口设计

### 核心 CRUD 方法

| 方法 | 说明 |
|------|------|
| `insert` | 插入新任务 |
| `find_by_id` | 根据 ID 查询任务，自动过滤已取消任务 |
| `list_by_assignee` | 根据分配对象列出任务，支持分页限制 |
| `list_by_status` | 根据分配对象 + 状态过滤列出任务，支持多个状态同时筛选（最多 4 个） |
| `update` | 更新任务完整信息 |
| `update_status` | 单独更新任务状态 |
| `count_by_assignee` | 统计分配对象的未取消任务总数 |
| `count_by_assignee_and_status` | 统计分配对象指定状态的任务数 |

## 设计决策

### 1. root_user_id 设计目的

任务系统支持任务分配，分配对象可以是用户或 Agent，但任何任务最终归属一个用户（root_user_id）。这保证：
- 派生任务也能追溯到最终用户
- 可以按用户统计和过滤所有任务
- 权限控制可以基于 root_user_id 快速过滤

### 2. 软删除约定

- 已取消任务 `status = 0`
- 所有查询默认添加 `AND "status" != 0` 过滤条件
- 保留历史数据便于审计和恢复

### 3. 多状态查询设计

由于 sqlx 不支持动态 IN 子句（需要 QueryBuilder 且类型推断复杂），当前使用静态可选条件方案：
- 最多支持同时筛选 4 个状态（满足绝大多数场景）
- 使用 `(? IS NOT NULL AND "status" = ?)` 模式实现可选匹配
- 空值不参与匹配，所有状态都保留

### 4. SQL 关键字转义

`status`、`assignee_type` 是 SQLite 保留关键字，作为列名时必须用双引号 `"status"` 转义。

## 版本历史

| 日期 | 变更 | 作者 |
|------|------|------|
| 2026-04-14 | 初始数据层设计，新增 root_user_id 字段 | 王挺 |
| 2026-04-15 | 新增 start_at/end_at/dependencies 三个字段，TaskStatus 新增 Archived 归档状态 | 王挺 |

## 后续待开发

- [ ] DAL 层业务逻辑实现
- [ ] Domain 层领域服务
- [ ] HTTP API 接口
- [ ] 任务通知机制
