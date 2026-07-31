# 项目管理

本指南帮助你使用项目管理工具，高效管理项目、任务、产物。这是你执行结构化工作的核心能力——从需求拆解、任务推进到成果沉淀的完整闭环。

## 工具分类与加载机制

**关键认知**：所有项目管理工具的 tag 是 `project_management`，附件工具的 tag 是 `file_management`，它们**全部非 neural**。这意味着：

- 这些工具**不会自动注入你的工具面板**，需要通过 `install_skill_pack`（tag=`project_management`）让该 tag 进入你的 `match_keys`，本指南才会加载到 Prompt
- 工具本身仍需通过显式绑定（`bind_tool_to_agent`）或工具包安装才能调用
- 本指南（`project_management` skill）的核心价值是教你**如何使用这些工具**

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
Completed     → Archived
相同状态      → 允许（no-op）
其它          → 非法
```

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

**参数**（全部可选）：`name` / `description` / `priority` / `tags`。

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

**参数**（全部可选）：`title` / `description` / `priority` / `tags` / `due_at` / `dependencies`。

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
| **Project Owner** | 1 个项目（至完结） | 同一时间只能负责 1 个未完结项目 | 可分配任务给其他 Agent；负责进度汇总和成果总结 |
| **Task Owner** | 1 个任务（至完结） | 同一时间只能负责 1 个未完成任务 | 任务优先，完成或遇阻时回应 Project Owner |

**前台 Agent 转交流程**：用户提出复杂需求 → 前台 Agent 分析后 → `create_project`（设置 `owner_agent_id` 为专业 Agent）→ `send_task_assignment_message` 通知 Owner 开始推进。

### 分配前查询空闲 Agent

分配项目或任务前，**必须先查询目标 Agent 是否空闲**：

1. **查能力匹配**：用 `query_agents`（`keyword` / `roles`）或 `search_agents`（语义搜索）找到候选 Agent
2. **查运行时状态**：用 `query_agents` 传 `runtime_state=0`（Idle）过滤出当前空闲的 Agent
3. **查串行约束**：
   - 分配项目前：用 `query_projects` 传 `owner_agent_id` + `status_in=[1,2,3]`（Active/PendingReview/InProgress）确认候选无未完结项目
   - 分配任务前：用 `list_agent_tasks` 传 `status=in_progress` 确认候选无进行中任务
4. **二次校验**：`create_project` / `create_task` 时目标 Agent 可能已被其他流程占用，若遇到繁忙错误应重新选择候选

**重新分配**：若目标 Agent 在最终分配时已繁忙（`runtime_state != 0` 或已有进行中项目/任务），回到步骤 1 重新选择候选。

## 项目推进流程

Project Owner 被通知开始推进项目后，遵循以下流程：

### 标准推进步骤

```
1. 接收项目分配通知（send_task_assignment_message）
   ↓
2. 第一步：优先完成技术方案设计
   ↓ 用 create_text_artifact 保存为产物（tags 标注 "technical_design"）
3. 基于方案拆分为多个子任务
   ↓ 用 create_task 创建子任务，填写 dependencies 构成 DAG
4. 为每个子任务选择合适的 Task Owner
   ↓ 遵循"分配前查询空闲 Agent"流程
5. 推进过程中更新项目状态
   ↓ update_project_status(InProgress)
6. 监控各任务进展，协调阻塞
   ↓ 收到 Task Agent 问题反馈后决策（详见"协作沟通"技能）
7. 所有任务完成后，总结项目成果
   ↓ 用 create_text_artifact 保存总结 + update_project_status(Completed)
```

### 技术方案设计（第一步）

项目 Owner 的首要职责是产出技术方案，**不要跳过直接拆任务**：

- **内容**：目标拆解、技术选型、模块划分、接口约定、风险点
- **保存**：用 `create_text_artifact`（`tags: ["technical_design"]`），关联 `project_id`
- **价值**：作为后续任务拆分的依据，供 Task Owner 参考对齐

### 任务拆分与 DAG 依赖

基于技术方案拆分任务时：

- **粒度**：每个任务应有明确边界和可验证的交付物
- **依赖写入**：`create_task` 时填写 `dependencies`（前置任务 ID 列表），构成 DAG
- **并行识别**：无依赖关系的任务可并行分配，缩短整体周期
- **关键路径**：依赖链最长的路径是关键路径，优先推进

```
示例 DAG：
  task_design (技术方案) ──► task_api (接口实现) ──► task_test (集成测试)
                          └─► task_ui (界面实现) ──┘
```

**依赖关系约束**：
- `dependencies` 中的任务 ID 应在同一项目内
- 避免循环依赖（A 依赖 B，B 又依赖 A）
- 自环依赖（任务依赖自身）是非法的

## 任务驱动工作流

### 标准工作流

```
1. create_task 创建任务（assignee_type=Agent, assignee_id=自己）
   ↓ 自动发送任务分配通知
2. update_task_status(status=InProgress) 启动任务
   ↓ 自动填入 start_at
3. update_task_progress 反映真实进度（0-100）
   ↓ 触发 progress_updated 事件
4. 执行过程中保存成果：
   - 文本成果 → create_text_artifact
   - 工作目录文件 → register_artifact_from_path
   - 已有附件 → create_artifact(source_type=Attachment)
5. mark_done 标记完成 + summary 总结
   ↓ 自动设置 status=Completed, progress=100, end_at
```

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
3. **方案优先**：Project Owner 第一步是产出技术方案（`create_text_artifact` + `tags: ["technical_design"]`），不要跳过直接拆任务
4. **任务驱动**：将工作拆解为任务，按任务推进，`create_task` 时填好 `dependencies` 构成 DAG
5. **状态机合规**：用 `update_task_status` 严格按状态机转换，避免非法转换错误
6. **快速完成**：`mark_done` 绕过状态机，适合快速闭环；需要严格校验用 `update_task_status`
7. **及时更新进度**：用 `update_task_progress` 反映真实进度，触发事件供下游消费
8. **先查后建**：创建产物前先 `query_artifacts` 检查是否已有相似成果，避免重复
9. **成果保存为产物**：重要工作用 `create_text_artifact` 或 `register_artifact_from_path` 保存
10. **关联溯源**：产物创建时关联 `project_id` / `task_id`，便于追溯
11. **产物更新选择**：`GeneratedContent` 用 `update_artifact(content=...)`；`Attachment` 类型改原附件
12. **乐观锁**：并发更新产物时携带 `expected_updated_at`，避免覆盖他人修改
13. **附件大小**：文本附件 ≤64KB，产物内容 ≤1MB，超限用文件路径注册
14. **路径安全**：`register_artifact_from_path` 的 `source_path` 必须在自己工作目录下，`../` 等穿越会被拒绝
15. **闭环完成**：任务完成后用 `mark_done`（带 `summary`）或 `update_task_status(Completed)` 标记，保持状态准确
