---
kind: RAG 原子知识卡
name: recommend_seed_nodes 种子节点推荐：度数排序（入边+出边）Top N + published 度数持平决胜 + KnowledgeGraph 组件两端复用 + Agent 神经工具免绑定
category: 记忆系统 / 前端组件复用
scope:
  - "src/service/dal/memory.rs"
  - "src/handlers/hr/agent/recommend_seed_nodes.rs"
  - "src/models/memory.rs"
  - "common/src/api/neural_tools.rs"
  - "frontend/src/pages/hr/knowledge_graph.rs"
  - "frontend/src/pages/hr/agent_detail.rs"
  - "frontend/src/components/graph_canvas.rs"
source_files:
  - src/service/dal/memory.rs#L139-L144 (MemoryDal trait：recommend_seed_nodes 签名 —— agent_id: Option<String>（None / 空串 = 蜂巢全域） + limit → Vec<SeedNodeRecommendation>)
  - src/service/dal/memory.rs#L419-L489 (DAL 实现：① 查知识节点（status Active、排除 Forgotten、query limit 500 总池上限）② list_relations_batch 批量拉边 ③ HashMap 统计每节点入度/出度 ④ degree=入+出 倒序、度数持平 `is_published` 决胜 ⑤ truncate(limit))
  - src/models/memory.rs#L608-L617 (领域模型 SeedNodeRecommendation { node: LongTermKnowledgeNodePo, degree, incoming_count, outgoing_count })
  - src/handlers/hr/agent/recommend_seed_nodes.rs#L20-L27 (register_handler_tool(id="recommend_seed_nodes", neural, tags="memory") —— 免绑定神经工具，所有 Agent 自动装配)
  - src/handlers/hr/agent/recommend_seed_nodes.rs#L29-L43 (双入口：HTTP + 工具；limit = params.limit.unwrap_or(5).min(50) → runtime_domain().memory().recommend_seed_nodes → to_api 扁平化映射)
  - common/src/api/neural_tools.rs#L221-L257 (DTO：RecommendSeedNodesParams{agent_id,limit} / RecommendSeedNodesResponse{recommendations} / SeedNodeRecommendation{node_id,node_name,node_description,node_type,summary,tags,degree,incoming_count,outgoing_count})
  - frontend/src/pages/hr/knowledge_graph.rs#L164 (KnowledgeGraph(agent_id: Option<String>) 可复用子组件：单 prop)
  - frontend/src/pages/hr/knowledge_graph.rs#L876 (HrKnowledgeGraph 路由入口：AppLayout + Agent 选择器 + KnowledgeGraph)
  - frontend/src/pages/hr/agent_detail.rs (Agent 详情页内嵌 KnowledgeGraph { agent_id: Some(...) } —— 两端复用落地证据)
  - frontend/src/components/graph_canvas.rs (画布渲染组件：nodes + edges + levels，与推荐算法完全解耦)
  - docs/archive/plan-archive/知识图谱推荐起点与组件复用重构.md (② Plan 快照：度数统计 + 前端组件拆分 HrKnowledgeGraph vs KnowledgeGraph 两端复用 + agent_id Option 语义)
  - docs/archive/design-archive/memory_search_enhancement_design.md (① Design 快照：记忆搜索增强决策表，含图谱推荐起点)
  - docs/wiki/zh/content/功能模块/AI Agent 管理/记忆系统管理.md (③ Wiki 长文：种子节点推荐面板 + 点击定位画布)
  - docs/wiki/zh/content/项目概述/核心功能特性/综合搜索能力/知识图谱搜索.md (③ Wiki 长文：推荐起点 + 画布渲染 + 链式探索三段)
  - 【平行卡】docs/wiki/knowledge/zh/知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出/知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出.md (下游：用户点推荐卡片后实际调用 traverse_knowledge_graph 展开邻域)
  - 【总卡】docs/wiki/knowledge/zh/记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 三因子推荐/记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 三因子推荐.md (本卡是「recommend_seed_nodes 起点推荐」这一段的总-分细卡)
---

## §1 概述

**本卡角色**：知识图谱页面「起步入口」推荐能力的双域（后端算法 + 前端组件）综合卡。覆盖后端 DAL 的**度数排序推荐算法**、HTTP/神经工具双入口、以及前端 `KnowledgeGraph { agent_id: Option<String> }` 可复用子组件——既能在 HrKnowledgeGraph 路由页以「全域 + Agent 选择器」模式用，也能在 Agent 详情页直接嵌入固定 agent_id 的单 Agent 模式。

