# 技能管理

技能本质上是「可复用的能力封装」——类比人类职业成长里的三样东西：**岗前培训教材**（告诉你这个岗位该怎么做）、**SOP 操作手册**（遇到某类事情按什么步骤处理）、**老同事的经验笔记**（踩过的坑、总结的规律）。`install_skill_to_agent` 相当于你报名去学一门新技能；`install_skill_pack` 是一次性参加某领域的完整培训包；你装到自己目录下的「技能副本」相当于你在教材上记满了自己的批注、补充了自己的实战经验；**`update_skill` 就是把自己实战的新方法写进这本属于你自己的教材里**，让你越来越擅长做这件事。技能和工具的区别是：工具是「一把锤子」（调用即有结果），技能是「如何用锤子打造家具的说明书」（指导你怎么组合工具完成任务）。

技能是可复用的工作方法 / 领域知识 / 操作规范的封装（与「工具」的区别：工具是调用即返回结果的执行接口；技能是指导你做事的知识）。

## 技能加载规则（重要）

技能加载是「安装范围限定 + 标签匹配」双层规则：

1. **第一层 · 必须先安装到你的目录**：只在你已安装（author_id = agent_id）且非 Expired 的副本范围内加载；**哪怕是 neural 常驻技能，也要先安装**；未安装的 Published 技能不会出现在 Prompt 里（但可被 `search_skill` 发现，再安装）。
2. **第二层 · 按 tags 分块**：
   - tags 含 `neural` → **神经技能**：你必加载，常驻 Prompt（你现在读的这本就是其中之一）
   - tags 不含 `neural` 但与你的 `match_keys` 有交集 → **必加载技能**：出现在 Prompt 中
   - 否则 → **隐藏技能**：不展示在 Prompt，但你仍可通过 `search_skill` 发现 + `get_skill_file_content` 读取内容使用

> **`match_keys` 是什么**：你的 `roles` 角色 ∪ `installed_tags` 已安装技能包标签。例如 roles=`backend_developer`、installed_tags=`project_management`，则 match_keys 两者并集。安装技能包（install_skill_pack）会把对应 tag 加入 installed_tags，从而让该 tag 的技能进入必加载。

## 技能查询 / 发现（你常用的都是 neural 常驻）

| 工具 | 用途 | 参数 |
|------|------|------|
| **`search_skill`**（你最常用） | 按关键词 / tags 搜技能库（精简参数，neural 常驻） | `keyword`、`tags`（OR 命中任一）、`limit`（默认 10） |
| `list_skill_tags` | 列出所有 Published 技能的不重复 tags（了解技能分类全貌） | 无 |
| `uninstall_skill_from_agent` | 卸载你目录下的技能副本（neural 常驻） | `skill_id` + `agent_id`（只能卸副本，不能卸原始技能） |

### 其他技能管理工具（非 neural 或管理用，简写）

- `list_skills` / `query_skills`：分页列表 / 按条件过滤（按 category/author/status/ids/parent_skill_id/tags 等），管理场景用
- **`search_skills`**（和 `search_skill` 只差一个 s）：搜 Published 技能的管理版本，条件更全；**对 Agent 而言，日常只用 `search_skill`（neural 常驻）即可**
- `get_skill` / `list_skill_files` / `get_skill_file_content`：查看技能详情与文件内容；**注意 neural 常驻技能已经在 Prompt 里了，不用再读文件**，隐藏技能按需才读
- `list_agent_skills(agent_id)`：查看某 Agent 已装了哪些技能

## 安装 / 卸载技能

### `install_skill_to_agent`

参数：`skill_id`（源技能 ID，路径）、`agent_id`（目标 Agent）。行为：创建你私有副本（author_id = agent_id，parent_skill_id 指向源），**幂等**（已存在副本就直接返回）。源技能后续更新不影响你的副本。

### `install_skill_pack`

参数：`agent_id`（路径）、`tag`（路径，如 `project_management`）。把所有 tags 含该 tag 的 Published 技能批量安装到 Agent，**并把该 tag 加入 Agent 的 installed_tags**，从而让该 tag 下的技能进入「必加载技能」分块。一次获取一个领域的完整能力包。

### `uninstall_skill_pack`

参数：`agent_id`、`tag`、可选 `delete_copies`（默认 false）：
- `false`：只从 installed_tags 移除 tag，副本保留但不再必加载
- `true`：同时删掉该 tag 下所有副本

## `update_skill`（更新技能内容）

参数全部可选按需传：`skill_id`（路径）、`name` / `description` / `tags` / `category` / `status` / `content`（新的 skill.md 主文件）/ `files`（附加文件）。场景：Draft 技能发布为 Published、调整 tags 改变匹配范围、**更新自己技能副本里的方法论（把实践沉淀为技能）**。

## 能力成长闭环（简要）

三段合并为一个循环：
1. **接新任务先搜技能**：`search_skill(keyword=任务领域)` → 发现可用 → 单个装（`install_skill_to_agent`）或整领域包（`install_skill_pack`）
2. **用技能做事 + 沉淀经验**：实践中总结新方法 → `update_skill(content=...)` 更新自己副本里的 skill.md；同时短期经验 `save_short_term_memory`（记忆认知技能）
3. **定期清理**：`list_agent_skills` 自查 → 过时/冗余 → `uninstall_skill_from_agent` / `uninstall_skill_pack` 精简；`list_skill_tags` 发现新分类 → 按需装包扩展

## 最佳实践

1. **先搜后装**：安装前先 `search_skill` 了解内容，别盲目装
2. **理解 match_keys**：非 neural 技能装了仍没出现在 Prompt → 检查 tags 与你的 roles/installed_tags 是否有交集；或直接用 `get_skill_file_content` 读
3. **按需安装**：只装当前任务需要的，避免 Prompt 过载；完成后可卸载以保持精简
4. **技能包是能力包**：`install_skill_pack(tag=领域)` 一次装齐并让该领域技能自动必加载，省事
5. **副本独立于源**：你装的副本不会跟着源技能自动更新，要跟进可以 `update_skill` 自己迭代
6. **学会更新自己的方法论**：`update_skill(content=新内容)` 是你把实践沉淀为「个人技能」的关键能力
7. **区分 search_skill vs search_skills**：日常只用前者（neural 常驻、精简参数）；后者是更全的管理版
8. **学以致用 + 记忆沉淀**：技能指导方法，短期经验用记忆技能保存，长期形成个人能力
