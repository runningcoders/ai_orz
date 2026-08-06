# 项目管理

项目管理技能是在模拟人类团队的「项目制工作法」：把一堆复杂的工作拆成可落地的「任务」、排好先后顺序（DAG 依赖 = 先做 A 才能做 B）、指派合适的人（Agent）去做、定期追进度（progress 0→100 = 项目经理每周开周会）、把交付物归档保存（Artifact = 项目文档/设计稿/代码提交/测试报告）。Project Owner 就像项目经理（PM）——对最终交付负责任务拆分和全局调度；Task Owner 就像负责某个模块的工程师——把分配到的任务做完、有阻塞就问 PM、交付时附交付物。**execution_plan 是你开干前写的「我准备怎么做」，execution_result 是你干完后写的「我实际做了什么 + 交付了什么 + 有哪些坑」**——这些不只是给别人看的，也是未来你自己或其他同事接手时能快速读懂背景的「工作交接单」。

本指南帮助你使用项目管理工具，高效管理项目、任务、产物。这是你执行结构化工作的核心能力——从需求拆解、任务推进到成果沉淀的完整闭环。

**核心协作模型**：项目采用 **Project Owner 主导 + Task Owner 执行** 的两层结构。Project Owner 负责需求拆解、任务分配、全局调度和项目重启规划；Task Owner 负责完成具体任务并上报结果。详见"协作沟通流程"章节。

## 工具分类与加载机制

**关键认知**：项目管理 / 产物工具 tags 是 `project_management` / `file_management`，非 neural。加载 / 匹配规则（「安装范围 + match_keys 匹配」双层机制）、如何安装技能包（`install_skill_pack(tag=project_management)`）等通用规则请参见「技能管理」技能，本指南不赘述。

## 项目/任务/产物层次

```
Project（项目）
  └─► Task（任务）
       └─► Artifact（产物）
```

- **项目**：顶层容器，聚合相关任务和对话，有 `owner_agent_id`（关联 Agent）
- **任务**：具体的工作单元，有状态、进度（0-100）、负责人、DAG 依赖
- **产物**：工作成果的持久化保存，分 `GeneratedContent`（独立存储）和 `Attachment`（引用附件）两类

## 项目管理

### 项目状态机

| 状态 | 值 | 含义 | 说明 |
|------|---|------|------|
| `Deleted` | 0 | 软删除 | **不可通过状态接口设置**，必须用删除 action |
| `Active` | 1 | 活跃（默认） | 正常可用状态 |
| `PendingReview` | 2 | 待审核 | Agent 创建后待用户审核 |
| `InProgress` | 3 | 进行中 | 自动填入 `start_at` |
| `Completed` | 4 | 已完成 | 自动填入 `end_at` |
| `Archived` | 5 | 已归档 | 归档保留 |

**合法转换**：
```
Active        → PendingReview / InProgress / Archived
PendingReview → Active / InProgress / Archived
InProgress    → Completed / Archived
Completed     → InProgress（项目重启）/ Archived
相同状态      → 允许（no-op）
其它          → 非法
```

> **项目重启**：`Completed → InProgress` 是特殊转换，支持 Project Owner 在项目完成后收到新需求时重新规划。详见"项目重启"章节。

### `create_project` — 创建项目

**参数**（`CreateProjectRequest`）：
- `name` — 项目名称（**必填**）
- `description` — 描述（可选）
- `priority` — 优先级（可选，整数）
- `tags` — 标签列表（可选）
- `owner_agent_id` — 关联 Agent ID（可选，**纯透传**，后端不调 `resolve_agent`）

**返回**：`CreateProjectResponse`（统一为 `GetProjectResponse` 完整字段）。

**注意**：Agent 创建的项目默认进入 `PendingReview` 状态待用户审核。

### `get_project` — 获取项目详情

**参数**：
- `id` — 项目 ID（必填，路径）
- `with_stats` — 是否加载唤醒次数统计
- `with_model_call_stats` — 是否加载模型调用统计
- `stats_time_start` / `stats_time_end` — 统计时间范围（毫秒，必须同时存在）
- `stats_interval` — 时序粒度：`hourly` / `daily`
- `with_task_graph` — 是否返回任务依赖图（Mermaid 字符串）
- `with_artifacts` — 是否一并返回产物列表
- `with_progress_summary` — 是否返回项目进度汇总（**推荐跟进项目时开启**）

**进度汇总（`with_progress_summary=true`）**：实时计算不持久化，根据项目下所有任务的 `status` + `progress` 汇总得出：
- `total_tasks` / `completed` / `in_progress` / `pending` / `blocked` / `cancelled`
- `overall_percent`：整体进度（Σ task.progress / total）
- **项目 Owner 跟进进度时务必传 `with_progress_summary=true`**，无需自己逐任务计算

### `list_projects` — 列出项目

**行为**：仅分页参数（`limit` / `offset`），**内部固定过滤 `root_user_id = ctx.uid()`**，排除 `status=0`，按 `priority DESC, created_at DESC` 排序。

### `query_projects` — 通用查询项目

**参数**（POST body）：
- `ids` — 按 ID 批量查询
- `keyword` — 关键词搜索
- `root_user_id` — 根用户过滤
- `status_in` — 状态列表过滤（OR 语义）
- `pagination` — 分页

### `update_project` — 更新项目

**参数**（全部可选）：`name` / `description` / `priority` / `tags` / **`execution_plan`** / **`execution_result`**。

> **`execution_plan`（Owner Agent 填）**：项目执行计划，描述阶段划分和推进策略。**项目启动时由 Project Owner 写入**，阶段有重大调整时更新。内容示例：
> ```
> # Phase 1: 项目脚手架（3 天）
> - [ ] 搭建 Dioxus 前端项目骨架，配置路由、状态管理
> - [ ] 建立 Axum 后端模块分层，接入 SQLite + sqlx
> - [ ] 集成用户鉴权（JWT + HttpOnly Cookie）
>
> # Phase 2: 核心功能（5 天）
> - [ ] 项目/任务/产物管理 CRUD
> - [ ] Agent 绑定工具 + 基础执行循环
>
> # Phase 3: 测试与部署（2 天）
> - [ ] 集成测试 + CI 流水线
> - [ ] 部署文档
> ```
>
> **`execution_result`（Owner Agent 填）**：项目完成后的总结。**项目收尾阶段填写**，包括成果概述、产出清单、经验教训。

