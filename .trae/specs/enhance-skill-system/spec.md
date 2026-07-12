# 技能系统增强 Spec

## Why

当前技能系统已实现基础 CRUD、向量语义搜索和单技能安装，但存在三个核心缺口：
1. **tag 过滤缺失**：SkillQuery 不支持按 tag 过滤，无法实现"技能包"概念
2. **批量安装机制缺失**：只能逐个 `install_to_agent`，无法按 tag 批量安装技能包
3. **技能未参与唤醒流程**：PromptBuilder 预留了 `skills` 字段但从未填充，Agent 唤醒时不会加载已安装的技能

同时，Tool 的 tag 过滤也存在相同问题——每次唤醒全量加载工具到内存再 Rust 遍历过滤，应在 SQL 层用 `json_each` 优化。

## What Changes

### 4C-1a：DAO 层 tag 过滤能力建设

- **修改** `SkillQuery`：新增 `tags: Option<Vec<String>>` 字段，支持按 tag 精确过滤
- **修改** `SkillDao.query` SQL：使用 `json_each` 在 SQL 层按 tag 精确匹配
- **修改** `ToolQuery`：新增 `tags: Option<Vec<String>>` 字段
- **修改** `ToolDao.query` SQL：使用 `json_each` 在 SQL 层按 tag 精确匹配
- **修改** 关键词搜索 SQL：扩展 LIKE 匹配范围到 tags 字段（Tool 和 Skill 统一）

### 4C-1b：技能包安装机制

- **修改** `AgentRuntimeConfig`：新增 `installed_skill_packs: Vec<String>` 字段，记录已安装技能包 tag
- **新增** `HrDomain.install_skill_pack(agent_id, tag)`：按 tag 查询 Published 技能 → 批量 `install_to_agent` → 记录 tag
- **新增** `HrDomain.uninstall_skill_pack(agent_id, tag)`：移除 tag 关联，**保留技能副本**（不删除 Agent 已有的技能）
- **新增** `HrDomain.reinstall_skill_pack(agent_id, tag)`：对已有副本的源技能直接覆盖，对新技能创建副本
- **新增** `HrDomain.list_installed_skill_packs(agent_id)`：返回已安装技能包 tag 列表
- **修改** `install_to_agent`：安装前检查 `parent_skill_id + author_id` 是否已存在副本，已存在则跳过（幂等）

### 4C-1c：技能包管理 API

- **新增** 3 个 Handler：`install_skill_pack` / `uninstall_skill_pack` / `list_installed_skill_packs`
- **新增** API DTO：请求/响应结构体
- **新增** 路由注册

### 4C-1d：唤醒时技能注入

- **修改** `RuntimeDomainImpl`：唤醒时通过 `list_for_agent` 加载 Agent 技能副本
- **修改** `PromptBuilder`：完善 `agent_skills` 方法，注入技能摘要（名称 + 描述）
- **新增** `SkillPo.to_prompt_summary()`：格式化技能摘要，不含完整内容
- **修改** `awaken()` 流程：在 Step 1.5 加载技能，注入 Prompt

### 4C-1e：search_skill 神经工具

- **新增** `search_skill` 神经工具：供 Agent 按需搜索技能库完整内容
- 支持关键词搜索 + tag 过滤
- 返回技能摘要列表（不含完整 skill.md 内容，Agent 可再通过文件 API 获取）

### Tool tag 过滤优化

- **修改** `load_builtin_tools`：从"全量加载 + 内存过滤"改为"SQL 层按 tag 过滤"
- **修改** `call_manual_tool_for_agent`：同样优化为 SQL 层过滤
- **修改** `filter_builtin_tools` 函数：保留用于测试，但生产路径改为 SQL 过滤

## Impact

- Affected specs: enhance-memory-system（无直接冲突，但 RuntimeDomain 接口扩展需保持兼容）
- Affected code:
  - `common/src/enums/` - 无变更
  - `src/models/skill.rs` - SkillPo 新增 to_prompt_summary
  - `src/models/agent.rs` - AgentRuntimeConfig 新增 installed_skill_packs
  - `src/service/dao/skill/mod.rs` - SkillQuery 新增 tags 字段
  - `src/service/dao/skill/sqlite.rs` - query SQL 增加 json_each 过滤
  - `src/service/dao/tool/mod.rs` - ToolQuery 新增 tags 字段
  - `src/service/dao/tool/sqlite.rs` - query SQL 增加 json_each 过滤
  - `src/service/dal/skill.rs` - 新增 install_skill_pack 等方法
  - `src/service/domain/hr/skill.rs` - 新增技能包管理方法
  - `src/service/domain/hr/agent.rs` - 新增技能包管理 API
  - `src/service/domain/runtime/awakening.rs` - 加载技能并注入 Prompt
  - `src/service/domain/runtime/context_assembly.rs` - PromptBuilder 完善
  - `src/handlers/hr/agent/` - 新增 3 个技能包 Handler
  - `src/handlers/hr/skill/` - 新增 search_skill Handler

## ADDED Requirements

### Requirement: Skill tag 精确过滤

系统 SHALL 支持在 SkillQuery 中通过 tags 参数精确过滤技能，使用 SQLite `json_each` 函数在 SQL 层完成匹配，避免全量加载到内存。

#### Scenario: 按单个 tag 查询技能
- **WHEN** 调用 `SkillDao.query(SkillQuery { tags: Some(vec!["project_management"]) })`
- **THEN** 返回 tags JSON 数组中包含 `"project_management"` 的所有技能

