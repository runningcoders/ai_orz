---
kind: wiki_knowledge_card
name: 知识图谱联想与关系边补写：update_memory relations 唯一建边入口 + 正文引用对遍历不可达 + 沉淀探索引导
category: 记忆系统 / 知识图谱
scope:
  - "src/handlers/hr/agent/update_memory.rs"
  - "common/src/api/neural_tools.rs"
  - "src/service/dal/agent/builder/default.rs"
  - "src/handlers/hr/agent/save_long_term_memory.rs"
  - "src/service/dal/memory.rs"
source_files:
  - common/src/api/neural_tools.rs#L172-L194 (UpdateMemoryParams：content/summary/tags/status/node_tags/relations；relations 注释即红线——给既有节点补边的唯一路径，正文提及/引用不算建边，遍历只认关系表)
  - common/src/api/neural_tools.rs#L413-L435 (KnowledgeRelationParam{source_node_id,target_node_id,relation_type,weight}；relation_type 原样保存原样展示不归一化；weight 0.0~1.0 仅在确有判断时给，省略渲染为基准线宽)
  - src/handlers/hr/agent/update_memory.rs#L14-L20 (register_handler_tool 描述：明确 "this is the ONLY way to add edges to an existing node"；正文引用对 traversal 不可达)
  - src/handlers/hr/agent/update_memory.rs#L46-L53 (relations 仅知识节点有效；短期记忆携带 relations → 400 InvalidRequest——禁止静默忽略，否则沉淀联想悄悄丢失)
  - src/handlers/hr/agent/update_memory.rs#L135-L157 (更新成功**之后**才建边：KnowledgeNodeRelationPo + relation_type 原样落库 + weight 归一化 + KnowledgeRelationStatus::Active → CreateRelations；顺序保证内容写失败不留下孤儿边)
  - src/handlers/hr/agent/update_memory.rs#L243-L301 (回归：非法 status 报 400 且绝不静默写库；短期记忆 relations 必须报 400)
  - src/handlers/hr/agent/update_memory.rs#L303-L399 (回归：既有节点补边——词表外关系名「实现」原样保留 + weight 穿透 + 内容更新与建边同一次调用生效 + 用 search_memory 遍历验证边真实可见)
  - src/service/dal/agent/builder/default.rs#L840-L880 (awaken/settle 双场景 System 指引：闲聊/简单消息**不豁免检索，而是换个目的**做联想检索；§4 检索节奏「递进探索 ✅ / 同义重试 ❌」防死循环)
  - src/service/dal/agent/builder/default.rs#L780-L800 (沉淀场景引导：还缺资料 → 检索知识图谱，鼓励沿命中节点递进探索)
  - src/service/dal/memory.rs (CreateRelations 落边路径：与 save_long_term_memory 共用同一建边入口)
  - src/handlers/hr/agent/save_long_term_memory.rs (节点+边同建路径：新节点用 save_long_term_memory，既有节点补边用 update_memory)
  - docs/wiki/zh/content/功能模块/AI Agent 管理/记忆系统管理.md
  - docs/wiki/zh/content/项目概述/核心功能特性/综合搜索能力/知识图谱搜索.md
  - 【关联卡·上游】docs/wiki/knowledge/zh/记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 三因子推荐/记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 三因子推荐.md
  - 【关联卡·下游】docs/wiki/knowledge/zh/知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出/知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出.md
---

# 知识图谱联想与关系边补写（update_memory relations）

## §1 概述

**本卡角色**：解决 Agent 知识图谱的一个静默缺陷——「**正文隐形边**」。Agent 沉淀经验时会在节点正文里写「本方案与 XX 相关」「参考 YY」，读起来像建立了关联，但**图谱遍历只查关系表**，正文引用完全不可达：用户沿图谱探索时，这些"看起来存在"的关联一个都走不通。本卡覆盖修复的两半：**工具侧**（`update_memory` 新增 `relations`，给既有节点补边的唯一入口）+ **提示侧**（沉淀/唤醒 System 指引改为「递进探索 ✅ / 同义重试 ❌」，闲聊从"豁免检索"改为"换个目的检索找联想"）。

- **为什么必须显式建边**：知识图谱的边是**结构化关系表**（`KnowledgeNodeRelationPo`，含 `relation_type` + `weight` + `status`），遍历（BFS/DFS）只沿这张表走。正文是自由文本，无法参与遍历——这是设计上的硬边界，不是 bug。因此"想建关联"必须走 `relations` 参数。
- **唯一入口**：给**新节点**建边用 `save_long_term_memory`（节点+边同建）；给**既有节点**补边只有 `update_memory.relations` 一条路。工具描述里已把这句话写死（"this is the ONLY way..."），让模型在决策时就知道必须显式传参。
- **失败要报错不要静默**：短期记忆传 `relations`、非法 `status` 值，都必须返回 `invalid_request`——静默忽略会让调用方以为边已建立/状态已改，比报错更糟。

---

## §2 关键文件与职责表

| 文件 | 角色 | 关键内容 | 锚点 |
|------|------|---------|------|
| [common/src/api/neural_tools.rs](common/src/api/neural_tools.rs) | DTO 契约 | `UpdateMemoryParams.relations`、`KnowledgeRelationParam`（relation_type 原样、weight 可省） | `:L172-L194`, `:L413-L435` |
| [src/handlers/hr/agent/update_memory.rs](src/handlers/hr/agent/update_memory.rs) | 建边入口 | relations 仅知识节点（否则 400）+ 更新成功后建边 + 回归测试 | `:L14-L20`, `:L46-L53`, `:L135-L157` |
| [src/service/dal/memory.rs](src/service/dal/memory.rs) | 落边路径 | `CreateRelations`：与 `save_long_term_memory` 共用同一建边实现 | — |
| [src/service/dal/agent/builder/default.rs](src/service/dal/agent/builder/default.rs) | Prompt 引导 | 闲聊换目的检索 + 递进探索/同义重试节奏 + 沉淀沿节点探索 | `:L780-L880` |