### `update_project_status` — 更新项目状态

**参数**：`status: ProjectStatus`。**不能设置为 `Deleted`**（需走删除 action）。

## 任务管理

### 任务状态机

| 状态 | 值 | 含义 | 说明 |
|------|---|------|------|
| `Cancelled` | 0 | 取消（相当于删除） | **不可通过状态接口设置** |
| `PendingReview` | 1 | 待审核 | Agent 创建后待用户审核 |
| `Pending` | 2 | 待启动（默认） | 审核通过后待启动 |
| `InProgress` | 3 | 进行中 | 自动填入 `start_at` |
| `Completed` | 4 | 已完成 | 自动填入 `end_at` |
| `Archived` | 5 | 已归档 | 总结后归档 |

**合法转换**：
```
PendingReview → Pending / InProgress / Archived
Pending       → InProgress / Archived
InProgress    → Completed / Archived
Completed     → Archived
相同状态      → 允许（no-op）
其它          → 非法
```

**负责人类型**（`AssigneeType`）：
- `User = 0`
- `Agent = 1`（默认）

### `create_task` — 创建任务

**参数**（`CreateTaskRequest`）：
- `title` — 任务标题（**必填**）
- `description` — 任务描述（可选）
- `priority` — 优先级（可选）
- `tags` — 标签列表（可选）
- `root_user_id` — 根用户（为空默认当前用户）
- `assignee_type` — 负责人类型（为空默认 `Agent`）
- `assignee_id` — 负责人 ID（**必填**）
- `project_id` — 关联项目 ID（可选）
- `due_at` — 截止时间（可选，毫秒时间戳）
- `dependencies` — 依赖任务 ID 列表（可选，构成 DAG）

**关键副作用**：当 `assignee_type = Agent` 时，**自动发送任务分配通知消息**给目标 Agent（通过 `send_task_assignment`）。通知失败不影响任务创建。

### `get_task` — 获取任务详情

**参数**：
- `id` — 任务 ID（必填，路径）
- `with_stats` / `with_model_call_stats` / `stats_time_*` / `stats_interval` — 统计选项
- `with_artifacts` — 是否一并返回关联产物

**返回**：完整字段含 `thinking_depth` / `progress` / `created_by` / `modified_by` / `dependencies`。

### `list_tasks` — 列出任务

**行为**：仅分页参数，**固定排除 `status=0`**，按 `priority DESC, created_at DESC` 排序。

### `list_project_tasks` — 列出项目下任务

**参数**：`project_id`（路径）+ `status`（可选）+ `limit`（可选）。

### `list_agent_tasks` — 列出 Agent 任务

**参数**：`agent_id`（路径）+ `status`（可选）+ `limit`（可选）。

**适用**：查看分配给自己（或指定 Agent）的待办任务。

### `query_tasks` — 通用查询任务

**参数**（POST body）：
- `ids` — 按 ID 批量查询
- `keyword` — 关键词搜索
- `project_id` / `assignee_type` / `assignee_id` — 上下文过滤
- `status_in` — 状态列表过滤
- `pagination` — 分页

### `update_task` — 更新任务

**参数**（全部可选）：`title` / `description` / `priority` / `tags` / `due_at` / `dependencies` / **`execution_plan`** / **`execution_result`**。

> **`execution_plan`（Task Owner 填）**：任务执行计划，**开始执行任务前写入**。描述你打算如何完成这个任务。内容示例：
> ```
> 步骤 1：确认需求与前置依赖
>   - 读取 task.description 中约定的接口协议
>   - 检查 dependencies 中的前置任务（Task-xxx）状态应为 Completed
>   - 如阻塞，立即通过 send_task_assignment_message 上报给 Project Owner
>
> 步骤 2：实现核心功能（预计占 70%）
>   - 新建 handlers/xxx 路由层：GET /xxx、POST /xxx
>   - 在 dal/xxx.rs 实现 SQL 查询与写入（含 sqlx::query! 宏）
>   - 用 create_task 新增"编写单测"子任务
>
> 步骤 3：测试与验证（占 30%）
>   - 写集成测试覆盖正常 / 异常 / 边界场景
>   - 本地跑 cargo test --test xxx_test 验证通过
>   - 产出物：create_text_artifact 保存测试报告
> ```
>
> **`execution_result`（Task Owner 填）**：任务执行结果总结，**完成或阻塞时写入**。描述实际完成情况、产出物、风险点。内容示例：
> ```
> 完成情况：
> - handlers/xxx 路由 3 个端点全部实现并自测通过
> - dal/xxx 新增 5 条 SQL（含事务 + 软删除校验）
> - 编写集成测试 8 条，全部通过
>
> 产出物：
> - Artifact: 测试报告.md（id=art-xxx）
> - Artifact: 接口说明.md（id=art-yyy）
> - Task 子任务"编写单测"已完成（task_id=tsk-zzz）
>
> 遗留与风险：
> - upload_file 端点超过 10MB 文件时可能报错（超出 64KB 文本附件限制），建议后续接入文件分片或走 register_artifact_from_path 方案
> ```
>
> 写 execution_plan / execution_result 时请**尽量具体**，方便 Project Owner 与后续接手者快速理解背景和进展。

### `update_task_status` — 更新任务状态

**参数**：`status: TaskStatus`。**不能设置为 `Cancelled`**（需走取消 action）。受状态机约束，非法转换返回 `InvalidRequest`。

### `update_task_progress` — 更新任务进度

**参数**：`progress: i32`（0-100）。

**行为**：进度自动 clamp 到 `[0, 100]`，触发 `TaskEvent { event_type: "progress_updated" }` 事件。

### `mark_done` — 标记任务完成

**参数**（`MarkDoneParams`）：
- `task_id` — 任务 ID（**必填**）
- `summary` — 完成总结（可选，但建议填写）

**行为**：调用 `task_manage().complete()` 便捷方法，**绕过状态机校验**——即使任务在 `Pending` 状态也能直接标记完成。会设置 `status = Completed` + `progress = 100` + `end_at`。

**适用**：快速完成任务闭环。若需要严格状态机校验，用 `update_task_status(status = Completed)`。

## 产物管理

产物是你工作的持久化成果，重要工作应保存为产物。

### 产物来源类型

