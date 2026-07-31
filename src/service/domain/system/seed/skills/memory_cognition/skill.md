# 记忆认知

本指南帮助你建立完整的记忆认知体系。记忆是你认知世界、积累经验、持续成长的基础。像人一样，你需要主动总结归纳，将重要信息沉淀为长期知识。

## 从 Soul 出发：你的认知起点

你的 `soul`（核心记忆）定义了你是谁——人格、角色、能力声明。这是你认知的起点，每次请求都会携带。

认知的延伸靠四层记忆协同工作：

```
Soul（核心认知）
  ↓ 定义"我是谁"
工作记忆（当前会话）
  ↓ 记住"正在做什么"
短期记忆（近期摘要）
  ↓ 归纳"最近学到了什么"
长期知识图谱（经验沉淀）
  ↓ 积累"我知道什么"
```

## 四层记忆模型

| 层级 | 类型 | 写入方式 | 用途 | 存储位置 |
|------|------|---------|------|---------|
| **核心记忆** | soul / capabilities | 配置定义 | 人格、能力声明 | 内存 + 数据库 |
| **工作记忆** | Trace | 系统自动记录 | 当前会话原始对话与思考 | 内存 + 每日文件 |
| **短期记忆** | ShortTerm | 主动调用工具保存 | 思考过程中总结的摘要 | 数据库 |
| **长期知识** | KnowledgeNode + Relation | 休息时自动沉淀 | 结构化知识图谱 | 数据库 |

### 核心记忆（Soul）
你的人格底色，定义你的角色和能力边界。由系统配置，不可通过工具直接修改。

### 工作记忆（Trace）
系统自动记录的当前会话原始对话和思考过程，客观不可变。按天存储为 markdown 文件，人类可读。你不需要主动操作。

### 短期记忆（主动归纳）
**这是你需要主动管理的核心能力。** 像人和人交流一样，在对话过程中提取重点，归纳总结后保存。

### 长期知识图谱（沉淀积累）
短期记忆积累到一定量后，通过"睡眠"机制沉淀为结构化知识图谱，支持关系遍历和语义检索。

## 短期记忆：思考中实时归纳

### 核心原则：像人一样提取重点

人和人交流时，不会记住每个字，而是提取关键信息：重要决定、需求约束、事实数据、经验教训。你也要这样。

### 何时保存短期记忆

用 `save_short_term_memory` 在以下时机保存：
- 用户提出了重要需求或约束
- 做出了关键决策或判断
- 发现了值得记录的经验或模式
- 获得了关键事实数据
- 会话即将结束，需要总结

### 保存什么

- **写摘要**，不写原文（原文由 Trace 自动记录）
- **提炼要点**，不是复述对话
- **结构化**表达，便于后续检索

示例：
- ✅ 好：`用户要求实现支持分页的技能搜索接口，每页默认 20 条，按相关度排序`
- ❌ 差：`用户说了要改搜索接口，然后说了分页，然后说了每页 20 条，然后说了排序...`

### 参数说明

- `summary` — 记忆摘要（必填，简洁精炼）
- `tags` — 标签列表（可选，便于分类检索，如 `["需求", "用户偏好"]`）
- `task_id` — 关联任务 ID（可选，用于任务关联）
- `trace_ids` — 关联原始对话 ID（可选，便于溯源）

## 长期知识图谱：从沉淀到复用

长期知识图谱是你经验沉淀的核心。它由**节点**（KnowledgeNode）和**关系**（Relation）组成，形成一个可遍历的网状结构。短期记忆通过"睡眠沉淀"机制转化为知识节点，你也可以主动创建节点。

### 节点设计

每个知识节点代表一个独立的知识单元：

| 字段 | 说明 | 示例 |
|------|------|------|
| `node_name` | 节点名称（简洁可识别） | `分页查询接口规范` |
| `node_description` | 节点描述（完整内容） | `所有列表接口必须支持 page/page_size 参数...` |
| `node_type` | 节点类型 | `concept` / `fact` / `skill` / `pattern` |
| `summary` | 节点摘要（用于向量检索） | `分页接口规范，统一 page/page_size 参数` |
| `tags` | 标签（用于过滤检索） | `["API", "规范"]` |

**节点类型选择**：
- `concept` — 抽象概念（如"记忆系统四层架构"）
- `fact` — 具体事实（如"项目使用 SQLite + LanceDB"）
- `skill` — 操作技能（如"使用 search_memory 进行图谱遍历"）
- `pattern` — 经验模式（如"创建节点前先检索相似知识"）

### 关系设计

关系是图谱的灵魂，连接独立节点形成网络。关系**有方向性**：`source → target`。

**可用关系类型**（创建时 `relation_type` 传字符串值）：

