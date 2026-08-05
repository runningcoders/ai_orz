# 技能管理

本指南帮助你理解平台的技能系统，掌握技能查询、搜索、安装、卸载、更新的完整成长性能力管理。技能是你扩展自身能力的核心机制——像人一样，你的能力可以通过持续学习成长。

## 技能是什么

技能是可复用的工作能力封装，包含工作方法、领域知识、操作规范。

**与工具的区别**：
- **工具**：具体的执行接口，有输入输出参数，调用后返回结果
- **技能**：方法论和知识，指导你如何使用工具和完成任务，无直接返回值

**技能状态**（`SkillStatus`）：

| 状态 | 含义 | 可见性 |
|------|------|--------|
| `Published` | 已发布到共享库 | 所有人可见可安装 |
| `Draft` | 草稿，私有迭代中 | 仅作者可见 |
| `Expired` | 已过期/废弃 | 不加载、不展示 |

## 技能加载机制（核心认知）

技能加载遵循"安装优先 + 标签匹配"双层规则，理解这点对你使用技能至关重要：

### 第一层：安装范围限定

**技能只在 Agent 已安装的副本范围内查询**（`author_id = agent_id`），排除 `Expired` 状态。这意味着：
- 即便是 `neural` 常驻技能，也**必须先安装到你的目录**才能使用
- 未安装的技能不会出现在你的 Prompt 中，即使它是 Published 状态
- 你可以通过 `search_skill` 发现未安装的技能，再通过 `install_skill_to_agent` 安装

### 第二层：标签匹配分块

已安装的技能会按 `tags` 分块展示在 Prompt 中：

| 分块 | 匹配条件 | 加载行为 |
|------|---------|---------|
| **神经技能** | tags 含 `neural` | 所有 Agent 必加载，常驻 Prompt |
| **必加载技能** | tags 不含 `neural`，但与你的 `match_keys` 有交集 | 按 `match_keys` 匹配加载 |
| **隐藏技能** | tags 不含 `neural`，且与 `match_keys` 无交集 | **不展示在 Prompt**，通过 `search_skill` 按需加载 |

**`match_keys` 是什么**：你的角色（roles）∪ 已安装工具包标签（installed_tags）。例如你是 `backend_developer` 角色 + 安装了 `project_management` 工具包，那么 `match_keys = ["backend_developer", "project_management"]`。

**关键认知**：你当前看到的本指南就是神经技能之一（tags 含 `neural`）。但如果你安装了一个 `tags = ["frontend"]` 的技能，而你的 `match_keys` 不含 `frontend`，它**不会出现在 Prompt 中**，你需要通过 `search_skill` 主动发现。

## 技能查询与发现

### `search_skill` — 搜索技能（neural 常驻）

**用途**：按关键词或标签搜索技能库，返回匹配的技能摘要列表。

**参数**：
- `keyword` — 搜索关键词（匹配名称、描述、tags）
- `tags` — 按 tag 过滤（OR 语义，命中任一即可）
- `limit` — 返回数量限制，默认 10

**返回**：`SearchSkillResponse`，包含 `skills` 数组（`SkillSummary` 摘要列表）。

**适用**：遇到不熟悉的领域时，优先用此工具发现可用技能。它是你扩展能力认知的入口。

### `search_skills` — 搜索已发布技能

**用途**：搜索 Published 状态的技能，支持更丰富的过滤条件。

**参数**：
- `keyword` — 搜索关键词
- `status` — 状态筛选（默认排除 Expired）
- `category` — 分类筛选
- `author_id` — 作者筛选
- `limit` — 返回数量限制

**返回**：`SearchSkillsResponse`，包含 `skills` 数组（`SkillListItem` 列表项）。

**与 `search_skill` 的区别**：`search_skill` 是神经工具（常驻），参数精简；`search_skills` 是管理工具，过滤条件更全。

### `list_skills` — 列出技能

**用途**：返回技能分页列表，**固定排除 Expired + 按 `updated_at` DESC 排序**。

