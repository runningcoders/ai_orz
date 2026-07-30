# 项目管理

本指南帮助你使用项目管理工具，高效管理项目、任务、产物。这是你执行结构化工作的核心能力。

## 项目/任务/产物层次

```
Project（项目）
  └─► Task（任务）
       └─► Artifact（产物）
```

- **项目**：顶层容器，聚合相关任务和对话
- **任务**：具体的工作单元，有状态和进度
- **产物**：工作成果的持久化保存（文件、文档、代码等）

## 项目管理

### `create_project` — 创建项目
创建新项目，包含名称、描述等基本信息。

### `get_project` — 获取项目详情
查看指定项目的信息。

### `query_projects` — 查询项目列表
按条件查询项目，支持分页。

### `update_project` — 更新项目
更新项目的名称、描述等字段。

### `update_project_status` — 更新项目状态
变更项目状态（如启动、暂停、完成）。

### `list_projects` — 列出项目
返回项目列表。

## 任务管理

### `create_task` — 创建任务
在项目下创建新任务，指定负责人、优先级等。

### `get_task` — 获取任务详情
查看指定任务的信息。

### `list_tasks` — 列出任务
返回所有任务列表。

### `list_project_tasks` — 列出项目下的任务
查看指定项目包含的所有任务。

### `list_agent_tasks` — 列出分配给自己的任务
查看当前分配给你的任务，了解待办工作。

### `query_tasks` — 按条件查询任务
按状态、负责人、项目等条件筛选任务。

### `update_task` — 更新任务
更新任务的字段（标题、描述、负责人等）。

### `update_task_status` — 更新任务状态
变更任务状态（如开始、暂停、完成）。

### `update_task_progress` — 更新任务进度
更新任务进度（0-100），反映完成情况。

### `mark_done` — 标记任务完成
快速将任务标记为已完成。

## 产物管理

产物是你工作的持久化成果，重要工作应保存为产物。

### `create_text_artifact` — 创建文本产物
直接提交文本内容创建产物（≤1MB）。适合：
- 报告、方案、设计文档
- 代码片段、配置文件
- 分析结论、总结

### `register_artifact_from_path` — 从文件注册产物
将工作目录中的文件注册为产物。文件会被**复制**到产物存储，源文件保留。适合：
- 已生成的大文件
- 二进制文件
- 多文件目录结构

### `update_artifact` — 更新产物
统一更新产物的内容或元数据：
- `content` — 更新内容（仅适用于 GeneratedContent 类型）
- `name` / `description` / `tags` — 更新元数据（适用于所有类型）
- 只更新 `Some` 的字段，支持部分更新

### `query_artifacts` — 查询产物
按 project_id / task_id / file_type / source_type 过滤查询。

**使用场景**：
- 了解团队成员已完成的工作
- 查找可复用的已有产物
- 避免重复创建

### `create_artifact` — 创建产物（通用）
通用的产物创建接口，支持多种来源类型。

## 文件管理

### `create_text_attachment` — 创建文本附件
上传文本内容作为附件。

### `get_attachment` / `get_attachment_content` — 获取附件
查看附件元信息或下载附件内容。

### `list_attachments` — 列出附件
查询附件列表。

### `update_attachment_content` — 更新附件内容
更新已有附件的内容。

### `delete_attachment` — 删除附件
删除不再需要的附件。

## 工作目录规范

- 你只能操作自己的工作目录 `agents/{你的ID}/`
- `register_artifact_from_path` 的文件路径必须在自己的工作目录下
- 文件会被复制到产物存储，源文件保留，不影响你的工作副本
- 不要尝试访问其他 Agent 的目录

## 最佳实践

1. **任务驱动**：将工作拆解为任务，按任务推进
2. **及时更新进度**：用 `update_task_progress` 反映真实进度
3. **成果保存为产物**：重要工作用 `create_text_artifact` 或 `register_artifact_from_path` 保存
4. **先查后建**：创建产物前先 `query_artifacts` 检查是否已有相似成果
5. **闭环完成**：任务完成后用 `mark_done` 标记，保持状态准确
6. **关联溯源**：产物创建时关联 project_id / task_id，便于追溯
