---
kind: RAG 原子知识卡
name: 知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出
category: 记忆系统 / 图谱遍历
scope:
  - "src/service/dal/memory.rs"
  - "src/service/dao/memory/**"
  - "src/handlers/hr/agent/traverse_graph.rs"
  - "common/src/api/memory.rs"
source_files:
  - src/service/dal/memory.rs#L139-L164 (MemoryDal trait：traverse_knowledge_graph 签名：seed_node_ids + traversal_depth + traversal_strategy(enum BFS/DFS))
  - src/service/dal/memory.rs#L518-L577 (traverse_knowledge_graph 总入口：按 strategy enum 分发给 traverse_bfs / traverse_dfs；最后统一做可见性过滤（agent_id + published）)
  - src/service/dal/memory.rs#L805-L890 (traverse_bfs：队列前沿 + levels Vec<Vec<NodeId>> 分层返回；每层 fetch_nodes_by_ids 批量；BFS 天然 = 最短路径链探索)
  - src/service/dal/memory.rs#L891-L990 (traverse_dfs：栈 + edge_cache + 栈前沿批量预取当前栈上所有未访问节点的所有边，修复 DFS N+1 问题；IN 列表 400 分块)
  - src/service/dal/memory.rs#L653-L720 (list_relations_batch + IN 分块 400：from_id IN 和 to_id IN 两个列表每节点 2 bind → 400 × 2 = 800 < SQLite 999 上限，留 199 安全余量；结果按 created_at ASC 重排保持稳定顺序契约)
  - src/service/dal/memory.rs#L721-L804 (fetch_nodes_by_ids + IN 分块 400：单列表 ids IN 1 bind/id → 400 × 1 = 400 < 999；多次 DAO query_knowledge_nodes 后做共享可见性过滤 + 去重)
  - src/models/memory/knowledge_relation_po.rs (KnowledgeRelationPo：from_node_id/to_node_id/relation_type/weight 四字段；traverse 返回的边会去重 weight 最大的一条)
  - common/src/api/memory.rs (TraverseKnowledgeGraphParams / TraverseKnowledgeGraphResponse：返回 nodes + edges + ordered_levels 三段，前端按 levels 做层级渲染)
  - docs/archive/plan-archive/图谱遍历查询优化.md（完整 7 章：DFS 栈预取批量缓存 + IN 分块 400 模式 + 测试覆盖 5 场景）
  - docs/archive/design-archive/memory_search_enhancement_design.md（§1 决策 2：图谱遍历位置放 DAL；§三 涉及文件 list_relations_batch 抽取）
  - （占位：待 ai-orz-doc-maintainer 落地后回填 design/traverse_performance_optimization.md 路径 → 目前只有 Plan，是 Batch 先落地、Design 后补的反模式，按 §4.10 两阶段初始化规范，Design 文档应补齐决策表）
  - docs/wiki/zh/content/项目概述/核心功能特性/四层记忆系统/短期记忆 (Short-term Memory)/记忆搜索机制.md（§6 图谱遍历 纯图谱/语义+遍历 两种使用模式 + §8 故障排查 N+1 定位）
  - docs/wiki/zh/content/项目概述/核心功能特性/四层记忆系统/长期记忆 (Long-term Memory)/知识关系管理.md（知识关系 PO 与 relation_type 枚举（因果/关联/前置/后置/引用））
  - docs/wiki/zh/content/架构设计/记忆系统架构.md（§知识图谱子系统：Node + Edge 存储 + 遍历 API 三层）
  - 【平行卡 1】docs/wiki/knowledge/zh/记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 三因子推荐/记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 三因子推荐.md（搜索扩展的 traverse 调用方 = 三位一体搜索里的「图谱关系」路径；本卡是 traverse 实现细节）
  - 【平行卡 2】docs/wiki/knowledge/zh/recommend_seed_nodes 种子节点推荐：三因子打分 0.45 连通度 0.35 内容丰富度 0.2 分享权重 + KnowledgeGraph 组件两端复用/recommend_seed_nodes 种子节点推荐：三因子打分 0.45 连通度 0.35 内容丰富度 0.2 分享权重 + KnowledgeGraph 组件两端复用.md（用户进入图谱页面的「起步入口」推荐；用户点击推荐卡片后调用的就是本卡的 traverse_knowledge_graph）
---

## §1 概述

**本卡角色**：知识图谱遍历底层实现的技术约束卡。覆盖 `MemoryDal::traverse_knowledge_graph` 对外统一入口签名、BFS/DFS 两种策略、针对 SQLite `SQLITE_MAX_VARIABLE_NUMBER = 999` 绑定参数上限做的 **IN 列表 400 分块**通用模式，以及 DFS「栈前沿批量预取 + edge_cache」改造修复的 DFS N+1 问题。**定位：排查图谱遍历慢/报 "too many SQL variables" 错 / 扩展新遍历策略时读。**