**参数**：仅分页参数（`limit` / `offset`）。

**适用**：浏览全部可用技能，配合 `query_skills` 做精准筛选。

### `query_skills` — 按条件查询

**用途**：支持完整过滤条件的技能查询。

**参数**：
- `ids` — 按 ID 批量查询
- `keyword` — 关键词搜索
- `status` — 状态过滤
- `category` — 分类过滤
- `author_id` — 作者过滤
- `parent_skill_id` — 父技能 ID 过滤（用于查找副本）
- `tags` — 标签过滤
- `limit` / `offset` — 分页

**适用**：按分类、作者、标签等条件精确筛选技能。

### `list_skill_tags` — 列出技能标签（neural 常驻）

**用途**：返回所有 **Published** 技能的不重复 tag 列表，按字母升序。

**参数**：无。

**适用**：发现技能包分类，了解平台技能全貌。仅聚合 Published 技能的 tags，Draft 和 Expired 的 tag 不出现。

### `get_skill` — 获取技能详情

**用途**：查看指定技能的完整信息，包含元数据和文件列表。

**返回**：`SkillDetail`，包含技能基本信息 + `files` 文件列表。

### `list_skill_files` / `get_skill_file_content` — 读取技能文件

**用途**：查看技能的具体内容文件。

**参数**：
- `list_skill_files` — `skill_id`（必填），返回文件列表
- `get_skill_file_content` — `skill_id` + `filename`（都必填），返回文件文本内容

**适用**：常驻技能已加载到 Prompt 无需读取；按需技能需要主动读取文件内容才能使用。

### `list_agent_skills` — 查看 Agent 已安装技能

**用途**：返回指定 Agent 已安装的技能列表。

**参数**：`agent_id`（必填，路径参数）。

**适用**：检查自己或他人 Agent 当前安装的技能集。

## 技能安装与卸载

### `install_skill_to_agent` — 安装技能

**用途**：将公共技能安装到你的技能目录，创建私有副本。

**参数**：
- `skill_id` — 源技能 ID（必填，路径参数）
- `agent_id` — 目标 Agent ID（必填）

**行为**：
1. 复制源技能到 `agents/{agent_id}/skills/` 目录
2. 创建新技能记录，`author_id = agent_id`，`parent_skill_id = source_skill_id`
3. **幂等**：若已存在该源技能的副本（`author_id = agent_id AND parent_skill_id = source_skill_id`），跳过创建直接返回已有副本
4. 副本独立于源技能：源技能后续更新不影响你的副本

**返回**：`InstallSkillToAgentResponse`，包含 `agent_id` / `source_skill_id` / 完整的 `skill` 详情。

### `install_skill_pack` — 安装技能包

**用途**：按 tag 批量安装一组技能到 Agent 目录。

**参数**：
- `agent_id` — 目标 Agent ID（必填，路径参数）
- `tag` — 技能包标签（必填，路径参数，如 `project_management`）

**行为**：查找所有 tags 包含该 tag 的 Published 技能，逐一安装到 Agent（幂等）。同时将该 tag 加入 Agent 的 `installed_tags`，影响 `match_keys` 匹配。

**返回**：`InstallSkillPackResponse`，包含 `installed_count`（成功安装数量）。

**适用**：一次获取某个领域的完整能力包，同时让该 tag 的技能进入"必加载技能"分块。

### `uninstall_skill_from_agent` — 卸载单个技能（neural 常驻）

**用途**：卸载 Agent 的技能副本，删除 DB 记录和文件目录。

**参数**：
- `skill_id` — 技能 ID（必填，路径参数）
- `agent_id` — Agent ID（必填）

**约束**：
- 只能卸载**安装副本**（`parent_skill_id` 不为空）
- 只能卸载**属于自己的技能**（`author_id = agent_id`）
- 不能卸载原始技能（非副本），会返回错误

**返回**：`UninstallSkillFromAgentResponse`，包含 `agent_id` / `skill_id` / `deleted: true`。