| 类型 | 值 | 存储方式 | 内容更新 |
|------|---|---------|---------|
| `Attachment` | 1（默认） | **不复制内容**，仅引用 `attachments/{relative_path}` | ❌ 不能直接更新内容 |
| `GeneratedContent` | 2 | 独立写入产物存储 | ✅ 可更新内容 |
| `RemoteUrl` | 3 | 预留 | 当前 `create_artifact` 返回 `Unsupported` |

### 文件类型（产物与附件共享）

| 类型 | 值 | 适用 |
|------|---|------|
| `Document` | 0（默认） | 文本类（Markdown / PDF / TXT 等） |
| `Image` | 1 | 图片 |
| `Audio` | 2 | 音频 |
| `Video` | 3 | 视频 |
| `Binary` | 4 | 通用二进制（ZIP / EXE 等） |

### `create_text_artifact` — 创建文本产物

**用途**：直接提交文本内容创建 `GeneratedContent` 产物（≤1MB）。

**参数**（`CreateTextArtifactParams`）：
- `project_id` — 关联项目 ID（**必填**）
- `task_id` — 关联任务 ID（可选，None = 项目级产物）
- `name` — 产物名称（**必填**）
- `description` — 描述（可选）
- `content` — 文本内容（**必填**，≤1MB）
- `file_name` — 文件名（可选，默认 `{name}.md`）
- `mime_type` — MIME 类型（可选，默认 `text/plain`）
- `file_type` — 文件类型（可选，默认 `Document`）
- `tags` — 标签列表（可选）

**适用**：报告、方案、设计文档、代码片段、配置文件、分析结论、总结。

### `register_artifact_from_path` — 从文件注册产物

**用途**：将 Agent 工作目录中的文件注册为 `GeneratedContent` 产物。文件被**复制**到产物存储，源文件保留。

**参数**（`RegisterArtifactFromPathParams`）：
- `project_id` — 关联项目 ID（**必填**）
- `task_id` — 关联任务 ID（可选）
- `name` — 产物名称（**必填**）
- `description` — 描述（可选）
- `source_path` — 源文件路径（**必填**，**相对 Agent 工作目录**）
- `file_name` — 文件名（可选，默认取 basename）
- `mime_type` — MIME 类型（可选，默认按扩展名推断）
- `file_type` — 文件类型（可选，默认按 mime 推断）
- `tags` — 标签列表（可选）

**关键约束**：
- 必须存在 `agent_id`（仅 Agent 可调用）
- `source_path` 必须是文件（不能是目录）
- **路径穿越防御**：使用 `canonicalize` 规范化后用 `starts_with` 校验，`source_path` 必须在 `agents/{agent_id}/` 目录之下
- 失败时自动回滚删除已创建的产物记录

**适用**：已生成的大文件、二进制文件、多文件目录结构。

### `create_artifact` — 通用创建产物

**用途**：按 `source_type` 分支创建产物，是连接附件与产物的桥梁。

**参数**（`CreateArtifactRequest`）：
- `source_type` — 来源类型（必填）
- **Attachment 模式**：需要 `attachment_id`，不能同时携带 `content` / `file_name` / `mime_type`
- **GeneratedContent 模式**：需要 `content` + `file_name`
- 公共字段：`project_id`（必填）/ `task_id`（可选）/ `name` / `description` / `mime_type` / `file_type` / `tags`

**Attachment 模式行为**：
- 跨 Domain 调用 `finance::domain().attachment_manage().get_attachment()` 获取附件
- 使用 `attachments/{relative_path}` 作为逻辑路径
- **不复制文件内容**，仅建立引用关系
- 文件实际存储仍在 Finance Attachment 模块下

### `update_artifact` — 更新产物（统一部分更新）

**用途**：统一更新产物的内容或元数据。**仅 `Some` 字段被更新，`None` 字段保持不变**。

**参数**（`UpdateArtifactRequest`，全部可选）：
- `artifact_id` — 产物 ID（必填，路径）
- `content` — 新内容（**仅 `GeneratedContent` 类型可用**，≤1MB）
- `name` — 新名称（trim 后不能为空）
- `description` — 新描述
- `tags` — 新标签列表
- `expected_updated_at` — 乐观锁（期望的 `updated_at`，不匹配返回 409 Conflict）

**关键约束**：
- `Attachment` 类型产物**不能直接更新内容**（须通过原 Attachment 修改）
- 元数据更新（`name` / `description` / `tags`）适用于所有类型
- 乐观锁冲突时返回 `Conflict`，需重新加载后再试

### `query_artifacts` — 查询产物

**参数**（`ArtifactQueryRequest`，POST body）：
- `project_id` / `task_id` — 上下文过滤
- `file_type` — 文件类型过滤
- `source_type` — 来源类型过滤
- `pagination` — 分页

**适用**：
- 了解团队成员已完成的工作
- 查找可复用的已有产物
- 避免重复创建

## 附件管理

附件是 Finance Domain 的实体，与产物（Project Domain）通过 `ArtifactSourceType::Attachment` 建立引用关系。

### 关键约束