- **算法（现行）**：按知识节点的**关联度数（入边 + 出边总数）倒序**取 Top N；**度数持平时 `is_published` 优先**。`published` 不再是入池门槛，而是降级为「同等连接度时更值得当起点」的决胜信号——因此即使全库没有 published 节点，全域推荐依然有结果。
- **⚠️ 卡片名历史口径**：卡片路径与标题沿用了早期实现「三因子打分（0.45 连通度 + 0.35 内容丰富度 + 0.2 分享权重）」的命名；该三因子加权实现**已重构为上述度数排序**（见 §5）。`name` 字段已更新为度数排序口径，路径保留不变以维持全库引用稳定（旧名仍可被检索命中）。
- **返回值扁平化**：DTO `SeedNodeRecommendation` 直接摊平节点字段（node_id / node_name / node_type / summary / tags）+ 三个度数指标（degree / incoming_count / outgoing_count），不再有早期 `reasons: Vec<String>` 打分明细。
- **神经工具免绑定**：Handler 带 `neural` 标记 + `tags = "memory"`，所有 Agent 自动装配（无需在工具绑定里手动勾选）。LLM 冷启动时可直接调用本工具挑起点，再用 `search_memory(seed_node_ids=[...], traversal_depth=1~2)` 沿邻域展开——这正是「推荐 → 展开」两段式图谱探索的入口。
- **组件拆分硬约束**：`KnowledgeGraph` 子组件对外只暴露一个 `agent_id: Option<String>` prop，其余状态（推荐结果、搜索参数、画布节点坐标、levels）全部收敛在组件内部 signal 中。

---

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 锚点 |
|------|------|---------|------|
| memory.rs (DAL trait) | 对外签名 | `recommend_seed_nodes(ctx, agent_id: Option<String>, limit)`：`None` / 空串 = 蜂巢全域候选；`Some(id)` = 只筛该 Agent 归属节点；返回 `Vec<SeedNodeRecommendation>` | `:L139-L144` |
| memory.rs (DAL impl) | 度数排序核心 | ① `query_knowledge_nodes`（`Status::Active`、排除 `Forgotten`、query `limit = 500` 总池上限）② `list_relations_batch` 一次批量拉候选点所有出入边 ③ `HashMap<NodeId,(in,out)>` 统计 ④ `degree = in + out`，`sort_by_key((Reverse(degree), Reverse(is_published)))` ⑤ `truncate(limit)` | `:L419-L489` |
| recommend_seed_nodes.rs (Handler) | 神经工具 + HTTP 双入口 | `#[register_handler_tool(id="recommend_seed_nodes", neural, tags="memory")]` 免绑定装配；`limit = params.limit.unwrap_or(5).min(50)`；经 `runtime_domain().memory()` 调 DAL；`to_api` 把 domain 模型映射为 API DTO 并 `parse_tags_json` 解析 tags | `:L20-L43` |
| models/memory.rs | 领域模型 | `SeedNodeRecommendation { node: LongTermKnowledgeNodePo, degree, incoming_count, outgoing_count }` | `:L608-L617` |
| neural_tools.rs (common) | API DTO | `RecommendSeedNodesParams{agent_id,limit}` / `RecommendSeedNodesResponse{recommendations}` / `SeedNodeRecommendation{node_id..tags, degree, incoming_count, outgoing_count}` | `:L221-L257` |
| knowledge_graph.rs (子组件) | KnowledgeGraph 复用子组件 | 单 prop `agent_id: Option<String>`；内部 `use_signal`(推荐结果) + `use_effect(deps=[agent_id])` 自动重拉；推荐卡片网格 → 点击 → seed_node_ids → 调 traverse 展开 → 喂给画布组件 | `:L164` |
| knowledge_graph.rs (路由入口) | HrKnowledgeGraph 页面 | `AppLayout` + 顶部 Agent 选择器（「不选」= None 语义）→ `KnowledgeGraph { agent_id }` | `:L876` |
| agent_detail.rs (Agent 详情页) | 两端复用证据 | Agent 详情页内嵌 `KnowledgeGraph { agent_id: Some(current_agent_id) }` —— 与路由页共用同一子组件 | 见文件 |
| graph_canvas.rs (前端组件) | 画布渲染 | 接收 `nodes + edges + levels`，独立 state，与推荐算法解耦 | 见文件 |