- **对外统一签名**：`traverse_knowledge_graph(ctx, TraverseParams { seed_node_ids, traversal_depth, traversal_strategy }) → TraverseResponse { nodes, edges, ordered_levels }`。`ordered_levels: Vec<Vec<NodeId>>` 对应 BFS/DFS 的每一层/每一步节点序列，前端可以做动画播放（按 level 渐进展示节点）。
- **IN 分块 400 普适规则**：凡 DAO SQL 中涉及 `ids IN (?, ?, ...)` 绑定参数列表，一律按 **400 个 ID 一块**分块逐次执行，结果拼接后按全局 created_at ASC 重排。原因：(1) 两个 IN 列表并存时（list_relations_batch: from_id IN + to_id IN），400×2=800 < 999；(2) 单 IN 列表 400 < 999，留 599 余量给未来 SQL 新增 bind；(3) 分块常量统一由 `src/service/dal/memory.rs` 顶部 `const IN_CHUNK_SIZE: usize = 400;` 管理，禁止散落各处 magic number。
- **DFS 改造收益**：旧 DFS 每 pop 一个节点查一次边（N+1 → 对宽前沿图 IO 爆炸），新 DFS 做法：栈上所有未 visited + 未 fetched 的节点一次性 batch 查边，结果塞 edge_cache（HashMap<NodeId, Vec<Edge>>），pop 时如果 cache 命中直接用未命中触发一次批量预取当前整个栈前沿。测试：10k 节点链 → 查询次数从 10k 降到 ≈ 25 次。

---

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 锚点 |
|------|------|---------|------|
| memory.rs (trait) | 对外签名 | TraverseParams 三参数：seed_node_ids(起始点数组；None = 从 recommend_seed_nodes 里先取 Top5) / traversal_depth(默认 3，>5 强制 truncate 防爆炸) / traversal_strategy(BFS/DFS) | `:L139-L164` |
| memory.rs (impl 总入口) | 策略分发 | 按 strategy enum dispatch → BFS 调 traverse_bfs / DFS 调 traverse_dfs → 结果 nodes + edges 过 `apply_visibility_filter(agent_id + published_flag)` 共享过滤 | `:L518-L577` |
| memory.rs (traverse_bfs) | BFS 分层实现 | queue<(node_id, level)> + visited HashSet；每 pop 一批同 level 节点 → fetch_nodes_by_ids 批量 → edges IN_CHUNK_SIZE 拉 → 推入 ordered_levels[level] | `:L805-L890` |
| memory.rs (traverse_dfs) | DFS 栈批量预取 | stack + edge_cache<NodeId, Vec<Edge>> + fetched_flag HashSet<NodeId>；每次 edge_cache miss 时，「当前栈上所有未 fetched 的节点」→ IN_CHUNK_SIZE 批量查，结果进 cache | `:L891-L990` |
| memory.rs (list_relations_batch) | 关系批量查 + IN 分块 | from_ids + to_ids 两 Vec → 按 IN_CHUNK_SIZE=400 zip 分块 → 每块 SQL "WHERE from_id IN (...) OR to_id IN (...)" → 全部块 UNION ALL 拼接后按 created_at ASC 重排 | `:L653-L720` |
| memory.rs (fetch_nodes_by_ids) | 节点批量查 + IN 分块 | ids Vec → IN_CHUNK_SIZE 400 分块 → 每块 query_knowledge_nodes(Query { ids: chunk, .. }) → 结果去重 → 共享可见性过滤 | `:L721-L804` |
| common api/memory.rs | DTO | TraverseKnowledgeGraphResponse：nodes 是去重节点、edges 是去重关系带 weight、ordered_levels 是 BFS/DFS 分层顺序（严格按层，前端渲染顺序的唯一来源）| 见 common DTO |

