# Tasks

- [x] Task 1: DAO 层 tag 过滤能力建设（Skill + Tool 统一）
  - [x] SubTask 1.1: SkillQuery 新增 `tags: Option<Vec<String>>` 字段
  - [x] SubTask 1.2: SkillDao.query SQL 增加 `json_each` tag 过滤条件（OR 语义）
  - [x] SubTask 1.3: ToolQuery 新增 `tags: Option<Vec<String>>` 字段
  - [x] SubTask 1.4: ToolDao.query SQL 增加 `json_each` tag 过滤条件
  - [x] SubTask 1.5: SkillDao 和 ToolDao 关键词搜索 SQL 扩展 LIKE 匹配到 tags 字段
  - [x] SubTask 1.6: 编写 DAO 层单元测试（按 tag 查询、多 tag OR、无 tag 保持现有行为、关键词匹配 tags）

- [x] Task 2: AgentRuntimeConfig 新增 installed_skill_packs 字段
  - [x] SubTask 2.1: AgentRuntimeConfig 新增 `installed_skill_packs: Vec<String>` 字段
  - [x] SubTask 2.2: 新增 `install_skill_pack_tag` / `uninstall_skill_pack_tag` / `has_skill_pack_tag` 方法（幂等）
  - [x] SubTask 2.3: AgentPo 层封装 `get_installed_skill_packs()` / `install_skill_pack_tag()` / `uninstall_skill_pack_tag()`
  - [x] SubTask 2.4: 编写单元测试

- [x] Task 3: install_to_agent 幂等性增强
  - [x] SubTask 3.1: SkillDal.install_to_agent 安装前检查 parent_skill_id + author_id 是否已有副本
  - [x] SubTask 3.2: 已存在副本时跳过安装，返回已有技能
  - [x] SubTask 3.3: 编写单元测试（重复安装不创建副本）

- [x] Task 4: 技能包安装/卸载/重装/列表 Domain + DAL 层
  - [x] SubTask 4.1: SkillDal 新增 `list_published_by_tag(tag)` 方法（按 tag 查 Published 技能）
  - [x] SubTask 4.2: SkillDal 新增 `find_agent_skill_copies(parent_skill_ids, agent_id)` 方法（查 Agent 已有副本）
  - [x] SubTask 4.3: HrDomain 新增 `install_skill_pack(ctx, agent_id, tag)` — 批量安装 + 记录 tag
  - [x] SubTask 4.4: HrDomain 新增 `uninstall_skill_pack(ctx, agent_id, tag)` — 移除 tag，保留副本
  - [x] SubTask 4.5: HrDomain 新增 `reinstall_skill_pack(ctx, agent_id, tag)` — 覆盖式重装
  - [x] SubTask 4.6: HrDomain 新增 `list_installed_skill_packs(ctx, agent_id)` — 返回 tag 列表
  - [x] SubTask 4.7: 编写 Domain 层单元测试

- [x] Task 5: 技能包管理 API（Handler + Router）
  - [x] SubTask 5.1: 新增 API DTO（请求/响应结构体，放在 common/src/api/skill.rs）
  - [x] SubTask 5.2: 新增 `install_skill_pack` Handler
  - [x] SubTask 5.3: 新增 `uninstall_skill_pack` Handler
  - [x] SubTask 5.4: 新增 `list_installed_skill_packs` Handler
  - [x] SubTask 5.5: 注册路由到 router.rs

- [x] Task 6: 唤醒时技能注入
  - [x] SubTask 6.1: SkillPo 新增 `to_prompt_summary()` 方法（名称 + 描述，不含完整内容）
  - [x] SubTask 6.2: PromptBuilder 完善 `agent_skills(skills: &[SkillPo])` 方法
  - [x] SubTask 6.3: RuntimeDomainImpl 新增 `load_agent_skills(ctx, agent_id)` 方法（调用 list_for_agent）
  - [x] SubTask 6.4: awaken() 流程加载技能，调用 PromptBuilder.agent_skills()
  - [x] SubTask 6.5: 编写单元测试（有技能/无技能两种场景）

- [x] Task 7: search_skill 神经工具
  - [x] SubTask 7.1: 新增 `search_skill` Handler（注册为神经工具，neural flag）
  - [x] SubTask 7.2: 支持关键词搜索 + tag 过滤参数
  - [x] SubTask 7.3: 返回技能摘要列表（skill_id + name + description + tags）
  - [x] SubTask 7.4: 注册路由

- [x] Task 8: Tool tag 过滤优化（load_builtin_tools）
  - [x] SubTask 8.1: load_builtin_tools 改为构造 ToolQuery { tags: Some(neural_tags + installed_tags) } 查询
  - [x] SubTask 8.2: call_manual_tool_for_agent 中的工具查找同样优化
  - [x] SubTask 8.3: 保留 filter_builtin_tools 函数用于单元测试
  - [x] SubTask 8.4: 编写测试验证 SQL 层过滤与原内存过滤结果一致

- [x] Task 9: 阶段验证（编译 + 全量测试）
  - [x] SubTask 9.1: cargo check 编译通过
  - [x] SubTask 9.2: cargo test 全量测试通过（601 个测试 100% 通过）
  - [x] SubTask 9.3: 检查无 warning 回归

# Task Dependencies
- Task 2 依赖 Task 1（无强依赖，可并行）
- Task 3 无依赖，可并行
- Task 4 依赖 Task 1（需要 tag 查询）+ Task 2（需要 installed_skill_packs）+ Task 3（需要幂等安装）
- Task 5 依赖 Task 4（需要 Domain 层方法）
- Task 6 依赖 Task 1（需要 list_for_agent，已有，无强依赖）
- Task 7 依赖 Task 1（需要 tag 过滤的 SkillQuery）
- Task 8 依赖 Task 1（需要 ToolQuery tag 过滤）
- Task 9 依赖所有任务完成

# Parallelizable Groups
- **Group A**（无依赖，可立即开始）：Task 1, Task 2, Task 3, Task 6（SubTask 6.1-6.2）
- **Group B**（依赖 Group A）：Task 4, Task 7, Task 8（SubTask 8.1-8.3）
- **Group C**（依赖 Group B）：Task 5, Task 6（SubTask 6.3-6.5）
- **Final**：Task 9
