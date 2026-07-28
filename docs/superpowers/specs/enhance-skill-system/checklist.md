# Checklist

## DAO 层 tag 过滤

- [x] SkillQuery 新增 tags 字段，支持按 tag 精确过滤
- [x] SkillDao.query SQL 使用 json_each 按 tag 过滤（OR 语义）
- [x] ToolQuery 新增 tags 字段，支持按 tag 精确过滤
- [x] ToolDao.query SQL 使用 json_each 按 tag 过滤
- [x] Skill 关键词搜索 LIKE 扩展到 tags 字段
- [x] Tool 关键词搜索 LIKE 扩展到 tags 字段
- [x] 不传 tags 时保持现有行为（无 tag 过滤条件）
- [x] DAO 层单元测试覆盖：单 tag、多 tag OR、无 tag、关键词匹配 tags

## AgentRuntimeConfig 扩展

- [x] installed_skill_packs 字段定义，serde 序列化/反序列化正确
- [x] install_skill_pack_tag 幂等（重复安装不重复添加）
- [x] uninstall_skill_pack_tag 幂等（不存在时无副作用）
- [x] has_skill_pack_tag 查询正确
- [x] AgentPo 层封装方法正确委托到 runtime_config

## install_to_agent 幂等性

- [x] 安装前检查 parent_skill_id + author_id 是否已有副本
- [x] 已有副本时跳过安装，返回已有技能
- [x] 无副本时正常创建新副本
- [x] 单元测试覆盖重复安装场景

## 技能包安装/卸载/重装/列表

- [x] install_skill_pack 按 tag 查询 Published 技能并批量安装
- [x] install_skill_pack 幂等（已安装的 tag 跳过，已有副本的技能跳过）
- [x] install_skill_pack 部分失败时不回滚已成功的安装
- [x] install_skill_pack 记录 tag 到 installed_skill_packs
- [x] uninstall_skill_pack 移除 tag 但保留技能副本
- [x] reinstall_skill_pack 覆盖已有副本（源技能有更新时）
- [x] reinstall_skill_pack 为新增技能创建新副本
- [x] list_installed_skill_packs 返回已安装 tag 列表
- [x] Domain 层单元测试覆盖各场景

## 技能包管理 API

- [x] install_skill_pack Handler 正确调用 Domain 层
- [x] uninstall_skill_pack Handler 正确调用 Domain 层
- [x] list_installed_skill_packs Handler 正确调用 Domain 层
- [x] API DTO 定义在 common/src/api/skill.rs
- [x] 路由注册到 router.rs
- [x] Handler 层不直接调用 DAL/DAO

## 唤醒时技能注入

- [x] SkillPo.to_prompt_summary() 只输出名称 + 描述，不含完整内容
- [x] PromptBuilder.agent_skills() 正确格式化技能摘要
- [x] awaken() 在加载工具后加载技能
- [x] 有技能时 Prompt 包含"可用技能"部分
- [x] 无技能时 Prompt 不包含"可用技能"部分
- [x] 单元测试覆盖有技能/无技能两种场景

## search_skill 神经工具

- [x] search_skill 注册为神经工具（neural flag）
- [x] 支持关键词搜索参数
- [x] 支持 tag 过滤参数
- [x] 返回技能摘要列表（skill_id + name + description + tags）
- [x] 不返回完整 skill.md 内容
- [x] 路由注册

## Tool tag 过滤优化

- [x] load_builtin_tools 使用 ToolQuery { tags } 在 SQL 层过滤
- [x] call_manual_tool_for_agent 中的工具查找同样优化
- [x] filter_builtin_tools 函数保留用于测试
- [x] SQL 层过滤结果与原内存过滤结果一致
- [x] 单元测试验证一致性

## 最终验证

- [x] cargo check 编译通过
- [x] cargo test 全量测试通过
- [x] 无新增 warning 回归
