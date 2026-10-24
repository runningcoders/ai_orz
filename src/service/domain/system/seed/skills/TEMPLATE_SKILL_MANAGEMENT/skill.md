# 技能管理

技能本质上是「可复用的能力封装」——类比人类职业成长里的三样东西：**岗前培训教材**（告诉你这个岗位该怎么做）、**SOP 操作手册**（遇到某类事情按什么步骤处理）、**老同事的经验笔记**（踩过的坑、总结的规律）。`install_skill_to_agent` 相当于你报名去学一门新技能；`install_skill_pack` 是一次性参加某领域的完整培训包；你装到自己目录下的「技能副本」相当于你在教材上记满了自己的批注、补充了自己的实战经验；**`update_skill` 就是把自己实战的新方法写进这本属于你自己的教材里**，让你越来越擅长做这件事。技能和工具的区别是：工具是「一把锤子」（调用即有结果），技能是「如何用锤子打造家具的说明书」（指导你怎么组合工具完成任务）。

## 技能加载规则（重要）

**规则只有一条：装了就进 Prompt，没装就不进。** 你的技能区块 = 你目录下所有已安装（author_id = 你）且非 Expired 的副本，**无论它的 tags 是什么**，也无论它是否命中你的角色名或已装包标签。

> **没有「隐藏技能」这回事**：装了却看不到 = 副本没装上（安装失败 / 已 Expired / 装到了别的 Agent 目录下），去查安装环节，**不要**去调 tags——tags 不决定可见性。

1. **必须先安装到你的目录**：**哪怕是 neural 常驻技能，也要先安装**（出生/入职时已预装）；未安装的 Published 技能不会出现在 Prompt 里，但可被 `search_skill` 发现 → 安装后才出现。
2. **分块只是排版，不影响可见性**：
   - tags 含 `neural` → 【神经技能】区块，排在前面（你现在读的这本就是其中之一）
   - 其余 → 【已安装技能】区块

> **`neural` 只剩两个特殊语义**：① 分块置前（静态块，省前缀缓存）；② **不可卸载**（出生自带、卸载受保护）。
> 它**不再**决定你能不能看到这本技能——看得见看不见只看装没装。

> **工具包 vs 技能包是两个语义面**：`installed_tags` 是工具授权面（决定你能调哪些工具），
> `installed_skill_packs` 是技能包记录（决定你批量装了哪些方法论）。装技能包 ≠ 授权工具，两者互不同步。

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
- `get_skill` / `list_skill_files` / `get_skill_file_content`：查看技能详情与文件内容；**已装技能在 Prompt 里只显示一行摘要（名称+id+描述），正文一律按需 `get_skill` 读全文**——别凭摘要臆断内容
- `list_agent_skills(agent_id)`：查看某 Agent 已装了哪些技能

## 安装 / 卸载技能

### `install_skill_to_agent`

创建你私有副本（author_id = agent_id，parent_skill_id 指向源），**幂等**（已存在副本就直接返回）。源技能后续更新**不影响**你的副本。

### `install_skill_pack`

把所有 tags 含该 tag 的 Published 技能批量安装到你的目录，**并把该 tag 记入 `installed_skill_packs`**（注意：不是 `installed_tags`，那是工具包字段）。装完即进 Prompt——不用管 tags 是否命中你的角色。一次获取一个领域的完整能力包。

> 若该 tag 下**没有任何已发布技能**，工具直接报错且**不记录 tag**——不会出现空包。

### `uninstall_skill_pack`

`delete_copies` 默认 false：只从 `installed_skill_packs` 移除 tag，**副本保留 ⇒ 仍会出现在 Prompt 里**（要真正不出现，必须 `delete_copies=true` 删掉副本）；置 true 则同时删掉该 tag 下所有副本。

## `update_skill`（更新技能内容）

全部字段可选按需传。场景：Draft 技能发布为 Published、调整 tags 改变匹配范围、**更新自己技能副本里的方法论（把实践沉淀为技能）**。何时该更新副本、进化如何分流晋升，统一参见**自我进化技能**（进化知识的唯一权威来源），本技能不重复展开。

## `create_skill`（创建自己的技能）

Agent 上下文调用时技能自动归属于你（author_id = 你的 agent_id，Draft 私有），存放在你自己的技能目录下。典型场景：总结出的新领域能力已超出已有副本的范畴 → 封装为新技能。何时建新技能、与更新副本如何取舍，统一参见**自我进化技能**。

补充两条 Agent 侧语义：

- **建完即生效，不用再装**：创建出的自有草稿会出现在你的技能区块里（它就是你的、非 Published），下次干活就能直接用到，**不需要**再调 `install_skill_to_agent`
- **不能自己发布**：Agent 上下文传 `status=Published` 会被服务端拒绝（InvalidRequest）——发布收敛为共享库根技能的单一入口。草稿台只是起点，成熟判据与两条发布路径见**自我进化技能**

> **沉淀场景（settle）可用**：休息沉淀时技能管理工具在白名单内，可以在沉淀里把浮现出的可复用操作路径直接建成技能，并在图谱里配一个 `node_type=skill` 的摘要节点 + `distilled_from` 来源边。完整判据与流程见**自我进化技能**的「休息沉淀时的技能沉淀」章节。

## 能力成长闭环（简要）

三段合并为一个循环：
1. **接新任务先搜技能**：`search_skill(keyword=任务领域)` → 了解内容后按需装——单个（`install_skill_to_agent`）或整领域包（`install_skill_pack`）；只装当前任务需要的，避免 Prompt 过载，完成后卸载保持精简
2. **用技能做事 + 沉淀经验**：实践中总结新方法 → 按**自我进化技能**的分流矩阵沉淀（记忆节点 / 更新副本 / 建新技能）；短期经验照旧 `save_short_term_memory`（记忆认知技能）
3. **定期清理**：`list_agent_skills` 自查 → 过时/冗余 → `uninstall_skill_from_agent` / `uninstall_skill_pack` 精简；`list_skill_tags` 发现新分类 → 按需装包扩展