- **文本大小限制**：64KB（`MAX_TEXT_CONTENT_BYTES`）
- **文件名安全校验**：不能含 `/` / `\` / `..`，不能是绝对路径，必须是单个文件名
- **文本类型校验**：`file_type` 必须是 `Document`，mime 必须是 text 类（`text/*` / `application/json` / `yaml` / `toml` / `xml` / `javascript` 等）
- **权限模型**：`attachment.root_user_id == ctx.uid()`，非 owner 访问返回 `NotFound`（避免泄露存在性）
- **软删除**：`delete_attachment` 是软删除，数据保留供审计

### `create_text_attachment` — 创建文本附件

**参数**（`CreateTextAttachmentRequest`）：
- `file_name` — 文件名（**必填**，安全校验）
- `content` — 文本内容（**必填**，≤64KB）
- `mime_type` — MIME 类型（可选，按扩展名推断）
- `purpose` — 用途标签（如 `skill` / `message` / `artifact` / `tool_result`）

### `get_attachment` / `get_attachment_content` — 获取附件

- `get_attachment` — 返回附件元信息（`AttachmentDetail`）
- `get_attachment_content` — 返回附件文本内容（仅 Document 类型可读）

### `list_attachments` — 列出附件

**参数**（`AttachmentListQuery`）：
- `purpose` — 用途过滤（如 `skill` / `message` / `artifact` / `tool_result`）
- `file_type` — 文件类型过滤
- `pagination` — 分页

### `update_attachment_content` — 更新附件内容

**参数**：
- `id` — 附件 ID（必填，路径）
- `content` — 新内容（**必填**，≤64KB）
- `expected_updated_at` — 乐观锁（可选）

### `delete_attachment` — 删除附件

**行为**：软删除，数据保留供审计。

## 权限模型

| 实体 | 权限校验 | 失败行为 |
|------|---------|---------|
| **产物** | `project.root_user_id == ctx.uid()` | 返回错误 |
| **附件** | `attachment.root_user_id == ctx.uid()` | 返回 `NotFound`（避免泄露存在性） |
| **项目** | `root_user_id == ctx.uid()`（`list_projects` 固定过滤） | 看不到他人项目 |
| **任务** | 通过项目关联校验 | 返回错误 |

**跨 Domain 创建引用型产物**：要求当前用户同时是 Attachment 的 owner 和 Project 的 root_user（隐式双重校验）。

## 工作目录规范

- 你只能操作自己的工作目录 `agents/{你的ID}/`
- `register_artifact_from_path` 的 `source_path` 必须相对该目录，且经过 `canonicalize + starts_with` 校验
- 文件会被**复制**到产物存储，源文件保留，不影响你的工作副本
- 不要尝试访问其他 Agent 的目录（路径穿越会被拒绝）

## 角色分配约束

项目与任务的分配遵循**串行负责制**，确保 Agent 专注且可追溯。

### 分配规则

| 角色 | 可负责 | 串行限制 | 约束 |
|------|--------|----------|------|
| **前台 Agent** | ❌ 不能负责项目 | — | 遇到复杂需求应创建项目并转交专业 Agent 作为 Owner |
| **Project Owner** | 1 个项目（至完结） | 同一时间只能负责 1 个未完结项目 | 负责需求拆解、任务分配、进度汇总、成果总结、项目重启规划 |
| **Task Owner** | 1 个任务（至完结） | 同一时间只能负责 1 个未完成任务 | 负责完成具体任务并上报最终结果给 Project Owner |

**前台 Agent 转交流程**：用户提出复杂需求 → 前台 Agent 分析后 → `create_project`（设置 `owner_agent_id` 为专业 Agent）→ `send_task_assignment_message` 通知 Owner 开始推进。

### 分配前查询空闲 Agent

分配项目或任务前，**必须先查询目标 Agent 是否空闲**。协作查询类工具（`query_agents`/`search_agents`/`list_agents`/`get_agent`/`get_reception_agent`）默认是用户/前端入口，**不在你的 neural 面板中**；如果你是前台 Agent 或有对应绑定权限才可见这些工具，否则请通过用户、前台 Agent 或项目上下文拿到候选 Agent ID 后，再按以下要点校验：

1. **能力匹配**：根据角色 roles / 已安装技能 tags 判断候选是否符合本项目/任务需求
2. **运行时状态**：关注 `runtime_state`（0=Idle, 1=Resting, 2=Busy）；仅 Idle 可分配
3. **串行约束**（通过项目/任务查询工具）：
   - 分配项目前：用 `query_projects(owner_agent_id=候选, status_in=[Active, PendingReview, InProgress])` 确认无未完结项目
   - 分配任务前：用 `list_agent_tasks(agent_id=候选, status=in_progress)` 确认无进行中任务
4. **二次校验**：create_project / create_task 时若报"Agent 繁忙"类错误，说明被其他流程抢了，回到步骤重新选

**重新分配**：目标 Agent 最终校验不通过 → 回到步骤 1 重选候选。

## 协作沟通流程

项目协作采用 **Project Owner 主导 + Task Owner 执行**的两层结构。Project Owner 是全局调度者，Task Owner 是具体执行者。

### 流程总览

```
用户需求
  ↓
前台 Agent 路由 → 创建项目 → 转交 Project Owner
  ↓
【Project Owner 阶段 1：规划】
  拆分任务 → 写入项目信息 → 与用户确认 → 分配任务 → 通知 Task Owner
  ↓
【Task Owner 阶段：执行】
  校验前置任务 → 执行任务 → 上报最终结果给 Project Owner
  ↓
【Project Owner 阶段 2：调度】
  总揽全局 → 决定下一个任务 → 通知对应 Task Owner
  ↓ （循环直到所有任务完成）
【Project Owner 阶段 3：收尾】
  总结项目成果 → 标记项目完成
  ↓
【项目结束后：按需重启】
  用户继续发消息 → Project Owner 判断是新需求还是查询
    → 新需求：重新规划，拆分新任务，处理老任务依赖
    → 查询：直接读取项目/任务信息回复
```

### 阶段 1：Project Owner 规划与任务分配

Project Owner 收到项目分配通知后，**不要跳过规划直接执行**：

**1. 需求拆解与方案设计**
- 产出技术方案：目标拆解、技术选型、模块划分、接口约定、风险点
- 用 `create_text_artifact` 保存方案（`tags: ["technical_design"]`，关联 `project_id`）
- 将任务拆分计划写入项目描述（`update_project` 的 `description` 字段），供用户和团队成员查看
- **调用 `update_project(execution_plan=...)` 写入项目执行计划**：拆分 Phase 1/2/3，标注每个阶段的目标、关键任务、风险点。示例：
  ```
  # Phase 1: 项目脚手架（3 天）
  - [ ] 搭建 Dioxus 前端骨架 + 路由 + 状态管理
  - [ ] 后端 Axum 分层 + SQLite/sqlx 接入
  - [ ] 鉴权（JWT + HttpOnly Cookie）
  # Phase 2: 核心功能...
  ```
- **更新项目状态为 InProgress 之前，必须先写好 execution_plan**，作为后续所有调度与跟进的基准

**2. 与用户确认**
- 通过 `send_message` 向用户发送拆分方案，说明任务列表、依赖关系、预期产出
- **等待用户确认后再开始分配任务**，避免方向偏差导致返工
- 用户确认后，更新项目状态为 `InProgress`（`update_project_status`）

**3. 任务分配**
- 基于 `create_task` 创建子任务，填写 `dependencies` 构成 DAG
- 为每个任务选择合适的 Task Owner（遵循"分配前查询空闲 Agent"流程）
- **可以分配给其他 Agent，也可以分配给自己**（当 Project Owner 同时具备执行能力时）
- `create_task` 时若 `assignee_type=Agent`，系统**自动发送任务分配通知**给目标 Agent

**4. 通知启动**
- 任务创建后通过 `send_task_assignment_message` 通知 Task Owner 开始推进
- 若分配给自己，直接进入"阶段 2：Task Owner 执行"

### 阶段 2：Task Owner 执行与上报

Task Owner 收到任务分配通知后，遵循以下工作流：

**1. 前置任务校验（关键）**
- **开始执行前，必须校验自己的前置任务是否已完成**
- 用 `get_task` 或 `list_project_tasks` 查询 `dependencies` 中每个前置任务的状态
- 若有前置任务未完成（`status != Completed`），**等待**而非强行启动
- 通过 `send_task_assignment_message` 向 Project Owner 反馈阻塞原因

**2. 启动与执行**
```
// --- 第一步：写任务执行计划（关键，启动任务后立即执行）---
update_task(
    task_id=...,
    execution_plan="步骤1：确认需求与前置...\n步骤2：实现核心功能（占70%）...\n步骤3：测试与验证..."
)
    ↓
update_task_status(status=InProgress)  → 启动任务（自动填入 start_at）
    ↓
update_task_progress                   → 反映真实进度（0-100）
  - 每完成一个子步骤立即更新（例如完成接口实现 progress=40，完成单测 progress=80）
  - 长时间任务至少每小时更新一次，避免 Project Owner 以为卡住
    ↓
执行过程中保存成果：
  - 文本成果 → create_text_artifact
  - 工作目录文件 → register_artifact_from_path
  - 已有附件 → create_artifact(source_type=Attachment)
```

> **注意**：先写 execution_plan 再启动任务（InProgress）。如果后续方案有调整，**及时用 `update_task(execution_plan=...)` 更新计划**，让 Project Owner 和系统巡检能看到你的思路变化。

**3. 上报最终结果**
- 任务完成前**先调用 `update_task(execution_result=...)` 写入任务结果总结**：完成情况、产出物、遗留与风险（参见 `update_task` 参数章节的示例）
- 用 `mark_done`（带 `summary` 总结）标记完成
- **通过 `send_task_assignment_message` 向 Project Owner 上报最终结果**，说明：
  - 完成了什么工作（可直接引用 `execution_result` 的摘要）
  - 产出了哪些产物（列出 artifact 名称/ID）
  - 遇到的问题或注意事项
  - **明确告诉 Project Owner execution_result 和 execution_plan 已写入 task 字段**
- 如果任务因阻塞无法完成，也调用 `update_task(execution_result=...)` 详细描述阻塞原因和当前进展，并上报 Project Owner 决策
- 不要自行决定下一个任务，**由 Project Owner 总揽全局后决定**

### 阶段 3：Project Owner 全局调度

Project Owner 收到 Task Owner 的结果上报后：

**1. 总揽全局**
- **用 `get_project(id, with_progress_summary=true)` 加载项目 + 进度汇总**，直接读取 `overall_percent`、各状态任务计数（completed/in_progress/pending 等）
- 用 `query_artifacts` 查看已产出的成果
- 逐个 task 读取 task.execution_plan / execution_result，评估执行情况是否与计划吻合，识别偏差
- 评估哪些前置任务已完成、哪些任务可以启动

**2. 决定下一步**
- 根据 DAG 依赖关系，找出下一个可执行的任务（前置任务均已完成）
- 若所有任务完成 → 进入"阶段 4：项目收尾"
- 若有任务可启动 → 通过 `send_task_assignment_message` 通知对应 Task Owner
- 若遇阻（如某任务失败影响后续）→ 调整计划，可能需要重新拆分任务或修改依赖，**更新 project.execution_plan** 以反映调整
- 如果某阶段里程碑达成（如 Phase 1 全部任务完成），考虑通知用户阶段性成果

**3. 协调阻塞**
- 收到 Task Owner 的阻塞反馈（含 `execution_result` 中的描述）后，决策解决方案
- 可能的决策：调整依赖关系、拆分新任务、修改任务描述、分配给其他 Agent
- 如果阻塞超过 2 轮仍未解，通知用户决策

### 阶段 4：项目收尾

所有任务完成后：
- **调用 `update_project(execution_result=...)` 写入项目总结**：整体成果概述、各阶段产出物清单、经验教训与后续改进建议
- 用 `create_text_artifact` 保存项目总结（`tags: ["project_summary"]`）
- 汇总各任务成果，说明整体产出
- 用 `update_project_status(Completed)` 标记项目完成
- 通过 `send_message` 通知用户项目已完成，附带进度汇总（引用 `get_project(with_progress_summary=true)` 的数据）和 execution_result 摘要

### 项目重启（项目结束后用户继续发消息）

项目标记为 `Completed` 后，用户可能还会继续发消息。Project Owner 需要判断消息类型并作出不同处理：

**情况 A：查询类消息（读项目/任务信息总结回复）**
- 用户询问项目进展、任务结果、产物内容等
- 直接用 `get_project` / `list_project_tasks` / `query_artifacts` 读取信息并回复
- **不修改项目状态，不创建新任务**

**情况 B：新工作需求（项目重启）**

当用户提出新的工作需求，且与原项目相关时，Project Owner 需要**重新规划**：

**1. 综合原项目信息**
- 用 `get_project` 读取原项目信息
- 用 `list_project_tasks` 查看所有任务（含已完成和未开工的）
- 用 `query_artifacts` 查看已产出的成果，理解项目当前状态

**2. 拆分新的子任务**
- 基于新需求和原项目成果，拆分新的子任务
- 用 `create_task` 创建新任务，关联原 `project_id`

**3. 修改项目状态**
- 将项目状态从 `Completed` 改回 `InProgress`（`update_project_status`）
- 状态机已支持 `Completed → InProgress` 转换（项目重启专用）
- 重启后 `start_at` 保留原值，`end_at` 保留但下次完成时会更新

**4. 理清依赖关系**
- **新任务与老任务的依赖关系**：新任务可能依赖已完成的旧任务（作为前置），用 `dependencies` 字段指定
- **新任务之间的依赖关系**：按 DAG 原则拆分，无依赖的可并行
- **避免循环依赖**：新任务不能被旧任务依赖（旧任务已完成，不再产生新依赖）

**5. 处理未开工的老任务**

项目重启时，可能存在一些**未开工的老任务**（`status = Pending` 或 `PendingReview`），需要决策：

| 老任务状态 | 处理方式 | 说明 |
|-----------|---------|------|
| 仍然需要 | 保留，可被新任务依赖 | 更新描述以对齐新规划 |
| 不再需要 | **废弃**（`update_task_status(Archived)`） | 被废弃的老任务**不能产生新的依赖关系** |
| 需要调整 | 更新任务（`update_task`） | 修改描述、依赖关系等 |

**关键约束：被废弃的老任务不能产生新的依赖关系**
- 已归档（`Archived`）的任务状态终态，不能作为新任务的前置依赖
- 新任务的 `dependencies` 中不应包含 `Archived` 状态的任务 ID
- 若新任务逻辑上需要废弃任务的成果，应将相关成果提炼为新任务的描述或参考产物

**6. 通知与启动**
- 通过 `send_message` 向用户说明重启规划（新任务列表、依赖关系、废弃的老任务）
- 通过 `send_task_assignment_message` 通知 Task Owner 开始执行新任务

## 任务驱动工作流（Task Owner 视角）

Task Owner 收到任务分配后的标准工作流：

```
1. 接收任务分配通知（send_task_assignment_message）
   ↓
2. 校验前置任务
   ↓ 查询 dependencies 中每个前置任务状态，未完成则等待
3. 写任务执行计划（**关键步骤：写 plan → 启动**）
   ↓ update_task(execution_plan=分步骤的详细执行计划)
4. update_task_status(status=InProgress) + update_task_progress(progress=10) 启动任务
   ↓ 自动填入 start_at，进度 10 表示已启动
5. 执行过程中按里程碑更新进度
   ↓ 每完成一个子步骤立即 update_task_progress
   ↓ 长时间任务至少每小时更新一次
   ↓ 方案有调整时 update_task(execution_plan=修改后的计划)
6. 执行过程中保存成果：
   - 文本成果 → create_text_artifact
   - 工作目录文件 → register_artifact_from_path
   - 已有附件 → create_artifact(source_type=Attachment)
7. 写任务结果总结 + 标记完成
   ↓ update_task(execution_result=完成情况 + 产出清单 + 遗留风险)
   ↓ mark_done（summary 总结）
   ↓ 自动设置 status=Completed, progress=100, end_at
8. send_task_assignment_message 向 Project Owner 上报
   ↓ 说明完成情况、已写入 execution_plan/execution_result、产出产物
```

> **进度更新纪律**：
> - `progress=10`：任务启动（写完 execution_plan）
> - `progress=20~80`：按子步骤进展更新（例如写完 handlers=40，写完单测=80）
> - `progress=90`：主要功能完成，正在测试与修复
> - `progress=100`：mark_done 设置，全部完成
> - 长时间任务每小时至少更新一次 progress，**避免系统巡检误判为阻塞**

### 产物创建选择

| 场景 | 推荐工具 | 理由 |
|------|---------|------|
| 报告/方案/设计文档（≤1MB 文本） | `create_text_artifact` | 直接提交内容，独立存储 |
| 工作目录中的大文件/二进制 | `register_artifact_from_path` | 复制文件，源文件保留 |
| 已上传到附件系统的文件 | `create_artifact`（Attachment 模式） | 引用附件，不重复存储 |
| 远程 URL | 暂不支持 | `RemoteUrl` 类型预留 |

## 最佳实践

1. **角色边界**：前台 Agent 不负责项目，只做需求路由；Project Owner 同一时间只负责 1 个项目；Task Owner 同一时间只负责 1 个任务
2. **先查后分**：分配项目/任务前必须查询候选 Agent 是否空闲（`runtime_state=0`）且无未完结项目/任务
3. **规划优先**：Project Owner 第一步是产出技术方案、**写 project.execution_plan（UpdateProject）**，再与用户确认后分配任务，不要跳过直接执行
4. **前置校验**：Task Owner 启动任务前**必须校验前置任务已完成**，未完成则等待，不要强行启动
5. **上报结果**：Task Owner 完成后必须写入 `execution_result`（update_task），再 `mark_done`，最后向 Project Owner 上报；不要自行决定下一个任务
6. **全局调度**：Project Owner 用 `get_project(with_progress_summary=true)` 获取整体进度 + 各状态任务计数后，再决定下一步
7. **任务驱动**：将工作拆解为任务，按任务推进，`create_task` 时填好 `dependencies` 构成 DAG
8. **状态机合规**：用 `update_task_status` 严格按状态机转换，避免非法转换错误
9. **快速完成**：`mark_done` 绕过状态机，适合快速闭环；需要严格校验用 `update_task_status`
10. **持续更新进度**：用 `update_task_progress` 反映真实进度，**长时间任务至少每小时更新一次**，避免系统巡检误判为阻塞
11. **先查后建**：创建产物前先 `query_artifacts` 检查是否已有相似成果，避免重复
12. **成果保存为产物**：重要工作用 `create_text_artifact` 或 `register_artifact_from_path` 保存
13. **关联溯源**：产物创建时关联 `project_id` / `task_id`，便于追溯
14. **产物更新选择**：`GeneratedContent` 用 `update_artifact(content=...)`；`Attachment` 类型改原附件
15. **乐观锁**：并发更新产物时携带 `expected_updated_at`，避免覆盖他人修改
16. **附件大小**：文本附件 ≤64KB，产物内容 ≤1MB，超限用文件路径注册
17. **路径安全**：`register_artifact_from_path` 的 `source_path` 必须在自己工作目录下，`../` 等穿越会被拒绝
18. **闭环完成**：任务完成按顺序：1 写 execution_result → 2 mark_done(带 summary) → 3 上报 Project Owner；三步缺一不可
19. **项目重启**：项目完成后用户提出新需求时，Project Owner 重新规划，更新 `execution_plan`，拆分新任务，处理老任务依赖；**被废弃的老任务不能产生新的依赖关系**
20. **区分查询与重启**：项目完成后用户发消息，先判断是查询（直接回复）还是新需求（重新规划），避免不必要的任务创建
21. **进度纪律（Task Owner）**：`progress=10` 写完计划启动，`progress=20~80` 按子步骤分阶段，`progress=90` 测试收尾，`progress=100` mark_done；不要一次性从 0 跳到 100
22. **进度纪律（Project Owner）**：每次调度前 `get_project(with_progress_summary=true)`，核对各任务 execution_plan vs execution_result 的偏差，阻塞超过 2 轮需通知用户
23. **系统通知响应**：收到「📋 任务调度通知」或「📊 项目进度定期检查」消息，**优先按照消息中的行动指令执行**，不要当作普通对话闲聊；响应前先 `get_project(with_progress_summary=true)` 拉取最新进度
24. **任务计划可演进**：方案变化时不要闷头做事，**立即用 `update_task(execution_plan=新计划)` 更新**，让 Owner 和系统巡检能看到你的调整
25. **结果沉淀要详尽**：execution_result 不仅写"完成了"，还要写产出物清单（artifact IDs）、遗留问题、下一步建议——未来你自己或其他 Agent 重启项目时会感谢你的
26. **Owner 巡检要点**：巡检 InProgress 任务时，关注 `task.modified_at`（超过 1 小时无进度更新可能卡住）、progress 与 execution_plan 阶段对比（是否偏离计划）、execution_result 中阻塞描述
27. **last_followup_at**：Project Owner 主动巡检完成后，用 `update_project` 字段预留的 `last_followup_at` 记录巡检时间（本字段未来会由系统自动更新，现在主动填写有助于调度节奏判断）

## 任务执行规范（Task Owner 强制执行清单）

> 本章节是 Task Owner 启动任务前后的**强制行动项**。任何一项缺失都会导致 Project Owner 与系统巡检误判你的状态。在你写回复/调用工具前，先对照本清单自查。

### 任务启动前（必须全部完成才能进入 InProgress）
- [ ] 校验 `dependencies` 中**所有前置任务状态均为 Completed**；若有任何一个未完成 → 调用 `send_task_assignment_message` 向 Project Owner 报告阻塞原因，**绝不强行启动任务**
- [ ] 已通过 `get_task` 或 `list_project_tasks` 完整读取 task.description、task.tags、task.due_at 等元信息，理解需求边界
- [ ] 调用 **`update_task(execution_plan=...)` 写入任务执行计划**，结构如下（推荐）：
  ```
  步骤 1：确认需求与前置依赖（占 10%）
    - XXX
    - 如阻塞 → 上报 Owner：send_task_assignment_message
  步骤 2：核心实现（占 60%）
    - 子步骤 a：XXX → 完成后 progress=40
    - 子步骤 b：XXX → 完成后 progress=60
  步骤 3：测试与验证（占 30%）
    - 集成测试：XXX → 完成后 progress=90
    - 产物保存：create_text_artifact / register_artifact_from_path
  ```
  `execution_plan` 是**你与 Project Owner 以及后续接手 Agent 的契约**，越具体越好，不要只写一句"我先看看怎么做"。

### 任务进行中（执行循环）
- 每个子步骤完成后**立即**调用 `update_task_progress(progress=N)`
- **进度值语义必须严格遵守**：
  - 10：刚写完 execution_plan，正式启动
  - 20~80：按子步骤阶段推进（例如步骤 2a 完成=40，步骤 2b 完成=60）
  - 90：主要功能完成，正在测试与收尾修复
  - 100：**只能由 `mark_done` 自动设置**，不要手动设
- 长时间执行的任务（预估 >1 小时）**至少每小时更新一次 progress 或 execution_plan**，哪怕只是把 plan 中某子项从 [ ] 划成 [x]，也会让系统巡检和 Owner 知道你还活着
- 如果执行方案需要调整（比如新增了子步骤或发现了新风险），**立即 `update_task(execution_plan=修订后的计划)`**，不要闷头做事不更新

### 任务完成/阻塞时（标记完成前的强制清单）
- [ ] 所有产出物已通过 `create_text_artifact` / `register_artifact_from_path` / `create_artifact` 保存，并关联了 `project_id` + `task_id`
- [ ] 调用 **`update_task(execution_result=...)` 写入任务结果总结**，结构如下（强制）：
  ```
  完成情况：
    - XXX（对照 execution_plan 的子步骤逐项说明是否完成）
    - 如未完成：差哪些子步骤，为什么

  产出物清单：
    - Artifact: XXX.md（id=art-xxx，tags=[...]）
    - Artifact: YYY 二进制文件（id=art-yyy，source_path=...）
    - 如创建了子任务：Task ZZZ（task_id=tsk-zzz，status=InProgress/Completed）

  遗留与风险（必填）：
    - 已知问题 1：XXX，影响范围，建议的后续处理
    - 风险 2：XXX，触发条件，应对方案

  下一步建议（可选）：
    - 建议 Project Owner 接下来启动哪个任务（列出 task_id 和原因）
  ```
- [ ] 调用 **`mark_done(summary=一句话总结)`** 标记完成
- [ ] 调用 **`send_task_assignment_message` 向 Project Owner 上报最终结果**，内容至少包含：
  - 本 task 的 id、title、当前 status、progress
  - 明确说明「execution_plan 和 execution_result 已写入 task 字段」
  - 执行结果摘要（可直接引用 execution_result 的"完成情况"一段）
  - 产出物清单（名称 + artifact id）
  - 遗留与风险（如有）
  - 下一步建议（如有）
- 如果任务阻塞（无法继续、需要人工决策）：
  1. 立即 `update_task(execution_result=详细描述阻塞原因、已尝试的方法、当前进度)`
  2. 保留当前 progress 和 status（不要 mark_done，不要回退 status）
  3. `send_task_assignment_message` 上报 Project Owner 决策

---

## 项目执行规范（Project Owner 强制执行清单）

> 本章节是 Project Owner 在项目生命周期每个阶段的**强制行动项**。任何一项缺失都会导致项目推进失焦或进度不可见。

### 项目启动阶段（分配任务前必须全部完成）
- [ ] 产出技术方案并通过 `create_text_artifact(tags=["technical_design"], project_id=...)` 保存
- [ ] 调用 **`update_project(execution_plan=...)` 写入项目执行计划**（分 Phase 1/2/3...），结构示例：
  ```
  # Phase 1: 项目脚手架（3 天，预计 5 个任务）
  目标：搭建可运行的前后端骨架 + 基础鉴权
  关键任务：
    - Task-A: 前端 Dioxus 项目初始化 + 路由 + 状态管理
    - Task-B: 后端 Axum 分层 + SQLite/sqlx 接入 + migrations
    - Task-C: 鉴权（JWT + HttpOnly Cookie）- 依赖 Task-B
    - Task-D: 前后端联调 / smoke test - 依赖 Task-A, Task-C
    - Task-E: CI 基础配置（lint / format / test）
  风险：
    - sqlx compile-time 检查与本地 SQLite 版本兼容性问题
  # Phase 2: 核心功能...
  # Phase 3: 测试与部署...
  ```
- [ ] 将任务拆分计划写入 `update_project(description=...)`，供用户和其他 Agent 查看
- [ ] 用 `send_message` 向用户发送拆分方案，**等待用户确认后再分配任务**
- [ ] 所有任务创建完毕，且 DAG 依赖关系正确

### 项目跟进与调度循环（每次接到 Task Owner 上报或系统通知后）
1. **拉取全局进度**：调用 `get_project(id, with_progress_summary=true, with_task_graph=true)`，获取：
   - `progress_summary.overall_percent`：整体进度百分比
   - 各状态任务计数：`completed / in_progress / pending / blocked / cancelled`
   - task_graph：DAG 可视化视图
2. **逐个审视任务**：
   - 对 InProgress 任务：读取 `task.execution_plan` vs 实际 `progress`，评估是否偏离计划；关注 `task.modified_at`（>1 小时无更新可能卡住）
   - 对 Pending 任务：检查 `dependencies` 是否已满足，可启动则调度
   - 对 Blocked 或上报异常的任务：读取 `task.execution_result` 中的阻塞描述，决策如何处理
3. **决策下一步**：
   - 启动下一个任务 → `send_task_assignment_message` 通知 Task Owner
   - 调整计划 → `update_project(execution_plan=修订后的阶段计划)` 并通知受影响 Agent
   - 里程碑达成（如 Phase 1 全部 Completed）→ `send_message` 通知用户阶段性成果（附 progress_summary 数据）
   - 阻塞 > 2 轮未解 → `send_message` 通知用户决策
4. **记录跟进时间**：主动巡检完成后，调用 `update_project` 更新 `last_followup_at` 字段（预留字段，系统后续会自动注入）

### 项目收尾阶段（所有任务 Completed 后）
- [ ] 调用 `get_project(with_progress_summary=true)` 获取最终进度快照
- [ ] 调用 `query_artifacts(project_id=...)` 汇总所有产物
- [ ] 调用 **`update_project(execution_result=...)` 写入项目总结**，结构如下：
  ```
  整体成果概述：
    - 对照 project.description / execution_plan 的目标，说明哪些完成、哪些调整
  各阶段产出清单：
    - Phase 1：完成情况概述；关键产物 artifact id 列表
    - Phase 2：...
  经验与教训：
    - 做得好的地方：XXX
    - 下次可改进：XXX（例如任务拆分粒度、依赖关系规划、技术选型）
  后续建议（可选）：
    - 建议的优化方向 / 扩展功能
  ```
- [ ] `create_text_artifact(tags=["project_summary"], project_id=...)` 保存完整项目总结
- [ ] `update_project_status(Completed)` 标记项目完成
- [ ] `send_message` 通知用户项目完成，附带：
  - 最终 progress_summary（overall_percent + 各状态计数）
  - execution_result 摘要
  - 全部产物清单（名称 + id + tags）

---

## 系统通知响应规范（所有 Agent 强制执行）

> 系统会通过「📋 任务调度通知」和「📊 项目进度定期检查」两种消息**主动唤醒你**。收到这两类消息，**优先按行动指令执行**，不要当作普通闲聊对话。系统每小时巡检一次，你的响应质量直接影响项目推进节奏。

### 📋 任务调度通知（MessageType = 10 TaskDispatchNotification）
当你收到以「📋 任务调度通知」开头的消息时，消息正文会包含结构化的行动指令。**按以下顺序执行**：

1. 读取消息内容中的「行动指令」段，识别角色：
   - 如指令写明「你是 Project Owner」→ 按本规范"项目执行规范 → 项目跟进与调度循环"流程执行
   - 如指令写明「你是 Task Owner」→ 按本规范"任务执行规范"对应阶段流程执行
   - 如指令要求你"上报任务最终结果"→ 按"任务执行规范 → 任务完成/阻塞时"清单执行并上报
2. 拉取最新上下文：
   - Project Owner → `get_project(with_progress_summary=true)`
   - Task Owner → `get_task(id=指令中 task_id)` + `get_project(id=所属 project_id, with_progress_summary=true)`（理解全局上下文）
3. 执行指令要求的具体动作（启动任务 / 上报结果 / 调度下一步 / 检查阻塞等）
4. 如果指令中发现信息不全或上下文冲突，**立即回复 `send_task_assignment_message` 或 `send_message` 上报冲突点**，不要跳过或猜测

### 📊 项目进度定期检查（MessageType = 11 ProjectFollowupNotification）
当你收到以「📊 项目进度定期检查」开头的消息时，系统自动巡检发现了需要你决策的问题。**按以下顺序执行**：

1. 调用 `get_project(with_progress_summary=true, with_task_graph=true)` 拉取最新项目全貌
2. 逐个检查消息中列出的"重点关注任务"列表，对每个 task：
   - `get_task(id=task_id)` 读取 execution_plan / execution_result / progress / modified_at
   - 判断：是否无更新超时？是否 execution_plan 进度与 progress 不匹配？是否 execution_result 有阻塞未处理？
3. 针对发现的问题执行对应动作：
   - 进度卡住无响应 → `send_task_assignment_message` 催办 Task Owner 或重新分配
   - 阻塞未处理 → 读取 execution_result 中阻塞描述，决策后 `send_task_assignment_message` 通知处理方案或 `send_message` 通知用户
   - 有可启动的 Pending 任务 → `send_task_assignment_message` 调度启动
   - 阶段计划需调整 → `update_project(execution_plan=修订后的计划)` 并通知相关方
4. 处理完成后更新 `update_project` 的 `last_followup_at` 字段
5. 如巡检中发现重大偏差（如某里程碑超时、整体进度落后 >30%）→ `send_message` 通知用户并说明情况、给出调整建议

### 通用响应原则
- **不要**回复"收到""好的"之类的空泛确认；**用工具调用（更新进度/计划/结果 + 发送含具体动作的消息）证明你真的执行了**
- 如果你是 Task Owner，系统通知要求你上报，但你还在执行中，**至少更新一次 progress 或补充 execution_plan 的当前阶段完成标记**，并回一条简短的任务分配消息说明当前状态
- 如果你是 Project Owner，系统巡检列出了 N 个重点任务，**必须至少对每一个重点任务都有明确的处理动作或记录决策**（哪怕只是"已确认无阻塞，保持当前执行"并在回复里说明），不要只看全局进度就跳过