| 关系类型 | 语义 | 使用场景 |
|---------|------|---------|
| `related` | 相关 | 两个节点内容相关，但无明确层级/因果 |
| `contains` | 包含（父→子） | 源节点是目标节点的父级（如"记忆系统" contains "短期记忆"） |
| `contained_by` | 被包含（子→父） | 反向包含 |
| `depends` | 依赖 | 源节点依赖目标节点（如"图谱遍历" depends "向量搜索"） |
| `depended_by` | 被依赖 | 反向依赖 |
| `prerequisite` | 前置知识 | 学习源节点前需先掌握目标节点 |
| `followup` | 后续知识 | 源节点是目标节点的延伸 |
| `similar` | 相似 | 两个节点内容相似（可合并） |
| `opposite` | 相反/矛盾 | 两个节点内容冲突（需判断取舍） |
| `causes` | 因果（源导致目标） | 源节点是因，目标节点是果 |
| `caused_by` | 反向因果 | |
| `instance_of` | 实例 | 源节点是目标节点的具体实例 |
| `category_of` | 分类 | 源节点是目标节点的分类 |
| `attribute_of` | 属性 | 源节点是目标节点的属性 |
| `value_of` | 属性值 | 源节点是目标节点的属性值 |
| `custom` | 自定义 | 以上都不适用 |

**关系建立原则**：
1. **方向正确**：`contains` 是父→子，`contained_by` 是子→父，选对方向
2. **语义准确**：用最精确的关系类型，避免滥用 `related`
3. **建立双向**：重要关系可建立双向（如 `depends` + `depended_by`），便于双向遍历
4. **适度关联**：一个节点的关系不宜过多（建议 < 10），避免图谱噪声

### 沉淀机制（自动）

调用 `settle_memory` 触发自动沉淀（像人睡觉时整理记忆）：

1. 系统获取近期**未沉淀**的短期记忆（status=Active）
2. 用 LLM 归纳总结为知识节点
3. 通过向量相似度检测已有知识：
   - **相似度 > 0.85** → 视为同一知识，**优先更新旧节点**
   - **相似度 < 0.85** → 视为新知识，新建节点
4. 自动建立节点间关系（`related` / `contains` / `depends` 等）
5. **自动建立引用**：每个新节点会关联其来源的短期记忆 ID（`references`），可溯源到原始对话
6. 标记已处理的短期记忆为 `Settled`（不再重复沉淀）

**沉淀时机**：
- 连续工作多轮后（如完成一个复杂任务）
- 感觉短期记忆过多需要整理时
- 上下文即将切换，需要保存当前阶段成果
- `settle_memory` 的 `limit` 参数控制每次处理数量（默认 10）

### 主动创建节点（手动）

用 `save_long_term_memory` 主动创建知识节点，适合**确定性的、不需要归纳的知识**：

```json
{
  "node_name": "分页查询接口规范",
  "node_description": "所有列表接口必须支持 page/page_size 参数...",
  "node_type": "pattern",
  "summary": "分页接口规范，统一 page/page_size 参数",
  "tags": ["API", "规范"],
  "relations": [
    {
      "source_node_id": "本节点ID（系统自动填充）",
      "target_node_id": "已有节点ID",
      "relation_type": "depends"
    }
  ]
}
```

**关键原则：创建新节点前，先用 `search_memory` 检查是否已有相似知识，优先更新而非重复创建。**

`save_long_term_memory` 会在内部把 `source_node_id` 替换为新创建节点的 ID，你只需提供 `target_node_id` 和 `relation_type`。

### 引用与溯源

每个通过 `settle_memory` 沉淀的知识节点，都会通过 `references` 表关联其来源的短期记忆 ID。这意味着：
- 知识节点不是凭空产生的，可以追溯到原始对话片段
- 短期记忆被删除时，知识节点仍保留（引用关系自动解除）
- 知识节点被删除时，会级联删除其所有关系（入边/出边）和引用记录

## 记忆搜索：三种模式

用 `search_memory` 检索记忆，通过参数组合切换三种模式：

### 模式 1：纯语义搜索（`traversal_depth=0` 或不传）

**行为**：关键词 + 向量语义匹配，返回最相关的短期记忆和知识节点。

**返回**：只有 `short_term` 和 `knowledge_node` 类型，无 `relation`。

**适用**：快速查找某个知识点，不需要关联扩展。

```json
{
  "query": "分页接口规范",
  "max_results": 5
}
```

### 模式 2：语义 + 图谱遍历（`traversal_depth>0`，不传 `seed_node_ids`）

**行为**：先用语义搜索找到种子节点，再沿关系遍历扩展，返回**节点 + 关系**混合结果。

**返回**：`knowledge_node` + `relation` 类型。`relation` 携带 `source_node_id` / `target_node_id` / `relation_type`，描述节点间关联。

**适用**：探索某个主题的关联知识，发现隐藏联系。

```json
{
  "query": "记忆系统",
  "traversal_depth": 2,
  "traversal_breadth": 5,
  "traversal_strategy": "breadth_first"
}
```

### 模式 3：纯图谱遍历（`traversal_depth>0`，传 `seed_node_ids`）

**行为**：跳过语义搜索，直接从指定节点出发遍历。

**返回**：`knowledge_node` + `relation` 类型。

**适用**：已知起点节点，探索其关联网络（如从一个知识点追溯到所有前置知识）。

```json
{
  "query": "",
  "seed_node_ids": ["node_id_1", "node_id_2"],
  "traversal_depth": 3,
  "traversal_strategy": "depth_first"
}
```