### `uninstall_skill_pack` — 卸载技能包

**用途**：从 Agent 的 `installed_tags` 中移除指定 tag，可选同时删除技能副本。

**参数**：
- `agent_id` — Agent ID（必填，路径参数）
- `tag` — 技能包标签（必填，路径参数）
- `delete_copies` — 是否同时删除 Agent 侧的技能副本（默认 `false`，仅移除 tag 关联）

**行为**：
- `delete_copies = false`（默认）：仅从 `installed_tags` 移除 tag，技能副本保留但不再进入"必加载技能"分块
- `delete_copies = true`：移除 tag + 批量删除该 tag 下的所有技能副本（通过 `parent_skill_id IS NOT NULL` 过滤）

**返回**：`UninstallSkillPackResponse`（空）。

## 技能更新

### `update_skill` — 更新技能

**用途**：更新技能的元数据、主内容（skill.md）、附加文件。

**参数**（全部可选，仅传需要更新的字段）：
- `skill_id` — 技能 ID（必填，路径参数）
- `name` — 新技能名称
- `description` — 新技能描述
- `tags` — 新标签列表
- `category` — 新分类
- `status` — 新状态（如 Draft → Published）
- `content` — 新的 skill.md 主文件内容
- `files` — 附加文件导入列表

**适用**：
- 将 Draft 技能发布为 Published（`status = Published`）
- 更新技能内容（`content`）
- 调整 tags 以改变技能的匹配范围

**成长性场景**：你在工作中总结出新方法论，可以 `update_skill` 更新自己的技能副本内容，让方法论持续迭代。

## 成长性能力管理

像人一样，你的能力可以通过持续学习成长：

### 能力扩展循环

```
接到新任务
  ↓
search_skill 搜索是否有相关技能
  ↓
发现有用技能 → install_skill_to_agent 安装到自己的技能库
  ↓
需要完整能力包 → install_skill_pack 按标签批量安装
  ↓
技能进入 Prompt（neural 常驻 或 match_keys 匹配）
  ↓
实际使用技能完成任务
```

### 能力维护循环

```
list_agent_skills 检查当前已安装技能
  ↓
发现过时技能 → uninstall_skill_from_agent 卸载
  ↓
发现冗余技能包 → uninstall_skill_pack 批量清理
  ↓
list_skill_tags 浏览最新技能分类
  ↓
发现有价值的技能包 → install_skill_pack 安装
```

### 知识沉淀循环

```
完成重要任务，总结经验
  ↓
update_skill 更新自己的技能副本内容（迭代方法论）
  ↓
或 save_short_term_memory 保存经验到短期记忆（参考"记忆认知"技能）
  ↓
下次遇到类似任务，技能和记忆共同指导你更好地完成
```

## 最佳实践

1. **先搜后装**：安装前先 `search_skill` 了解技能内容和适用性，避免盲目安装
2. **理解 match_keys**：非 neural 技能需要 tags 与你的 roles/installed_tags 有交集才会加载，安装后若没出现在 Prompt，检查 tags 是否匹配
3. **按需安装**：只安装当前任务需要的技能，避免 Prompt 过载
4. **善用技能包**：`install_skill_pack` 一次安装整个领域的技能包，同时让 tag 进入 `match_keys`
5. **及时清理**：完成的任务相关技能及时 `uninstall_skill_from_agent` 卸载，保持技能库精简
6. **卸载副本**：`uninstall_skill_from_agent` 只能卸载副本（`parent_skill_id` 不为空），不能卸载原始技能
7. **持续迭代**：通过 `update_skill` 更新自己的技能副本内容，让方法论持续进化
8. **关注标签**：用 `list_skill_tags` 发现新技能包，持续扩展能力认知
9. **隐藏技能不等于不可用**：未匹配 match_keys 的技能不在 Prompt 展示，但 `search_skill` 仍可发现、`get_skill_file_content` 仍可读取内容
10. **学以致用**：安装的技能要实际使用，通过实践巩固能力，将经验沉淀到记忆形成闭环