**章节来源**
- [memory.rs:L419-L489](src/service/dal/memory.rs#L419-L489)
- [recommend_seed_nodes.rs:L20-L43](src/handlers/hr/agent/recommend_seed_nodes.rs#L20-L43)
- [neural_tools.rs:L221-L257](common/src/api/neural_tools.rs#L221-L257)
- [knowledge_graph.rs:L164](frontend/src/pages/hr/knowledge_graph.rs#L164)
- [knowledge_graph.rs:L876](frontend/src/pages/hr/knowledge_graph.rs#L876)

---

## §3 架构约定与扩展模式

**关联声明（Level 4 细卡）**：本卡是【总卡】`记忆搜索增强三合一`（FTS5 / traverse / recommend_seed_nodes 三位一体）中「起点推荐」这一段的分细卡，二者构成总-分关系；下游展开能力见【平行卡】`知识图谱 traverse`。

### 3.1 agent_id 语义与候选池

| 入参 | 候选池 | 说明 |
|------|--------|------|
| `agent_id = None` 或 `Some("")` | **蜂巢全域**所有 Agent 的知识节点 | DAL 内 `agent_id.clone().filter(\|s\| !s.is_empty())` 把空串归一为 None；`published` 只参与决胜 |
| `agent_id = Some(id)` | 只筛该 Agent 归属的知识节点 | 用于 Agent 详情页 Tab 内嵌场景 |

### 3.2 排序与决胜

1. 主排序键：`degree = incoming_count + outgoing_count`（有向图总度数，**入边与出边同等计入**），倒序。
2. 次排序键：`is_published` 倒序 —— 仅当度数完全相等时生效。
3. 截断：`limit`（Handler 侧 `unwrap_or(5).min(50)`）；候选池另有 query `limit = 500` 的总量上限，避免节点过多拖慢应用层统计。

### 3.3 双端复用数据流

```
后端推荐算法（DAL 应用层 HashMap 度数统计）
  query_knowledge_nodes(agent_id_filter, status=Active, !Forgotten, pool<=500)
        + list_relations_batch(node_ids)
        │
        ▼  统计 (in, out) → degree=in+out → 倒序 → is_published 决胜 → truncate(limit)
  Vec<SeedNodeRecommendation>（领域模型）
        │  to_api 扁平化
        ▼  RecommendSeedNodesResponse
  ┌─ HTTP：GET/POST /api/v1/hr/agents/recommend_seed_nodes
  └─ 工具：register_handler_tool(neural, tags="memory")
                ▲
                │ 三种调用者：
前端路由页 HrKnowledgeGraph ─┐  Agent 详情页内嵌 ─┐  所有 Agent（神经工具）
     agent_id = None / 选择器值  agent_id = Some(id)   冷启动自主调用
                └────────┬───────────────┘
                         ▼  统一 KnowledgeGraph { agent_id: Option<String> }
                         │  use_effect(agent_id): 自动拉推荐
                         │  推荐卡片网格：点击 → seed_node_ids → 图谱 traverse
                         ▼
                  graph_canvas 画布 (nodes + edges + ordered_levels)
```

### 3.4 扩展模式：想引入「度数与发布之外」的新排序信号

现行实现**没有因子框架**（三因子加权已废弃），新增信号时按下面口径扩展，避免重新引入不可解释的加权：

1. **在 DAL 排序键里加第二/第三排序键**，而不是恢复加权求和。例如「近 7 天访问热度」→ `sort_by_key((Reverse(degree), Reverse(hotness), Reverse(is_published)))`，语义直观、可解释。
2. **DTO 扁平化扩展**：若要把新信号回传前端展示，在 `SeedNodeRecommendation` 加一个明确命名的字段（如 `recent_hits: usize`），不要恢复成自由文本 `reasons`。
3. **前端零改动**：`KnowledgeGraph` 组件不改；`agent_id` 仍是唯一 prop。业务差异（项目/团队维度）通过「过滤 agent_id」实现，不允许污染通用组件。
4. **组件新增嵌入位置**：例如 Project 详情页要嵌项目维度图谱 → 直接新增一行 `KnowledgeGraph { agent_id: Some(owner_agent_id) }`。

---

## §4 硬约束与故障排查

### 4.1 必守红线

1. **红线 1：`published` 只做决胜、绝不做入池门槛**。全域推荐（`agent_id = None`）池子必须是**全部**知识节点；把 `is_published` 当 WHERE 过滤会让「没有 published 节点时全域推荐为空」成为必然 bug。它只在度数相等时提升排序。
2. **红线 2：`limit` 两层保护缺一不可** —— Handler 层 `min(50)`（对外契约）+ DAL 候选池 `limit = 500`（一次统计的总量上限）。禁止只做一层：绕过 Handler 直调 DAL 时仍要有池上限兜底。
3. **红线 3：度数口径必须 `in + out`**。`degree` 是入边 + 出边总数；只算一边会让「被大量引用」或「大量引用他人」的节点排名严重失真。`incoming_count` / `outgoing_count` 必须与 `degree` 自洽（`degree == incoming_count + outgoing_count`）。
4. **红线 4：`KnowledgeGraph` 子组件绝不暴露除 `agent_id` 外的状态**。需要自定义筛选时，正确做法是 (a) 后端 DTO 加字段、(b) 组件内部按 `agent_id` 派生；绝不把内部 signal 通过 prop 透出，否则两端调用方会写出大量 `if custom_mode { ... }`，复用退化为复制粘贴。
5. **红线 5：`recommend_seed_nodes` 是纯读操作**。注册为 `neural` 免绑定工具意味着所有 Agent 都会自动加载，它必须无副作用（不写库、不改状态），否则一次误调用会污染全库。

### 4.2 故障排查路径

| 症状 | 起点锚点 | 次级排查 |
|------|---------|---------|
| 全域图谱（HrKnowledgeGraph 未选 Agent）推荐卡片为空 | [memory.rs:L419-L489](src/service/dal/memory.rs#L419-L489) 检查候选查询 | 典型：知识库确实为空（无 Active 知识节点）。注意**不是** published 过滤导致（published 已不做门槛）；若库内有节点仍为空，检查 `status`/`exclude_status` 是否把节点误排除 |
| 推荐顺序「看起来和关联数不符」 | [memory.rs:L455-L484](src/service/dal/memory.rs#L455-L484) 度数统计与排序 | 典型：只统计了单侧边（入或出）；或两条边指向同一对节点的重复统计；核对 `degree == incoming_count + outgoing_count` 自洽性 |
| 同一 Agent 在路由页与详情页推荐结果不同 | [knowledge_graph.rs:L164](frontend/src/pages/hr/knowledge_graph.rs#L164) 两端调用参数 | 典型：两端 `limit` 不同（截断点不同）→ 节点集合有交集但顺序不保证；或 `agent_id` 一端 `None`（全域）一端 `Some(id)`（仅该 Agent），候选池本就不同 |
| Agent 详情页切到知识图谱 Tab 卡顿/重复请求 | 检查 `use_effect` 是否每次切换都重拉 | [agent_detail.rs](frontend/src/pages/hr/agent_detail.rs) 确认传给 `KnowledgeGraph` 的 `agent_id` 是否每次 render 新建 String（依赖判定为变化 → 不停重拉）；应传稳定值 |
| 新加的排序信号生效但前端看不到 | [neural_tools.rs:L221-L257](common/src/api/neural_tools.rs#L221-L257) | 典型：只改了 DAL 排序键但没在 `SeedNodeRecommendation` / `to_api` 里补字段回传；或前端卡片按固定字段渲染，新字段未接入展示 |

---

## §5 历史演进

- **卡片命名沿革**：最早实现为三因子加权打分（0.45 连通度 + 0.35 内容丰富度 + 0.2 分享权重），返回 `RecommendedSeedNode { node, score, reasons: Vec<String> }`，DTO 落在 `common/src/api/memory.rs`。上述实现与命名一并被本卡标题/路径继承。
- **重构为度数排序**：`recommend_seed_nodes` 改为按关联度数（入边 + 出边）倒序、度数持平用 `is_published` 决胜；`published` 从「入池门槛」降级为「决胜信号」；返回模型改为 `SeedNodeRecommendation { node, degree, incoming_count, outgoing_count }`，DTO 迁至 `common/src/api/neural_tools.rs` 并扁平化（去掉 `score` / `reasons`）。
- **神经工具化（2026-09，b82d3f7f→8fa050d0 区间）**：Handler 补 `#[register_handler_tool(id="recommend_seed_nodes", neural, tags="memory")]`，从纯 HTTP 接口升级为**所有 Agent 自动装配的神经工具**（免绑定），使 Agent 在冷启动时能自主挑选图谱起点再沿 `search_memory(seed_node_ids, traversal_depth)` 展开邻域。
- **前端组件复用**：拆出 `KnowledgeGraph { agent_id: Option<String> }` 子组件，HrKnowledgeGraph 路由页与 Agent 详情页两处共用（早期 Plan 快照见 `docs/archive/plan-archive/知识图谱推荐起点与组件复用重构.md`）。