### 遍历参数调节

| 参数 | 含义 | 建议值 | 调节策略 |
|------|------|--------|---------|
| `traversal_depth` | 最大遍历深度 | 1-3 | 越大探索越远，但结果越多越噪声。1=直接关联，2=两跳，3=三跳（通常足够） |
| `traversal_breadth` | 每层最大展开数 | 5-10 | 0=不限制。限制可防止热门节点爆炸，建议设 5-10 平衡覆盖与噪声 |
| `traversal_strategy` | 遍历策略 | 见下 | 根据目标选择 |

### 遍历策略选择

| 策略 | 行为 | 适用场景 |
|------|------|---------|
| `breadth_first` | 广度优先，先遍历所有直接关联，再深入下一层 | 找横向联系（如"这个概念涉及哪些领域"）、看全貌 |
| `depth_first` | 深度优先，沿一条分支深入到底，再回溯 | 追溯因果链（如"A 导致 B，B 导致 C"）、找根因 |

**策略选择示例**：
- 想知道"记忆系统"包含哪些子模块 → `breadth_first` + `depth=1`
- 想追溯"任务失败"的根因链 → `depth_first` + `depth=3`
- 想了解某个概念的全貌 → `breadth_first` + `depth=2` + `breadth=5`

### 返回结果结构

`search_memory` 返回 `results` 数组，每项是 `MemoryResult`，根据 `memory_type` 区分：

| 字段 | `short_term` | `knowledge_node` | `relation` | `trace` |
|------|:---:|:---:|:---:|:---:|
| `id` | ✅ | ✅ | ✅ | ✅ |
| `content` | summary | node_description | relation_type | input |
| `summary` | ✅ | ✅ | ❌ | ❌ |
| `score` | 向量距离 | 向量距离 | ❌ | 向量距离 |
| `tags` | ✅ | ✅ | ❌ | ❌ |
| `source_node_id` | ❌ | ❌ | ✅ | ❌ |
| `target_node_id` | ❌ | ❌ | ✅ | ❌ |
| `relation_type` | ❌ | ❌ | ✅ | ❌ |

**关键：遍历结果包含节点和关系两类，你需要**：
1. 先按 `memory_type` 分离节点和关系
2. 用 `relation` 的 `source_node_id` / `target_node_id` / `relation_type` 重建关联拓扑
3. `relation_type` 字符串如 `"contains"` / `"depends"` / `"causes"` 描述关系语义
4. 节点的 `score` 是向量距离（越小越相似），关系的 `score` 为空

**结果解读示例**：
```json
[
  {"id":"n1","memory_type":"knowledge_node","content":"记忆系统四层架构","summary":"...","score":0.12,"tags":["架构"]},
  {"id":"r1","memory_type":"relation","content":"contains","source_node_id":"n1","target_node_id":"n2","relation_type":"contains"},
  {"id":"n2","memory_type":"knowledge_node","content":"短期记忆","summary":"...","score":0.0,"tags":["记忆"]}
]
```
读作：`n1 (记忆系统四层架构) --contains--> n2 (短期记忆)`。

## 记忆查询与维护

- `query_memory` — 按字段精确查询（`agent_id`、`memory_type`、`tags`），无向量计算，适合结构化筛选
- `update_memory` — 更新 `ShortTerm` / `KnowledgeNode` 内容（自动重新向量化）
- `delete_memory` — 删除记忆：
  - `ShortTerm` → 删库 + 删向量
  - `KnowledgeNode` → **级联删除**入边/出边关系 + 引用记录 + 节点 + 向量

## 认知闭环

完整的认知流程，形成闭环：

```
感知（接收任务/信息）
  ↓
思考（工作记忆 + 已有知识检索）
  ↓
行动（使用工具执行）
  ↓
归纳（save_short_term_memory 提取重点）
  ↓
沉淀（settle_memory 转化为长期知识，自动建立关系和引用）
  ↓
复用（下次 search_memory 检索应用，遍历图谱发现关联）
```

## 最佳实践

1. **实时归纳**：重要信息出现时立即 `save_short_term_memory`，不要依赖会后再回忆
2. **写摘要不写原文**：提炼要点，不是复述对话
3. **善用标签**：为记忆添加准确的 tags，提高后续检索效率
4. **定期沉淀**：工作多轮后主动 `settle_memory`，让短期记忆转化为长期知识
5. **先检索再创建**：创建新知识前先 `search_memory` 检查，优先更新旧知识（避免重复节点）
6. **精准建关系**：创建节点时尽量建立合理关联，选择最精确的 `relation_type`，注意方向性
7. **善用遍历**：不要只做 `depth=0` 的纯语义搜索，复杂问题用 `depth=2-3` 遍历发现关联
8. **策略选对**：找横向联系用 `breadth_first`，追溯因果链用 `depth_first`
9. **广度限制**：遍历时设 `traversal_breadth=5-10`，防止热门节点结果爆炸
10. **清理过时**：发现错误记忆时及时 `update_memory` 或 `delete_memory`，删除节点会自动清理关联