#### Scenario: 按多个 tag 查询技能
- **WHEN** 调用 `SkillDao.query(SkillQuery { tags: Some(vec!["project_management", "communication"]) })`
- **THEN** 返回 tags 中包含任一指定 tag 的技能（OR 语义）

#### Scenario: 不传 tags 时保持现有行为
- **WHEN** 调用 `SkillDao.query(SkillQuery { tags: None })`
- **THEN** 不添加 tag 过滤条件，返回所有匹配其他条件的技能

### Requirement: Tool tag 精确过滤

系统 SHALL 支持在 ToolQuery 中通过 tags 参数精确过滤工具，使用 SQLite `json_each` 函数在 SQL 层完成匹配。

#### Scenario: 按 tag 查询工具
- **WHEN** 调用 `ToolDao.query(ToolQuery { tags: Some(vec!["neural"]) })`
- **THEN** 返回 tags 中包含 `"neural"` 的所有启用工具

### Requirement: 关键词搜索扩展到 tags

系统 SHALL 在技能和工具的关键词搜索中，将 tags 字段纳入 LIKE 匹配范围。

#### Scenario: 关键词匹配 tags
- **WHEN** 搜索关键词 `"management"`，某技能 tags 为 `["project_management"]`
- **THEN** 该技能出现在搜索结果中

### Requirement: 技能包批量安装

系统 SHALL 支持通过 tag 批量安装技能到 Agent，安装时将 Published 技能复制为 Agent 的 Draft 副本。

#### Scenario: 首次安装技能包
- **WHEN** 调用 `install_skill_pack(agent_id, "project_management")`
- **AND** 系统中有 3 个 Published 技能带有 `"project_management"` tag
- **THEN** 3 个技能被复制到 Agent 目录（Draft 状态）
- **AND** `installed_skill_packs` 记录 `"project_management"` tag

#### Scenario: 幂等安装
- **WHEN** 对同一 Agent 再次调用 `install_skill_pack(agent_id, "project_management")`
- **THEN** 跳过已安装的技能（通过 parent_skill_id 检测）
- **AND** 不创建重复副本

#### Scenario: 部分失败
- **WHEN** 批量安装中某个技能安装失败
- **THEN** 已成功的安装保留，失败项记录日志，不回滚
- **AND** tag 仍然记录到 installed_skill_packs（部分成功也算安装）

### Requirement: 技能包卸载（保留副本）

系统 SHALL 在卸载技能包时仅移除 tag 关联，不删除 Agent 已有的技能副本。

#### Scenario: 卸载技能包
- **WHEN** 调用 `uninstall_skill_pack(agent_id, "project_management")`
- **THEN** `installed_skill_packs` 移除 `"project_management"` tag
- **AND** Agent 的技能副本保留不删除
- **AND** 唤醒时仍通过 `list_for_agent` 加载这些技能

### Requirement: 技能包重新安装（覆盖式）

系统 SHALL 支持重新安装技能包，对已有副本的源技能直接覆盖更新。

#### Scenario: 重新安装技能包
- **WHEN** 调用 `reinstall_skill_pack(agent_id, "project_management")`
- **AND** Agent 已有该包的技能副本
- **THEN** 对每个副本，检查源技能是否更新（content_hash 对比）
- **AND** 源技能有更新时，覆盖 Agent 副本内容（文件 + 元数据）
- **AND** 新增的技能（源技能库新增的）创建新副本

### Requirement: 唤醒时技能注入

系统 SHALL 在 Agent 唤醒时加载 Agent 的技能副本，将技能摘要注入 Prompt。

#### Scenario: 唤醒时加载技能
- **WHEN** Agent 唤醒
- **AND** Agent 已安装 3 个技能
- **THEN** Prompt 中包含"可用技能"部分，列出 3 个技能的名称和描述
- **AND** 不注入完整 skill.md 内容（避免 Token 膨胀）

#### Scenario: Agent 无技能
- **WHEN** Agent 唤醒
- **AND** Agent 未安装任何技能
- **THEN** Prompt 中不包含"可用技能"部分

### Requirement: search_skill 神经工具

系统 SHALL 提供 `search_skill` 神经工具，供 Agent 按需搜索技能库。

#### Scenario: Agent 搜索技能
- **WHEN** Agent 调用 `search_skill(keyword="数据分析")`
- **THEN** 返回匹配的技能列表（名称 + 描述 + tags + skill_id）
- **AND** 不返回完整 skill.md 内容

#### Scenario: 按 tag 搜索技能
- **WHEN** Agent 调用 `search_skill(tags=["project_management"])`
- **THEN** 返回包含该 tag 的技能列表

## MODIFIED Requirements

### Requirement: load_builtin_tools 优化

现有 `load_builtin_tools` 加载全部启用工具到内存再 Rust 过滤， SHALL 改为在 SQL 层按 tag 过滤。

#### Scenario: 加载神经工具
- **WHEN** 唤醒时加载神经工具
- **THEN** SQL 查询条件包含 `EXISTS (SELECT 1 FROM json_each(tags) WHERE value = 'neural')`
- **AND** 不加载非神经工具到内存

#### Scenario: 加载已安装工具包工具
- **WHEN** Agent 已安装 `"project_management"` 工具包
- **THEN** SQL 查询条件包含 `EXISTS (SELECT 1 FROM json_each(tags) WHERE value IN ('neural', 'project_management'))`
