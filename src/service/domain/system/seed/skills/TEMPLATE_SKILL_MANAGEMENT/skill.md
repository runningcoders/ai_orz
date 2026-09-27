# 技能管理

技能本质上是「可复用的能力封装」——类比人类职业成长里的三样东西：**岗前培训教材**（告诉你这个岗位该怎么做）、**SOP 操作手册**（遇到某类事情按什么步骤处理）、**老同事的经验笔记**（踩过的坑、总结的规律）。`install_skill_to_agent` 相当于你报名去学一门新技能；`install_skill_pack` 是一次性参加某领域的完整培训包；你装到自己目录下的「技能副本」相当于你在教材上记满了自己的批注、补充了自己的实战经验；**`update_skill` 就是把自己实战的新方法写进这本属于你自己的教材里**，让你越来越擅长做这件事。技能和工具的区别是：工具是「一把锤子」（调用即有结果），技能是「如何用锤子打造家具的说明书」（指导你怎么组合工具完成任务）。

## 技能加载规则（重要）

技能加载是「安装范围限定 + 标签匹配」双层规则：

1. **第一层 · 必须先安装到你的目录**：只在你已安装（author_id = agent_id）且非 Expired 的副本范围内加载；**哪怕是 neural 常驻技能，也要先安装**；未安装的 Published 技能不会出现在 Prompt 里（但可被 `search_skill` 发现，再安装）。
2. **第二层 · 按 tags 分块**：
   - tags 含 `neural` → **神经技能**：你必加载，常驻 Prompt（你现在读的这本就是其中之一）
   - tags 不含 `neural` 但与你的 `match_keys` 有交集 → **必加载技能**：出现在 Prompt 中
   - 否则 → **隐藏技能**：不展示在 Prompt，但你仍可通过 `search_skill` 发现 + `get_skill_file_content` 读取内容使用（装了却没出现在 Prompt = tags 与 match_keys 无交集，排查方向）

> **`match_keys` 是什么**：你的 `roles` 角色 ∪ `installed_tags` 已安装技能包标签。例如 roles=`backend_developer`、installed_tags=`project_management`，则 match_keys 两者并集。安装技能包（install_skill_pack）会把对应 tag 加入 installed_tags，从而让该 tag 的技能进入必加载。

## 技能查询 / 发现（你常用的都是 neural 常驻）

> 参数与默认值见工具 Schema，此处只记 Schema 看不出来的语义与坑。

| 工具 | 用途与语义 |
|------|------|
| **`search_skill`**（你最常用，neural 常驻） | 按关键词 / tags 搜技能库；多 tag 为 **OR 命中任一** |
| `list_skill_tags` | 列出所有 Published 技能的不重复 tags（了解技能分类全貌） |
| `uninstall_skill_from_agent`（neural 常驻） | 卸载你目录下的技能副本；**只能卸副本，不能卸原始技能** |

### 其他技能管理工具（非 neural 或管理用，简写）

- `list_skills` / `query_skills`：分页列表 / 按条件过滤，管理场景用
- **`search_skills`**（和 `search_skill` 只差一个 s）：搜 Published 技能的管理版本，条件更全；**对 Agent 而言，日常只用 `search_skill`（neural 常驻）即可**
- `get_skill` / `list_skill_files` / `get_skill_file_content`：查看技能详情与文件内容；**注意 neural 常驻技能已经在 Prompt 里了，不用再读文件**，隐藏技能按需才读
- `list_agent_skills(agent_id)`：查看某 Agent 已装了哪些技能

## 安装 / 卸载技能

### `install_skill_to_agent`

创建你私有副本（author_id = agent_id，parent_skill_id 指向源），**幂等**（已存在副本就直接返回）。源技能后续更新**不影响**你的副本。

### `install_skill_pack`

把所有 tags 含该 tag 的 Published 技能批量安装到 Agent，**并把该 tag 加入 Agent 的 installed_tags**，从而让该 tag 下的技能进入「必加载技能」分块。一次获取一个领域的完整能力包。

### `uninstall_skill_pack`

`delete_copies` 默认 false：只从 installed_tags 移除 tag，副本保留但不再必加载；置 true 则同时删掉该 tag 下所有副本。

## `update_skill`（更新技能内容）

全部字段可选按需传。场景：Draft 技能发布为 Published、调整 tags 改变匹配范围、**更新自己技能副本里的方法论（把实践沉淀为技能）**。何时该更新副本、进化如何分流晋升，统一参见**自我进化技能**（进化知识的唯一权威来源），本技能不重复展开。

## `create_skill`（创建自己的技能）

Agent 上下文调用时技能自动归属于你（author_id = 你的 agent_id，Draft 私有），存放在你自己的技能目录下。典型场景：总结出的新领域能力已超出已有副本的范畴 → 封装为新技能。何时建新技能、与更新副本如何取舍，统一参见**自我进化技能**。

## 能力成长闭环（简要）

三段合并为一个循环：
1. **接新任务先搜技能**：`search_skill(keyword=任务领域)` → 了解内容后按需装——单个（`install_skill_to_agent`）或整领域包（`install_skill_pack`）；只装当前任务需要的，避免 Prompt 过载，完成后卸载保持精简
2. **用技能做事 + 沉淀经验**：实践中总结新方法 → 按**自我进化技能**的分流矩阵沉淀（记忆节点 / 更新副本 / 建新技能）；短期经验照旧 `save_short_term_memory`（记忆认知技能）
3. **定期清理**：`list_agent_skills` 自查 → 过时/冗余 → `uninstall_skill_from_agent` / `uninstall_skill_pack` 精简；`list_skill_tags` 发现新分类 → 按需装包扩展