---

## §3 架构约定

### 3.1 两条建边路径的分工

| 场景 | 入口 | 说明 |
|------|------|------|
| 新建知识节点 + 同时建边 | `save_long_term_memory(relations=...)` | 节点与边一次成型 |
| **既有**知识节点补边 | `update_memory(relations=...)` | **唯一路径**；正文提及不算 |
| 正文里写「与 X 相关」 | — | ❌ 不产生边；遍历不可达 |

**关联卡说明**：本卡是「记忆搜索增强三合一」卡与「知识图谱 traverse」卡的**上游写入侧**——那两张卡讲"怎么把关联读出来"（FTS5 + traverse 遍历 + 种子推荐），本卡讲"关联怎么写进去"。三者构成「写入 → 遍历 → 推荐」的完整闭环；本卡不重复描述遍历算法（见 traverse 卡）与推荐打分（见 recommend_seed_nodes 卡）。

### 3.2 建边时序（为什么放在 update 之后）

```
query(memory_id) → 校验（存在 / relations 仅知识节点）
      ▼
match po: ShortTerm | KnowledgeNode → 逐字段更新（content / summary / node_tags / status）
      ▼
update(updated_memory)                      ← ① 内容先落库
      ▼
if relations 非空 → CreateRelations(...)    ← ② 再落边
```

顺序是刻意的：内容写失败时直接返回错误，**不会留下挂在新内容上的孤儿边**。

### 3.3 关系类型与权重语义

- `relation_type`：**原样保存、原样展示、不归一化**。优先用规范词（`related`/`contains`/`depends`/`prerequisite`/`causes` …），词表不贴切时**直接写判断出的关系名**（如「实现」「被测试覆盖」「退化自」），不会被替换成「自定义」。原则：**语义准确 > 用词规范**。
- `weight`：0.0~1.0，只在确有强度判断时给（"直接依赖" ≈ 1.0、"顺带提到" ≈ 0.2）；拿不准就省略 → 图上渲染为基准线宽（好过随手给 0.5）。

### 3.4 Prompt 侧引导（awaken / settle 双场景）

| 旧口径 | 新口径 |
|--------|--------|
| 纯闲聊 Chat 型可「豁免检索」 | 闲聊**也是检索**，只是**换个目的**：为找联想、找已有沉淀（不深挖、不重试） |
| （无明确节奏约束） | §4 检索节奏：**递进探索 ✅**（命中节点后展开关联网络、追因果链） / **同义重试 ❌**（空结果 = 系统暂无相关知识，换词重试 = 死循环） |

两条一起看才成立：**鼓励往前走（沿图谱递进）**，**禁止原地打转（同义重试）**。

---

## §4 硬约束与回归红线

1. ❌ **禁止把正文引用当边**：任何"在正文/描述里提到另一个节点就算关联"的实现都违反图谱契约；遍历只认关系表。
2. ❌ **禁止静默忽略 `relations` 的非法使用**：短期记忆传 relations → 必须 400；静默忽略 = 沉淀联想悄悄丢失。
3. ❌ **禁止 `status` 非法值兜底成 `active`**：拼错状态必须 400；兜底会让"拼错"变成一次静默写入。
4. ❌ **禁止先建边后更新内容**：建边必须在 `update` 成功之后；否则内容失败会留下孤儿边。
5. ✅ **`relation_type` 必须原样落库**：不得归一化成「自定义」或丢弃词表外关系名；违反映射信息丢失。
6. ✅ **`weight` 必须穿透到遍历结果**：建边写入的 weight 要在 traverse / search 结果里可见（回归测试已锁）。
7. ✅ **`update_memory` 工具描述必须写明"唯一建边入口"**：让模型在决策层就知道要显式传 relations，而不是在正文里"提一句"。
8. ✅ **Prompt 引导必须同时约束"探索"与"重试"**：只鼓励探索而不禁止同义重试 = 保留死循环风险。
9. ✅ **内容更新与建边允许同一次调用完成**：不要拆成两次调用要求模型先更新再建边（回归测试已覆盖同调用生效）。

---

## §5 历史演进

| 版本 | 变更 | 影响范围 |
|------|------|---------|
| v1.0（补边入口落地） | `UpdateMemoryParams` 新增 `relations`；`update_memory` 更新成功后经 `CreateRelations` 落边；relations 仅知识节点（否则 400）；工具描述写明"唯一建边入口 + 正文不可达" | `common/src/api/neural_tools.rs` + `src/handlers/hr/agent/update_memory.rs` |
| v1.1（Prompt 引导对齐） | awaken/settle 双场景 System 指引改为「闲聊换目的检索」+「递进探索 ✅ / 同义重试 ❌」；沉淀场景鼓励沿命中节点递进探索 | `src/service/dal/agent/builder/default.rs` |

**引入原因**：修复"沉淀联想落成隐形边"——Agent 的关联意图写在正文里，图谱却读不到，导致知识图谱看起来稀疏、探索断链。修复没有改遍历算法（遍历本就正确），而是补上"写入侧缺的唯一入口"并在 Prompt 里把探索节奏说清。