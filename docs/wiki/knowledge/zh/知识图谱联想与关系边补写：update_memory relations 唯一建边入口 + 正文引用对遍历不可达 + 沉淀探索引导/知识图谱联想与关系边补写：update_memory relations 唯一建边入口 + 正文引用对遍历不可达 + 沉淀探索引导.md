---
kind: wiki_knowledge_card
name: 知识图谱联想与关系边补写：update_memory relations 唯一建边入口 + 正文引用对遍历不可达 + 沉淀探索引导
category: 记忆系统 / 知识图谱
scope:
  - "src/handlers/hr/agent/update_memory.rs"
  - "common/src/api/neural_tools.rs"
  - "common/src/error/code.rs"
  - "src/service/dal/agent/builder/default.rs"
  - "src/handlers/hr/agent/mod.rs"
  - "src/handlers/hr/agent/save_long_term_memory.rs"
  - "src/service/dal/memory/graph.rs"
  - "src/service/dao/memory/*.rs"
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
  - src/service/dal/memory/store.rs (CreateRelations 落边路径：与 save_long_term_memory 共用同一建边入口)
  - src/handlers/hr/agent/save_long_term_memory.rs (节点+边同建路径：新节点用 save_long_term_memory，既有节点补边用 update_memory)
  - docs/wiki/zh/content/功能模块/AI Agent 管理/记忆系统管理.md
  - docs/wiki/zh/content/项目概述/核心功能特性/综合搜索能力/知识图谱搜索.md
  - 【关联卡·上游】docs/wiki/knowledge/zh/记忆搜索增强三合一：FTS5%20tags%20语义过滤%20+%20图谱%20traverse%20BFS／DFS%20遍历%20+%20recommend_seed_nodes%20三因子推荐/记忆搜索增强三合一：FTS5%20tags%20语义过滤%20+%20图谱%20traverse%20BFS／DFS%20遍历%20+%20recommend_seed_nodes%20三因子推荐.md
  - 【关联卡·下游】docs/wiki/knowledge/zh/知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出/知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出.md
  - src/handlers/hr/agent/mod.rs#L294-L420 (v1.2 增量 87c35228：两写入口共享 write_relations_with_receipt——is_placeholder_node_id L294（self/kn_self/kn_new_self 占位符逐项拒）/is_valid_node_id L303（kn_ 前缀）/静态校验→批内同键去重→端点批量 Active 预检→逐条独立落库；整批永不 Err，逐条 RelationWriteResult{key,status,relation_id,error_code,message,field})
  - common/src/api/neural_tools.rs (v1.2：RelationWriteResult / RelationWriteStatus；save_long_term_memory 与 update_memory 两 Response 挂 relations_result + property_issues_v2，skip 序列化默认空)
  - common/src/error/code.rs#L132-L156 (v1.2：建边 5 错误码 placeholder_node_id_rejected(400) / invalid_node_id(400) / source_node_not_found(404) / target_node_not_found(404) / duplicate_active_edge(409)；落库内部不一致回 internal(500))
  - src/handlers/hr/agent/save_long_term_memory.rs + src/handlers/hr/agent/update_memory.rs (v1.2：顶层 relation_ids 收紧为真实落库成功边 ID——删除「预生成 ID 直出」假成功；delete 路径补 rows_affected 检查)
  - 【关联卡·属性侧】docs/wiki/knowledge/zh/本体属性层：ontology_properties TBox 第四张词表 + knowledge_property EAV 版本化 ABox + 四类型列软门禁写入 + property_filters 结构化反查/本体属性层：ontology_properties TBox 第四张词表 + knowledge_property EAV 版本化 ABox + 四类型列软门禁写入 + property_filters 结构化反查.md (v1.2：边也是属性载体 target_type=relation；properties 回执 property_issues_v2 与本卡 relations_result 同批结构化)
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
| [src/service/dal/memory/store.rs](src/service/dal/memory/store.rs) | 落边路径 | `CreateRelations`：与 `save_long_term_memory` 共用同一建边实现 | — |
| [src/handlers/hr/agent/mod.rs#L294-L420](src/handlers/hr/agent/mod.rs#L294-L420) | **v1.2 逐项回执** | write_relations_with_receipt：两写入口共享；占位符/kn_ 前缀静态校验 + 批内同键去重 + 端点批量 Active 预检 + 逐条独立落库 | `:L294-L420` |
| [common/src/error/code.rs#L132-L156](common/src/error/code.rs#L132-L156) | **v1.2 错误码** | 建边 5 码：placeholder_node_id_rejected / invalid_node_id / source_node_not_found / target_node_not_found / duplicate_active_edge（+ internal 兜底） | — |
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

### 3.5 v1.2 逐项 keyed 回执（整批永不 Err）

v1.0/v1.1 建边是「整批 CreateRelations + 顶层 relation_ids」，存在三类假成功：① 新节点场景预生成 relation_id 直接回传，边实际未落库也报成功；② 端点是占位符（SELF / kn_new_self）或非 `kn_` 前缀 ID（任务/项目 ID 误入）时静默丢弃；③ 批内同键重复只落一条但两条都回成功 ID。v1.2 两入口收敛到共享 `write_relations_with_receipt`：

1. **静态校验逐项过**：占位符（self/kn_self/kn_new_self）→ `placeholder_node_id_rejected`；非 kn_ 前缀 → `invalid_node_id`。**relation_type 仍原文直落不归一化**（延续 v1.0 语义，词表外关系名不拦）。
2. **批内同键去重**：同一批次重复 `{source,target,type 小写归一}` → 第二条起 `duplicate_active_edge`（409）。
3. **端点批量预检**：一次查询核全部端点，只认 **Active 知识节点**；源缺失 → `source_node_not_found`（404），目标缺失 → `target_node_not_found`（404）；任务/项目等非节点实体同样落 404（消灭 N+1 与 TOCTOU）；预检基础设施故障时把底层错误码透传给每条待写项，仍不整体上抛。
4. **逐条独立落库**：每条边单独 try，成功才生成真实 relation_id；任一失败不影响其他边。
5. **回执契约**：`relations_result[]{key,status,relation_id,error_code,message,field}`，`key = "{index}:{source}->{target}"` 稳定可定位；顶层 `relation_ids` **只列成功边**；整批不 Err（节点已建成功时建边问题永远走回执，不把整次沉淀打成失败）。

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
10. ✅ （v1.2）**整批建边永不整体 Err**：逐条失败必须进 `relations_result[]` 且带稳定 key（`{index}:{source}->{target}`）；禁止「一条坏边整批 400」——节点已建成时整批失败会逼模型重复建节点。
11. ✅ （v1.2）**占位符与非 kn_ ID 必须逐项拒绝**：SELF/kn_self/kn_new_self 只能用于节点自身引用语义，绝不能进边端点（400 占位符/格式码）；kn_ 前缀但不存在或非 Active（含软删、误入的非节点实体）走批量预检 404（source/target 分码）；两关都禁止静默丢弃。
12. ✅ （v1.2）**顶层 `relation_ids` 只准包含真实落库成功边 ID**：禁止预生成 ID 直出（旧假名成功根源）；批内同键重复首条外回 `duplicate_active_edge`。
13. ✅ （v1.2）**relation_type 词表外原文直落口径不变**：新回执链不得借机做关系名归一化/拒绝（`relation_type` 原样落库仍由 DAO 写侧归一化承担）。

---

## §5 历史演进

| 版本 | 变更 | 影响范围 |
|------|------|---------|
| v1.0（补边入口落地） | `UpdateMemoryParams` 新增 `relations`；`update_memory` 更新成功后经 `CreateRelations` 落边；relations 仅知识节点（否则 400）；工具描述写明"唯一建边入口 + 正文不可达" | `common/src/api/neural_tools.rs` + `src/handlers/hr/agent/update_memory.rs` |
| v1.1（Prompt 引导对齐） | awaken/settle 双场景 System 指引改为「闲聊换目的检索」+「递进探索 ✅ / 同义重试 ❌」；沉淀场景鼓励沿命中节点递进探索 | `src/service/dal/agent/builder/default.rs` |
| v1.2（逐项 keyed 回执，87c35228） | 两写入口收敛到 `write_relations_with_receipt`：静态校验（占位符/kn_ 前缀）+ 批内同键去重 + 端点批量 Active 预检 + 逐条落库；`relations_result[]{key,status,...}` 强回执，整批永不 Err；`relation_ids` 收紧为成功边（根治预生成 ID 假成功）；新增建边 5 错误码；边同时成为属性载体（target_type=relation，见本体属性层卡） | `src/handlers/hr/agent/mod.rs#L294-L420` + `common/src/api/neural_tools.rs` + `common/src/error/code.rs#L132-L156` |

**引入原因**：修复"沉淀联想落成隐形边"——Agent 的关联意图写在正文里，图谱却读不到，导致知识图谱看起来稀疏、探索断链。修复没有改遍历算法（遍历本就正确），而是补上"写入侧缺的唯一入口"并在 Prompt 里把探索节奏说清。