**章节来源**
- [memory.rs:L518-L577](src/service/dal/memory.rs#L518-L577)
- [memory.rs:L805-L890](src/service/dal/memory.rs#L805-L890)
- [memory.rs:L891-L990](src/service/dal/memory.rs#L891-L990)
- [memory.rs:L653-L804](src/service/dal/memory.rs#L653-L804)

---

## §3 架构约定与扩展模式

### 3.1 遍历数据流向

```
Handler / Domain 调用方
  │  TraverseKnowledgeGraphParams { seed_node_ids, depth, strategy }
  ▼
traverse_knowledge_graph() (总入口)
  ├─ seed_node_ids 未传 → 内部先 recommend_seed_nodes(ctx, agent_id, limit=5) 取 Top5 作为默认起点
  ├─ strategy == BFS → traverse_bfs()
  │     ├─ queue<(id, level)> 队列推进
  │     ├─ per-level fetch_nodes_by_ids() (IN_CHUNK_SIZE 400 分块)
  │     └─ per-level list_relations_batch() (IN_CHUNK_SIZE 400 分块)
  └─ strategy == DFS → traverse_dfs()
        ├─ stack + visited + fetched_flag + edge_cache
        └─ cache miss → 栈前沿批量预取（一次性拉当前栈上所有未 fetched 的节点的边）
  ▼
  共享出口：apply_visibility_filter()
        ├─ 仅保留：node.agent_id == 传入 agent_id  OR  node.is_published = true
        └─ 对应的 edge 两端节点都保留的 edge 才保留（否则孤立孤立端要被剔除）
  ▼
TraverseKnowledgeGraphResponse { nodes, edges, ordered_levels }
```

### 3.2 扩展模式：加第 3 种遍历策略（如 Dijkstra 最短带权路径）

1. **扩展点 1**：在 `common/src/enums/`（或 `common/src/api/memory.rs` 内）给 `TraversalStrategy` enum 加新变体 `Dijkstra { weight_field: RelationWeight }`。
2. **扩展点 2**：在 `memory.rs (DAL impl)` 总入口 `traverse_knowledge_graph` 的 match 加新分支，调用新建的 `traverse_dijkstra()` 私有函数。
3. **扩展点 3**：**复用 IN_CHUNK_SIZE 400 的 list_relations_batch + fetch_nodes_by_ids**，不要重写 SQL（重写就会丢分块）。
4. **扩展点 4**：在 `图谱遍历查询优化.md` Plan 附录追加新策略的测试场景，特别是极端：一条 10k 节点长链 + 一个 100 边扇出的 hub 节点——验证不会触发 "too many SQL variables"（防回归）。

---

## §4 硬约束与故障排查

### 4.1 必守红线

1. **红线 1**：**所有 `IN (?, ?, ...)` 列表必须走 IN_CHUNK_SIZE=400 分块**，不管你目测 IDs 有多短——代码评审里看到 ids.len() 小就不走分块的，一律打回。理由：上线后数据量一涨，那天刚好 ids 破 999，所有遍历全部报 "too many SQL variables" 炸库。
2. **红线 2**：**traversal_depth > 5 必须强制 truncate**，哪怕调用方明确传了 depth=100。图谱里一个连通分量上万节点，深度 10 的 BFS 能一次性把内存拉爆。
3. **红线 3**：**ordered_levels 里的节点必须同时在 nodes Vec 里出现**，前端按 levels 渲染时，按 level node_id 去 nodes 里找 node——找不到直接 panic。出口 `apply_visibility_filter` 会「先砍 nodes+edges，再同步从 ordered_levels 里删不可见节点」，不要绕过这个函数。
4. **红线 4**：list_relations_batch / fetch_nodes_by_ids **分块后必须做全局去重 + created_at ASC 重排序**。否则前端每次刷新同一个 traverse 参数，返回的 edges 顺序会因为 SQLite IN 查出来顺序不稳定而抖动，画布上节点位置每次刷新都跳，体验极差。

### 4.2 故障排查路径

| 症状 | 起点锚点 | 次级排查 |
|------|---------|---------|
| 图谱遍历偶尔报 "too many SQL variables (code 1 too many SQL variables)" | [memory.rs:L653-L804](src/service/dal/memory.rs#L653-L804) IN_CHUNK_SIZE 是否被改成 > 400 | 查调用方：是否有人绕过 list_relations_batch 直接写了 SQL？grep 整个 src/service/dao 下所有 `IN (` 后面接参数绑定的 SQL，统计每个 SQL 的 bind 数量 |
| 小图（<50 节点）BFS 正常，大图（>5k 节点）DFS 慢到超时（>30s） | [memory.rs:L891-L990](src/service/dal/memory.rs#L891-L990) 检查 edge_cache 命中日志 | 确认「栈前沿批量预取」逻辑是否被误改回到每节点单查——典型：重构 DFS 时不小心删了 fetched_flag，导致每 pop 必触发一次批量预取（反向变成 worse） |
| traverse 返回 nodes 数 = 0，明明 seed_node_ids 是正确的 | [memory.rs:L518-L577](src/service/dal/memory.rs#L518-L577) apply_visibility_filter 的过滤逻辑 | 场景：传入的 agent_id 错了（用了组织 ID 不是 Agent ID），导致所有节点都「不是我的 agent_id 也没 published」被全过滤了。调试方法：临时在 total nodes 前和后打日志，看 filter 前后数量差 |
| ordered_levels 的 level 0 有节点，但前端画出来点都挤在同一个角落 | 检查 TraverseKnowledgeGraphResponse.ordered_levels 是否正确填 | 典型：traverse_bfs 中 visited set 没与 level 同步推进，BFS 层序错乱，levels[0] 把所有节点都标成 level 0。前端按 level 布局时 y 坐标都一样，挤成一根线 |
| traverse 10 次有 1 次返回重复节点（同一 node_id 在 nodes Vec 出现两次） | [memory.rs:L721-L804](src/service/dal/memory.rs#L721-L804) fetch_nodes_by_ids 分块后的拼接逻辑 | 是否漏了 `dedup_by(|a,b| a.id == b.id)`？多块执行同一节点如果跨两块边界（第 399 个和 401 个刚好同一个 node_id 出现两次），不去重就会出现重复 |
